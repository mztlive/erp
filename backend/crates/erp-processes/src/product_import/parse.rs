//! 解释产品报价模板和 DISPIMG；ZIP、XML 与单元格读取复用 XLSX 技术边界。

use std::collections::{BTreeMap, HashMap};

use erp_catalog::{
    MAX_PRODUCT_IMPORT_FILE_BYTES, PRODUCT_IMPORT_HEADERS, PRODUCT_IMPORT_SHEET_NAME,
    ensure_product_import_headers,
};

use super::legacy;
use crate::xlsx::{self, Archive, ArchiveLimits, Cell, MergeRange, SheetLimits};
use crate::{Error, Result};

/// 解析后的报价表。
#[derive(Debug, Clone)]
pub struct ParsedProductSheet {
    /// 数据行（不含表头）。
    pub rows: Vec<ParsedProductRow>,
    /// DISPIMG ID → zip 内媒体路径。
    pub image_targets: HashMap<String, String>,
}

impl ParsedProductSheet {
    /// 新提交必须携带供给列；历史任务仍由解析器按旧布局恢复。
    pub(super) fn ensure_supply_template(&self) -> Result<()> {
        if self.rows.iter().any(|row| row.cells.len() != PRODUCT_IMPORT_HEADERS.len()) {
            return Err(invalid("请使用包含供应商编号、供应商订货编码和进项税率的 16 列商品与供给导入模板"));
        }
        Ok(())
    }
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
    let width = template_width(&headers.cells)?;
    let legacy_layout = width == legacy::HEADERS.len();
    let data_rows = rows
        .into_iter()
        .skip(1)
        .filter(|row| row.cells.iter().take(width).any(|cell| !cell.trim().is_empty()))
        .map(|mut row| {
            if legacy_layout {
                row.cells = legacy::compact_cells(&row.cells);
            } else {
                row.cells.truncate(width);
            }
            row
        })
        .collect::<Vec<_>>();
    if data_rows.is_empty() {
        return Err(invalid("模板没有可导入的数据行"));
    }
    if data_rows.len() > 1000 {
        return Err(invalid("每次导入最多 1000 行"));
    }
    let image_targets = parse_image_targets(&mut archive)?;
    Ok(ParsedProductSheet { rows: data_rows, image_targets })
}

/// 先匹配完整旧表头，再严格校验新模板，避免将历史售价解释为供给成本。
fn template_width(headers: &[String]) -> Result<usize> {
    for layout in [legacy::HEADERS, legacy::COMPACT_HEADERS] {
        if headers.iter().take(layout.len()).map(|cell| cell.trim()).eq(layout.iter().copied()) {
            return Ok(layout.len());
        }
    }
    ensure_product_import_headers(headers).map_err(Error::from)?;
    Ok(PRODUCT_IMPORT_HEADERS.len())
}

/// 模板后的附加列继续忽略；DISPIMG 保留公式，其余单元格使用缓存值。
fn parse_sheet_rows(xml: &str, shared: &[String]) -> Result<Vec<ParsedProductRow>> {
    let limits = SheetLimits { columns: 16_384, rows: 10_000, retained_columns: legacy::HEADERS.len() };
    let sheet = xlsx::rows(xml, shared, limits)?;
    let mut rows = BTreeMap::new();
    for row in sheet.rows {
        let mut cells = row.cells.into_iter().map(product_cell).collect::<Vec<_>>();
        cells.resize(legacy::HEADERS.len(), String::new());
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
    let width = legacy::HEADERS.len();
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
            assert!(rows.iter().all(|row| row.cells.len() == super::legacy::HEADERS.len()));
            let row = normalize_import_row(2, &super::legacy::compact_cells(&rows[1].cells)).unwrap();
            assert_eq!(row.sales_price.unwrap().to_string(), "40.00");
        }
    }

    fn product_workbook(shared: Option<&[u8]>) -> Vec<u8> {
        let value = if shared.is_some() { "<v>0</v>" } else { "<is><t>inline&amp;lt;</t></is>" };
        let kind = if shared.is_some() { "s" } else { "inlineStr" };
        let cells = format!(
            r#"<c s='1' t='{kind}' r='H2'>{value}</c><c r='C2'><f>_xlfn.DISPIMG(&quot;ID_ABC&quot;,1)</f><v/></c>"#
        );
        workbook_with_cells(PRODUCT_IMPORT_HEADERS, &cells, shared)
    }

    /// 生成真实 ZIP 工作簿，覆盖两种表头布局的完整解析入口。
    fn workbook_with_cells(headers: &[&str], cells: &str, shared: Option<&[u8]>) -> Vec<u8> {
        use std::io::{Cursor, Write};

        use zip::ZipWriter;
        use zip::write::SimpleFileOptions;

        let headers = headers
            .iter()
            .enumerate()
            .map(|(index, header)| {
                let column = char::from(b'A' + u8::try_from(index).unwrap());
                format!(r#"<c t='inlineStr' r='{column}1'><is><t>{header}</t></is></c>"#)
            })
            .collect::<String>();
        let sheet = format!("<worksheet><sheetData>{headers}{cells}</sheetData></worksheet>");
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
        assert_eq!(parsed.rows[0].cells[7], "A&lt;中");
        assert_eq!(parsed.rows[0].cells[2], "=_xlfn.DISPIMG(\"ID_ABC\",1)");
        let parsed = super::parse_product_quote_xlsx(&product_workbook(None)).unwrap();
        assert_eq!(parsed.rows[0].cells[7], "inline&lt;");
    }

    /// 新旧模板都映射到同一建品输入；旧成本价与模板外附加内容不得串入参考售价。
    #[test]
    fn compact_and_legacy_workbooks_preserve_product_fields() {
        for (headers, positions) in [
            (super::legacy::COMPACT_HEADERS, ['H', 'I', 'J', 'K', 'L', 'M']),
            (super::legacy::HEADERS, ['I', 'J', 'M', 'N', 'O', 'Q']),
        ] {
            let mut cells = String::from(
                r#"<c r="A2" t="inlineStr"><is><t>00123</t></is></c>
                <c r="B2" t="inlineStr"><is><t>0691234567890</t></is></c>
                <c r="G2" t="inlineStr"><is><t>米</t></is></c>"#,
            );
            for (column, value) in
                positions.into_iter().zip(["东北大米", "5kg", "40.00", "38.00", "12.5", "50.00"])
            {
                cells.push_str(&format!(r#"<c r="{column}2" t="inlineStr"><is><t>{value}</t></is></c>"#));
            }
            if headers.len() == 24 {
                cells.push_str(r#"<c r="K2"><v>31.00</v></c><c r="L2"><v>29.00</v></c>"#);
            }
            let extra = char::from(b'A' + u8::try_from(headers.len()).unwrap());
            cells.push_str(&format!(r#"<c r="{extra}2"><v>99.00</v></c><c r="{extra}3"><v>88.00</v></c>"#));
            let bytes = workbook_with_cells(headers, &cells, None);
            let parsed = super::parse_product_quote_xlsx(&bytes).unwrap();
            assert_eq!(parsed.rows.len(), 1);
            assert_eq!(parsed.rows[0].cells.len(), 13);
            let row = normalize_import_row(2, &parsed.rows[0].cells).unwrap();
            assert_eq!(row.product_no, "00123");
            assert_eq!(row.barcode.as_deref(), Some("0691234567890"));
            assert_eq!(row.name, "东北大米");
            assert_eq!(row.specification.as_deref(), Some("5kg"));
            assert_eq!(row.sales_price.unwrap().to_string(), "40.00");
            assert_eq!(row.bulk_price.unwrap().to_string(), "38.00");
            assert_eq!(row.bulk_min_quantity.unwrap().to_string(), "12.5");
            assert_eq!(row.market_price.unwrap().to_string(), "50.00");
        }
    }

    /// 表头调换必须明确拒绝，不能按近似布局读入错误业务字段。
    #[test]
    fn reordered_headers_are_rejected() {
        for original in [PRODUCT_IMPORT_HEADERS, super::legacy::HEADERS] {
            let mut headers = original.to_vec();
            headers.swap(7, 8);
            let bytes = workbook_with_cells(&headers, "", None);
            assert!(matches!(super::parse_product_quote_xlsx(&bytes), Err(crate::Error::ValidationError(_))));
        }
    }

    /// 新列全部保留并进入供给 DTO；成本与起订量不得写入公司参考售价。
    #[test]
    fn supply_template_preserves_supplier_columns_and_separates_prices() {
        let values = [
            "RICE-1",
            "",
            "",
            "",
            "",
            "品牌",
            "米",
            "大米",
            "5kg",
            "31.1234",
            "29.50",
            "12",
            "49.00",
            "SUP-001",
            "SUP-RICE-5",
            "13",
        ];
        let cells = values
            .iter()
            .enumerate()
            .map(|(index, value)| {
                let column = char::from(b'A' + u8::try_from(index).unwrap());
                format!(r#"<c r="{column}2" t="inlineStr"><is><t>{value}</t></is></c>"#)
            })
            .collect::<String>();
        let parsed =
            super::parse_product_quote_xlsx(&workbook_with_cells(PRODUCT_IMPORT_HEADERS, &cells, None))
                .unwrap();
        parsed.ensure_supply_template().unwrap();
        assert_eq!(parsed.rows[0].cells, values);
        let row = normalize_import_row(2, &parsed.rows[0].cells).unwrap();
        assert!(row.sales_price.is_none());
        assert!(row.bulk_price.is_none());
        assert!(row.bulk_min_quantity.is_none());
        assert_eq!(row.market_price.unwrap().to_string(), "49.00");
        let supply = crate::product_import::supply_input::SupplyImportInput::from_cells(
            &parsed.rows[0].cells,
            "2026-10-26".parse().unwrap(),
        )
        .unwrap();
        assert_eq!(supply.supplier_no, "SUP-001");
        assert_eq!(supply.supplier_sku_code, "SUP-RICE-5");
        assert_eq!(supply.terms.input_tax_rate, "0.13");
        assert_eq!(supply.terms.bulk_minimum_order_quantity, "12");
    }

    /// 旧文件可恢复既有任务，但不能作为新的联合导入提交。
    #[test]
    fn historical_templates_are_recovery_only() {
        for headers in [super::legacy::HEADERS, super::legacy::COMPACT_HEADERS] {
            let parsed = super::parse_product_quote_xlsx(&workbook_with_cells(
                headers,
                r#"<c r="A2" t="inlineStr"><is><t>old</t></is></c>"#,
                None,
            ))
            .unwrap();
            assert_eq!(parsed.rows[0].cells.len(), 13);
            assert!(
                matches!(parsed.ensure_supply_template(), Err(crate::Error::ValidationError(message)) if message.contains("16 列"))
            );
        }
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
