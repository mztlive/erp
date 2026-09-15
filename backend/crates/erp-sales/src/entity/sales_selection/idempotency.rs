//! 幂等记录：同键同返，同键异拒。
//!
//! 覆盖该册公开访问有效期，方案与唯一关系按业务记录保留。

use entity_core::BaseModel;
use entity_macros::Entity;
use erp_core::ids::{SalesSelectionBookletId, SalesSelectionIdempotencyId};
use erp_core::{Error, Result};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

/// 幂等操作域。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum IdempotencyOperation {
    /// 创建选品册。
    Create,
    /// 准备（含重生成与重新准备）。
    Prepare,
    /// 发布。
    Publish,
    /// 保存会话。
    SaveSession,
    /// 提交。
    Submit,
    /// 更换链接。
    RotateLink,
    /// 关闭。
    Close,
    /// 撤销访问。
    RevokeAccess,
    /// 作废。
    Void,
}

impl IdempotencyOperation {
    /// 返回稳定代码。
    ///
    /// # 参数
    /// 无。
    ///
    /// # 返回
    /// 返回持久化与索引使用的代码。
    ///
    /// # 错误
    /// 无。
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Create => "CREATE",
            Self::Prepare => "PREPARE",
            Self::Publish => "PUBLISH",
            Self::SaveSession => "SAVE_SESSION",
            Self::Submit => "SUBMIT",
            Self::RotateLink => "ROTATE_LINK",
            Self::Close => "CLOSE",
            Self::RevokeAccess => "REVOKE_ACCESS",
            Self::Void => "VOID",
        }
    }

    /// 重试时是否回读选品册当前事实，而不是返回落库时的摘要。
    ///
    /// 创建已包含首次准备排队，摘要会过时；发布后令牌也可能再次变化。
    ///
    /// # 参数
    /// 无。
    ///
    /// # 返回
    /// 册级写操作返回 `true`。
    ///
    /// # 错误
    /// 无。
    pub fn replays_live_booklet(self) -> bool {
        matches!(
            self,
            Self::Create
                | Self::Prepare
                | Self::Publish
                | Self::RotateLink
                | Self::Close
                | Self::RevokeAccess
                | Self::Void
        )
    }
}

/// 幂等记录创建数据。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SalesSelectionIdempotencyData {
    /// 操作域。
    pub operation: IdempotencyOperation,
    /// 作用域（调用方身份或公开令牌版本锚点）。
    pub scope_id: String,
    /// 幂等键（调用方已校验长度）。
    pub key: String,
    /// 请求载荷哈希。
    pub request_hash: String,
    /// 原操作事实摘要（JSON）。
    pub result_json: String,
    /// 记录时的令牌版本。
    pub token_version: Option<u32>,
    /// 关联选品册。
    pub booklet_id: Option<SalesSelectionBookletId>,
}

/// 幂等记录实体.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Entity)]
pub struct SalesSelectionIdempotency {
    #[serde(flatten)]
    /// 持久化元数据。
    pub base: BaseModel,
    /// 操作域。
    pub operation: IdempotencyOperation,
    /// 作用域（调用方身份或公开令牌版本锚点）。
    pub scope_id: String,
    /// 幂等键。
    pub idempotency_key: String,
    /// 请求载荷哈希。
    pub request_hash: String,
    /// 原操作事实摘要（JSON）。
    pub result_json: String,
    /// 记录时的令牌版本；令牌变化后重试不重新启用旧令牌。
    pub token_version: Option<u32>,
    /// 关联选品册。
    pub booklet_id: Option<SalesSelectionBookletId>,
}

impl SalesSelectionIdempotency {
    /// 创建幂等记录。
    ///
    /// # 参数
    /// * `id` - 记录稳定身份
    /// * `data` - 创建字段
    ///
    /// # 返回
    /// 返回新记录。
    ///
    /// # 错误
    /// 无。键长度由调用方经 `normalize_idempotency_key` 校验。
    pub fn new(id: SalesSelectionIdempotencyId, data: SalesSelectionIdempotencyData) -> Self {
        Self {
            base: BaseModel::new(id.to_string()),
            operation: data.operation,
            scope_id: data.scope_id,
            idempotency_key: data.key.trim().to_string(),
            request_hash: data.request_hash,
            result_json: data.result_json,
            token_version: data.token_version,
            booklet_id: data.booklet_id,
        }
    }

    /// 判断是否为相同请求。
    ///
    /// # 参数
    /// * `request_hash` - 本次请求哈希
    ///
    /// # 返回
    /// 相同载荷返回 `true`。
    ///
    /// # 错误
    /// 无。
    pub fn is_same_request(&self, request_hash: &str) -> bool {
        self.request_hash == request_hash
    }

    /// 校验同键同载荷。
    ///
    /// # 参数
    /// * `request_hash` - 本次请求哈希
    ///
    /// # 返回
    /// 一致返回 `Ok(())`。
    ///
    /// # 错误
    /// 同键不同请求时拒绝。
    pub fn ensure_same_request(&self, request_hash: &str) -> Result<()> {
        if self.is_same_request(request_hash) {
            return Ok(());
        }
        Err(Error::from("相同幂等键的请求载荷不一致"))
    }
}

/// 计算请求载荷哈希。
///
/// # 参数
/// * `payload` - 规范化后的请求载荷
///
/// # 返回
/// 返回十六进制哈希。
///
/// # 错误
/// 无。
pub fn request_hash(payload: &str) -> String {
    hex::encode(Sha256::digest(payload.as_bytes()))
}

#[cfg(test)]
mod tests {
    use erp_core::ids::SalesSelectionIdempotencyId;

    use super::{
        IdempotencyOperation, SalesSelectionIdempotency, SalesSelectionIdempotencyData, request_hash,
    };

    fn record() -> SalesSelectionIdempotency {
        SalesSelectionIdempotency::new(
            SalesSelectionIdempotencyId::new("r1"),
            SalesSelectionIdempotencyData {
                operation: IdempotencyOperation::Submit,
                scope_id: "actor-1".into(),
                key: "k1".into(),
                request_hash: request_hash("a"),
                result_json: "{}".into(),
                token_version: None,
                booklet_id: None,
            },
        )
    }

    #[test]
    fn same_key_same_return_and_conflict() {
        let record = record();
        assert_eq!(record.operation.as_str(), "SUBMIT");
        assert!(record.ensure_same_request(&request_hash("a")).is_ok());
        assert!(record.ensure_same_request(&request_hash("b")).is_err());
    }

    #[test]
    fn operations_have_stable_codes() {
        assert_eq!(IdempotencyOperation::Create.as_str(), "CREATE");
        assert_eq!(IdempotencyOperation::SaveSession.as_str(), "SAVE_SESSION");
    }

    #[test]
    fn booklet_writes_replay_live_state() {
        assert!(IdempotencyOperation::Create.replays_live_booklet());
        assert!(IdempotencyOperation::Prepare.replays_live_booklet());
        assert!(IdempotencyOperation::Publish.replays_live_booklet());
        assert!(IdempotencyOperation::RotateLink.replays_live_booklet());
        assert!(IdempotencyOperation::Close.replays_live_booklet());
        assert!(IdempotencyOperation::RevokeAccess.replays_live_booklet());
        assert!(IdempotencyOperation::Void.replays_live_booklet());
        assert!(!IdempotencyOperation::SaveSession.replays_live_booklet());
        assert!(!IdempotencyOperation::Submit.replays_live_booklet());
    }
}
