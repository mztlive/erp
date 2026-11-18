use std::collections::BTreeMap;

use roxmltree::Node;

use super::{document, invalid, rich_text, text};
use crate::Result;

#[derive(Clone, Default)]
pub(crate) struct Cell {
    pub(crate) value: String,
    pub(crate) text: bool,
    pub(crate) numeric: bool,
    pub(crate) formula: bool,
    pub(crate) formula_text: Option<String>,
}

#[derive(Clone)]
pub(crate) struct Row {
    pub(crate) number: u32,
    pub(crate) cells: Vec<Cell>,
}

#[derive(Clone, Copy)]
pub(crate) struct SheetLimits {
    pub(crate) columns: usize,
    pub(crate) rows: u32,
    pub(crate) retained_columns: usize,
}

impl SheetLimits {
    pub(crate) const CUSTOMER: Self = Self { columns: 64, rows: 10_000, retained_columns: 64 };
}

pub(crate) struct MergeRange {
    pub(crate) start_column: usize,
    pub(crate) start_row: u32,
    pub(crate) end_column: usize,
    pub(crate) end_row: u32,
}

pub(crate) struct Sheet {
    pub(crate) rows: Vec<Row>,
    pub(crate) merges: Vec<MergeRange>,
    pub(crate) hyperlinks: bool,
}

// 读取有界单元格、合并区及公式元数据，不执行模板业务规则。
//
// `xml` 为工作表，`shared` 为字符串表，`limits` 为行列和保留列限制。
//
// 稀疏行转成的行列表、合并范围及超链接标记。
//
// XML、坐标、共享文本索引、重复单元格或合并范围非法时拒绝。
pub(crate) fn rows(xml: &str, shared: &[String], limits: SheetLimits) -> Result<Sheet> {
    let doc = document(xml)?;
    let root = doc.root_element();
    if root.tag_name().name() != "worksheet" && root.tag_name().name() != "sheetData" {
        return Err(invalid("工作表无法读取，请用原模板重新另存为 .xlsx 后上传"));
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
        let address = node
            .attribute("r")
            .ok_or_else(|| invalid("工作表包含无法定位的单元格，请将数据复制到新的导入模板"))?;
        let (column, row) = coordinate(address, limits)?;
        if rows.entry(row).or_default().insert(column, read_cell(node, shared)?).is_some() {
            return Err(invalid("工作表中同一位置包含多份数据，请将数据复制到新的导入模板"));
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
        .map(|(number, mut cells)| {
            let width = cells.keys().next_back().map_or(0, |index| index + 1).min(limits.retained_columns);
            let cells = (0..width).map(|index| cells.remove(&index).unwrap_or_default()).collect();
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
        Some("inlineStr") => rich_text(node),
        Some("s") => {
            let index = raw
                .as_deref()
                .unwrap_or_default()
                .parse::<usize>()
                .map_err(|_| invalid("工作表中的文字无法读取，请用原模板重新另存后上传"))?;
            shared
                .get(index)
                .cloned()
                .ok_or_else(|| invalid("工作表中的文字无法读取，请用原模板重新另存后上传"))?
        },
        _ => raw.unwrap_or_default(),
    };

    let formula_text = formula_node.filter(|node| node.first_child().is_some()).map(text);
    Ok(Cell { value, text: text_cell, numeric, formula: formula_node.is_some(), formula_text })
}

fn coordinate(address: &str, limits: SheetLimits) -> Result<(usize, u32)> {
    let split = address
        .find(|ch: char| ch.is_ascii_digit())
        .ok_or_else(|| invalid("工作表包含无法定位的单元格，请将数据复制到新的导入模板"))?;
    let column = address[..split]
        .bytes()
        .try_fold(0_usize, |value, byte| {
            byte.is_ascii_uppercase().then_some(())?;
            value.checked_mul(26)?.checked_add(usize::from(byte - b'A' + 1))
        })
        .and_then(|column| column.checked_sub(1))
        .ok_or_else(|| invalid("工作表包含无法定位的列，请将数据复制到新的导入模板"))?;
    let row = address[split..]
        .parse::<u32>()
        .map_err(|_| invalid("工作表包含无法定位的行，请将数据复制到新的导入模板"))?;
    if row == 0 || row > limits.rows || column >= limits.columns {
        return Err(invalid("工作表行列范围超过模板限制"));
    }
    Ok((column, row))
}

fn merge_range(node: Node<'_, '_>, limits: SheetLimits) -> Result<MergeRange> {
    let reference =
        node.attribute("ref").ok_or_else(|| invalid("工作表中的合并单元格无法识别，请取消合并后重新上传"))?;
    let (start, end) = reference
        .split_once(':')
        .ok_or_else(|| invalid("工作表中的合并单元格无法识别，请取消合并后重新上传"))?;
    let (start_column, start_row) = coordinate(start, limits)?;
    let (end_column, end_row) = coordinate(end, limits)?;
    if start_column > end_column || start_row > end_row {
        return Err(invalid("工作表中的合并单元格无法识别，请取消合并后重新上传"));
    }
    Ok(MergeRange { start_column, start_row, end_column, end_row })
}
