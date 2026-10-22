use std::collections::{HashMap, HashSet};

use application_core::AuditActor;
use base64::Engine;
use base64::engine::general_purpose::STANDARD;
use chrono::{Duration, NaiveDate};
use id_generator::next_id;
use rust_decimal::Decimal;
use serde_json::{Value, json};
use tokio::task::spawn_blocking;

use super::BusinessTransfers;
use super::imports::required;
use super::quotes::{HEADERS, date};
use super::workbook::text;
use crate::sales_inquiry::read::InquiryReadService;
use crate::sales_inquiry_import::parse::{Row, quote_sheets};
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
pub(super) fn parse_files(files: &[Value], source: &Value, prior: &Value) -> Result<Value> {
    let mut rows = Vec::new();
    let mut issues = Vec::new();
    let mut skipped = 0;
    let mut total = 0;
    let mut seen = HashSet::new();
    for file in files {
        let name = required(file, "file_name")?;
        let bytes =
            STANDARD.decode(required(file, "content_base64")?).map_err(|_| invalid("报价文件编码无效"))?;
        total += bytes.len();
        if total > 50 * 1024 * 1024 {
            return Err(invalid("报价文件合计不得超过 50 MiB"));
        }
        let parsed = (|| {
            if !name.to_ascii_lowercase().ends_with(".xlsx") {
                return Err(invalid("请选择 XLSX 文件"));
            }
            let sheets = quote_sheets(&bytes)?;
            let (supplier, legacy) = metadata(&sheets, &source["inquiry"])?;
            let sheet = sheets.get("报价表").ok_or_else(|| invalid("缺少报价表"))?;
            check_headers(sheet, legacy)?;
            for row in sheet.iter().filter(|r| r.number >= 5) {
                let source_label = format!("{name} 第 {} 行", row.number);
                match parse_row(row, &supplier, legacy, source, prior) {
                    Ok(Some(mut value)) => {
                        let key = format!("{}:{}", supplier, value["quote"]["line_id"]);
                        if !seen.insert(key) {
                            issues.push(format!("{source_label}：同一供应商商品有多份回复，请保留一份"));
                            continue;
                        }
                        value["source"] = json!(source_label);
                        rows.push(value);
                    },
                    Ok(None) => skipped += 1,
                    Err(error) => issues.push(format!("{source_label}：{error}")),
                }
            }
            Ok(())
        })();
        if let Err(error) = parsed {
            issues.push(format!("{name}：{error}"));
        }
    }
    if rows.len() > 1000 {
        return Err(invalid("每次最多导入 1000 条回复"));
    }
    Ok(json!({"rows":rows,"issues":issues,"skipped":skipped}))
}
fn cell(row: &Row, index: usize) -> &str {
    row.cells.get(index).map_or("", |c| c.value.as_str())
}
fn metadata(sheets: &HashMap<String, Vec<Row>>, inquiry: &Value) -> Result<(String, bool)> {
    let rows = sheets.get("回导信息").ok_or_else(|| invalid("缺少回导信息，请重新生成报价表"))?;
    let at = |number| rows.iter().find(|r| r.number == number).map_or("", |r| cell(r, 0));
    let legacy = at(1) == "supplier-inquiry-quote-v2";
    if !legacy && at(1) != "supplier-inquiry-quote-v3" {
        return Err(invalid("报价表版本不支持，请重新生成"));
    }
    if at(2) != text(inquiry, "id")
        || at(3).parse::<u64>().ok() != inquiry["requirement_version"].as_u64()
        || at(4) != (inquiry["feedback_round"].as_u64().unwrap_or_default() + 1).to_string()
    {
        return Err(invalid("报价表不属于当前询价、需求版本或反馈轮次"));
    }
    if at(5).is_empty() {
        return Err(invalid("缺少供应商标识"));
    }
    Ok((at(5).into(), legacy))
}
fn check_headers(rows: &[Row], legacy: bool) -> Result<()> {
    if rows.iter().any(|r| r.number > 204) {
        return Err(invalid("报价表超过 200 条商品"));
    }
    let mut headers = HEADERS.to_vec();
    if legacy {
        headers.insert(13, "承诺交付日");
    }
    let row = rows.iter().find(|r| r.number == 4).ok_or_else(|| invalid("报价表缺少表头"))?;
    if headers.iter().enumerate().any(|(i, h)| cell(row, i) != *h) {
        return Err(invalid("报价表列已变化，请保留原模板"));
    }
    Ok(())
}
fn parse_row(
    row: &Row,
    supplier: &str,
    legacy: bool,
    source: &Value,
    prior: &Value,
) -> Result<Option<Value>> {
    let len = if legacy { 17 } else { 16 };
    if (10..len).all(|i| cell(row, i).is_empty()) {
        return Ok(None);
    }
    let line = original_line(row, len, source)?;
    let previous = prior
        .as_array()
        .into_iter()
        .flatten()
        .filter(|q| q["line_id"] == line["line_id"] && q["supplier_id"] == supplier)
        .max_by_key(|q| (q["feedback_round"].as_u64(), q["revision_no"].as_u64()));
    let candidates: Vec<_> = source["offerings"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|o| o["supplier_id"] == supplier && o["sku_id"] == line["sku_id"] && o["status"] == "ACTIVE")
        .collect();
    let offering = previous
        .and_then(|p| candidates.iter().copied().find(|o| o["id"] == p["supplier_offering_id"]))
        .or_else(|| if candidates.len() == 1 { candidates.first().copied() } else { None })
        .ok_or_else(|| invalid("可用供给缺失或存在多个供给，请先手工登记以确定供给关系"))?;
    let terms = terms(row, line, legacy)?;
    let note = cell(row, len - 1);
    if note.chars().count() > 4000 || (terms["unavailable"] == true && note.is_empty()) {
        return Err(invalid("报价备注最多 4000 字；无法供货须填写原因"));
    }
    let duplicate = prior.as_array().into_iter().flatten().any(|q| {
        q["line_id"] == line["line_id"]
            && q["supplier_id"] == supplier
            && q["supplier_offering_id"] == offering["id"]
            && q["feedback_round"].as_u64() == source["inquiry"]["feedback_round"].as_u64().map(|n| n + 1)
            && same_terms(&q["terms"], &terms)
            && q["internal_note"] == note
    });
    let revision = previous.filter(|p| p["supplier_offering_id"] == offering["id"]);
    Ok(Some(
        json!({"sku_id":line["sku_id"],"product":line["product_name"],"supplier":offering["supplier_name"],"status":if duplicate{"已导入"}else if previous.is_some(){"新版本"}else{"新增"},"previous":previous,"quote":{"quote_id":revision.map(|p|text(p,"quote_id")).unwrap_or_else(next_id),"revision_no":revision.and_then(|p|p["revision_no"].as_u64()).unwrap_or(0)+1,"line_id":line["line_id"],"supplier_id":supplier,"supplier_offering_id":offering["id"],"terms":terms,"internal_note":note}}),
    ))
}
fn original_line<'a>(row: &Row, len: usize, source: &'a Value) -> Result<&'a Value> {
    let identity: Value = serde_json::from_str(cell(row, len)).map_err(|_| invalid("商品行标识无效"))?;
    let line = source["lines"]
        .as_array()
        .into_iter()
        .flatten()
        .find(|l| l["line_id"] == identity["lineId"] && l["sku_id"] == identity["skuId"])
        .ok_or_else(|| invalid("商品行与当前责任需求不匹配"))?;
    let product = source["products"]
        .as_array()
        .into_iter()
        .flatten()
        .find(|p| p["sku_id"] == line["sku_id"])
        .ok_or_else(|| invalid("商品单位资料不可用"))?;
    if cell(row, 3) != text(line, "sku_code")
        || cell(row, 5) != text(product, "base_unit_name")
        || decimal(cell(row, 6), 6, "需求数量")? != decimal(required(line, "quantity")?, 6, "需求数量")?
        || parse_date(cell(row, 7))? != date(&line["delivery_due_at"])
        || cell(row, 8) != text(line, "delivery_terms").trim()
    {
        return Err(invalid("原商品、单位、需求数量或交付要求已被修改，请保留原需求列"));
    }
    Ok(line)
}
fn terms(row: &Row, line: &Value, legacy: bool) -> Result<Value> {
    let scale =
        u32::try_from(line["quantity_scale"].as_u64().unwrap_or(6)).map_err(|_| invalid("数量精度无效"))?;
    let quantity = decimal(cell(row, 12), scale, "可供数量")?;
    if quantity > decimal(required(line, "quantity")?, 6, "需求数量")? {
        return Err(invalid("可供数量不得超过本行需求数量"));
    }
    let due = date(&line["delivery_due_at"]);
    if quantity.is_zero() {
        return Ok(
            json!({"unavailable":true,"unit_cost_gross":"0","input_tax_rate":"0","available_quantity":"0","expected_delivery_date":due,"valid_to":null,"freight_gross":null}),
        );
    }
    let price = decimal(cell(row, 10), 4, "含税单价")?;
    if price.is_zero() {
        return Err(invalid("含税单价须为正数"));
    }
    let tax = decimal(cell(row, 11), 4, "税率")?;
    if tax > Decimal::from(100) {
        return Err(invalid("税率不得超过 100%"));
    }
    let freight_index = if legacy { 14 } else { 13 };
    let valid = cell(row, freight_index + 1);
    Ok(
        json!({"unavailable":false,"unit_cost_gross":price.normalize().to_string(),"input_tax_rate":(tax/Decimal::from(100)).normalize().to_string(),"available_quantity":quantity.normalize().to_string(),"expected_delivery_date":if legacy{parse_date(cell(row,13))?}else{due},"valid_to":if valid.is_empty(){None}else{Some(parse_date(valid)?)},"freight_gross":decimal(cell(row,freight_index),2,"运费（已含运费填 0）")?.normalize().to_string()}),
    )
}
fn decimal(value: &str, scale: u32, label: &str) -> Result<Decimal> {
    let amount =
        value.parse::<Decimal>().map_err(|_| invalid(&format!("{label}须为非负数，最多 {scale} 位小数")))?;
    if amount.is_sign_negative() || amount.scale() > scale {
        return Err(invalid(&format!("{label}须为非负数，最多 {scale} 位小数")));
    }
    Ok(amount)
}
fn parse_date(value: &str) -> Result<String> {
    if let Ok(date) = NaiveDate::parse_from_str(value, "%Y-%m-%d")
        && date.format("%Y-%m-%d").to_string() == value
    {
        return Ok(value.into());
    }
    if let Ok(days) = value.parse::<i64>()
        && (61..=2_958_465).contains(&days)
        && let Some(date) =
            NaiveDate::from_ymd_opt(1899, 12, 30).and_then(|d| d.checked_add_signed(Duration::days(days)))
    {
        return Ok(date.format("%Y-%m-%d").to_string());
    }
    Err(invalid("日期须为有效的 YYYY-MM-DD"))
}
fn same_terms(a: &Value, b: &Value) -> bool {
    ["unit_cost_gross", "input_tax_rate", "available_quantity", "freight_gross"].iter().all(|k| {
        if a[k].is_null() || b[k].is_null() {
            a[k] == b[k]
        } else {
            a[k].as_str().and_then(|s| s.parse::<Decimal>().ok())
                == b[k].as_str().and_then(|s| s.parse::<Decimal>().ok())
        }
    }) && ["unavailable", "expected_delivery_date", "valid_to"].iter().all(|k| a[k] == b[k])
}
fn invalid(message: &str) -> Error {
    Error::ValidationError(message.into())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sales_inquiry_import::parse::Cell;
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
        let value = terms(&row, &line, false).unwrap();
        assert_eq!(value["unit_cost_gross"], "1.2345");
        assert_eq!(value["input_tax_rate"], "0.13");
        row.cells[13].value.clear();
        assert!(terms(&row, &line, false).is_err());
        row.cells[13].value = "0".into();
        row.cells[12].value = "2.01".into();
        assert!(terms(&row, &line, false).is_err());
        assert!(decimal("-1", 4, "单价").is_err());
        assert!(decimal("1.23456", 4, "单价").is_err());
    }
    #[test]
    fn unavailable_quote_ignores_price_but_sets_zero_supply() {
        let row = row(&["", "", "", "", "", "", "", "", "", "", "", "", "0"]);
        let value =
            terms(&row, &json!({"quantity":"2","quantity_scale":0,"delivery_due_at":1792627200}), false)
                .unwrap();
        assert_eq!(value["unavailable"], true);
        assert_eq!(value["available_quantity"], "0");
        assert!(parse_date("2026-02-30").is_err());
        assert_eq!(parse_date("2026-10-22").unwrap(), "2026-10-22");
    }
}
