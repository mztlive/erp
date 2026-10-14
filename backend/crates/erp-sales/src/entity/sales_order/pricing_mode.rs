//! 销售成交单价的编辑方式；历史行保留手工成交价。

use erp_core::money::UnitPrice;
use erp_core::{Error, Result};
use serde::{Deserialize, Serialize};

/// 销售草稿成交价采用数量参考价或用户手工价格。
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum SalesPricingMode {
    /// 根据精确 SKU 修订和销售数量匹配含税参考价。
    Auto,
    /// 保留用户录入的成交单价；缺省历史行使用该模式。
    #[default]
    Manual,
    /// 赠品明细：成交单价固定为 0，按普通明细选源、履约和验收，不形成应收（§4.4.4）。
    Gift,
}

impl SalesPricingMode {
    /// 判断是否保留用户成交价。
    ///
    /// # 参数
    /// 无。
    ///
    /// # 返回
    /// 手工模式返回 `true`。
    ///
    /// # 错误
    /// 无。
    pub fn is_manual(&self) -> bool {
        *self == Self::Manual
    }

    /// 判断是否为赠品明细。
    ///
    /// # 参数
    /// 无。
    ///
    /// # 返回
    /// 赠品模式返回 `true`。
    ///
    /// # 错误
    /// 无。
    pub fn is_gift(&self) -> bool {
        *self == Self::Gift
    }

    /// 校验成交单价与赠品标记一致：赠品必须为 0，非赠品必须大于 0。
    ///
    /// # 参数
    /// * `unit_price_gross` - 含税成交单价
    ///
    /// # 返回
    /// 一致时返回 `Ok(())`。
    ///
    /// # 错误
    /// 赠品单价不为 0，或非赠品单价不大于 0 时返回错误。
    pub fn ensure_price(&self, unit_price_gross: UnitPrice) -> Result<()> {
        let price = unit_price_gross.to_decimal();
        match (self.is_gift(), price.is_zero()) {
            (true, false) => Err(Error::from("赠品明细的含税成交单价必须为 0")),
            (false, _) if price <= rust_decimal::Decimal::ZERO => {
                Err(Error::from("含税成交单价必须大于 0；随单赠送的商品请标记为赠品"))
            },
            _ => Ok(()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::types::GoodsLineFields;
    use super::super::working_copy_test_support::line_data;
    use super::*;

    /// 历史缺省模式使用手工价，并保持已有序列化及命令载荷形状。
    #[test]
    fn historical_goods_default_to_manual_without_serialization_drift() {
        let goods = line_data(1).goods.unwrap();
        let historical = serde_json::to_value(&goods).unwrap();
        assert!(historical.get("pricing_mode").is_none());
        let restored: GoodsLineFields = serde_json::from_value(historical.clone()).unwrap();
        assert_eq!(restored.pricing_mode, SalesPricingMode::Manual);
        assert_eq!(serde_json::to_value(restored).unwrap(), historical);
        let automatic = GoodsLineFields { pricing_mode: SalesPricingMode::Auto, ..goods.clone() };
        assert_eq!(serde_json::to_value(automatic).unwrap()["pricing_mode"], "AUTO");
        let gift = GoodsLineFields { pricing_mode: SalesPricingMode::Gift, ..goods };
        assert_eq!(serde_json::to_value(gift).unwrap()["pricing_mode"], "GIFT");
    }

    #[test]
    fn gift_price_must_be_zero_and_others_positive() {
        use std::str::FromStr;
        let zero = UnitPrice::from_str("0").unwrap();
        let positive = UnitPrice::from_str("9.90").unwrap();
        assert!(SalesPricingMode::Gift.ensure_price(zero).is_ok());
        assert!(SalesPricingMode::Gift.ensure_price(positive).is_err());
        assert!(SalesPricingMode::Manual.ensure_price(positive).is_ok());
        assert!(SalesPricingMode::Manual.ensure_price(zero).is_err());
        assert!(SalesPricingMode::Auto.ensure_price(zero).is_err());
    }
}
