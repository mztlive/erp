//! 采购邮件发送记录兼作一次性发件占用；结果不明时禁止自动重发。
use std::fmt;

use entity_core::BaseModel;
use entity_macros::Entity;
use erp_core::{Error, Result};
use serde::{Deserialize, Serialize};

use crate::entity::purchase_order::PurchaseOrderStatus;
use crate::{Error as ProcurementError, Result as ProcurementResult};

/// 浏览器发件命令；正文和附件由服务端读取采购版本生成。
#[derive(Clone, Deserialize, Serialize)]
pub struct SendPurchaseEmail {
    pub expected_version: u64,
    pub recipient: String,
    pub idempotency_key: String,
}
impl SendPurchaseEmail {
    /// 校验发件依据为调用人确认的已生效采购版本。
    /// # 参数
    /// `status` 为采购状态，`version` 为当前版本，`has_revision` 表示完整版本内容已读取。
    /// # 返回
    /// 状态及版本一致时允许发送。
    /// # 错误
    /// 版本过期、草稿、审批中、作废或缺少生效版本时拒绝。
    pub fn validate_source(
        &self,
        status: PurchaseOrderStatus,
        version: u64,
        has_revision: bool,
    ) -> Result<()> {
        if version != self.expected_version {
            return Err(Error::from("采购单已变化，请刷新后重新确认"));
        }
        if !has_revision
            || !matches!(
                status,
                PurchaseOrderStatus::Effective
                    | PurchaseOrderStatus::PartiallyExecuted
                    | PurchaseOrderStatus::Completed
            )
        {
            return Err(Error::from("采购单审批生效后才能发送给供应商"));
        }
        Ok(())
    }
    /// 校验单一邮箱及命令键，拒绝邮件头注入。
    /// # 参数
    /// 无。
    /// # 返回
    /// 返回规范化的收件邮箱。
    /// # 错误
    /// 邮箱非法、命令键为空或过长时拒绝。
    pub fn recipient(&self) -> Result<String> {
        let email = self.recipient.trim();
        let parts = email.split('@').collect::<Vec<_>>();
        if email.len() > 254
            || parts.len() != 2
            || parts[0].is_empty()
            || !parts[1].contains('.')
            || !email.bytes().all(|b| b.is_ascii_alphanumeric() || b"@.!#$%&'*+-/=?^_`{|}~".contains(&b))
            || self.idempotency_key.trim().is_empty()
            || self.idempotency_key.len() > 128
        {
            return Err(Error::from("请填写有效的单个供应商邮箱，并重新发起发送"));
        }
        Ok(email.into())
    }
}
/// 邮件发送状态；Accepted 仅表示发件服务受理，不表示收件人已收到。
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum PurchaseEmailStatus {
    Sending,
    Accepted,
    Failed,
    Unknown,
}
/// 持久化原发件内容、操作人和结果；不自动清理，保证永久去重。
#[derive(Clone, Serialize, Deserialize, Entity)]
pub struct PurchaseEmail {
    #[serde(flatten)]
    pub base: BaseModel,
    pub purchase_order_id: String,
    pub revision_id: String,
    pub actor_id: String,
    pub fingerprint: String,
    pub recipient: String,
    pub html: String,
    pub status: PurchaseEmailStatus,
    pub provider_message_id: Option<String>,
}
impl PurchaseEmail {
    /// 校验重试命令仍对应原发送内容。
    /// # 参数
    /// `fingerprint` 为订单、版本和规范化收件人的命令指纹。
    /// # 返回
    /// 一致时允许读取原发送状态。
    /// # 错误
    /// 同键异载荷返回冲突，禁止再次发送。
    pub fn validate_replay(&self, fingerprint: &str) -> ProcurementResult<()> {
        if self.fingerprint != fingerprint {
            return Err(ProcurementError::ConflictError("本次发送内容已变化，请重新发起发送".into()));
        }
        Ok(())
    }
}
impl fmt::Debug for PurchaseEmail {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("PurchaseEmail")
            .field("id", &self.base.id)
            .field("status", &self.status)
            .finish_non_exhaustive()
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn only_confirmed_effective_versions_can_be_sent() {
        let request = SendPurchaseEmail {
            expected_version: 3,
            recipient: "a@example.com".into(),
            idempotency_key: "key".into(),
        };
        for status in [
            PurchaseOrderStatus::Effective,
            PurchaseOrderStatus::PartiallyExecuted,
            PurchaseOrderStatus::Completed,
        ] {
            assert!(request.validate_source(status, 3, true).is_ok());
            assert!(request.validate_source(status, 2, true).is_err());
            assert!(request.validate_source(status, 3, false).is_err());
        }
        for status in [
            PurchaseOrderStatus::Draft,
            PurchaseOrderStatus::InApproval,
            PurchaseOrderStatus::PendingFinanceReview,
            PurchaseOrderStatus::Voided,
        ] {
            assert!(request.validate_source(status, 3, true).is_err());
        }
    }
    #[test]
    fn validates_single_recipient_and_command_identity() {
        let mut request = SendPurchaseEmail {
            expected_version: 1,
            recipient: " orders@example.com ".into(),
            idempotency_key: "send-1".into(),
        };
        assert_eq!(request.recipient().unwrap(), "orders@example.com");
        for email in
            ["a@example.com\r\nBcc:x@example.com", "a@example.com,b@example.com", "@example.com", "missing"]
        {
            request.recipient = email.into();
            assert!(request.recipient().is_err());
        }
    }
}
