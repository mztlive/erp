use std::collections::HashMap;

use serde_json::Value;

use super::invalid;
use crate::Result;
use crate::data_transfer::quotes::HEADERS;
use crate::data_transfer::workbook::text;
use crate::sales_inquiry_import::parse::Row;

#[derive(Clone, Copy)]
pub(super) enum Template {
    V2,
    V3,
    V4,
}

#[derive(Clone, Copy)]
pub(super) enum DeliverySource {
    ReplyColumn(usize),
    Requirement,
}

#[derive(Clone, Copy)]
pub(super) struct Layout {
    pub note: usize,
    pub identity: usize,
    pub freight: usize,
    pub delivery: DeliverySource,
    expiry: Option<usize>,
}

pub(super) struct Metadata {
    pub supplier: String,
    pub layout: Layout,
}

impl Template {
    fn parse(marker: &str) -> Result<Self> {
        match marker {
            "supplier-inquiry-quote-v2" => Ok(Self::V2),
            "supplier-inquiry-quote-v3" => Ok(Self::V3),
            "supplier-inquiry-quote-v4" => Ok(Self::V4),
            _ => Err(invalid("报价表版本不支持，请重新生成")),
        }
    }

    pub(super) fn layout(self) -> Layout {
        match self {
            Self::V2 => Layout {
                note: 16,
                identity: 17,
                freight: 14,
                delivery: DeliverySource::ReplyColumn(13),
                expiry: Some(15),
            },
            Self::V3 => Layout {
                note: 15,
                identity: 16,
                freight: 13,
                delivery: DeliverySource::Requirement,
                expiry: Some(14),
            },
            Self::V4 => Layout {
                note: 14,
                identity: 15,
                freight: 13,
                delivery: DeliverySource::Requirement,
                expiry: None,
            },
        }
    }
}

impl Layout {
    pub(super) fn check_headers(self, rows: &[Row]) -> Result<()> {
        if rows.iter().any(|row| row.number > 204) {
            return Err(invalid("报价表超过 200 条商品"));
        }
        let mut headers = HEADERS.to_vec();
        if let DeliverySource::ReplyColumn(index) = self.delivery {
            headers.insert(index, "承诺交付日");
        }
        if let Some(index) = self.expiry {
            headers.insert(index, "报价失效日");
        }
        let row = rows.iter().find(|row| row.number == 4).ok_or_else(|| invalid("报价表缺少表头"))?;
        if headers.iter().enumerate().any(|(index, header)| cell(row, index) != *header) {
            return Err(invalid("报价表列已变化，请保留原模板"));
        }
        Ok(())
    }

    pub(super) fn is_blank(self, row: &Row) -> bool {
        (10..self.identity).all(|index| cell(row, index).is_empty())
    }
}

pub(super) fn metadata(sheets: &HashMap<String, Vec<Row>>, inquiry: &Value) -> Result<Metadata> {
    let rows = sheets.get("回导信息").ok_or_else(|| invalid("缺少回导信息，请重新生成报价表"))?;
    let at = |number| rows.iter().find(|row| row.number == number).map_or("", |row| cell(row, 0));
    let template = Template::parse(at(1))?;
    if at(2) != text(inquiry, "id")
        || at(3).parse::<u64>().ok() != inquiry["requirement_version"].as_u64()
        || at(4) != (inquiry["feedback_round"].as_u64().unwrap_or_default() + 1).to_string()
    {
        return Err(invalid("报价表不属于当前询价、需求版本或反馈轮次"));
    }
    if at(5).is_empty() {
        return Err(invalid("缺少供应商标识"));
    }
    Ok(Metadata { supplier: at(5).into(), layout: template.layout() })
}

pub(super) fn cell(row: &Row, index: usize) -> &str {
    row.cells.get(index).map_or("", |cell| cell.value.as_str())
}
