use chrono::{Duration, NaiveDate};
use erp_supply::inquiry::QuoteTerms;
use id_generator::next_id;
use rust_decimal::Decimal;
use serde::Serialize;
use serde_json::Value;

use super::invalid;
use super::layout::{DeliverySource, Layout, cell};
use crate::Result;
use crate::data_transfer::imports::required;
use crate::data_transfer::quotes::date;
use crate::data_transfer::workbook::text;
use crate::sales_inquiry_import::parse::Row;

#[derive(Serialize)]
pub(super) struct QuotePreview {
    sku_id: Value,
    product: Value,
    supplier: Value,
    status: &'static str,
    previous: Option<Value>,
    pub quote: ReplyQuote,
}

#[derive(Serialize)]
pub(super) struct ReplyQuote {
    quote_id: String,
    revision_no: u64,
    pub line_id: Value,
    supplier_id: String,
    supplier_offering_id: Value,
    terms: ReplyTerms,
    internal_note: String,
}

#[derive(Serialize)]
pub(super) struct ReplyTerms {
    unavailable: bool,
    unit_cost_gross: String,
    input_tax_rate: String,
    available_quantity: String,
    expected_delivery_date: String,
    freight_gross: Option<String>,
}

struct ParsedReply<'a> {
    line: &'a Value,
    note: &'a str,
}

struct MatchedReply<'a> {
    parsed: ParsedReply<'a>,
    offering: &'a Value,
    previous: Option<&'a Value>,
}

pub(super) fn parse_row(
    row: &Row,
    supplier: &str,
    layout: Layout,
    source: &Value,
    prior: &Value,
) -> Result<Option<QuotePreview>> {
    if layout.is_blank(row) {
        return Ok(None);
    }
    let parsed = ParsedReply { line: original_line(row, layout, source)?, note: cell(row, layout.note) };
    // 先核验供应事实，再解释条款，保留原有首错顺序。
    let matched = parsed.match_supplier(supplier, source, prior)?;
    let terms = terms(row, matched.parsed.line, layout)?;
    if matched.parsed.note.chars().count() > 4000 || (terms.unavailable && matched.parsed.note.is_empty()) {
        return Err(invalid("报价备注最多 4000 字；无法供货须填写原因"));
    }
    Ok(Some(matched.preview(supplier, terms, source, prior)))
}

impl<'a> ParsedReply<'a> {
    fn match_supplier(self, supplier: &str, source: &'a Value, prior: &'a Value) -> Result<MatchedReply<'a>> {
        let previous = prior
            .as_array()
            .into_iter()
            .flatten()
            .filter(|quote| quote["line_id"] == self.line["line_id"] && quote["supplier_id"] == supplier)
            .max_by_key(|quote| (quote["feedback_round"].as_u64(), quote["revision_no"].as_u64()));
        let candidates: Vec<_> = source["offerings"]
            .as_array()
            .into_iter()
            .flatten()
            .filter(|offering| {
                offering["supplier_id"] == supplier
                    && offering["sku_id"] == self.line["sku_id"]
                    && offering["status"] == "ACTIVE"
            })
            .collect();
        let offering = previous
            .and_then(|quote| {
                candidates.iter().copied().find(|offering| offering["id"] == quote["supplier_offering_id"])
            })
            .or_else(|| if candidates.len() == 1 { candidates.first().copied() } else { None })
            .ok_or_else(|| invalid("可用供给缺失或存在多个供给，请先手工登记以确定供给关系"))?;
        Ok(MatchedReply { parsed: self, offering, previous })
    }
}

impl MatchedReply<'_> {
    fn preview(self, supplier: &str, terms: ReplyTerms, source: &Value, prior: &Value) -> QuotePreview {
        let duplicate = prior.as_array().into_iter().flatten().any(|quote| {
            quote["line_id"] == self.parsed.line["line_id"]
                && quote["supplier_id"] == supplier
                && quote["supplier_offering_id"] == self.offering["id"]
                && quote["feedback_round"].as_u64()
                    == source["inquiry"]["feedback_round"].as_u64().map(|n| n + 1)
                && terms.matches(&quote["terms"])
                && quote["internal_note"] == self.parsed.note
        });
        let revision = self.previous.filter(|quote| quote["supplier_offering_id"] == self.offering["id"]);
        let status = if duplicate {
            "已导入"
        } else if self.previous.is_some() {
            "新版本"
        } else {
            "新增"
        };
        let quote = ReplyQuote {
            quote_id: revision.map(|quote| text(quote, "quote_id")).unwrap_or_else(next_id),
            revision_no: revision.and_then(|quote| quote["revision_no"].as_u64()).unwrap_or(0) + 1,
            line_id: self.parsed.line["line_id"].clone(),
            supplier_id: supplier.into(),
            supplier_offering_id: self.offering["id"].clone(),
            terms,
            internal_note: self.parsed.note.into(),
        };
        QuotePreview {
            sku_id: self.parsed.line["sku_id"].clone(),
            product: self.parsed.line["product_name"].clone(),
            supplier: self.offering["supplier_name"].clone(),
            status,
            previous: self.previous.cloned(),
            quote,
        }
    }
}

fn original_line<'a>(row: &Row, layout: Layout, source: &'a Value) -> Result<&'a Value> {
    let identity: Value =
        serde_json::from_str(cell(row, layout.identity)).map_err(|_| invalid("商品行标识无效"))?;
    let line = source["lines"]
        .as_array()
        .into_iter()
        .flatten()
        .find(|line| line["line_id"] == identity["lineId"] && line["sku_id"] == identity["skuId"])
        .ok_or_else(|| invalid("商品行与当前责任需求不匹配"))?;
    let product = source["products"]
        .as_array()
        .into_iter()
        .flatten()
        .find(|product| product["sku_id"] == line["sku_id"])
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

pub(super) fn terms(row: &Row, line: &Value, layout: Layout) -> Result<ReplyTerms> {
    let scale =
        u32::try_from(line["quantity_scale"].as_u64().unwrap_or(6)).map_err(|_| invalid("数量精度无效"))?;
    let quantity = decimal(cell(row, 12), scale, "可供数量")?;
    if quantity > decimal(required(line, "quantity")?, 6, "需求数量")? {
        return Err(invalid("可供数量不得超过本行需求数量"));
    }
    let due = date(&line["delivery_due_at"]);
    if quantity.is_zero() {
        return Ok(ReplyTerms {
            unavailable: true,
            unit_cost_gross: "0".into(),
            input_tax_rate: "0".into(),
            available_quantity: "0".into(),
            expected_delivery_date: due,
            freight_gross: None,
        });
    }
    let price = decimal(cell(row, 10), 4, "含税单价")?;
    if !QuoteTerms::unit_cost_is_valid(price, false) {
        return Err(invalid("含税单价须为正数"));
    }
    let tax = decimal(cell(row, 11), 4, "税率")? / Decimal::from(100);
    if !QuoteTerms::tax_rate_is_valid(tax) {
        return Err(invalid("税率不得超过 100%"));
    }
    let expected_delivery_date = match layout.delivery {
        DeliverySource::ReplyColumn(index) => parse_date(cell(row, index))?,
        DeliverySource::Requirement => due,
    };
    let freight = decimal(cell(row, layout.freight), 2, "运费（已含运费填 0）")?;
    Ok(ReplyTerms {
        unavailable: false,
        unit_cost_gross: price.normalize().to_string(),
        input_tax_rate: tax.normalize().to_string(),
        available_quantity: quantity.normalize().to_string(),
        expected_delivery_date,
        freight_gross: Some(freight.normalize().to_string()),
    })
}

impl ReplyTerms {
    fn matches(&self, prior: &Value) -> bool {
        [
            ("unit_cost_gross", Some(self.unit_cost_gross.as_str())),
            ("input_tax_rate", Some(self.input_tax_rate.as_str())),
            ("available_quantity", Some(self.available_quantity.as_str())),
            ("freight_gross", self.freight_gross.as_deref()),
        ]
        .iter()
        .all(|(key, value)| match value {
            None => prior[key].is_null(),
            Some(value) => {
                !prior[key].is_null()
                    && prior[key].as_str().and_then(|value| value.parse::<Decimal>().ok())
                        == value.parse::<Decimal>().ok()
            },
        }) && prior["unavailable"] == self.unavailable
            && prior["expected_delivery_date"] == self.expected_delivery_date
    }
}

pub(super) fn decimal(value: &str, scale: u32, label: &str) -> Result<Decimal> {
    let amount =
        value.parse::<Decimal>().map_err(|_| invalid(&format!("{label}须为非负数，最多 {scale} 位小数")))?;
    if amount.is_sign_negative() || amount.scale() > scale {
        return Err(invalid(&format!("{label}须为非负数，最多 {scale} 位小数")));
    }
    Ok(amount)
}

pub(super) fn parse_date(value: &str) -> Result<String> {
    if let Ok(date) = NaiveDate::parse_from_str(value, "%Y-%m-%d")
        && date.format("%Y-%m-%d").to_string() == value
    {
        return Ok(value.into());
    }
    if let Ok(days) = value.parse::<i64>()
        && (61..=2_958_465).contains(&days)
        && let Some(date) = NaiveDate::from_ymd_opt(1899, 12, 30)
            .and_then(|date| date.checked_add_signed(Duration::days(days)))
    {
        return Ok(date.format("%Y-%m-%d").to_string());
    }
    Err(invalid("日期须为有效的 YYYY-MM-DD"))
}
