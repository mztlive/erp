//! 销售开单的公司商品池取价方式，不决定采购或履约路线。

use serde::{Deserialize, Serialize};

/// 采购供给参考价所选的公司 SKU 价格档位。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum SalesProcurementPriceBasis {
    /// 公司商品池集采价。
    Bulk,
    /// 公司商品池一件代发价。
    Dropship,
}

#[cfg(test)]
mod tests {
    use crate::entity::sales_order::GoodsLineFields;
    use crate::entity::sales_order::working_copy_test_support::line_data;

    #[test]
    fn historical_goods_preserve_price_and_unknown_basis_is_rejected() {
        let goods = line_data(1).goods.unwrap();
        let mut value = serde_json::to_value(&goods).unwrap();
        assert!(value.get("procurement_price_basis").is_none());
        let restored: GoodsLineFields = serde_json::from_value(value.clone()).unwrap();
        assert_eq!(restored.procurement_price_basis, None);
        assert_eq!(restored.procurement_supply_price_gross, goods.procurement_supply_price_gross);
        value["procurement_price_basis"] = "INVALID".into();
        assert!(serde_json::from_value::<GoodsLineFields>(value).is_err());
    }
}
