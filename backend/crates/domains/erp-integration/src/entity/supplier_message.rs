//! 拉取消息的可靠接收记录；来源身份、补查处理和远端删除分别持久化。

use std::fmt;

use entity_core::BaseModel;
use entity_macros::Entity;
use erp_core::common::time::Instant;
use erp_core::ids::SupplierApiConnectionId;
use erp_core::validation::normalize_required_text;
use serde::{Deserialize, Serialize};

use super::supplier_callback::SupplierRefreshHint;
use crate::{Error, Result};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum SupplierMessageState {
    Received,
    Applying,
    Quarantined,
    AckPending,
    AckSending,
    AckUnknown,
    Acknowledged,
}

/// 同一来源消息号异载荷时保留首份冲突密文并停止处理。
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SupplierMessageConflict {
    pub fingerprint: String,
    pub body_encrypted: String,
    pub received_at: Instant,
}

#[derive(Clone, Serialize, Deserialize, Entity)]
pub struct SupplierPulledMessage {
    #[serde(flatten)]
    pub base: BaseModel,
    pub account_scope: String,
    pub connection_id: SupplierApiConnectionId,
    pub source_message_id: String,
    pub message_type: String,
    pub content_fingerprint: String,
    pub body_encrypted: String,
    pub refresh_hints: Vec<SupplierRefreshHint>,
    pub received_at: Instant,
    pub state: SupplierMessageState,
    pub processing_error: Option<String>,
    pub next_attempt_at: Option<Instant>,
    pub applied_at: Option<Instant>,
    pub acknowledged_at: Option<Instant>,
    pub conflict: Option<SupplierMessageConflict>,
}

/// 所有接收字段均由服务端已绑定连接及解码适配器提供。
pub struct SupplierMessageData {
    pub id: String,
    pub account_scope: String,
    pub connection_id: SupplierApiConnectionId,
    pub source_message_id: String,
    pub message_type: String,
    pub content_fingerprint: String,
    pub body_encrypted: String,
    pub refresh_hints: Vec<SupplierRefreshHint>,
    pub quarantine_reason: Option<String>,
    pub received_at: Instant,
}

impl SupplierPulledMessage {
    /// 构造待补查或隔离的拉取证据，不赋予回调认证语义。
    /// # 参数
    /// `data` 为绑定账号、来源消息身份、密文及刷新意图。
    /// # 返回
    /// 可以单文档原子保存的证据和处理意图。
    /// # 错误
    /// 身份、正文或意图不满足接收边界时拒绝。
    pub fn received(data: SupplierMessageData) -> Result<Self> {
        validate_data(&data)?;

        let state = if data.quarantine_reason.is_some() {
            SupplierMessageState::Quarantined
        } else {
            SupplierMessageState::Received
        };
        Ok(Self {
            base: BaseModel::new(data.id),
            account_scope: data.account_scope,
            connection_id: data.connection_id,
            source_message_id: data.source_message_id,
            message_type: data.message_type,
            content_fingerprint: data.content_fingerprint,
            body_encrypted: data.body_encrypted,
            refresh_hints: data.refresh_hints,
            received_at: data.received_at,
            state,
            processing_error: data.quarantine_reason,
            next_attempt_at: None,
            applied_at: None,
            acknowledged_at: None,
            conflict: None,
        })
    }

    /// 校验重复接收，异载荷保留证据并隔离，不覆盖原消息。
    /// # 参数
    /// `incoming` 为同一账号消息号的另一次接收。
    /// # 返回
    /// 同载荷返回 true；异载荷保存冲突并返回 false。
    /// # 错误
    /// 身份或连接不一致时拒绝，禁止借此转移消息归属。
    pub fn observe_replay(&mut self, incoming: &Self) -> Result<bool> {
        if self.account_scope != incoming.account_scope
            || self.connection_id != incoming.connection_id
            || self.source_message_id != incoming.source_message_id
        {
            return Err(Error::ConflictError("供应商消息账号或连接归属冲突".into()));
        }
        if self.content_fingerprint == incoming.content_fingerprint {
            return Ok(true);
        }

        if self.conflict.is_none() {
            self.conflict = Some(SupplierMessageConflict {
                fingerprint: incoming.content_fingerprint.clone(),
                body_encrypted: incoming.body_encrypted.clone(),
                received_at: incoming.received_at,
            });
        }
        self.state = SupplierMessageState::Quarantined;
        self.processing_error = Some("SOURCE_MESSAGE_PAYLOAD_CONFLICT".into());
        Ok(false)
    }

    /// 在补查前持久化正在处理状态；恢复时仍须按稳定消息身份幂等应用。
    /// # 参数
    /// 无。
    /// # 返回
    /// 允许 Received 或崩溃遗留的 Applying 进入补查。
    /// # 错误
    /// 已结束、未知删除或隔离记录不允许重新应用。
    pub fn begin_apply(&mut self) -> Result<()> {
        if !matches!(self.state, SupplierMessageState::Received | SupplierMessageState::Applying) {
            return Err(Error::BusinessLogicError("供应商消息不允许重新执行业务补查".into()));
        }
        self.state = SupplierMessageState::Applying;
        Ok(())
    }

    /// 所有补查意图已经应用后登记待确认，尚未删除远端消息。
    /// # 参数
    /// `at` 是本地应用完成时间。
    /// # 返回
    /// 状态变更为 AckPending。
    /// # 错误
    /// 非 Applying 状态不允许绕过业务处理。
    pub fn applied(&mut self, at: Instant) -> Result<()> {
        self.require(SupplierMessageState::Applying)?;
        self.state = SupplierMessageState::AckPending;
        self.applied_at = Some(at);
        self.processing_error = None;
        self.next_attempt_at = None;
        Ok(())
    }

    /// 在远端删除调用之前持久化已经开始发送，崩溃恢复必须按未知处理。
    /// # 参数
    /// 无。
    /// # 返回
    /// 状态变更为 AckSending。
    /// # 错误
    /// 非待确认状态拒绝重复删除。
    pub fn begin_acknowledge(&mut self) -> Result<()> {
        self.require(SupplierMessageState::AckPending)?;
        self.state = SupplierMessageState::AckSending;
        Ok(())
    }

    /// 保存已发送删除的结果，未知结果不能回到可重发状态。
    /// # 参数
    /// `confirmed` 为供应商明确确认；`at` 为本地观察时间。
    /// # 返回
    /// 明确结果变为 Acknowledged，其他结果保留 AckUnknown。
    /// # 错误
    /// 未发送删除的记录不能直接结束。
    pub fn acknowledged(&mut self, confirmed: bool, at: Instant) -> Result<()> {
        self.require(SupplierMessageState::AckSending)?;
        self.state =
            if confirmed { SupplierMessageState::Acknowledged } else { SupplierMessageState::AckUnknown };
        self.acknowledged_at = confirmed.then_some(at);
        self.processing_error = (!confirmed).then(|| "REMOTE_ACK_RESULT_UNKNOWN".into());
        Ok(())
    }

    /// 补查失败后保留处理意图并安排下一次补查。
    /// # 参数
    /// `next_attempt_at` 为有界退避后的时间；`code` 仅允许脱敏错误码。
    /// # 返回
    /// 维持 Applying，稳定消息身份保证恢复不制造第二次业务事实。
    /// # 错误
    /// 删除阶段不允许回到应用阶段。
    pub fn defer_application(&mut self, next_attempt_at: Instant, code: String) -> Result<()> {
        self.require(SupplierMessageState::Applying)?;
        self.processing_error = Some(normalize_required_text(code, "错误码不能为空", 128, "错误码过长")?);
        self.next_attempt_at = Some(next_attempt_at);
        Ok(())
    }

    fn require(&self, state: SupplierMessageState) -> Result<()> {
        if self.state != state {
            return Err(Error::BusinessLogicError("供应商消息处理状态不允许此操作".into()));
        }
        Ok(())
    }
}

fn validate_data(data: &SupplierMessageData) -> Result<()> {
    for (value, max) in [
        (data.id.as_str(), 128),
        (data.account_scope.as_str(), 256),
        (data.source_message_id.as_str(), 256),
        (data.message_type.as_str(), 128),
        (data.content_fingerprint.as_str(), 128),
        (data.body_encrypted.as_str(), 2 * 1024 * 1024),
    ] {
        if value.trim().is_empty() || value.len() > max {
            return Err(Error::ValidationError("供应商消息身份或证据长度无效".into()));
        }
    }

    if data.connection_id.is_empty()
        || data.refresh_hints.len() > 1000
        || (data.quarantine_reason.is_none() && data.refresh_hints.is_empty())
        || data.quarantine_reason.as_ref().is_some_and(|reason| reason.is_empty() || reason.len() > 128)
        || data.refresh_hints.iter().any(|hint| {
            hint.object_id.is_empty()
                || hint.object_id.len() > 256
                || hint.funds_transaction_id.as_ref().is_some_and(|id| id.trim().is_empty() || id.len() > 256)
        })
    {
        return Err(Error::ValidationError("供应商消息刷新意图无效".into()));
    }
    Ok(())
}

impl fmt::Debug for SupplierPulledMessage {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("SupplierPulledMessage")
            .field("id", &self.base.id)
            .field("state", &self.state)
            .field("body_encrypted", &"<redacted>")
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn message(hash: &str) -> SupplierPulledMessage {
        SupplierPulledMessage::received(SupplierMessageData {
            id: "message-1".into(),
            account_scope: "jd:account-1".into(),
            connection_id: SupplierApiConnectionId::new("connection-1"),
            source_message_id: "source-1".into(),
            message_type: "1".into(),
            content_fingerprint: hash.into(),
            body_encrypted: "private-encrypted-body".into(),
            refresh_hints: vec![SupplierRefreshHint {
                topic: "order".into(),
                target_kind: "external_order".into(),
                object_id: "root-1".into(),
                product_id: None,
                funds_transaction_id: None,
            }],
            quarantine_reason: None,
            received_at: Instant::from_unix_secs(10),
        })
        .unwrap()
    }

    #[test]
    fn deletion_unknown_never_reopens_application_or_deletion() {
        let mut message = message("one");
        assert!(message.begin_acknowledge().is_err());

        message.begin_apply().unwrap();
        message.applied(Instant::from_unix_secs(11)).unwrap();
        message.begin_acknowledge().unwrap();
        message.acknowledged(false, Instant::from_unix_secs(12)).unwrap();
        assert_eq!(message.state, SupplierMessageState::AckUnknown);

        assert!(message.begin_apply().is_err());
        assert!(message.begin_acknowledge().is_err());
        assert!(message.acknowledged_at.is_none());
    }

    #[test]
    fn identical_replay_preserves_completion_and_conflict_preserves_both_evidence() {
        let mut existing = message("one");
        existing.begin_apply().unwrap();
        existing.applied(Instant::from_unix_secs(11)).unwrap();

        assert!(existing.observe_replay(&message("one")).unwrap());
        assert_eq!(existing.state, SupplierMessageState::AckPending);

        assert!(!existing.observe_replay(&message("two")).unwrap());
        assert_eq!(existing.state, SupplierMessageState::Quarantined);
        assert_eq!(existing.content_fingerprint, "one");
        assert_eq!(existing.conflict.unwrap().fingerprint, "two");
    }

    #[test]
    fn unknown_type_is_durable_quarantine_and_debug_redacts_evidence() {
        let mut existing = message("one");
        existing.state = SupplierMessageState::Quarantined;
        assert!(existing.begin_apply().is_err());
        assert!(!format!("{existing:?}").contains("private-encrypted-body"));

        let mut moved = message("one");
        moved.connection_id = SupplierApiConnectionId::new("another");
        assert!(existing.observe_replay(&moved).is_err());
    }
}
