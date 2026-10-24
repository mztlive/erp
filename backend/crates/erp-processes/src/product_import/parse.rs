//! 解释产品报价模板和 DISPIMG；ZIP、XML 与单元格读取复用 XLSX 技术边界。

use std::collections::{BTreeMap, HashMap};

use erp_catalog::{
    MAX_PRODUCT_IMPORT_FILE_BYTES, PRODUCT_IMPORT_HEADERS, PRODUCT_IMPORT_SHEET_NAME,
    ensure_product_import_headers,
};

use crate::xlsx::{self, Archive, ArchiveLimits, Cell, MergeRange, SheetLimits};
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
/// * `bytes` - 不超过商品上传限制的 XLSX 原文。
/// # 返回
/// 「对内」工作表数据行与图片映射。
/// # 错误
/// ZIP/XML 损坏、解压超限、模板缺失或表头不匹配时返回校验错误。
pub fn parse_product_quote_xlsx(bytes: &[u8]) -> Result<ParsedProductSheet> {
    if bytes.is_empty()
        || u64::try_from(bytes.len()).map_or(true, |size| size > MAX_PRODUCT_IMPORT_FILE_BYTES)
    {
        return Err(invalid("请上传大小不超过 700 MiB 的产品报价表 .xlsx 文件"));
    }
    let mut archive = Archive::open(bytes, ArchiveLimits::PRODUCT)?;
    let shared = archive.shared_strings()?;
    let sheet_path = xlsx::sheet_paths(&mut archive)?
        .into_iter()
        .find(|(name, _)| name == PRODUCT_IMPORT_SHEET_NAME)
        .map(|(_, path)| path)
        .ok_or_else(|| invalid("未找到「对内」工作表，请使用原产品报价表模板"))?;
    let rows = parse_sheet_rows(&archive.xml(&sheet_path)?, &shared)?;
    let headers = rows.first().ok_or_else(|| invalid("模板没有可导入的数据行"))?;
    ensure_product_import_headers(&headers.cells).map_err(Error::from)?;
    let data_rows = rows
        .into_iter()
        .skip(1)
        .filter(|row| row.cells.iter().any(|cell| !cell.trim().is_empty()))
        .collect::<Vec<_>>();
    if data_rows.is_empty() {
        return Err(invalid("模板没有可导入的数据行"));
    }
    if data_rows.len() > 1000 {
        return Err(invalid("每次导入最多 1000 行"));
    }
    let image_targets = parse_image_targets(&mut archive)?;
    Ok(ParsedProductSheet {
        sheet_name: PRODUCT_IMPORT_SHEET_NAME.to_string(),
        rows: data_rows,
        image_targets,
    })
}

/// 模板后的附加列继续忽略；DISPIMG 保留公式，其余单元格使用缓存值。
fn parse_sheet_rows(xml: &str, shared: &[String]) -> Result<Vec<ParsedProductRow>> {
    let limits =
        SheetLimits { columns: 16_384, rows: 10_000, retained_columns: PRODUCT_IMPORT_HEADERS.len() };
    let sheet = xlsx::rows(xml, shared, limits)?;
    let mut rows = BTreeMap::new();
    for row in sheet.rows {
        let mut cells = row.cells.into_iter().map(product_cell).collect::<Vec<_>>();
        cells.resize(PRODUCT_IMPORT_HEADERS.len(), String::new());
        if cells.iter().any(|cell| !cell.is_empty()) {
            rows.insert(row.number, cells);
        }
    }
    fill_merged_cells(&mut rows, &sheet.merges);
    Ok(rows.into_iter().map(|(row_number, cells)| ParsedProductRow { row_number, cells }).collect())
}

fn product_cell(cell: Cell) -> String {
    match cell.formula_text {
        Some(formula) if formula.contains("DISPIMG") => {
            if formula.starts_with('=') {
                formula
            } else {
                format!("={formula}")
            }
        },
        _ => cell.value,
    }
}

fn fill_merged_cells(rows: &mut BTreeMap<u32, Vec<String>>, merges: &[MergeRange]) {
    let width = PRODUCT_IMPORT_HEADERS.len();
    for range in merges {
        let origin = rows.get(&range.start_row).and_then(|row| row.get(range.start_column)).cloned();
        let Some(value) = origin.filter(|value| !value.trim().is_empty()) else {
            continue;
        };
        for number in range.start_row..=range.end_row {
            let row = rows.entry(number).or_insert_with(|| vec![String::new(); width]);
            for cell in row.iter_mut().take((range.end_column + 1).min(width)).skip(range.start_column) {
                if cell.trim().is_empty() {
                    *cell = value.clone();
                }
            }
        }
    }
}

fn parse_image_targets(archive: &mut Archive<'_>) -> Result<HashMap<String, String>> {
    let Some(xml) = archive.optional_xml("xl/cellimages.xml")? else {
        return Ok(HashMap::new());
    };
    let Some(rels) = archive.optional_xml("xl/_rels/cellimages.xml.rels")? else {
        return Ok(HashMap::new());
    };
    let paths = xlsx::relationships(&rels, "xl/cellimages.xml")?;
    let doc = xlsx::document(&xml)?;
    let mut targets = HashMap::new();
    for picture in doc.descendants().filter(|node| node.is_element() && node.tag_name().name() == "pic") {
        let name = picture
            .descendants()
            .find(|node| node.is_element() && node.tag_name().name() == "cNvPr")
            .and_then(|node| node.attribute("name"));
        let rid = picture
            .descendants()
            .find(|node| node.is_element() && node.tag_name().name() == "blip")
            .and_then(|node| node.attributes().find(|attribute| attribute.name() == "embed"))
            .map(|attribute| attribute.value());
        if let (Some(name), Some(path)) = (name, rid.and_then(|rid| paths.get(rid))) {
            // 兼容旧 WPS 仅写文件名的目标；标准包内路径存在时优先按关系读取。
            let legacy = path
                .strip_prefix("xl/")
                .filter(|path| !path.contains('/'))
                .map(|file| format!("xl/media/{file}"));
            let path =
                if archive.contains(path) { path.clone() } else { legacy.unwrap_or_else(|| path.clone()) };
            targets.insert(name.to_string(), path);
        }
    }
    Ok(targets)
}

fn invalid(message: &str) -> Error {
    Error::ValidationError(message.into())
}

#[cfg(test)]
mod tests {
    use erp_catalog::PRODUCT_IMPORT_HEADERS;
    use erp_catalog::entity::catalog::product_import::dispimg_id;

    use super::{parse_sheet_rows as parse_rows, product_cell};
    use crate::product_import::identity::normalize_import_row;
    use crate::xlsx::{self, SheetLimits};

    fn parse_sheet_rows(xml: &str, shared: &[String]) -> crate::Result<Vec<super::ParsedProductRow>> {
        let xml = if xml.starts_with("<worksheet>") {
            xml.to_string()
        } else {
            format!("<worksheet>{xml}</worksheet>")
        };
        parse_rows(&xml, shared)
    }

    fn cell_text(attrs: &str, inner: &str, shared: &[String]) -> String {
        let xml = format!(r#"<worksheet><c r="A1" {attrs}>{inner}</c></worksheet>"#);
        let cell = xlsx::rows(&xml, shared, SheetLimits::CUSTOMER).unwrap().rows.remove(0).cells.remove(0);
        product_cell(cell)
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

    /// 旧版报价表第 25 列（含出厂价列）只被忽略，不影响原 24 列取价。
    #[test]
    fn columns_after_the_template_are_ignored() {
        for header in ["备注", "出厂价（含税）"] {
            let xml = format!(
                r#"<sheetData>
                <c r="A1" t="inlineStr"><is><t>产品编码</t></is></c>
                <c r="Y1" t="inlineStr"><is><t>{header}</t></is></c>
                <c r="G2" t="inlineStr"><is><t>米</t></is></c>
                <c r="I2" t="inlineStr"><is><t>东北大米</t></is></c>
                <c r="M2"><v>40.00</v></c>
                <c r="Y2"><v>32.00</v></c>
                </sheetData>"#
            );
            let rows = parse_sheet_rows(&xml, &[]).unwrap();
            assert!(rows.iter().all(|row| row.cells.len() == PRODUCT_IMPORT_HEADERS.len()));
            let row = normalize_import_row(2, &rows[1].cells).unwrap();
            assert_eq!(row.sales_price.unwrap().to_string(), "40.00");
        }
    }

    fn product_workbook(shared: Option<&[u8]>) -> Vec<u8> {
        use std::io::{Cursor, Write};

        use zip::ZipWriter;
        use zip::write::SimpleFileOptions;

        let headers = PRODUCT_IMPORT_HEADERS
            .iter()
            .enumerate()
            .map(|(index, header)| {
                let column = char::from(b'A' + u8::try_from(index).unwrap());
                format!(r#"<c t='inlineStr' r='{column}1'><is><t>{header}</t></is></c>"#)
            })
            .collect::<String>();
        let value = if shared.is_some() { "<v>0</v>" } else { "<is><t>inline&amp;lt;</t></is>" };
        let kind = if shared.is_some() { "s" } else { "inlineStr" };
        let sheet = format!(
            r#"<worksheet><sheetData>{headers}<c s='1' t='{kind}' r='I2'>{value}</c><c r='C2'><f>_xlfn.DISPIMG(&quot;ID_ABC&quot;,1)</f><v/></c></sheetData></worksheet>"#
        );
        let workbook = r#"<workbook xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships"><sheets><sheet r:id='r1' name='对内'/></sheets></workbook>"#;
        let rels = r#"<Relationships><Relationship Target='worksheets/sheet1.xml' Id='r1'/></Relationships>"#;
        let mut files = vec![
            ("xl/workbook.xml", workbook.as_bytes()),
            ("xl/_rels/workbook.xml.rels", rels.as_bytes()),
            ("xl/worksheets/sheet1.xml", sheet.as_bytes()),
        ];
        if let Some(shared) = shared {
            files.push(("xl/sharedStrings.xml", shared));
        }
        let mut writer = ZipWriter::new(Cursor::new(Vec::new()));
        for (name, bytes) in files {
            writer.start_file(name, SimpleFileOptions::default()).unwrap();
            writer.write_all(bytes).unwrap();
        }
        writer.finish().unwrap().into_inner()
    }

    #[test]
    fn workbook_bytes_accept_attribute_order_and_decode_xml_once() {
        let bytes = product_workbook(Some("<sst><si><t>A&amp;lt;&#x4e2d;</t></si></sst>".as_bytes()));
        let parsed = super::parse_product_quote_xlsx(&bytes).unwrap();
        assert_eq!(parsed.rows[0].row_number, 2);
        assert_eq!(parsed.rows[0].cells[8], "A&lt;中");
        assert_eq!(parsed.rows[0].cells[2], "=_xlfn.DISPIMG(\"ID_ABC\",1)");
        let parsed = super::parse_product_quote_xlsx(&product_workbook(None)).unwrap();
        assert_eq!(parsed.rows[0].cells[8], "inline&lt;");
    }

    #[test]
    fn workbook_bytes_reject_damaged_shared_strings() {
        let bytes = product_workbook(Some(b"<sst><si><t>broken</si></sst>"));
        assert!(
            matches!(super::parse_product_quote_xlsx(&bytes), Err(crate::Error::ValidationError(message)) if message.contains("XML"))
        );
    }

    #[test]
    fn workbook_images_accept_reordered_relationships_and_legacy_wps_targets() {
        use std::io::{Cursor, Read, Write};

        use zip::write::SimpleFileOptions;
        use zip::{ZipArchive, ZipWriter};

        for target in ["media/image.png", "/xl/media/image.png", "image.png"] {
            let bytes = product_workbook(None);
            let mut archive = ZipArchive::new(Cursor::new(bytes)).unwrap();
            let mut writer = ZipWriter::new(Cursor::new(Vec::new()));
            for index in 0..archive.len() {
                let mut entry = archive.by_index(index).unwrap();
                let mut bytes = Vec::new();
                entry.read_to_end(&mut bytes).unwrap();
                writer.start_file(entry.name(), SimpleFileOptions::default()).unwrap();
                writer.write_all(&bytes).unwrap();
            }
            let images = r#"<cellImages xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships"><pic><nvPicPr><cNvPr name="ID_ABC"/></nvPicPr><blipFill><blip r:embed="r2"/></blipFill></pic></cellImages>"#;
            let rels = format!(r#"<Relationships><Relationship Target="{target}" Id="r2"/></Relationships>"#);
            for (name, bytes) in [
                ("xl/cellimages.xml", images.as_bytes()),
                ("xl/_rels/cellimages.xml.rels", rels.as_bytes()),
                ("xl/media/image.png", b"image".as_slice()),
            ] {
                writer.start_file(name, SimpleFileOptions::default()).unwrap();
                writer.write_all(bytes).unwrap();
            }
            let parsed = super::parse_product_quote_xlsx(&writer.finish().unwrap().into_inner()).unwrap();
            assert_eq!(parsed.image_targets.get("ID_ABC").map(String::as_str), Some("xl/media/image.png"));
        }
    }
}
