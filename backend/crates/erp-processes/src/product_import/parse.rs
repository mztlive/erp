//! 解析产品报价表 xlsx：对内工作表单元格与 DISPIMG 图片映射。

use std::collections::HashMap;
use std::io::{Cursor, Read};

use erp_catalog::{PRODUCT_IMPORT_HEADERS, PRODUCT_IMPORT_SHEET_NAME, ensure_product_import_headers};
use regex::Regex;
use zip::ZipArchive;

use crate::{Error, Result};

/// 解析后的报价表。
#[derive(Debug, Clone)]
pub struct ParsedProductSheet {
    /// 工作表名。
    #[allow(dead_code)]
    pub sheet_name: String,
    /// 数据行（不含表头）。
    pub rows: Vec<ParsedProductRow>,
    /// DISPIMG ID → zip 内媒体路径。
    pub image_targets: HashMap<String, String>,
}

/// 解析后的一行。
#[derive(Debug, Clone)]
pub struct ParsedProductRow {
    /// Excel 行号（1 起，含表头）。
    pub row_number: u32,
    /// 按模板列序的单元格文本。
    pub cells: Vec<String>,
}

/// 解析产品报价表字节。
///
/// # 参数
/// * `bytes` - xlsx 文件内容
///
/// # 返回
/// 返回「对内」工作表数据行与图片映射。
///
/// # 错误
/// 不是 zip/xlsx、缺少对内表或表头不匹配时返回校验错误。
pub fn parse_product_quote_xlsx(bytes: &[u8]) -> Result<ParsedProductSheet> {
    let mut archive = ZipArchive::new(Cursor::new(bytes))
        .map_err(|_| Error::ValidationError("请上传原产品报价表 .xlsx 文件".into()))?;
    let shared = read_shared_strings(&mut archive)?;
    let sheet_path = locate_internal_sheet(&mut archive)?;
    let sheet_xml = read_zip_text(&mut archive, &sheet_path)?;
    let rows = parse_sheet_rows(&sheet_xml, &shared)?;
    if rows.is_empty() {
        return Err(Error::ValidationError("模板没有可导入的数据行".into()));
    }
    let headers = &rows[0].cells;
    ensure_product_import_headers(headers).map_err(Error::from)?;
    let data_rows = rows.into_iter().skip(1).filter(|row| row_has_content(&row.cells)).collect::<Vec<_>>();
    if data_rows.is_empty() {
        return Err(Error::ValidationError("模板没有可导入的数据行".into()));
    }
    if data_rows.len() > 1000 {
        return Err(Error::ValidationError("每次导入最多 1000 行".into()));
    }
    let image_targets = parse_image_targets(&mut archive)?;
    Ok(ParsedProductSheet {
        sheet_name: PRODUCT_IMPORT_SHEET_NAME.to_string(),
        rows: data_rows,
        image_targets,
    })
}

/// 从 xlsx 中读取指定媒体文件。
///
/// # 参数
/// * `bytes` - xlsx 文件内容
/// * `target` - zip 内相对路径，如 `xl/media/image1.jpeg`
///
/// # 返回
/// 返回文件字节。
///
/// # 错误
/// 压缩包损坏或条目不存在时返回错误。
pub fn read_xlsx_media(bytes: &[u8], target: &str) -> Result<Vec<u8>> {
    let mut archive = ZipArchive::new(Cursor::new(bytes))
        .map_err(|_| Error::ValidationError("源文件已损坏，无法读取图片".into()))?;
    read_zip_bytes(&mut archive, target)
}

fn row_has_content(cells: &[String]) -> bool {
    cells.iter().any(|cell| !cell.trim().is_empty())
}

fn locate_internal_sheet(archive: &mut ZipArchive<Cursor<&[u8]>>) -> Result<String> {
    let workbook = read_zip_text(archive, "xl/workbook.xml")?;
    let sheet_re = Regex::new(r"<sheet\b[^>]*>").expect("sheet regex");
    let name_re = Regex::new(r#"name="([^"]+)""#).expect("name regex");
    let rid_re = Regex::new(r#"r:id="([^"]+)""#).expect("rid regex");
    let mut sheets = Vec::new();
    for tag in sheet_re.find_iter(&workbook) {
        let Some(name) = name_re.captures(tag.as_str()) else {
            continue;
        };
        let Some(rid) = rid_re.captures(tag.as_str()) else {
            continue;
        };
        sheets.push((name[1].to_string(), rid[1].to_string()));
    }
    let rels = read_zip_text(archive, "xl/_rels/workbook.xml.rels")?;
    let rel_re = Regex::new(r#"Id="([^"]+)"[^>]*Target="([^"]+)""#).expect("rel regex");
    let mut rid_to_target = HashMap::new();
    for cap in rel_re.captures_iter(&rels) {
        rid_to_target.insert(cap[1].to_string(), cap[2].to_string());
    }
    let Some((_, rid)) = sheets.iter().find(|(name, _)| name == PRODUCT_IMPORT_SHEET_NAME) else {
        return Err(Error::ValidationError("未找到「对内」工作表，请使用原产品报价表模板".into()));
    };
    let target = rid_to_target.get(rid).ok_or_else(|| Error::ValidationError("对内工作表路径无效".into()))?;
    if target.starts_with("xl/") { Ok(target.clone()) } else { Ok(format!("xl/{target}")) }
}

fn read_shared_strings(archive: &mut ZipArchive<Cursor<&[u8]>>) -> Result<Vec<String>> {
    let Ok(xml) = read_zip_text(archive, "xl/sharedStrings.xml") else {
        return Ok(Vec::new());
    };
    let si_re = Regex::new(r"<si>([\s\S]*?)</si>").expect("si regex");
    let t_re = Regex::new(r"<t[^>]*>([\s\S]*?)</t>").expect("t regex");
    let mut shared = Vec::new();
    for cap in si_re.captures_iter(&xml) {
        let mut text = String::new();
        for t in t_re.captures_iter(&cap[1]) {
            text.push_str(&unescape_xml(&t[1]));
        }
        shared.push(text);
    }
    Ok(shared)
}

fn parse_sheet_rows(xml: &str, shared: &[String]) -> Result<Vec<ParsedProductRow>> {
    let cell_re = Regex::new(r#"<c r="([A-Z]+)(\d+)"([^>]*)>([\s\S]*?)</c>"#).expect("cell regex");
    let mut by_row: HashMap<u32, HashMap<u32, String>> = HashMap::new();
    for cap in cell_re.captures_iter(xml) {
        let col = column_index(&cap[1]);
        let row: u32 = cap[2].parse().map_err(|_| Error::ValidationError("工作表行号无效".into()))?;
        let attrs = &cap[3];
        let inner = &cap[4];
        let value = cell_text(attrs, inner, shared);
        if !value.is_empty() {
            by_row.entry(row).or_default().insert(col, value);
        }
    }
    fill_merged_cells(&mut by_row, &parse_merge_ranges(xml));
    let mut rows = by_row.into_iter().collect::<Vec<_>>();
    rows.sort_by_key(|(row, _)| *row);
    Ok(rows
        .into_iter()
        .map(|(row_number, cols)| {
            let width = PRODUCT_IMPORT_HEADERS.len() as u32;
            let mut cells = Vec::with_capacity(width as usize);
            for col in 1..=width {
                cells.push(cols.get(&col).cloned().unwrap_or_default());
            }
            ParsedProductRow { row_number, cells }
        })
        .collect())
}

fn cell_text(attrs: &str, inner: &str, shared: &[String]) -> String {
    let is_shared = attrs.contains(r#"t="s""#);
    if let Some(formula) = capture_tag(inner, "f") {
        let formula = unescape_xml(&formula);
        if formula.contains("DISPIMG") {
            return if formula.starts_with('=') { formula } else { format!("={formula}") };
        }
    }
    if let Some(value) = capture_tag(inner, "v") {
        let value = unescape_xml(&value);
        if is_shared {
            if let Ok(index) = value.parse::<usize>() {
                return shared.get(index).cloned().unwrap_or_default();
            }
        }
        return value;
    }
    if let Some(inline) = capture_tag(inner, "t") {
        return unescape_xml(&inline);
    }
    String::new()
}

fn capture_tag(xml: &str, tag: &str) -> Option<String> {
    let start = format!("<{tag}");
    let end = format!("</{tag}>");
    let begin = xml.find(&start)?;
    let after = xml.get(begin..)?;
    let inner_start = after.find('>')? + 1;
    let rest = after.get(inner_start..)?;
    let close = rest.find(&end)?;
    Some(rest[..close].to_string())
}

fn parse_image_targets(archive: &mut ZipArchive<Cursor<&[u8]>>) -> Result<HashMap<String, String>> {
    let Ok(xml) = read_zip_text(archive, "xl/cellimages.xml") else {
        return Ok(HashMap::new());
    };
    let Ok(rels) = read_zip_text(archive, "xl/_rels/cellimages.xml.rels") else {
        return Ok(HashMap::new());
    };
    let name_re = Regex::new(r#"name="(ID_[A-Fa-f0-9]+)""#).expect("name regex");
    let embed_re = Regex::new(r#"r:embed="(rId\d+)""#).expect("embed regex");
    let names = name_re.captures_iter(&xml).map(|cap| cap[1].to_string()).collect::<Vec<_>>();
    let embeds = embed_re.captures_iter(&xml).map(|cap| cap[1].to_string()).collect::<Vec<_>>();
    let rel_re = Regex::new(r#"Id="(rId\d+)"[^>]*Target="([^"]+)""#).expect("image rel regex");
    let mut rid_to_target = HashMap::new();
    for cap in rel_re.captures_iter(&rels) {
        let target = cap[2].to_string();
        let path = if target.starts_with("xl/") {
            target
        } else if target.starts_with("media/") {
            format!("xl/{target}")
        } else {
            format!("xl/media/{target}")
        };
        rid_to_target.insert(cap[1].to_string(), path);
    }
    let mut map = HashMap::new();
    for (name, rid) in names.into_iter().zip(embeds) {
        if let Some(path) = rid_to_target.get(&rid) {
            map.insert(name, path.clone());
        }
    }
    Ok(map)
}

fn read_zip_text(archive: &mut ZipArchive<Cursor<&[u8]>>, name: &str) -> Result<String> {
    let bytes = read_zip_bytes(archive, name)?;
    String::from_utf8(bytes).map_err(|_| Error::ValidationError("工作表编码无效".into()))
}

fn read_zip_bytes(archive: &mut ZipArchive<Cursor<&[u8]>>, name: &str) -> Result<Vec<u8>> {
    let mut file = archive.by_name(name).map_err(|_| Error::ValidationError(format!("缺少文件 {name}")))?;
    let mut buf = Vec::new();
    file.read_to_end(&mut buf).map_err(|_| Error::ValidationError("读取压缩包失败".into()))?;
    Ok(buf)
}

/// 读取合并单元格范围 `(起始列, 起始行, 结束列, 结束行)`。
///
/// # 参数
/// * `xml` - 工作表 XML
///
/// # 返回
/// 返回合并区列表；没有合并区时为空。
///
/// # 错误
/// 无。
fn parse_merge_ranges(xml: &str) -> Vec<(u32, u32, u32, u32)> {
    let re = Regex::new(r#"<mergeCell[^>]*\bref="([A-Z]+)(\d+):([A-Z]+)(\d+)""#).expect("merge regex");
    re.captures_iter(xml)
        .filter_map(|cap| {
            Some((column_index(&cap[1]), cap[2].parse().ok()?, column_index(&cap[3]), cap[4].parse().ok()?))
        })
        .collect()
}

/// 把合并区左上角的值填到空白单元格，便于同一产品编码覆盖多行 SKU。
///
/// # 参数
/// * `by_row` - 行号到列值的表
/// * `merges` - 合并区
///
/// # 返回
/// 无。
///
/// # 错误
/// 无。
fn fill_merged_cells(by_row: &mut HashMap<u32, HashMap<u32, String>>, merges: &[(u32, u32, u32, u32)]) {
    for &(start_col, start_row, end_col, end_row) in merges {
        let origin = by_row.get(&start_row).and_then(|cols| cols.get(&start_col)).cloned();
        let Some(value) = origin else {
            continue;
        };
        if value.trim().is_empty() {
            continue;
        }
        for row in start_row..=end_row {
            for col in start_col..=end_col {
                let cell = by_row.entry(row).or_default().entry(col).or_default();
                if cell.trim().is_empty() {
                    *cell = value.clone();
                }
            }
        }
    }
}

fn column_index(col: &str) -> u32 {
    col.chars().fold(0, |acc, ch| acc * 26 + u32::from(ch as u8 - b'A' + 1))
}

fn unescape_xml(value: &str) -> String {
    value
        .replace("&amp;", "&")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&apos;", "'")
}

#[cfg(test)]
mod tests {
    use erp_catalog::entity::catalog::product_import::dispimg_id;

    use super::{cell_text, column_index, parse_sheet_rows};

    #[test]
    fn column_index_matches_excel() {
        assert_eq!(column_index("A"), 1);
        assert_eq!(column_index("X"), 24);
    }

    #[test]
    fn shared_string_and_formula_cells_parse() {
        let shared = vec!["产品编码".into(), "法丽兹".into()];
        let xml = r#"<sheetData>
            <c r="A1" t="s"><v>0</v></c>
            <c r="F2" t="s"><v>1</v></c>
            <c r="C2"><f>DISPIMG("ID_ABC",1)</f><v></v></c>
        </sheetData>"#;
        let rows = parse_sheet_rows(xml, &shared).unwrap();
        assert_eq!(rows[0].cells[0], "产品编码");
        assert_eq!(rows[1].cells[5], "法丽兹");
        assert!(rows[1].cells[2].contains("ID_ABC"));
        assert_eq!(cell_text(r#" t="s""#, "<v>1</v>", &shared), "法丽兹");
    }

    #[test]
    fn xlfn_dispimg_formula_keeps_image_id() {
        let shared = Vec::<String>::new();
        let inner = r#"<f>_xlfn.DISPIMG(&quot;ID_DC3C0313483242B2B7BF94A18EC5BDA4&quot;,1)</f><v>=DISPIMG(&quot;ID_DC3C0313483242B2B7BF94A18EC5BDA4&quot;,1)</v>"#;
        let text = cell_text(r#" t="str""#, inner, &shared);
        assert!(text.contains("ID_DC3C0313483242B2B7BF94A18EC5BDA4"));
        assert_eq!(dispimg_id(&text), Some("ID_DC3C0313483242B2B7BF94A18EC5BDA4"));
    }

    #[test]
    fn merged_product_code_fills_down() {
        let shared = vec!["FSY-1".into(), "名称A".into(), "名称B".into()];
        let xml = r#"<sheetData>
            <c r="A1" t="s"><v>0</v></c>
            <c r="A2" t="s"><v>0</v></c>
            <c r="I2" t="s"><v>1</v></c>
            <c r="I3" t="s"><v>2</v></c>
        </sheetData>
        <mergeCells><mergeCell ref="A2:A3"/></mergeCells>"#;
        let rows = parse_sheet_rows(xml, &shared).unwrap();
        let row2 = rows.iter().find(|row| row.row_number == 2).unwrap();
        let row3 = rows.iter().find(|row| row.row_number == 3).unwrap();
        assert_eq!(row2.cells[0], "FSY-1");
        assert_eq!(row3.cells[0], "FSY-1");
        assert_eq!(row3.cells[8], "名称B");
    }

    #[test]
    fn non_merge_refs_do_not_fill_empty_cells() {
        let shared = vec!["产品编码".into(), "名称A".into(), "名称B".into()];
        let xml = r#"<worksheet><dimension ref="A1:X3"/>
        <sheetData>
            <c r="A1" t="s"><v>0</v></c>
            <c r="I2" t="s"><v>1</v></c>
            <c r="I3" t="s"><v>2</v></c>
            <c r="M1"><f t="shared" ref="M1:M2">ROUNDUP(K1/0.97,0)</f><v>68</v></c>
        </sheetData>
        <autoFilter ref="A1:X2"/>
        <conditionalFormatting sqref="I1:J1"></conditionalFormatting></worksheet>"#;
        let rows = parse_sheet_rows(xml, &shared).unwrap();
        let row3 = rows.iter().find(|row| row.row_number == 3).unwrap();
        assert_eq!(row3.cells[0], "");
        assert_eq!(row3.cells[8], "名称B");
    }
}
