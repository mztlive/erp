use std::collections::BTreeMap;

use roxmltree::Node;

use super::{document, invalid, text};
use crate::Result;

#[derive(Clone, Default)]
pub(crate) struct Cell {
    pub value: String,
    pub text: bool,
    pub numeric: bool,
    pub formula: bool,
    pub formula_text: Option<String>,
}

#[derive(Clone)]
pub(crate) struct Row {
    pub number: u32,
    pub cells: Vec<Cell>,
}

#[derive(Clone, Copy)]
pub(crate) struct SheetLimits {
    pub columns: usize,
    pub rows: u32,
    pub retained_columns: usize,
}

impl SheetLimits {
    pub const CUSTOMER: Self = Self { columns: 64, rows: 10_000, retained_columns: 64 };
}

pub(crate) struct MergeRange {
    pub start_column: usize,
    pub start_row: u32,
    pub end_column: usize,
    pub end_row: u32,
}

pub(crate) struct Sheet {
    pub rows: Vec<Row>,
    pub merges: Vec<MergeRange>,
    pub hyperlinks: bool,
}

/// 读取有界单元格、合并区及公式元数据，不执行模板业务规则。
/// # 参数
/// `xml` 为工作表，`shared` 为字符串表，`limits` 为行列和保留列限制。
/// # 返回
/// 稀疏行转成的行列表、合并范围及超链接标记。
/// # 错误
/// XML、坐标、共享文本索引、重复单元格或合并范围非法时拒绝。
pub(crate) fn rows(xml: &str, shared: &[String], limits: SheetLimits) -> Result<Sheet> {
    let doc = document(xml)?;
    let root = doc.root_element();
    if root.tag_name().name() != "worksheet" && root.tag_name().name() != "sheetData" {
        return Err(invalid("工作表 XML 结构无效"));
    }
    let namespace = root.tag_name().namespace();
    let mut rows: BTreeMap<u32, BTreeMap<usize, Cell>> = BTreeMap::new();
    for node in doc.descendants().filter(|node| {
        node.is_element()
            && node.tag_name().name() == "c"
            && node.tag_name().namespace() == namespace
            && node.parent_element().is_some_and(|parent| {
                matches!(parent.tag_name().name(), "row" | "sheetData" | "worksheet")
                    && parent.tag_name().namespace() == namespace
            })
    }) {
        let address = node.attribute("r").ok_or_else(|| invalid("单元格缺少坐标"))?;
        let (column, row) = coordinate(address, limits)?;
        if rows.entry(row).or_default().insert(column, read_cell(node, shared)?).is_some() {
            return Err(invalid("工作表存在重复单元格坐标"));
        }
    }
    let merges = doc
        .descendants()
        .filter(|node| node.is_element() && node.tag_name().name() == "mergeCell")
        .map(|node| merge_range(node, limits))
        .collect::<Result<_>>()?;
    let hyperlinks = doc.descendants().any(|node| node.is_element() && node.tag_name().name() == "hyperlink");
    let rows = rows
        .into_iter()
        .map(|(number, cells)| {
            let width = cells.keys().next_back().map_or(0, |index| index + 1).min(limits.retained_columns);
            let cells = (0..width).map(|index| cells.get(&index).cloned().unwrap_or_default()).collect();
            Row { number, cells }
        })
        .collect();
    Ok(Sheet { rows, merges, hyperlinks })
}

fn read_cell(node: Node<'_, '_>, shared: &[String]) -> Result<Cell> {
    let kind = node.attribute("t");
    let text_cell = matches!(kind, Some("s" | "inlineStr" | "str"));
    let numeric = matches!(kind, None | Some("n"));
    let formula_node = node.children().find(|child| child.is_element() && child.tag_name().name() == "f");
    let raw = node.children().find(|child| child.is_element() && child.tag_name().name() == "v").map(text);
    let value = match kind {
        Some("inlineStr") => node
            .descendants()
            .filter(|child| child.is_element() && child.tag_name().name() == "t")
            .filter(|child| {
                !child.ancestors().any(|parent| parent.is_element() && parent.tag_name().name() == "rPh")
            })
            .map(text)
            .collect(),
        Some("s") => {
            let index = raw
                .as_deref()
                .unwrap_or_default()
                .parse::<usize>()
                .map_err(|_| invalid("共享文本索引无效"))?;
            shared.get(index).cloned().ok_or_else(|| invalid("共享文本索引越界"))?
        },
        _ => raw.unwrap_or_default(),
    };
    let formula_text = formula_node.filter(|node| node.first_child().is_some()).map(text);
    Ok(Cell { value, text: text_cell, numeric, formula: formula_node.is_some(), formula_text })
}

fn coordinate(address: &str, limits: SheetLimits) -> Result<(usize, u32)> {
    let split = address.find(|ch: char| ch.is_ascii_digit()).ok_or_else(|| invalid("单元格坐标无效"))?;
    let column = address[..split]
        .bytes()
        .try_fold(0_usize, |value, byte| {
            byte.is_ascii_uppercase().then_some(())?;
            value.checked_mul(26)?.checked_add(usize::from(byte - b'A' + 1))
        })
        .and_then(|column| column.checked_sub(1))
        .ok_or_else(|| invalid("单元格列号无效"))?;
    let row = address[split..].parse::<u32>().map_err(|_| invalid("单元格行号无效"))?;
    if row == 0 || row > limits.rows || column >= limits.columns {
        return Err(invalid("工作表行列范围超过模板限制"));
    }
    Ok((column, row))
}

fn merge_range(node: Node<'_, '_>, limits: SheetLimits) -> Result<MergeRange> {
    let reference = node.attribute("ref").ok_or_else(|| invalid("合并单元格范围缺失"))?;
    let (start, end) = reference.split_once(':').ok_or_else(|| invalid("合并单元格范围无效"))?;
    let (start_column, start_row) = coordinate(start, limits)?;
    let (end_column, end_row) = coordinate(end, limits)?;
    if start_column > end_column || start_row > end_row {
        return Err(invalid("合并单元格范围无效"));
    }
    Ok(MergeRange { start_column, start_row, end_column, end_row })
}
