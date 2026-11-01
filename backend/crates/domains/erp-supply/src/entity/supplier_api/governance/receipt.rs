//! 供应商连接治理命令幂等回执。

use entity_core::BaseModel;
use entity_macros::Entity;
use erp_core::ids::SupplierApiConnectionId;
use erp_core::validation::normalize_optional_text;
use erp_core::{Error, Result};
use serde::{Deserialize, Serialize};

use super::{ACTOR_ID_MAX_LEN, HASH_MAX_LEN, OPERATION_ID_MAX_LEN, SupplierConnectionAction, required};

/// 命令回执终态。
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum SupplierCommandOutcome {
    Succeeded,
    Processing,
    Rejected,
    Unknown,
}

/// 供应商连接命令回执创建数据。
///
/// # 用途
/// 将回执构造所需字段打包，供 [`SupplierConnectionCommandReceipt::new`] 一次性接收。
///
/// # 参数
/// 无
///
/// # 返回
/// 无
///
/// # 错误
/// 无
///
/// # 关键业务约束
/// 本结构不校验；连接版本、操作人、幂等摘要、请求摘要与审计号由构造函数校验。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SupplierConnectionCommandReceiptData {
    /// 连接。
    pub connection_id: SupplierApiConnectionId,
    /// 命令动作。
    pub action: SupplierConnectionAction,
    /// 操作人。
    pub actor_id: String,
    /// 客户端幂等键的不可逆摘要。
    pub idempotency_key_hash: String,
    /// 完整请求的不可逆摘要。
    pub request_fingerprint: String,
    /// 命令终态。
    pub outcome: SupplierCommandOutcome,
    /// 提交时连接版本。
    pub connection_version: u64,
    /// 后台任务 ID。
    pub job_id: Option<String>,
    /// 审计号。
    pub audit_event_id: String,
}

/// 供应商连接命令幂等回执。
#[derive(Debug, Clone, Serialize, Deserialize, Entity, PartialEq, Eq)]
pub struct SupplierConnectionCommandReceipt {
    #[serde(flatten)]
    pub base: BaseModel,
    pub connection_id: SupplierApiConnectionId,
    pub action: SupplierConnectionAction,
    pub actor_id: String,
    pub idempotency_key_hash: String,
    pub request_fingerprint: String,
    pub outcome: SupplierCommandOutcome,
    pub connection_version: u64,
    pub job_id: Option<String>,
    pub audit_event_id: String,
}

impl SupplierConnectionCommandReceipt {
    /// 构造不可变命令回执。
    ///
    /// # 用途
    /// 校验并规范化回执字段后创建不可变实体。
    ///
    /// # 参数
    /// * `id` - 回执主键
    /// * `data` - 回执字段
    ///
    /// # 返回
    /// 校验通过后的命令回执。
    ///
    /// # 错误
    /// 必填摘要、审计号为空或连接版本为零时返回错误。
    ///
    /// # 关键业务约束
    /// 连接版本必须大于零；不保存客户端原始幂等键或密钥正文。
    pub fn new(id: impl Into<String>, data: SupplierConnectionCommandReceiptData) -> Result<Self> {
        if data.connection_version == 0 {
            return Err(Error::from("命令回执的连接版本必须大于零"));
        }
        Ok(Self {
            base: BaseModel::new(id.into()),
            connection_id: data.connection_id,
            action: data.action,
            actor_id: required(data.actor_id, "操作人", ACTOR_ID_MAX_LEN)?,
            idempotency_key_hash: required(data.idempotency_key_hash, "幂等摘要", HASH_MAX_LEN)?,
            request_fingerprint: required(data.request_fingerprint, "请求摘要", HASH_MAX_LEN)?,
            outcome: data.outcome,
            connection_version: data.connection_version,
            job_id: normalize_optional_text(data.job_id, "后台任务ID", OPERATION_ID_MAX_LEN)?,
            audit_event_id: required(data.audit_event_id, "审计号", OPERATION_ID_MAX_LEN)?,
        })
    }
}
