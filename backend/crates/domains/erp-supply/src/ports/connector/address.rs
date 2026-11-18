//! 供应商地址要求与已确认的四级地址；不得用 ERP 地区身份猜测外部编码。

use async_trait::async_trait;
use serde::{Deserialize, Serialize};

use super::common::{ConnectorResult, Snapshot};
use super::order::CoordinateSystem;
use crate::{Error, Result};

/// 明确地址准备要求，流程不得对所有供应商统一要求地图坐标。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(tag = "type", content = "coordinate_system", rename_all = "snake_case")]
pub enum AddressRequirement {
    Text,
    Geocoded(CoordinateSystem),
    FourLevel,
}

/// 地址候选按供应商自己的层级逐级查询，禁止代入 ERP 地区编码。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum SupplierAddressDepth {
    Province,
    City,
    County,
    Town,
}

/// 省级无父编码，其他层级必须来自上一层的已核实候选。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AddressCandidateQuery {
    pub depth: SupplierAddressDepth,
    pub parent_code: Option<String>,
}

/// 候选仅表示来源返回，不等于用户已确认的执行地址。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SupplierAddressCandidate {
    pub depth: SupplierAddressDepth,
    pub code: String,
    pub name: String,
    pub parent_code: Option<String>,
}

/// 供应商原生地址候选及下单前有效性核验的窄读取接口。
#[async_trait]
pub trait SupplierAddressSource: Send + Sync {
    /// 读取一个明确层级的全部候选，不把地址转换结果冒充已确认地址。
    /// 可供人工逐级选择的来源候选及读取证据。
    /// 父级缺失、编码混用、授权不足或读取失败时返回分类错误。
    /// `query` 为层级及供应商父编码；连接身份在适配器实例中绑定。
    async fn candidates(
        &self,
        query: &AddressCandidateQuery,
    ) -> ConnectorResult<Snapshot<Vec<SupplierAddressCandidate>>>;

    /// 使用来源接口重新核验已经由用户确认的四级地址。
    /// 来源明确有效或失效的当前证据，不修改已确认地址。
    /// 连接不符、资料不完整、来源状态未知或读取失败时拒绝。
    /// `address` 为原加密执行快照中与本连接绑定的地址。
    async fn validate(&self, address: &ConfirmedSupplierAddress) -> ConnectorResult<Snapshot<bool>>;
}

/// 供应商的一个地址层级；编码及名称均来自其明确候选。
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SupplierAddressLevel {
    pub code: String,
    pub name: String,
}

/// 明确确认的四级地址，存于加密执行快照，不派生 Debug。
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ConfirmedSupplierAddress {
    pub connection_id: String,
    pub snapshot_reference: String,
    pub address_fingerprint: String,
    pub version: u64,
    pub province: SupplierAddressLevel,
    pub city: SupplierAddressLevel,
    pub county: SupplierAddressLevel,
    pub town: SupplierAddressLevel,
}

impl ConfirmedSupplierAddress {
    /// 校验已确认地址绑定本连接、原执行快照和收件地址。
    /// 四级资料完整且精确匹配时允许后续只读核验。
    /// 缺失层级、版本无效或绑定变化时阻断，不能复用旧转换结果。
    /// 连接身份、快照引用和地址指纹均由服务端从原执行取得。
    pub fn validate_binding(&self, connection: &str, snapshot: &str, fingerprint: &str) -> Result<()> {
        if self.version == 0
            || self.connection_id.is_empty()
            || self.snapshot_reference.is_empty()
            || self.address_fingerprint.is_empty()
            || self.connection_id != connection
            || self.snapshot_reference != snapshot
            || self.address_fingerprint != fingerprint
        {
            return Err(Error::BusinessLogicError("供应商收货地址已变化，请重新确认四级地址".into()));
        }

        for level in [&self.province, &self.city, &self.county, &self.town] {
            if level.code.trim().is_empty() || level.name.trim().is_empty() {
                return Err(Error::ValidationError("请完整确认供应商四级收货地址".into()));
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn address() -> ConfirmedSupplierAddress {
        let level = |code: &str| SupplierAddressLevel { code: code.into(), name: code.into() };
        ConfirmedSupplierAddress {
            connection_id: "connection".into(),
            snapshot_reference: "snapshot".into(),
            address_fingerprint: "address-hash".into(),
            version: 1,
            province: level("1"),
            city: level("2"),
            county: level("3"),
            town: level("4"),
        }
    }

    #[test]
    fn four_level_address_is_bound_to_connection_snapshot_and_content() {
        let value = address();
        value.validate_binding("connection", "snapshot", "address-hash").unwrap();
        assert!(value.validate_binding("other", "snapshot", "address-hash").is_err());
        assert!(value.validate_binding("connection", "new-snapshot", "address-hash").is_err());
        assert!(value.validate_binding("connection", "snapshot", "changed").is_err());

        let mut incomplete = value;
        incomplete.town.code.clear();
        assert!(incomplete.validate_binding("connection", "snapshot", "address-hash").is_err());
    }
}
