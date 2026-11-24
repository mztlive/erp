//! 销售成交单价的编辑方式；历史行保留手工成交价。

use serde::{Deserialize, Deserializer, Serialize};

/// 销售草稿成交价采用数量参考价或用户手工价格。
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum SalesPricingMode {
    /// 根据精确 SKU 修订和销售数量匹配含税参考价。
    Auto,
    /// 保留用户录入的成交单价；缺省历史行使用该模式。
    #[default]
    Manual,
}

impl SalesPricingMode {
    /// 判断是否保留用户成交价。
    /// 结果：手工模式返回 `true`。
    pub fn is_manual(&self) -> bool {
        *self == Self::Manual
    }

    // 读出旧单据时统一使用手工模式，保留已记录的成交单价和金额。
    pub(crate) fn deserialize_stored<'de, D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(remote = "SalesPricingMode", rename_all = "SCREAMING_SNAKE_CASE")]
        enum StoredPricingMode {
            Auto,
            #[serde(alias = "GIFT")]
            Manual,
        }

        StoredPricingMode::deserialize(deserializer)
    }
}

#[cfg(test)]
mod tests {
    use super::super::types::GoodsLineFields;
    use super::super::working_copy_test_support::line_data;
    use super::*;

    #[test]
    fn historical_goods_default_to_manual_without_serialization_drift() {
        let goods = line_data(1).goods.unwrap();
        let historical = serde_json::to_value(&goods).unwrap();
        assert!(historical.get("pricing_mode").is_none());
        let restored: GoodsLineFields = serde_json::from_value(historical.clone()).unwrap();
        assert_eq!(restored.pricing_mode, SalesPricingMode::Manual);
        assert_eq!(serde_json::to_value(restored).unwrap(), historical);

        let automatic = GoodsLineFields { pricing_mode: SalesPricingMode::Auto, ..goods };
        assert_eq!(serde_json::to_value(automatic).unwrap()["pricing_mode"], "AUTO");
    }

    #[test]
    fn legacy_mode_is_read_from_stored_lines_and_rejected_in_requests() {
        use crate::entity::sales_order::{
            SalesOrderWorkingCopyId, SalesOrderWorkingCopyLine, SalesOrderWorkingCopyLineId,
        };

        let mut data = line_data(1);
        let zero = "0".parse().unwrap();
        data.goods.as_mut().unwrap().unit_price_gross = zero;
        let line = SalesOrderWorkingCopyLine::new(
            SalesOrderWorkingCopyLineId::new("line"),
            SalesOrderWorkingCopyId::new("draft"),
            data,
        )
        .unwrap();
        let mut stored = bson::serialize_to_document(&line).unwrap();
        stored.insert("pricing_mode", "GIFT");

        let restored: SalesOrderWorkingCopyLine = bson::deserialize_from_document(stored).unwrap();
        assert_eq!(restored, line);
        assert_eq!(restored.pricing_mode, SalesPricingMode::Manual);
        assert_eq!(restored.unit_price_gross, Some(zero));
        assert!(serde_json::from_value::<SalesPricingMode>(serde_json::json!("GIFT")).is_err());
    }
}
