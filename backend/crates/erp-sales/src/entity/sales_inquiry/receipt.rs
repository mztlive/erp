//! 询价专用不可变命令回执，与业务和任务同事务写入。
use application_core::CommandFingerprint;
use entity_core::BaseModel;
use entity_macros::Entity;
use serde::{Deserialize, Serialize};

use super::{InquiryStatus, validate_text};
use crate::{Error, Result};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum InquiryCommandResult {
    Created {
        inquiry_id: String,
    },
    Mutation {
        inquiry_id: String,
        version: u64,
        requirement_version: u32,
        feedback_round: u32,
        status: InquiryStatus,
        task_ids: Vec<String>,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Entity)]
pub struct InquiryCommandReceipt {
    #[serde(flatten)]
    pub base: BaseModel,
    pub schema_version: u32,
    pub actor_id: String,
    pub action: String,
    pub payload_fingerprint: String,
    pub idempotency_key_hash: CommandFingerprint,
    pub result: InquiryCommandResult,
}

impl InquiryCommandReceipt {
    /// 构造不保存原始幂等键的成功回执。
    /// # 参数
    /// `id`、`actor`、`key`、`action` 确定命令身份；`fingerprint` 为规范化载荷 SHA256；`result` 为原结果。
    /// # 返回
    /// 返回独立强类型回执。
    /// # 错误
    /// 身份或指纹格式不正确时拒绝。
    pub fn new(
        id: String,
        actor: &str,
        key: &str,
        action: &str,
        fingerprint: String,
        result: InquiryCommandResult,
    ) -> Result<Self> {
        validate_text(&id, "命令身份", 128)?;
        validate_text(actor, "操作人", 128)?;
        validate_text(key, "幂等键", 256)?;
        validate_text(action, "命令动作", 128)?;
        if fingerprint.len() != 64 || !fingerprint.bytes().all(|byte| byte.is_ascii_hexdigit()) {
            return Err(Error::ValidationError("命令载荷指纹非法".into()));
        }
        let receipt = Self {
            base: BaseModel::new(id),
            schema_version: 1,
            actor_id: actor.into(),
            action: action.into(),
            payload_fingerprint: fingerprint,
            idempotency_key_hash: CommandFingerprint::from_parts([key.trim().to_string()]),
            result,
        };
        receipt.validate()?;
        Ok(receipt)
    }

    /// 核对独立回执的存储版本、结构和强类型结果。
    /// # 参数
    /// 无。
    /// # 返回
    /// 回执可安全用于恢复核对时通过。
    /// # 错误
    /// 未知schema、软删、损坏指纹或结果身份缺失时拒绝，不能当作未执行。
    pub fn validate(&self) -> Result<()> {
        let (id, valid_result) = match &self.result {
            InquiryCommandResult::Created { inquiry_id } => (inquiry_id, true),
            InquiryCommandResult::Mutation {
                inquiry_id,
                version,
                requirement_version,
                feedback_round,
                task_ids,
                ..
            } => (
                inquiry_id,
                *version > 0
                    && (*feedback_round == 0 || *requirement_version > 0)
                    && task_ids.iter().all(|id| !id.trim().is_empty()),
            ),
        };
        if self.schema_version != 1
            || self.base.is_deleted()
            || self.base.id.trim().is_empty()
            || self.actor_id.trim().is_empty()
            || self.action.trim().is_empty()
            || id.trim().is_empty()
            || !valid_result
            || self.payload_fingerprint.len() != 64
            || !self.payload_fingerprint.bytes().all(|byte| byte.is_ascii_hexdigit())
        {
            return Err(Error::Internal("询价命令回执格式无效，禁止重新执行".into()));
        }
        CommandFingerprint::parse(self.idempotency_key_hash.as_str())?;
        Ok(())
    }

    /// 验证回放命令的完整身份和载荷指纹。
    /// # 参数
    /// `actor`、`key`、`action`、`fingerprint` 来自本次规范化命令。
    /// # 返回
    /// 一致时返回原强类型结果引用。
    /// # 错误
    /// 身份不一致或同键异载荷时拒绝。
    pub fn matches(
        &self,
        actor: &str,
        key: &str,
        action: &str,
        fingerprint: &str,
    ) -> Result<&InquiryCommandResult> {
        self.validate()?;
        if self.base.is_deleted()
            || self.actor_id != actor
            || self.action != action
            || self.idempotency_key_hash != CommandFingerprint::from_parts([key.trim().to_string()])
        {
            return Err(Error::ConflictError("询价命令回执与当前命令不一致".into()));
        }
        if self.payload_fingerprint != fingerprint {
            return Err(Error::ConflictError("同一操作号已用于不同询价内容".into()));
        }
        Ok(&self.result)
    }
}
