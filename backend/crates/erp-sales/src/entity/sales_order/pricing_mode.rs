//! 销售成交单价的编辑方式；历史行保留手工成交价。

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
        let automatic = GoodsLineFields { pricing_mode: SalesPricingMode::Auto, ..goods };
        assert_eq!(serde_json::to_value(automatic).unwrap()["pricing_mode"], "AUTO");
    }
}
