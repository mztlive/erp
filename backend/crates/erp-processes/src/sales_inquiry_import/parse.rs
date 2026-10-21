//! 有界读取客户工作簿；只接受版本化的三个白名单工作表。

use std::collections::{BTreeMap, HashMap};
use std::io::{Cursor, Read};
use std::sync::LazyLock;

use erp_sales::entity::sales_inquiry_import::{
    INQUIRY_IMPORT_MAX_BYTES, INQUIRY_IMPORT_MAX_ROWS, INQUIRY_TEMPLATE_VERSION, InquiryImportIssue,
};
use regex::Regex;
use zip::ZipArchive;

use crate::{Error, Result};

pub(super) const PRODUCT_SHEET: &str = "商品清单";
pub(super) const DELIVERY_SHEET: &str = "配送需求";
const INSTRUCTIONS: &str = "填写说明";
const MAX_EXPANDED_BYTES: u64 = 32 * 1024 * 1024;
const MAX_ENTRY_BYTES: u64 = 16 * 1024 * 1024;
static CELL_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?s)<c\b([^>]*?)(?:/\s*>|>(.*?)</c>)").expect("static cell regex"));
static ATTRIBUTE_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r#"(?:^|\s)([\w:]+)\s*=\s*["']([^"']*)["']"#).expect("static attribute regex")
});
static FORMULA_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"<(?:[A-Za-z_][\w.-]*:)?f(?:\s|/|>)").expect("static formula regex"));
static TEXT_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?s)<t\b[^>]*>(.*?)</t>").expect("static text regex"));
pub(super) const PRODUCT_HEADERS: [&str; 7] =
    ["SKU编码", "商品名称", "规格", "单位", "需求数量", "销售参考价", "需求备注"];
pub(super) const DELIVERY_HEADERS: [&str; 10] =
    ["SKU编码", "收件人", "联系电话", "省", "市", "区县", "详细地址", "配送数量", "要求交付日期", "交付条件"];

#[derive(Clone, Default)]
pub(super) struct Cell {
    pub value: String,
    pub text: bool,
    pub formula: bool,
}

#[derive(Clone)]
pub(super) struct Row {
    pub number: u32,
    pub cells: Vec<Cell>,
}

pub(super) struct Workbook {
    pub products: Vec<Row>,
    pub deliveries: Vec<Row>,
    pub errors: Vec<InquiryImportIssue>,
}

pub(super) fn parse(bytes: &[u8]) -> Result<Workbook> {
    if bytes.is_empty() || bytes.len() > INQUIRY_IMPORT_MAX_BYTES {
        return Err(invalid("工作簿不能为空且不得超过 5 MiB"));
    }
    let mut archive = ZipArchive::new(Cursor::new(bytes)).map_err(|_| invalid("请上传有效 .xlsx 工作簿"))?;
    check_archive(&mut archive)?;
    let shared = shared_strings(&mut archive)?;
    let paths = sheet_paths(&mut archive)?;
    let mut sheets = HashMap::new();
    for (name, path) in paths {
        sheets.insert(name, rows(&read_text(&mut archive, &path)?, &shared)?);
    }
    check_version(&sheets)?;
    let mut errors = Vec::new();
    let products = data_rows(
        sheets.remove(PRODUCT_SHEET).unwrap_or_default(),
        PRODUCT_SHEET,
        &PRODUCT_HEADERS,
        &mut errors,
    );
    let deliveries = data_rows(
        sheets.remove(DELIVERY_SHEET).unwrap_or_default(),
        DELIVERY_SHEET,
        &DELIVERY_HEADERS,
        &mut errors,
    );
    if products.len() > INQUIRY_IMPORT_MAX_ROWS || deliveries.len() > INQUIRY_IMPORT_MAX_ROWS {
        return Err(invalid("商品行及配送行每次均不得超过 2000 行"));
    }
    Ok(Workbook { products, deliveries, errors })
}

fn check_archive(archive: &mut ZipArchive<Cursor<&[u8]>>) -> Result<()> {
    if archive.len() > 128 {
        return Err(invalid("工作簿压缩条目过多"));
    }
    let mut expanded = 0_u64;
    let mut names = std::collections::HashSet::new();
    for index in 0..archive.len() {
        let file = archive.by_index(index).map_err(|_| invalid("压缩条目无效"))?;
        expanded = expanded.checked_add(file.size()).ok_or_else(|| invalid("解压大小越限"))?;
        if file.size() > MAX_ENTRY_BYTES
            || expanded > MAX_EXPANDED_BYTES
            || file.enclosed_name().is_none()
            || !names.insert(file.name().to_string())
        {
            return Err(invalid("工作簿路径重复、路径不安全或解压大小超过 32 MiB"));
        }
        if file.name().contains("vbaProject") || file.name().starts_with("xl/externalLinks/") {
            return Err(invalid("客户工作簿不得包含宏或外部链接"));
        }
    }
    Ok(())
}

fn sheet_paths(archive: &mut ZipArchive<Cursor<&[u8]>>) -> Result<Vec<(String, String)>> {
    let workbook = read_text(archive, "xl/workbook.xml")?;
    let relationships = read_text(archive, "xl/_rels/workbook.xml.rels")?;
    let mut paths = HashMap::new();
    for tag in tags(&relationships, "Relationship") {
        if attribute(tag, "TargetMode").is_some_and(|mode| mode == "External") {
            return Err(invalid("工作簿不得使用外部工作表"));
        }
        if let (Some(id), Some(target)) = (attribute(tag, "Id"), attribute(tag, "Target")) {
            let path = if let Some(path) = target.strip_prefix('/') {
                path.to_string()
            } else {
                format!("xl/{target}")
            };
            if path.split('/').any(|part| part == "..") {
                return Err(invalid("工作表路径不安全"));
            }
            paths.insert(id, path);
        }
    }
    let mut result = Vec::new();
    for tag in tags(&workbook, "sheet") {
        let name = attribute(tag, "name").ok_or_else(|| invalid("工作表名称无效"))?;
        let id = attribute(tag, "r:id").ok_or_else(|| invalid("工作表引用无效"))?;
        if ![PRODUCT_SHEET, DELIVERY_SHEET, INSTRUCTIONS].contains(&name.as_str())
            || result.iter().any(|(existing, _)| existing == &name)
        {
            return Err(invalid("只接受商品清单、配送需求、填写说明三个工作表"));
        }
        result.push((name, paths.get(&id).cloned().ok_or_else(|| invalid("工作表路径缺失"))?));
    }
    if result.len() != 3 {
        return Err(invalid("须包含商品清单、配送需求及填写说明三个工作表"));
    }
    Ok(result)
}

fn check_version(sheets: &HashMap<String, Vec<Row>>) -> Result<()> {
    let row = sheets.get(INSTRUCTIONS).and_then(|rows| rows.iter().find(|row| row.number == 1));
    let valid = row.is_some_and(|row| {
        row.cells.len() >= 2
            && row.cells[0].value == "模板版本"
            && row.cells[1].value == INQUIRY_TEMPLATE_VERSION
            && !row.cells.iter().any(|cell| cell.formula)
    });
    if !valid {
        return Err(invalid("填写说明第 1 行须标注模板版本 sales-inquiry-v1"));
    }
    if sheets.values().flatten().any(|row| row.cells.iter().any(|cell| cell.formula)) {
        // 明细公式在 data_rows 中定位；说明页公式不参与业务，但仍须先转值。
        if sheets
            .get(INSTRUCTIONS)
            .is_some_and(|rows| rows.iter().any(|row| row.cells.iter().any(|cell| cell.formula)))
        {
            return Err(invalid("填写说明含公式，请先转换为值"));
        }
    }
    Ok(())
}

fn data_rows(
    mut rows: Vec<Row>,
    sheet: &str,
    headers: &[&str],
    errors: &mut Vec<InquiryImportIssue>,
) -> Vec<Row> {
    let valid = rows.first().is_some_and(|row| {
        row.number == 1
            && row.cells.len() <= headers.len()
            && headers.iter().enumerate().all(|(index, header)| {
                row.cells.get(index).is_some_and(|cell| cell.value == *header && !cell.formula)
            })
    });
    if !valid {
        issue(errors, sheet, 1, "表头", "表头必须与当前客户模板完全一致，不接受供应商或内部字段");
    }
    rows.retain(|row| row.number > 1 && row.cells.iter().any(|cell| !cell.value.is_empty() || cell.formula));
    for row in &mut rows {
        if row.cells.len() > headers.len() {
            issue(errors, sheet, row.number, "附加列", "客户模板不得添加字段");
        }
        row.cells.resize_with(headers.len(), Cell::default);
        for (index, cell) in row.cells.iter().enumerate() {
            if cell.formula {
                issue(errors, sheet, row.number, headers[index], "公式单元格须先转换为值");
            }
            if cell.value.chars().count() > 2000 {
                issue(errors, sheet, row.number, headers[index], "单元格内容过长");
            }
        }
    }
    rows
}

fn shared_strings(archive: &mut ZipArchive<Cursor<&[u8]>>) -> Result<Vec<String>> {
    if archive.index_for_name("xl/sharedStrings.xml").is_none() {
        return Ok(Vec::new());
    }
    let xml = read_text(archive, "xl/sharedStrings.xml")?;
    let re = Regex::new(r"(?s)<si\b[^>]*>(.*?)</si>").expect("static shared-string regex");
    re.captures_iter(&xml).map(|capture| text_parts(&capture[1])).collect()
}

fn rows(xml: &str, shared: &[String]) -> Result<Vec<Row>> {
    let mut rows: BTreeMap<u32, BTreeMap<usize, Cell>> = BTreeMap::new();
    for captures in CELL_RE.captures_iter(xml) {
        let address = attribute(&captures[1], "r").ok_or_else(|| invalid("单元格缺少坐标"))?;
        let (column, row) = coordinate(&address)?;
        if column >= 64 || row > 10_000 {
            return Err(invalid("工作表行列范围超过模板限制"));
        }
        let inner = captures.get(2).map_or("", |value| value.as_str());
        let cell = read_cell(&captures[1], inner, shared)?;
        if rows.entry(row).or_default().insert(column, cell).is_some() {
            return Err(invalid("工作表存在重复单元格坐标"));
        }
    }
    Ok(rows
        .into_iter()
        .map(|(number, cells)| {
            let width = cells.keys().next_back().map_or(0, |index| index + 1);
            let cells = (0..width).map(|index| cells.get(&index).cloned().unwrap_or_default()).collect();
            Row { number, cells }
        })
        .collect())
}

fn read_cell(attrs: &str, inner: &str, shared: &[String]) -> Result<Cell> {
    let kind = attribute(attrs, "t");
    let text = matches!(kind.as_deref(), Some("s" | "inlineStr" | "str"));
    let formula = FORMULA_RE.is_match(inner);
    let value = if kind.as_deref() == Some("inlineStr") {
        text_parts(inner)?
    } else {
        let raw = element_text(inner, "v")?.unwrap_or_default();
        if kind.as_deref() == Some("s") {
            let index: usize = raw.parse().map_err(|_| invalid("共享文本索引无效"))?;
            shared.get(index).cloned().ok_or_else(|| invalid("共享文本索引越界"))?
        } else {
            raw
        }
    };
    Ok(Cell { value: value.trim().to_string(), text, formula })
}

fn coordinate(address: &str) -> Result<(usize, u32)> {
    let split = address.find(|ch: char| ch.is_ascii_digit()).ok_or_else(|| invalid("单元格坐标无效"))?;
    let column = address[..split]
        .bytes()
        .try_fold(0_usize, |value, byte| {
            if !byte.is_ascii_uppercase() {
                return None;
            }
            value.checked_mul(26)?.checked_add(usize::from(byte - b'A' + 1))
        })
        .and_then(|column| column.checked_sub(1))
        .ok_or_else(|| invalid("单元格列号无效"))?;
    let row = address[split..].parse::<u32>().map_err(|_| invalid("单元格行号无效"))?;
    if row == 0 {
        return Err(invalid("单元格行号无效"));
    }
    Ok((column, row))
}

fn read_text(archive: &mut ZipArchive<Cursor<&[u8]>>, name: &str) -> Result<String> {
    let file = archive.by_name(name).map_err(|_| invalid("工作簿缺少必要 XML 内容"))?;
    let mut bytes = Vec::new();
    file.take(MAX_ENTRY_BYTES + 1).read_to_end(&mut bytes).map_err(|_| invalid("工作簿解压失败"))?;
    if u64::try_from(bytes.len()).map_err(|_| invalid("工作簿过大"))? > MAX_ENTRY_BYTES {
        return Err(invalid("XML 解压大小越限"));
    }
    let xml = String::from_utf8(bytes).map_err(|_| invalid("工作簿编码无效"))?;
    if xml.contains("<!DOCTYPE") || xml.contains("<!ENTITY") {
        return Err(invalid("工作簿不得声明外部实体"));
    }
    Ok(xml)
}

fn tags<'a>(xml: &'a str, name: &str) -> Vec<&'a str> {
    let regex = Regex::new(&format!(r"<{name}\b[^>]*>")).expect("static tag-name regex");
    regex.find_iter(xml).map(|value| value.as_str()).collect()
}

fn attribute(tag: &str, name: &str) -> Option<String> {
    ATTRIBUTE_RE.captures_iter(tag).find(|value| &value[1] == name).and_then(|value| decode(&value[2]).ok())
}

fn text_parts(xml: &str) -> Result<String> {
    TEXT_RE
        .captures_iter(xml)
        .map(|value| decode(&value[1]))
        .collect::<Result<Vec<_>>>()
        .map(|parts| parts.concat())
}

fn element_text(xml: &str, name: &str) -> Result<Option<String>> {
    let regex = Regex::new(&format!(r"(?s)<{name}\b[^>]*>(.*?)</{name}>")).expect("static element regex");
    regex.captures(xml).map(|value| decode(&value[1])).transpose()
}

fn decode(value: &str) -> Result<String> {
    let mut output = String::new();
    let mut remaining = value;
    while let Some(index) = remaining.find('&') {
        output.push_str(&remaining[..index]);
        let after = &remaining[index + 1..];
        let end = after.find(';').ok_or_else(|| invalid("XML 转义无效"))?;
        let entity = &after[..end];
        match entity {
            "amp" => output.push('&'),
            "lt" => output.push('<'),
            "gt" => output.push('>'),
            "quot" => output.push('"'),
            "apos" => output.push('\''),
            _ => {
                let code = if let Some(hex) = entity.strip_prefix("#x") {
                    u32::from_str_radix(hex, 16).ok()
                } else {
                    entity.strip_prefix('#').and_then(|decimal| decimal.parse().ok())
                };
                output.push(code.and_then(char::from_u32).ok_or_else(|| invalid("XML 字符实体无效"))?);
            },
        }
        remaining = &after[end + 1..];
    }
    output.push_str(remaining);
    Ok(output)
}

pub(super) fn issue(
    errors: &mut Vec<InquiryImportIssue>,
    sheet: &str,
    row_number: u32,
    column: &str,
    message: &str,
) {
    errors.push(InquiryImportIssue {
        sheet: sheet.into(),
        row_number,
        column: column.into(),
        message: message.into(),
    });
}

fn invalid(message: &str) -> Error {
    Error::ValidationError(message.into())
}

#[cfg(test)]
mod tests {
    use std::io::Write;

    use zip::ZipWriter;
    use zip::write::SimpleFileOptions;

    use super::*;

    fn sheet(headers: &[&str], data: &str) -> String {
        let header = headers
            .iter()
            .enumerate()
            .map(|(index, value)| {
                let column = char::from(b'A' + u8::try_from(index).unwrap());
                format!(r#"<c r="{column}1" t="inlineStr"><is><t>{value}</t></is></c>"#)
            })
            .collect::<String>();
        format!("<worksheet><sheetData>{header}{data}</sheetData></worksheet>")
    }

    fn package(version: &str, product_data: &str, delivery_data: &str) -> Vec<u8> {
        let workbook = r#"<workbook><sheets><sheet name="商品清单" r:id="r1"/><sheet name="配送需求" r:id="r2"/><sheet name="填写说明" r:id="r3"/></sheets></workbook>"#;
        let rels = r#"<Relationships><Relationship Target="worksheets/sheet1.xml" Id="r1"/><Relationship Id="r2" Target="worksheets/sheet2.xml"/><Relationship Id="r3" Target="worksheets/sheet3.xml"/></Relationships>"#;
        let instructions = format!(
            r#"<worksheet><sheetData><c r="A1" t="inlineStr"><is><t>模板版本</t></is></c><c r="B1" t="inlineStr"><is><t>{version}</t></is></c></sheetData></worksheet>"#
        );
        let files = [
            ("xl/workbook.xml", workbook.to_string()),
            ("xl/_rels/workbook.xml.rels", rels.to_string()),
            ("xl/worksheets/sheet1.xml", sheet(&PRODUCT_HEADERS, product_data)),
            ("xl/worksheets/sheet2.xml", sheet(&DELIVERY_HEADERS, delivery_data)),
            ("xl/worksheets/sheet3.xml", instructions),
        ];
        let mut writer = ZipWriter::new(Cursor::new(Vec::new()));
        for (path, xml) in files {
            writer.start_file(path, SimpleFileOptions::default()).unwrap();
            writer.write_all(xml.as_bytes()).unwrap();
        }
        writer.finish().unwrap().into_inner()
    }

    #[test]
    fn parses_versioned_three_sheet_package_and_locates_formula_rows() {
        let bytes = package(
            INQUIRY_TEMPLATE_VERSION,
            r#"<c r="A2" t="inlineStr"><is><t>001</t></is></c>"#,
            r#"<c r="A9" t="inlineStr"><is><t>001</t></is></c><c r="H9"><f>1+1</f><v>2</v></c>"#,
        );
        let parsed = parse(&bytes).unwrap();
        assert_eq!(parsed.products[0].cells[0].value, "001");
        assert_eq!(parsed.deliveries[0].number, 9);
        assert!(parsed.errors.iter().any(|error| error.sheet == DELIVERY_SHEET
            && error.row_number == 9
            && error.column == "配送数量"));
        assert!(parse(&package("future-v2", "", "")).is_err());
    }

    #[test]
    fn refuses_archives_with_excessive_expanded_entry_size() {
        let mut writer = ZipWriter::new(Cursor::new(Vec::new()));
        writer
            .start_file(
                "large.xml",
                SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated),
            )
            .unwrap();
        writer.write_all(&vec![b'a'; usize::try_from(MAX_ENTRY_BYTES).unwrap() + 1]).unwrap();
        let bytes = writer.finish().unwrap().into_inner();
        assert!(bytes.len() < INQUIRY_IMPORT_MAX_BYTES);
        assert!(parse(&bytes).is_err());
    }

    #[test]
    fn preserves_text_codes_and_rejects_formula_even_with_cached_value() {
        let rows = rows(r#"<sheetData><c r="A2" t="inlineStr"><is><t>00123</t></is></c><c r="C2"><f t="shared"/><v>123</v></c></sheetData>"#, &[]).unwrap();
        assert_eq!(rows[0].cells[0].value, "00123");
        assert!(rows[0].cells[0].text);
        assert!(rows[0].cells[2].formula);
        assert!(!rows[0].cells[2].text);
        assert!(read_cell("", "<x:f>1+1</x:f><v>2</v>", &[]).unwrap().formula);
    }

    #[test]
    fn rejects_duplicate_coordinates_and_out_of_bounds() {
        assert!(rows(r#"<c r="A2"><v>1</v></c><c r="A2"><v>2</v></c>"#, &[]).is_err());
        assert!(rows(r#"<c r="A10001"><v>1</v></c>"#, &[]).is_err());
        assert!(parse(&[]).is_err());
        assert!(parse(&vec![0; INQUIRY_IMPORT_MAX_BYTES + 1]).is_err());
    }

    #[test]
    fn decodes_named_and_numeric_entities_without_double_decoding() {
        assert_eq!(decode("A&amp;B&#x4e2d;&#25991;&amp;lt;").unwrap(), "A&B中文&lt;");
        assert!(decode("&bad;").is_err());
    }
}
