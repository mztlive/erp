//! 命令幂等收据。相同键同载荷回读，不同载荷冲突。

use entity_core::BaseModel;
use entity_macros::Entity;
use serde::{Deserialize, Serialize};

use crate::ids::ApprovalCommandReceiptId;
use crate::model::types::{
    base_model_at, normalize_required, ApprovalCommandKind, ModelError, ModelResult, DIGEST_MAX_LEN,
    SCOPE_MAX_LEN,
};
use crate::model::Timestamp;

/// 命令执行收据。
#[derive(Debug, Serialize, Deserialize, Clone, Entity, PartialEq, Eq)]
pub struct ApprovalCommandReceipt {
    #[serde(flatten)]
    pub base: BaseModel,
    /// 命令种类。
    pub command_kind: ApprovalCommandKind,
    /// 作用域 ID。
    pub scope_id: String,
    /// 调用方幂等键。
    pub idempotency_key: String,
    /// 规范化请求摘要。
    pub payload_digest: String,
    /// 不可变结果引用。
    pub result_ref: String,
    /// 创建时间。
    pub created_at: Timestamp,
}

impl ApprovalCommandReceipt {
    /// 创建命令收据。
    ///
    /// # 参数
    /// * `id` - 收据主键
    /// * `command_kind` - 命令种类
    /// * `scope_id` - 作用域
    /// * `idempotency_key` - 幂等键
    /// * `payload_digest` - 请求摘要
    /// * `result_ref` - 结果引用
    /// * `at` - 创建时间
    ///
    /// # 错误
    /// 作用域、幂等键、摘要或结果引用非法时返回错误。
    pub fn new(
        id: ApprovalCommandReceiptId,
        command_kind: ApprovalCommandKind,
        scope_id: impl Into<String>,
        idempotency_key: impl Into<String>,
        payload_digest: impl Into<String>,
        result_ref: impl Into<String>,
        at: Timestamp,
    ) -> ModelResult<Self> {
        Ok(Self {
            base: base_model_at(id.to_string(), at)?,
            command_kind,
            scope_id: normalize_required(scope_id, "作用域不能为空", SCOPE_MAX_LEN, "作用域过长")?,
            idempotency_key: normalize_required(
                idempotency_key,
                "幂等键不能为空",
                SCOPE_MAX_LEN,
                "幂等键过长",
            )?,
            payload_digest: normalize_required(
                payload_digest,
                "请求摘要不能为空",
                DIGEST_MAX_LEN,
                "请求摘要过长",
            )?,
            result_ref: normalize_required(result_ref, "结果引用不能为空", SCOPE_MAX_LEN, "结果引用过长")?,
            created_at: at,
        })
    }

    /// 按相同键回读：摘要相同返回自身，摘要不同冲突。
    ///
    /// # 参数
    /// * `payload_digest` - 本次请求摘要
    ///
    /// # 返回
    /// 摘要相同时返回本收据。
    ///
    /// # 错误
    /// 摘要不同返回 [`ModelError::CommandReceiptConflict`]。
    pub fn reconcile(&self, payload_digest: &str) -> ModelResult<&Self> {
        if self.payload_digest == payload_digest {
            return Ok(self);
        }
        Err(ModelError::CommandReceiptConflict)
    }
}

#[cfg(test)]
mod tests {
    use super::ApprovalCommandReceipt;
    use crate::ids::ApprovalCommandReceiptId;
    use crate::model::types::{ApprovalCommandKind, ModelError};
    use crate::model::Timestamp;

    fn receipt() -> ApprovalCommandReceipt {
        ApprovalCommandReceipt::new(
            ApprovalCommandReceiptId::new("r1"),
            ApprovalCommandKind::SubmitDecision,
            "inst-1",
            "key-1",
            "digest-a",
            "exec-1",
            Timestamp::from_unix_secs(1).unwrap(),
        )
        .unwrap()
    }

    /// 同载荷回读，不同载荷冲突。
    #[test]
    fn same_key_same_payload_reads_back() {
        let receipt = receipt();
        assert!(receipt.reconcile("digest-a").is_ok());
        assert_eq!(
            receipt.reconcile("digest-b"),
            Err(ModelError::CommandReceiptConflict)
        );
    }
}
