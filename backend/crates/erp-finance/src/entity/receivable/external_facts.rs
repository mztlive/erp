//! 财务消费的外域最小事实；组合层必须从提供方公开合同显式映射。

use serde::{Deserialize, Serialize};

/// 来源销售的业务性质快照，仅用于应收复核状态及销售差额规则。
///
/// 序列化保留销售业务性质的既有稳定值；不持有销售聚合或实时资料。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum SalesBusinessTypeFact {
    /// 卡券销售。
    Voucher,
    /// 实物及服务销售。
    GoodsService,
}

#[cfg(test)]
mod tests {
    use super::SalesBusinessTypeFact;

    #[test]
    fn sales_business_type_preserves_stable_serialization() {
        for (fact, wire) in [
            (SalesBusinessTypeFact::Voucher, "VOUCHER"),
            (SalesBusinessTypeFact::GoodsService, "GOODS_SERVICE"),
        ] {
            let value = serde_json::Value::String(wire.to_string());
            assert_eq!(serde_json::to_value(fact).unwrap(), value);
            assert_eq!(
                serde_json::from_value::<SalesBusinessTypeFact>(value).unwrap(),
                fact
            );
        }
    }
}
