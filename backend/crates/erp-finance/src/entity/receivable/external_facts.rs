//! 财务消费的外域最小事实；组合层必须从提供方公开合同显式映射。

use erp_core::ids::FileAssetId;
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

/// 复核证据的存在性及指定命令时点可用性事实。
///
/// 提供方适配器须按同一命令时点调用文件资产的权威可用性判断；
/// 不存在的文件不得伪造事实行，财务按证据输入顺序解释缺失与不可用。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReviewEvidenceAssetFact {
    /// 资产唯一标识。
    pub id: FileAssetId,
    /// 提供方在本次命令时点判断的可用性。
    pub usable: bool,
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
