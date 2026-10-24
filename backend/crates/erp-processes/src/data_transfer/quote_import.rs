mod layout;
mod reply;

use std::collections::HashSet;

use application_core::AuditActor;
use base64::Engine;
use base64::engine::general_purpose::STANDARD;
use serde::Serialize;
use serde_json::Value;
use tokio::task::spawn_blocking;

use self::layout::metadata;
use self::reply::{QuotePreview, parse_row};
use super::BusinessTransfers;
use super::imports::required;
use crate::sales_inquiry::read::InquiryReadService;
use crate::sales_inquiry_import::parse::quote_sheets;
use crate::{Error, Result};

impl BusinessTransfers {
    pub(super) async fn quote_preview(&self, input: &Value, actor: &AuditActor) -> Result<Value> {
        let source = self.quote_source(input, actor).await?;
        let prior = InquiryReadService::new(self.db.clone(), self.rbac.clone())
            .internal_quotes(actor, required(input, "inquiry_id")?, required(input, "task_id")?)
            .await?;
        let prior = serde_json::to_value(prior).map_err(|_| invalid("报价数据格式无效"))?;
        let files = input["files"].as_array().ok_or_else(|| invalid("请选择报价文件"))?.clone();
        if files.is_empty() || files.len() > 20 {
            return Err(invalid("每次选择 1 至 20 个文件"));
        }
        spawn_blocking(move || parse_files(&files, &source, &prior))
            .await
            .map_err(|_| Error::Internal("报价解析中断".into()))?
    }
}

#[derive(Default, Serialize)]
struct Preview {
    rows: Vec<LocatedQuote>,
    issues: Vec<String>,
    skipped: usize,
    #[serde(skip)]
    seen: HashSet<String>,
}

#[derive(Serialize)]
struct LocatedQuote {
    source: String,
    #[serde(flatten)]
    preview: QuotePreview,
}

struct ParsedFile {
    supplier: String,
    rows: Vec<ParsedFileRow>,
}

enum ParsedFileRow {
    Blank,
    Reply(Box<LocatedQuote>),
    Invalid(String),
}

impl Preview {
    fn extend(&mut self, file: ParsedFile) {
        for row in file.rows {
            match row {
                ParsedFileRow::Blank => self.skipped += 1,
                ParsedFileRow::Invalid(issue) => self.issues.push(issue),
                ParsedFileRow::Reply(row) => {
                    let key = format!("{}:{}", file.supplier, row.preview.quote.line_id);
                    if self.seen.insert(key) {
                        self.rows.push(*row);
                    } else {
                        self.issues.push(format!("{}：同一供应商商品有多份回复，请保留一份", row.source));
                    }
                },
            }
        }
    }
}

pub(super) fn parse_files(files: &[Value], source: &Value, prior: &Value) -> Result<Value> {
    let mut preview = Preview::default();
    let mut total = 0;
    for file in files {
        let name = required(file, "file_name")?;
        let bytes =
            STANDARD.decode(required(file, "content_base64")?).map_err(|_| invalid("报价文件编码无效"))?;
        total += bytes.len();
        if total > 50 * 1024 * 1024 {
            return Err(invalid("报价文件合计不得超过 50 MiB"));
        }
        match parse_file(name, &bytes, source, prior) {
            Ok(file) => preview.extend(file),
            Err(error) => preview.issues.push(format!("{name}：{error}")),
        }
    }
    if preview.rows.len() > 1000 {
        return Err(invalid("每次最多导入 1000 条回复"));
    }
    serde_json::to_value(preview).map_err(|_| Error::Internal("报价数据格式无效".into()))
}

fn parse_file(name: &str, bytes: &[u8], source: &Value, prior: &Value) -> Result<ParsedFile> {
    if !name.to_ascii_lowercase().ends_with(".xlsx") {
        return Err(invalid("请选择 XLSX 文件"));
    }
    let sheets = quote_sheets(bytes)?;
    let metadata = metadata(&sheets, &source["inquiry"])?;
    let sheet = sheets.get("报价表").ok_or_else(|| invalid("缺少报价表"))?;
    metadata.layout.check_headers(sheet)?;
    let rows = sheet
        .iter()
        .filter(|row| row.number >= 5)
        .map(|row| {
            let source_label = format!("{name} 第 {} 行", row.number);
            match parse_row(row, &metadata.supplier, metadata.layout, source, prior) {
                Ok(Some(preview)) => {
                    ParsedFileRow::Reply(Box::new(LocatedQuote { source: source_label, preview }))
                },
                Ok(None) => ParsedFileRow::Blank,
                Err(error) => ParsedFileRow::Invalid(format!("{source_label}：{error}")),
            }
        })
        .collect();
    Ok(ParsedFile { supplier: metadata.supplier, rows })
}

fn invalid(message: &str) -> Error {
    Error::ValidationError(message.into())
}

#[cfg(test)]
mod tests {
    use rust_xlsxwriter::Workbook;
    use serde_json::json;

    use super::layout::Template;
    use super::reply::{decimal, parse_date, terms};
    use super::*;
    use crate::data_transfer::quotes::{HEADERS, date};
    use crate::sales_inquiry_import::parse::{Cell, Row};
    fn row(values: &[&str]) -> Row {
        Row {
            number: 5,
            cells: values
                .iter()
                .map(|s| Cell { value: (*s).into(), text: true, ..Default::default() })
                .collect(),
        }
    }
    #[test]
    fn exact_decimal_quote_requires_freight_and_limits_supply() {
        let line = json!({"quantity":"2","quantity_scale":2,"delivery_due_at":1792627200});
        let mut row = row(&["", "", "", "", "", "", "", "", "", "", "1.2345", "13", "2", "0", "", "备注"]);
        let value = serde_json::to_value(terms(&row, &line, Template::V4.layout()).unwrap()).unwrap();
        assert_eq!(value["unit_cost_gross"], "1.2345");
        assert_eq!(value["input_tax_rate"], "0.13");
        row.cells[13].value.clear();
        assert!(terms(&row, &line, Template::V4.layout()).is_err());
        row.cells[13].value = "0".into();
        row.cells[12].value = "2.01".into();
        assert!(terms(&row, &line, Template::V4.layout()).is_err());
        assert!(decimal("-1", 4, "单价").is_err());
        assert!(decimal("1.23456", 4, "单价").is_err());
    }
    #[test]
    fn unavailable_quote_ignores_price_but_sets_zero_supply() {
        let row = row(&["", "", "", "", "", "", "", "", "", "", "", "", "0"]);
        let value = serde_json::to_value(
            terms(
                &row,
                &json!({"quantity":"2","quantity_scale":0,"delivery_due_at":1792627200}),
                Template::V4.layout(),
            )
            .unwrap(),
        )
        .unwrap();
        assert_eq!(value["unavailable"], true);
        assert_eq!(value["available_quantity"], "0");
        assert!(parse_date("2026-02-30").is_err());
        assert_eq!(parse_date("2026-10-22").unwrap(), "2026-10-22");
    }
    #[test]
    fn quote_workbooks_v2_v3_v4_import_without_expiry_and_keep_delivery_and_price() {
        let line = json!({"line_id":"L1","sku_id":"SKU","sku_code":"001","product_name":"商品",
            "quantity":"2","quantity_scale":0,"delivery_due_at":1792627200,"delivery_terms":"按期交付"});
        let source = json!({"inquiry":{"id":"I1","requirement_version":1,"feedback_round":0},
            "lines":[line.clone()],"products":[{"sku_id":"SKU","base_unit_name":"盒"}],
            "offerings":[{"id":"O1","sku_id":"SKU","supplier_id":"S1","supplier_name":"供应商","status":"ACTIVE"}]});
        for version in [2, 3, 4] {
            let mut book = Workbook::new();
            let sheet = book.add_worksheet();
            sheet.set_name("报价表").unwrap();
            let mut headers = HEADERS.to_vec();
            if version < 4 {
                headers.insert(14, "报价失效日");
            }
            if version == 2 {
                headers.insert(13, "承诺交付日");
            }
            for (index, header) in headers.iter().enumerate() {
                sheet.write_string(3, u16::try_from(index).unwrap(), *header).unwrap();
            }
            let mut values = vec![
                "".to_string(),
                "商品".into(),
                "".into(),
                "001".into(),
                "".into(),
                "盒".into(),
                "2".into(),
                date(&line["delivery_due_at"]),
                "按期交付".into(),
                "".into(),
                "10.25".into(),
                "13".into(),
                "2".into(),
                "0".into(),
                "备注".into(),
            ];
            if version < 4 {
                values.insert(14, "旧失效日期内容不再参与解析".into());
            }
            if version == 2 {
                values.insert(13, "2026-10-25".into());
            }
            values.push(json!({"lineId":"L1","skuId":"SKU"}).to_string());
            for (index, value) in values.iter().enumerate() {
                sheet.write_string(4, u16::try_from(index).unwrap(), value).unwrap();
            }
            let meta = book.add_worksheet();
            meta.set_name("回导信息").unwrap();
            for (index, value) in [
                format!("supplier-inquiry-quote-v{version}"),
                "I1".into(),
                "1".into(),
                "1".into(),
                "S1".into(),
            ]
            .iter()
            .enumerate()
            {
                meta.write_string(u32::try_from(index).unwrap(), 0, value).unwrap();
            }
            let bytes = book.save_to_buffer().unwrap();
            let result = parse_files(
                &[json!({"file_name":"报价.xlsx","content_base64":STANDARD.encode(bytes)})],
                &source,
                &json!([]),
            )
            .unwrap();
            assert_eq!(result["issues"], json!([]), "v{version}: {result}");
            assert_eq!(result["rows"].as_array().unwrap().len(), 1);
            let terms = &result["rows"][0]["quote"]["terms"];
            assert_eq!(terms["unit_cost_gross"], "10.25");
            assert_eq!(terms["input_tax_rate"], "0.13");
            assert!(terms.get("valid_to").is_none());
            assert_eq!(
                terms["expected_delivery_date"],
                if version == 2 { "2026-10-25".into() } else { date(&line["delivery_due_at"]) }
            );
        }
    }
    fn source() -> Value {
        json!({"inquiry":{"id":"I1","requirement_version":1,"feedback_round":0},
            "lines":[{"line_id":"L1","sku_id":"SKU","sku_code":"001","product_name":"商品",
                "quantity":"2","quantity_scale":6,"delivery_due_at":1792627200,"delivery_terms":"按期交付"}],
            "products":[{"sku_id":"SKU","base_unit_name":"盒"}],
            "offerings":[{"id":"O1","sku_id":"SKU","supplier_id":"S1","supplier_name":"供应商","status":"ACTIVE"}]})
    }

    fn filled_row() -> Row {
        let mut reply = row(&[
            "",
            "商品",
            "",
            "001",
            "",
            "盒",
            "2",
            "",
            "按期交付",
            "",
            "10.25",
            "13",
            "2",
            "0",
            "备注",
            "",
        ]);
        reply.cells[7].value = date(&source()["lines"][0]["delivery_due_at"]);
        reply.cells[15].value = json!({"lineId":"L1","skuId":"SKU"}).to_string();
        reply
    }

    fn preview(row: &Row, source: &Value, prior: &Value) -> Value {
        serde_json::to_value(parse_row(row, "S1", Template::V4.layout(), source, prior).unwrap().unwrap())
            .unwrap()
    }

    #[test]
    fn blank_reply_skips_identity_but_zero_supply_is_an_unavailable_reply() {
        let source = source();
        let mut reply = filled_row();
        for cell in reply.cells.iter_mut().skip(10) {
            cell.value.clear();
        }
        assert!(parse_row(&reply, "S1", Template::V4.layout(), &source, &json!([])).unwrap().is_none());
        reply.cells[15].value = filled_row().cells[15].value.clone();
        reply.cells[12].value = "0".into();
        assert!(matches!(parse_row(&reply, "S1", Template::V4.layout(), &source, &json!([])),
            Err(Error::ValidationError(message)) if message == "报价备注最多 4000 字；无法供货须填写原因"));
        reply.cells[14].value = "无法按期供货".into();
        reply.cells[10].value = "无须解析的价格".into();
        let value = preview(&reply, &source, &json!([]));
        assert_eq!(value["quote"]["terms"]["unavailable"], true);
        assert_eq!(value["quote"]["terms"]["available_quantity"], "0");
        assert_eq!(value["quote"]["terms"]["freight_gross"], Value::Null);
    }

    #[test]
    fn history_selects_its_active_offering_and_only_one_candidate_can_fallback() {
        let mut source = source();
        let mut alternative = source["offerings"][0].clone();
        alternative["id"] = json!("O2");
        source["offerings"].as_array_mut().unwrap().push(alternative);
        let reply = filled_row();
        assert!(matches!(parse_row(&reply, "S1", Template::V4.layout(), &source, &json!([])),
            Err(Error::ValidationError(message)) if message == "可用供给缺失或存在多个供给，请先手工登记以确定供给关系"));
        let prior = json!([
            {"line_id":"L1","supplier_id":"S1","supplier_offering_id":"O1","feedback_round":1,"revision_no":9,"quote_id":"Q1"},
            {"line_id":"L1","supplier_id":"S1","supplier_offering_id":"O2","feedback_round":2,"revision_no":3,"quote_id":"Q2"}
        ]);
        let value = preview(&reply, &source, &prior);
        assert_eq!(value["status"], "新版本");
        assert_eq!(value["quote"]["supplier_offering_id"], "O2");
        assert_eq!(value["quote"]["quote_id"], "Q2");
        assert_eq!(value["quote"]["revision_no"], 4);
        source["offerings"].as_array_mut().unwrap().pop();
        let value = preview(&reply, &source, &prior);
        assert_eq!(value["quote"]["supplier_offering_id"], "O1");
        assert_eq!(value["quote"]["revision_no"], 1);
        assert_ne!(value["quote"]["quote_id"], "Q2");
    }

    #[test]
    fn duplicate_reply_ignores_numeric_formatting_and_historical_expiry() {
        let source = source();
        let reply = filled_row();
        let first = preview(&reply, &source, &json!([]));
        let mut old = first["quote"].clone();
        old["feedback_round"] = json!(1);
        old["terms"]["unit_cost_gross"] = json!("10.2500");
        old["terms"]["input_tax_rate"] = json!("0.1300");
        old["terms"]["valid_to"] = json!("2020-01-01");
        old["terms"]["address_limits"] = json!([{"legacy":"not compared by preview"}]);
        let value = preview(&reply, &source, &json!([old.clone()]));
        assert_eq!(value["status"], "已导入");
        assert_eq!(value["previous"], old);
        assert_eq!(value["quote"]["quote_id"], first["quote"]["quote_id"]);
        assert_eq!(value["quote"]["revision_no"], 2);
    }

    #[test]
    fn supplier_error_precedes_terms_and_terms_keep_original_first_error() {
        let mut source = source();
        source["offerings"] = json!([]);
        let mut reply = filled_row();
        reply.cells[10].value = "0".into();
        reply.cells[11].value = "not a tax".into();
        assert!(matches!(parse_row(&reply, "S1", Template::V4.layout(), &source, &json!([])),
            Err(Error::ValidationError(message)) if message == "可用供给缺失或存在多个供给，请先手工登记以确定供给关系"));
        assert!(matches!(terms(&reply, &source["lines"][0], Template::V4.layout()),
            Err(Error::ValidationError(message)) if message == "含税单价须为正数"));
        reply.cells[10].value = "1".into();
        reply.cells[11].value = "101".into();
        reply.cells[13].value.clear();
        assert!(matches!(terms(&reply, &source["lines"][0], Template::V4.layout()),
            Err(Error::ValidationError(message)) if message == "税率不得超过 100%"));
    }

    #[test]
    fn unavailable_v2_reply_ignores_invalid_legacy_delivery_and_freight() {
        let mut reply = row(&[
            "",
            "",
            "",
            "",
            "",
            "",
            "",
            "",
            "",
            "",
            "bad price",
            "bad tax",
            "0",
            "bad date",
            "bad freight",
        ]);
        let line = json!({"quantity":"2","quantity_scale":0,"delivery_due_at":null});
        let value = serde_json::to_value(terms(&reply, &line, Template::V2.layout()).unwrap()).unwrap();
        assert_eq!(value["expected_delivery_date"], "");
        assert_eq!(value["freight_gross"], Value::Null);
        // 列解释只按冻结需求精度约束，预览不新增另一套 Quantity 精度拒绝。
        reply.cells[12].value = "0.1234567".into();
        reply.cells[10].value = "1".into();
        reply.cells[11].value = "0".into();
        reply.cells[13].value = "0".into();
        let value = serde_json::to_value(
            terms(&reply, &json!({"quantity":"2","quantity_scale":7}), Template::V4.layout()).unwrap(),
        )
        .unwrap();
        assert_eq!(value["available_quantity"], "0.1234567");
    }

    fn workbook(rows: &[Row]) -> Vec<u8> {
        let mut book = Workbook::new();
        let sheet = book.add_worksheet();
        sheet.set_name("报价表").unwrap();
        for (index, header) in HEADERS.iter().enumerate() {
            sheet.write_string(3, u16::try_from(index).unwrap(), *header).unwrap();
        }
        for (index, row) in rows.iter().enumerate() {
            for (column, cell) in row.cells.iter().enumerate() {
                sheet
                    .write_string(
                        u32::try_from(index + 4).unwrap(),
                        u16::try_from(column).unwrap(),
                        &cell.value,
                    )
                    .unwrap();
            }
        }
        let meta = book.add_worksheet();
        meta.set_name("回导信息").unwrap();
        for (index, value) in ["supplier-inquiry-quote-v4", "I1", "1", "1", "S1"].iter().enumerate() {
            meta.write_string(u32::try_from(index).unwrap(), 0, *value).unwrap();
        }
        book.save_to_buffer().unwrap()
    }

    #[test]
    fn file_and_row_failures_preserve_usable_rows_and_issue_order() {
        let mut invalid_row = filled_row();
        invalid_row.cells[12].value = "too much".into();
        let bytes = workbook(&[filled_row(), invalid_row, filled_row()]);
        let files = json!([
            {"file_name":"wrong.csv","content_base64":STANDARD.encode(b"ignored")},
            {"file_name":"valid.xlsx","content_base64":STANDARD.encode(bytes)},
            {"file_name":"again.xlsx","content_base64":STANDARD.encode(workbook(&[filled_row()]))}
        ]);
        let result = parse_files(files.as_array().unwrap(), &source(), &json!([])).unwrap();
        assert_eq!(result["rows"].as_array().unwrap().len(), 1);
        assert_eq!(result["rows"][0]["source"], "valid.xlsx 第 5 行");
        let issues = result["issues"].as_array().unwrap();
        assert_eq!(issues.len(), 4);
        assert_eq!(issues[0], format!("wrong.csv：{}", invalid("请选择 XLSX 文件")));
        assert_eq!(
            issues[1],
            format!("valid.xlsx 第 6 行：{}", invalid("可供数量须为非负数，最多 6 位小数"))
        );
        assert_eq!(issues[2], "valid.xlsx 第 7 行：同一供应商商品有多份回复，请保留一份");
        assert_eq!(issues[3], "again.xlsx 第 5 行：同一供应商商品有多份回复，请保留一份");
        assert!(
            matches!(parse_files(&[json!({"file_name":"bad.xlsx","content_base64":"!"})], &source(), &json!([])),
            Err(Error::ValidationError(message)) if message == "报价文件编码无效")
        );
    }
}
