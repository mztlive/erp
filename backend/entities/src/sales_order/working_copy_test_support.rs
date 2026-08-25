//! 工作副本表头测试与行测试共享的测试数据解析/构造辅助（仅测试构建）。
//!
//! 只包含跨 `working_copy::tests` 与 `working_copy_line::tests` 复用的
//! 金额/税率/单价解析与行数据构造；`qty`、`goods_line` 仅被本模块内部调用，
//! 保持私有；`header_data` 只被表头测试使用，留在 `working_copy.rs` 的测试内。
//! 本模块不包含任何生产逻辑。

use std::str::FromStr;

use crate::common::time::Instant;
use crate::ids::{SalesOrderLineId, SkuId, SkuRevisionId};
use crate::money::{Amount, Quantity, Rate, UnitPrice};

use super::types::{GoodsLineFields, LineType, WelfareScenario};
use super::working_copy_line::SalesOrderWorkingCopyLineData;

pub(super) fn amt(value: &str) -> Amount {
    Amount::from_str(value).unwrap()
}

pub(super) fn rate(value: &str) -> Rate {
    Rate::from_str(value).unwrap()
}

fn qty(value: &str) -> Quantity {
    Quantity::from_str(value).unwrap()
}

pub(super) fn price(value: &str) -> UnitPrice {
    UnitPrice::from_str(value).unwrap()
}

fn goods_line() -> GoodsLineFields {
    GoodsLineFields {
        sku_id: SkuId::new("sku-1"),
        sku_revision_id: SkuRevisionId::new("skurev-1"),
        welfare_scenario: Some(WelfareScenario::AnnualGiftBag),
        service_region: Some("east".to_string()),
        fulfillment_due_at: Instant::from_unix_secs(1_800_000_000),
        quantity: qty("3.000000"),
        base_unit_code: "箱".to_string(),
        unit_price_gross: price("9.9900"),
    }
}

pub(super) fn line_data(line_no: u32) -> SalesOrderWorkingCopyLineData {
    SalesOrderWorkingCopyLineData {
        sales_order_line_id: SalesOrderLineId::new(format!("line-{line_no}")),
        line_no,
        line_type: LineType::GoodsService,
        sales_tax_rate: rate("0.130000"),
        item_name_snapshot: " 年货礼盒 ".to_string(),
        spec_snapshot: Some(" 10kg ".to_string()),
        unit_snapshot: Some(" 箱 ".to_string()),
        goods: Some(goods_line()),
        voucher: None,
    }
}
