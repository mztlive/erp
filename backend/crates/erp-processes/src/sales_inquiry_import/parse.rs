//! 有界读取客户工作簿；只接受版本化的三个白名单工作表。

use std::collections::HashMap;

use erp_sales::entity::sales_inquiry_import::{
    INQUIRY_IMPORT_MAX_BYTES, INQUIRY_IMPORT_MAX_ROWS, INQUIRY_TEMPLATE_VERSION, InquiryImportIssue,
};
use erp_sales::entity::sales_inquiry_pool::{INSTRUCTIONS_SHEET, POOL_SHEET, PoolTemplateVersion};

use super::pool::pool_rows;
use super::pool_delivery_checks::strip_checks;
use super::{pool_delivery, scalar_cells};
use crate::xlsx::{self, Archive, ArchiveLimits, SheetLimits};
pub(crate) use crate::xlsx::{Cell, Row};
use crate::{Error, Result};

pub(super) const PRODUCT_SHEET: &str = "商品清单";
pub(super) use erp_sales::entity::sales_inquiry_pool::DELIVERY_SHEET;
pub(super) const PRODUCT_HEADERS: [&str; 7] =
    ["SKU编码", "商品名称", "规格", "单位", "需求数量", "销售价", "需求备注"];
pub(super) const DELIVERY_HEADERS: [&str; 10] =
    ["SKU编码", "收件人", "联系电话", "省", "市", "区县", "详细地址", "配送数量", "要求交付日期", "交付条件"];

pub(crate) struct Workbook {
    pub product_sheet: &'static str,
    pub automatic_delivery: bool,
    pub products: Vec<Row>,
    pub deliveries: Vec<Row>,
    pub errors: Vec<InquiryImportIssue>,
}

pub(crate) fn parse(bytes: &[u8]) -> Result<Workbook> {
    if bytes.is_empty() || bytes.len() > INQUIRY_IMPORT_MAX_BYTES {
        return Err(invalid("工作簿不能为空且不得超过 5 MiB"));
    }
    let mut archive = Archive::open(bytes, ArchiveLimits::CUSTOMER)?;
    let shared = archive.shared_strings()?;
    let paths = sheet_paths(&mut archive, false)?;
    let mut sheets = HashMap::new();
    for (name, path) in paths {
        sheets.insert(name, rows(&archive.xml(&path)?, &shared)?);
    }
    let date1904 = xlsx::date1904(&archive.xml("xl/workbook.xml")?)?;
    let mut workbook = parse_sheets(sheets)?;
    scalar_cells::normalize(&mut workbook.deliveries, date1904);
    Ok(workbook)
}

/// 有界读取供应商报价工作簿，复用 XML 和 ZIP 安全边界。
/// # 参数
/// 不超过 20 MiB 的工作簿原文。
/// # 返回
/// 固定报价工作表的行列值。
/// # 错误
/// 宏、外部链接、公式、重复路径及超限展开内容均拒绝。
pub(crate) fn quote_sheets(bytes: &[u8]) -> Result<HashMap<String, Vec<Row>>> {
    if bytes.is_empty() || bytes.len() > 20 * 1024 * 1024 {
        return Err(invalid("报价表须为 1 至 20 MiB 的 XLSX 文件"));
    }
    let mut archive = Archive::open(bytes, ArchiveLimits::CUSTOMER)?;
    if xlsx::date1904(&archive.xml("xl/workbook.xml")?)? {
        return Err(invalid("报价表不支持 1904 日期系统，请使用原模板"));
    }
    let shared = archive.shared_strings()?;
    let mut sheets = HashMap::new();
    for (name, path) in sheet_paths(&mut archive, true)? {
        let xml = archive.xml(&path)?;
        let sheet = xlsx::rows(&xml, &shared, SheetLimits::CUSTOMER)?;
        if sheet.hyperlinks {
            return Err(invalid("报价表不能包含超链接"));
        }
        let mut rows = sheet.rows;
        trim_rows(&mut rows);
        if rows.iter().flat_map(|r| &r.cells).any(|c| c.formula) {
            return Err(invalid("报价表不能包含公式，请先转换为值"));
        }
        sheets.insert(name, rows);
    }
    Ok(sheets)
}

fn parse_sheets(mut sheets: HashMap<String, Vec<Row>>) -> Result<Workbook> {
    let version = check_version(&sheets)?;
    let automatic = version.is_some_and(PoolTemplateVersion::automatic_delivery);
    let mut errors = Vec::new();
    let product_sheet = if sheets.contains_key(POOL_SHEET) { POOL_SHEET } else { PRODUCT_SHEET };
    let product_rows = sheets.remove(product_sheet).unwrap_or_default();
    let products = if product_sheet == POOL_SHEET {
        pool_rows(product_rows.clone(), &mut errors)?
    } else {
        data_rows(product_rows.clone(), PRODUCT_SHEET, &PRODUCT_HEADERS, &mut errors)
    };
    let mut delivery_rows = sheets.remove(DELIVERY_SHEET).unwrap_or_default();
    if let Some(version) = version {
        strip_checks(&mut delivery_rows, version, &mut errors);
    }
    let mut deliveries = if automatic {
        pool_delivery::delivery_rows(&product_rows, delivery_rows, &mut errors)?
    } else {
        data_rows(delivery_rows, DELIVERY_SHEET, &DELIVERY_HEADERS, &mut errors)
    };
    if product_sheet == POOL_SHEET {
        deliveries.retain(|row| row.cells.iter().skip(1).any(|cell| !cell.value.is_empty() || cell.formula));
    }
    if products.len() > INQUIRY_IMPORT_MAX_ROWS || deliveries.len() > INQUIRY_IMPORT_MAX_ROWS {
        return Err(invalid("商品行及实际填写的配送行每次均不得超过 2000 行"));
    }
    Ok(Workbook { product_sheet, automatic_delivery: automatic, products, deliveries, errors })
}

fn sheet_paths(archive: &mut Archive<'_>, quote: bool) -> Result<Vec<(String, String)>> {
    let result = xlsx::sheet_paths(archive)?;
    let allowed = if quote {
        &["报价表", "回导信息", "配送需求"][..]
    } else {
        &[PRODUCT_SHEET, POOL_SHEET, DELIVERY_SHEET, INSTRUCTIONS_SHEET][..]
    };
    if result.iter().any(|(name, _)| !allowed.contains(&name.as_str())) {
        return Err(invalid("只接受选品（或商品清单）、配送需求、填写说明三个工作表"));
    }
    if !quote
        && (result.len() != 3
            || !result.iter().any(|(name, _)| name == DELIVERY_SHEET)
            || !result.iter().any(|(name, _)| name == INSTRUCTIONS_SHEET))
    {
        return Err(invalid("须包含选品（或商品清单）、配送需求及填写说明三个工作表"));
    }
    Ok(result)
}

fn check_version(sheets: &HashMap<String, Vec<Row>>) -> Result<Option<PoolTemplateVersion>> {
    let row = sheets.get(INSTRUCTIONS_SHEET).and_then(|rows| rows.iter().find(|row| row.number == 1));
    let marker = row.and_then(|row| row.cells.get(1)).map(|cell| cell.value.as_str());
    let pool_version = marker.and_then(PoolTemplateVersion::from_marker);
    let matches_template = if sheets.contains_key(POOL_SHEET) {
        pool_version.is_some()
    } else {
        marker == Some(INQUIRY_TEMPLATE_VERSION)
    };
    let valid = row.is_some_and(|row| {
        row.cells.len() >= 2
            && row.cells[0].value == "模板版本"
            && matches_template
            && !row.cells.iter().any(|cell| cell.formula)
    });
    if !valid {
        return Err(invalid("填写说明第 1 行模板版本与工作表不匹配，请使用当前导出文件"));
    }
    // 说明页公式不参与业务，但仍须先转值；明细公式由各模板解析器定位。
    if sheets
        .get(INSTRUCTIONS_SHEET)
        .is_some_and(|rows| rows.iter().any(|row| row.cells.iter().any(|cell| cell.formula)))
    {
        return Err(invalid("填写说明含公式，请先转换为值"));
    }
    Ok(pool_version)
}

pub(super) fn data_rows(
    mut rows: Vec<Row>,
    sheet: &str,
    headers: &[&str],
    errors: &mut Vec<InquiryImportIssue>,
) -> Vec<Row> {
    let valid = rows.first().is_some_and(|row| {
        row.number == 1
            && row.cells.len() <= headers.len()
            && headers.iter().enumerate().all(|(index, header)| {
                row.cells.get(index).is_some_and(|cell| {
                    let matches = cell.value == *header
                        || (sheet == PRODUCT_SHEET && *header == "销售价" && cell.value == "销售参考价");
                    matches && !cell.formula
                })
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

fn rows(xml: &str, shared: &[String]) -> Result<Vec<Row>> {
    let mut rows = xlsx::rows(xml, shared, SheetLimits::CUSTOMER)?.rows;
    trim_rows(&mut rows);
    Ok(rows)
}

fn trim_rows(rows: &mut [Row]) {
    for row in rows {
        for cell in &mut row.cells {
            cell.value = cell.value.trim().to_string();
        }
    }
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
    use std::io::{Cursor, Read, Write};

    use erp_sales::entity::sales_inquiry::{InquiryFulfillmentKind, InquiryRequirementData};
    use erp_sales::entity::sales_inquiry_import::{InquiryImportSku, InquiryImportValidation};
    use erp_sales::entity::sales_inquiry_pool::POOL_DELIVERY_HEADERS;
    use regex::Regex;
    use zip::write::SimpleFileOptions;
    use zip::{ZipArchive, ZipWriter};

    use super::*;
    use crate::sales_inquiry_import::preview::{delivery_rows, product_rows};
    use crate::xlsx::MAX_XML_BYTES;

    #[test]
    fn customer_sales_price_header_accepts_legacy_name_but_rejects_extra_column() {
        for price_header in ["销售价", "销售参考价"] {
            let mut headers = PRODUCT_HEADERS
                .map(|value| Cell { value: value.into(), text: true, ..Default::default() })
                .to_vec();
            headers[5].value = price_header.into();
            let mut errors = Vec::new();
            let products = data_rows(
                vec![
                    Row { number: 1, cells: headers.clone() },
                    Row {
                        number: 2,
                        cells: ["001", "", "", "PCS", "1", "0", ""]
                            .map(|value| Cell { value: value.into(), text: true, ..Default::default() })
                            .to_vec(),
                    },
                ],
                PRODUCT_SHEET,
                &PRODUCT_HEADERS,
                &mut errors,
            );
            assert!(errors.is_empty());
            assert_eq!(products[0].cells[5].value, "0");
            headers.push(Cell { value: "销售价".into(), text: true, ..Default::default() });
            data_rows(vec![Row { number: 1, cells: headers }], PRODUCT_SHEET, &PRODUCT_HEADERS, &mut errors);
            assert_eq!(errors.len(), 1);
            assert_eq!(errors[0].column, "表头");
        }
    }

    #[test]
    fn numeric_phone_and_excel_date_produce_a_valid_requirement() {
        let products = r#"<c r="A2" t="inlineStr"><is><t>001</t></is></c><c r="D2" t="inlineStr"><is><t>BOX</t></is></c><c r="E2"><v>1</v></c>"#;
        let delivery = r#"<c r="A2" t="inlineStr"><is><t>001</t></is></c><c r="B2" t="inlineStr"><is><t>收件人</t></is></c><c r="C2"><v>1.3800000000E+10</v></c><c r="D2" t="inlineStr"><is><t>广东省</t></is></c><c r="E2" t="inlineStr"><is><t>广州市</t></is></c><c r="F2" t="inlineStr"><is><t>天河区</t></is></c><c r="G2" t="inlineStr"><is><t>测试地址</t></is></c><c r="H2"><v>1</v></c><c r="I2" s="1"><v>46296</v></c>"#;
        let parsed = parse(&package(INQUIRY_TEMPLATE_VERSION, products, delivery)).unwrap();
        assert_valid_delivery(&parsed);
        // 两种日期系统必须得到同一天，不受 XML 属性引号形式影响。
        let bytes = package(INQUIRY_TEMPLATE_VERSION, products, &delivery.replace("46296", "44834"));
        let mut archive = ZipArchive::new(Cursor::new(bytes)).unwrap();
        let mut writer = ZipWriter::new(Cursor::new(Vec::new()));
        for index in 0..archive.len() {
            let mut entry = archive.by_index(index).unwrap();
            let name = entry.name().to_string();
            let mut xml = String::new();
            entry.read_to_string(&mut xml).unwrap();
            if name == "xl/workbook.xml" {
                xml = xml.replace("<sheets>", "<workbookPr date1904='true'/><sheets>");
            }
            writer.start_file(name, SimpleFileOptions::default()).unwrap();
            writer.write_all(xml.as_bytes()).unwrap();
        }
        assert_valid_delivery(&parse(&writer.finish().unwrap().into_inner()).unwrap());
    }

    fn assert_valid_delivery(parsed: &Workbook) {
        assert!(parsed.errors.is_empty(), "{:?}", parsed.errors);
        let products = product_rows(parsed);
        let deliveries = delivery_rows(parsed);
        assert_eq!(deliveries[0].phone, "13800000000");
        assert!(deliveries[0].phone_is_text);
        assert_eq!(deliveries[0].due_date, "2026-10-01");
        let sku = InquiryImportSku {
            sku_id: "sku".into(),
            sku_revision_id: "revision".into(),
            sku_code: "001".into(),
            name: "商品".into(),
            specification: String::new(),
            unit_code: "BOX".into(),
            quantity_scale: 0,
            fulfillment_kind: InquiryFulfillmentKind::Physical,
        };
        let mut identity = 0;
        let result = InquiryImportValidation::build(
            &products,
            &deliveries,
            &[sku],
            &InquiryRequirementData::default(),
            || {
                identity += 1;
                identity.to_string()
            },
        );
        assert!(result.errors.is_empty(), "{:?}", result.errors);
        assert_eq!(result.data.lines.len(), 1);
        assert_eq!(result.data.lines[0].quantity.to_string(), "1");
        assert_eq!(result.data.lines[0].allocations[0].phone, "13800000000");
    }

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
        package_sheet(version, PRODUCT_SHEET, sheet(&PRODUCT_HEADERS, product_data), delivery_data)
    }

    fn package_sheet(version: &str, name: &str, product_xml: String, delivery_data: &str) -> Vec<u8> {
        let workbook = r#"<workbook xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships"><sheets><sheet name="商品清单" r:id="r1"/><sheet name="配送需求" r:id="r2"/><sheet name="填写说明" r:id="r3"/></sheets></workbook>"#;
        let rels = r#"<Relationships><Relationship Target="worksheets/sheet1.xml" Id="r1"/><Relationship Id="r2" Target="worksheets/sheet2.xml"/><Relationship Id="r3" Target="worksheets/sheet3.xml"/></Relationships>"#;
        let instructions = format!(
            r#"<worksheet><sheetData><c r="A1" t="inlineStr"><is><t>模板版本</t></is></c><c r="B1" t="inlineStr"><is><t>{version}</t></is></c></sheetData></worksheet>"#
        );
        let files = [
            ("xl/workbook.xml", workbook.replace(PRODUCT_SHEET, name)),
            ("xl/_rels/workbook.xml.rels", rels.to_string()),
            ("xl/worksheets/sheet1.xml", product_xml),
            (
                "xl/worksheets/sheet2.xml",
                sheet(
                    if version == PoolTemplateVersion::V2.marker() {
                        &POOL_DELIVERY_HEADERS[..]
                    } else {
                        &DELIVERY_HEADERS[..]
                    },
                    delivery_data,
                ),
            ),
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
    fn pool_workbook_preserves_selection_rows_and_ignores_unused_delivery_placeholders() {
        let headers = ["SKU 编号", "单位编码", "需求数量", "销售参考价", "需求备注", "供应商编号"];
        let xml = sheet(&headers, "").replace("1\"", "2\"").replace("</sheetData>", r#"<c r="A4" t="inlineStr"><is><t>001</t></is></c><c r="B4" t="inlineStr"><is><t>PCS</t></is></c><c r="C4" t="inlineStr"><is><t>2</t></is></c><c r="F4" t="inlineStr"><is><t>private</t></is></c><c r="A5" t="inlineStr"><is><t>unused</t></is></c></sheetData>"#);
        let delivery = r#"<c r="A2" t="inlineStr"><is><t>unused</t></is></c><c r="A8" t="inlineStr"><is><t>001</t></is></c><c r="C8" t="inlineStr"><is><t>0012345</t></is></c><c r="H8" t="inlineStr"><is><t>2</t></is></c>"#;
        let parsed =
            parse(&package_sheet(PoolTemplateVersion::V1.marker(), POOL_SHEET, xml.clone(), delivery))
                .unwrap();
        assert!(parsed.errors.is_empty());
        assert_eq!(parsed.product_sheet, POOL_SHEET);
        assert_eq!(parsed.products.len(), 1);
        assert_eq!(parsed.products[0].number, 4);
        assert_eq!(parsed.products[0].cells[0].value, "001");
        assert!(!parsed.products[0].cells.iter().any(|cell| cell.value == "private"));
        assert_eq!(parsed.deliveries.len(), 1);
        assert_eq!(parsed.deliveries[0].number, 8);
        assert_eq!(parsed.deliveries[0].cells[2].value, "0012345");
        assert!(parsed.deliveries[0].cells[2].text);
        assert!(parse(&package_sheet(INQUIRY_TEMPLATE_VERSION, POOL_SHEET, xml, delivery)).is_err());
    }

    #[test]
    fn automatic_pool_formula_is_recomputed_from_file_cells_without_using_cache() {
        let headers = [
            "配送关联",
            "商品名称",
            "SKU 编号",
            "SPU 编号",
            "需求数量",
            "单位编码",
            "销售参考价",
            "需求备注",
        ];
        let data = ["line-1", "礼盒", "00123", "000-SPU", "2.5", "PCS", "", ""]
            .iter()
            .enumerate()
            .map(|(index, value)| {
                let column = char::from(b'A' + u8::try_from(index).unwrap());
                format!(r#"<c r="{column}3" t="inlineStr"><is><t>{value}</t></is></c>"#)
            })
            .collect::<String>();
        let xml =
            sheet(&headers, "").replace("1\"", "2\"").replace("</sheetData>", &format!("{data}</sheetData>"));
        let delivery = [("A", "商品名称"), ("B", "SKU 编号"), ("C", "SPU 编号"), ("J", "需求数量")]
            .iter().map(|(column, field)| format!(r#"<c r="{column}2" t="str"><f>IF(INDEX(InquirySelection[需求数量],MATCH("line-1",InquirySelection[配送关联],0))="","",INDEX(InquirySelection[{field}],MATCH("line-1",InquirySelection[配送关联],0)))</f><v>stale-cache</v></c>"#)).collect::<String>();
        let parsed =
            parse(&package_sheet(PoolTemplateVersion::V2.marker(), POOL_SHEET, xml, &delivery)).unwrap();
        assert!(parsed.errors.is_empty());
        assert_eq!(parsed.products[0].cells[0].value, "00123");
        assert_eq!(parsed.deliveries[0].cells[0].value, "00123");
        assert_eq!(parsed.deliveries[0].cells[7].value, "2.5");
        assert!(parsed.deliveries[0].cells[0].text);
        assert!(!parsed.deliveries[0].cells.iter().any(|cell| cell.formula));
    }

    #[test]
    fn exported_v3_workbook_imports_split_addresses_without_using_check_caches() {
        // 由生产 ExcelJS 导出器生成，填写 60 + 40；自动公式缓存仍为空。
        let parsed = parse(include_bytes!("fixtures/pool-v3.xlsx.fixture")).unwrap();
        assert!(parsed.errors.is_empty(), "{:?}", parsed.errors);
        assert_eq!(parsed.products.len(), 1);
        assert_eq!(parsed.products[0].cells[0].value, "00123");
        assert_eq!(parsed.products[0].cells[4].value, "100");
        assert_eq!(parsed.deliveries.len(), 2);
        assert_eq!(parsed.deliveries[0].cells[1].value, "张三");
        assert_eq!(parsed.deliveries[1].cells[1].value, "李四");
        assert_eq!(parsed.deliveries[0].cells[2].value, "0013800000000");
        assert_eq!(parsed.deliveries[0].cells[7].value, "60");
        assert_eq!(parsed.deliveries[1].cells[7].value, "40");
        assert!(parsed.deliveries.iter().all(|row| row.cells.len() == 10));
    }

    #[test]
    fn v4_copied_rows_keep_both_quantities_without_trusting_summary_cells() {
        let parsed = parse(include_bytes!("fixtures/pool-v4.xlsx.fixture")).unwrap();
        assert!(parsed.errors.is_empty(), "{:?}", parsed.errors);
        assert_eq!(parsed.products[0].cells[4].value, "100");
        assert_eq!(parsed.deliveries.len(), 2);
        for row in parsed.deliveries {
            assert_eq!(row.cells.len(), 10);
            assert_eq!(row.cells[0].value, "00123");
            assert_eq!(row.cells[7].value, "100");
        }
    }

    #[test]
    fn unused_address_slots_do_not_count_towards_the_delivery_limit() {
        let bytes = include_bytes!("fixtures/pool-v3.xlsx.fixture");
        let mut archive = Archive::open(bytes, ArchiveLimits::CUSTOMER).unwrap();
        let shared = archive.shared_strings().unwrap();
        let mut sheets = HashMap::new();
        for (name, path) in sheet_paths(&mut archive, false).unwrap() {
            sheets.insert(name, rows(&archive.xml(&path).unwrap(), &shared).unwrap());
        }
        let deliveries = sheets.get_mut(DELIVERY_SHEET).unwrap();
        let mut spare = deliveries.last().unwrap().clone();
        // 核对列即使转值也不参与实际地址计数。
        for cell in spare.cells.iter_mut().skip(12) {
            *cell = Cell::default();
        }
        for number in 5..=6001 {
            spare.number = number;
            deliveries.push(spare.clone());
        }
        let parsed = parse_sheets(sheets).unwrap();
        assert!(parsed.errors.is_empty());
        assert_eq!(parsed.deliveries.len(), 2);
    }

    fn shared_check_workbook() -> Vec<u8> {
        let bytes = include_bytes!("fixtures/pool-v4.xlsx.fixture");
        let mut archive = ZipArchive::new(Cursor::new(bytes.as_slice())).unwrap();
        let mut writer = ZipWriter::new(Cursor::new(Vec::new()));
        for index in 0..archive.len() {
            let mut entry = archive.by_index(index).unwrap();
            let name = entry.name().to_string();
            let mut content = Vec::new();
            entry.read_to_end(&mut content).unwrap();
            if name == "xl/worksheets/sheet2.xml" {
                let mut xml = String::from_utf8(content).unwrap();
                for (id, column) in ["N", "O", "P", "Q", "R"].iter().enumerate() {
                    let anchor =
                        Regex::new(&format!(r#"(<c\b[^>]*r="{column}2"[^>]*>)<f>(.*?)</f>"#)).unwrap();
                    xml = anchor
                        .replace(
                            &xml,
                            format!(r#"${{1}}<f t="shared" ref="{column}2:{column}3" si="{id}">${{2}}</f>"#),
                        )
                        .into_owned();
                    let follower =
                        Regex::new(&format!(r#"(<c\b[^>]*r="{column}3"[^>]*>)<f>.*?</f>"#)).unwrap();
                    xml = follower.replace(&xml, format!(r#"${{1}}<f t="shared" si="{id}"/>"#)).into_owned();
                }
                content = xml.into_bytes();
            }
            writer.start_file(name, SimpleFileOptions::default()).unwrap();
            writer.write_all(&content).unwrap();
        }
        writer.finish().unwrap().into_inner()
    }

    #[test]
    fn shared_check_formulas_are_ignored_but_overallocation_is_still_rejected() {
        use erp_sales::entity::sales_inquiry::{InquiryFulfillmentKind, InquiryRequirementData};
        use erp_sales::entity::sales_inquiry_import::{InquiryImportSku, InquiryImportValidation};

        use crate::sales_inquiry_import::preview::{delivery_rows, product_rows};

        let parsed = parse(&shared_check_workbook()).unwrap();
        assert!(parsed.errors.is_empty(), "{:?}", parsed.errors);
        let products = product_rows(&parsed);
        let mut deliveries = delivery_rows(&parsed);
        assert_eq!(deliveries.len(), 2);
        assert_eq!(deliveries[0].quantity, "100");
        assert_eq!(deliveries[1].quantity, "100");
        for delivery in &mut deliveries {
            delivery.due_date = "2026-11-01".into();
        }
        let sku = InquiryImportSku {
            sku_id: "sku".into(),
            sku_revision_id: "revision".into(),
            sku_code: products[0].sku_code.clone(),
            name: "商品".into(),
            specification: "".into(),
            unit_code: products[0].unit.clone(),
            quantity_scale: 0,
            fulfillment_kind: InquiryFulfillmentKind::Physical,
        };
        let mut identity = 0;
        let result = InquiryImportValidation::build(
            &products,
            &deliveries,
            std::slice::from_ref(&sku),
            &InquiryRequirementData::default(),
            || {
                identity += 1;
                identity.to_string()
            },
        );
        assert_eq!(result.errors.len(), 1, "{:?}", result.errors);
        assert!(result.errors.iter().any(|error| error.column == "需求数量"), "{:?}", result.errors);
        deliveries[0].quantity = "60".into();
        deliveries[1].quantity = "40".into();
        let mut identity = 0;
        let valid = InquiryImportValidation::build(
            &products,
            &deliveries,
            &[sku],
            &InquiryRequirementData::default(),
            || {
                identity += 1;
                identity.to_string()
            },
        );
        assert!(valid.errors.is_empty(), "{:?}", valid.errors);
        assert_eq!(valid.data.lines[0].quantity.to_string(), "100");
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
        writer.write_all(&vec![b'a'; usize::try_from(MAX_XML_BYTES).unwrap() + 1]).unwrap();
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
        assert!(
            super::rows(
                r#"<worksheet xmlns:x="urn:formula"><c r="A1"><x:f>1+1</x:f><v>2</v></c></worksheet>"#,
                &[]
            )
            .unwrap()[0]
                .cells[0]
                .formula
        );
    }

    #[test]
    fn rejects_duplicate_coordinates_and_out_of_bounds() {
        assert!(rows(r#"<sheetData><c r="A2"><v>1</v></c><c r="A2"><v>2</v></c></sheetData>"#, &[]).is_err());
        assert!(rows(r#"<sheetData><c r="A10001"><v>1</v></c></sheetData>"#, &[]).is_err());
        assert!(parse(&[]).is_err());
        assert!(parse(&vec![0; INQUIRY_IMPORT_MAX_BYTES + 1]).is_err());
    }
}
