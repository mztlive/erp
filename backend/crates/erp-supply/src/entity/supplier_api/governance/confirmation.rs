//! 采购业务能力确认。

use entity_core::BaseModel;
use entity_macros::Entity;
use erp_core::common::time::Instant;
use erp_core::ids::{SupplierApiCapabilityId, SupplierApiConnectionId};
use erp_core::validation::{normalize_optional_text, normalize_required_text};
use erp_core::{Error, Result};
use serde::{Deserialize, Serialize};

use super::{
    ACTOR_ID_MAX_LEN, HASH_MAX_LEN, MAX_EVIDENCE_REFERENCES, OPERATION_ID_MAX_LEN, REASON_MAX_LEN,
    REFERENCE_MAX_LEN, required,
};
use crate::entity::supplier_api::{SupplierApiCapability, SupplierApiCapabilityCode};

/// 采购确认的业务能力需求结论。
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum BusinessCapabilityRequirement {
    /// 该连接业务上必须具备此能力。
    Required,
    /// 当前业务范围不需要此能力。
    NotRequired,
}

/// 追加式采购业务能力确认创建数据。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct BusinessCapabilityConfirmationData {
    /// 连接。
    pub connection_id: SupplierApiConnectionId,
    /// 被确认的能力实体。
    pub capability_id: SupplierApiCapabilityId,
    /// 被确认的固定能力代码。
    pub capability_code: SupplierApiCapabilityCode,
    /// 需求结论。
    pub requirement: BusinessCapabilityRequirement,
    /// 适用范围的不透明业务引用。
    pub applicability_reference: Option<String>,
    /// 追加证据引用；只保存引用，不保存证据正文。
    pub evidence_references: Vec<String>,
    /// 固定原因代码。
    pub reason_code: String,
    /// 提交时连接版本。
    pub connection_version: u64,
    /// 提交时能力版本。
    pub capability_version: u64,
    /// 稳定操作 ID。
    pub operation_id: String,
    /// 客户端幂等键的不可逆摘要。
    pub idempotency_key_hash: String,
    /// 完整请求的不可逆摘要，用于拒绝同键异参。
    pub request_fingerprint: String,
    /// 采购确认人。
    pub confirmed_by: String,
    /// 确认时间。
    pub confirmed_at: Instant,
}

/// 采购业务能力确认事实。
///
/// 同一连接/能力可持续追加确认；最新确认由时间和主键稳定排序决定。该实体没有
/// 更新入口，采购确认不能借此修改 [`crate::entity::supplier_api::SupplierApiCapability`]。
#[derive(Debug, Clone, Serialize, Deserialize, Entity, PartialEq, Eq)]
pub struct BusinessCapabilityConfirmation {
    #[serde(flatten)]
    pub base: BaseModel,
    pub connection_id: SupplierApiConnectionId,
    pub capability_id: SupplierApiCapabilityId,
    pub capability_code: SupplierApiCapabilityCode,
    pub requirement: BusinessCapabilityRequirement,
    pub applicability_reference: Option<String>,
    pub evidence_references: Vec<String>,
    pub reason_code: String,
    pub connection_version: u64,
    pub capability_version: u64,
    pub operation_id: String,
    pub idempotency_key_hash: String,
    pub request_fingerprint: String,
    pub confirmed_by: String,
    pub confirmed_at: Instant,
}

impl BusinessCapabilityConfirmation {
    /// 构造不可变业务确认事实。
    ///
    /// # Errors
    /// 必填字段为空、引用过长、证据超限或对象版本为零时返回错误。
    pub fn new(id: impl Into<String>, data: BusinessCapabilityConfirmationData) -> Result<Self> {
        if data.connection_version == 0 || data.capability_version == 0 {
            return Err(Error::from("业务确认的对象版本必须大于零"));
        }
        if data.evidence_references.len() > MAX_EVIDENCE_REFERENCES {
            return Err(Error::from("业务确认的证据引用不能超过20条"));
        }
        let applicability_reference =
            normalize_optional_text(data.applicability_reference, "适用范围引用", REFERENCE_MAX_LEN)?;
        let mut evidence_references = Vec::with_capacity(data.evidence_references.len());
        for reference in data.evidence_references {
            let normalized =
                normalize_required_text(reference, "证据引用不能为空", REFERENCE_MAX_LEN, "证据引用过长")?;
            if !evidence_references.contains(&normalized) {
                evidence_references.push(normalized);
            }
        }
        Ok(Self {
            base: BaseModel::new(id.into()),
            connection_id: data.connection_id,
            capability_id: data.capability_id,
            capability_code: data.capability_code,
            requirement: data.requirement,
            applicability_reference,
            evidence_references,
            reason_code: required(data.reason_code, "原因代码", REASON_MAX_LEN)?,
            connection_version: data.connection_version,
            capability_version: data.capability_version,
            operation_id: required(data.operation_id, "操作ID", OPERATION_ID_MAX_LEN)?,
            idempotency_key_hash: required(data.idempotency_key_hash, "幂等摘要", HASH_MAX_LEN)?,
            request_fingerprint: required(data.request_fingerprint, "请求摘要", HASH_MAX_LEN)?,
            confirmed_by: required(data.confirmed_by, "确认人", ACTOR_ID_MAX_LEN)?,
            confirmed_at: data.confirmed_at,
        })
    }

    /// 判断该确认是否仍覆盖当前能力配置。
    ///
    /// 能力必须被确认为业务必需，且确认版本等于当前能力版本或仅落后一版
    /// （同一命令启用能力会使版本递增一次）。
    ///
    /// # 参数
    /// * `capability` - 当前连接能力实体
    ///
    /// # 返回
    /// 采购确认仍可用于当前能力配置时返回 `true`。
    pub fn covers(&self, capability: &SupplierApiCapability) -> bool {
        if self.capability_code != capability.capability_code
            || self.requirement != BusinessCapabilityRequirement::Required
        {
            return false;
        }
        self.capability_version == capability.base.version
            || self.capability_version.checked_add(1) == Some(capability.base.version)
    }

    /// 从最新优先历史中返回指定能力代码的最近确认。
    ///
    /// # 参数
    /// * `confirmations` - 最新确认优先的追加式历史
    /// * `capability_code` - 固定能力代码
    ///
    /// # 返回
    /// 返回首个匹配确认；没有时返回 `None`。
    pub fn latest_for(confirmations: &[Self], capability_code: SupplierApiCapabilityCode) -> Option<&Self> {
        confirmations.iter().find(|confirmation| confirmation.capability_code == capability_code)
    }
}
