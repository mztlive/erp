//! 采购驳回后照原条件承接的低毛利上级确认事实。

use entity_core::BaseModel;
use entity_macros::Entity;
use serde::{Deserialize, Serialize};

use crate::common::time::Instant;
use crate::errors::{Error, Result};
use crate::ids::{
    LowMarginManagerConfirmationId, ProcurementConfirmationId, SalesOrderId, SalesOrderSubmissionId,
};
use crate::validation::{normalize_optional_text, normalize_required_text};

const REASON_MAX_LEN: usize = 512;
const EVIDENCE_REFERENCE_MAX_LEN: usize = 128;
const ACTOR_MAX_LEN: usize = 128;
const DECISION_REASON_CODE_MAX_LEN: usize = 64;
const COMMENT_MAX_LEN: usize = 512;
const MAX_EVIDENCE_REFERENCES: usize = 20;

/// 低毛利上级确认状态。
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum LowMarginManagerConfirmationStatus {
    /// 等待上级决定。
    Pending,
    /// 上级同意承接；后续仍须采购重新确认。
    Approved,
    /// 上级拒绝承接；销售回到固定三路处置。
    Rejected,
}

impl LowMarginManagerConfirmationStatus {
    /// 返回持久化稳定代码。
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Pending => "PENDING",
            Self::Approved => "APPROVED",
            Self::Rejected => "REJECTED",
        }
    }
}

/// 创建低毛利上级确认所需的冻结事实。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct LowMarginManagerConfirmationData {
    /// 销售单。
    pub sales_order_id: SalesOrderId,
    /// 本轮处理来源的已驳回采购确认。
    pub rejected_procurement_confirmation_id: ProcurementConfirmationId,
    /// 被驳回的原不可变提交。
    pub rejected_submission_id: SalesOrderSubmissionId,
    /// 照原条件重新冻结的不可变提交。
    pub low_margin_submission_id: SalesOrderSubmissionId,
    /// 销售承接理由。
    pub acceptance_reason: String,
    /// 受控证据引用；不得承载临时展示引用。
    pub evidence_reference_ids: Vec<String>,
    /// 申请人。
    pub requested_by: String,
    /// 申请时间。
    pub requested_at: Instant,
}

/// 低毛利上级确认事实；申请形成后只允许追加一次正式决定。
#[derive(Debug, Clone, Serialize, Deserialize, Entity, PartialEq, Eq)]
pub struct LowMarginManagerConfirmation {
    #[serde(flatten)]
    pub base: BaseModel,
    /// 销售单。
    pub sales_order_id: SalesOrderId,
    /// 本轮处理来源的已驳回采购确认。
    pub rejected_procurement_confirmation_id: ProcurementConfirmationId,
    /// 被驳回的原不可变提交。
    pub rejected_submission_id: SalesOrderSubmissionId,
    /// 照原条件重新冻结的不可变提交。
    pub low_margin_submission_id: SalesOrderSubmissionId,
    /// 销售承接理由。
    pub acceptance_reason: String,
    /// 受控证据引用。
    pub evidence_reference_ids: Vec<String>,
    /// 申请人。
    pub requested_by: String,
    /// 申请时间。
    pub requested_at: Instant,
    /// 当前状态。
    pub status: LowMarginManagerConfirmationStatus,
    /// 决定人。
    pub decided_by: Option<String>,
    /// 决定时间。
    pub decided_at: Option<Instant>,
    /// 驳回原因代码；仅驳回决定必填。
    pub decision_reason_code: Option<String>,
    /// 决定意见。
    pub decision_comment: Option<String>,
}

impl LowMarginManagerConfirmation {
    /// 创建待处理的低毛利上级确认并规范化理由、证据与申请人。
    ///
    /// # Errors
    /// 理由、证据或申请人为空、超长、重复时返回领域错误。
    pub fn new(id: LowMarginManagerConfirmationId, data: LowMarginManagerConfirmationData) -> Result<Self> {
        let acceptance_reason = normalize_required_text(
            data.acceptance_reason,
            "低毛利承接理由不能为空",
            REASON_MAX_LEN,
            "低毛利承接理由过长",
        )?;
        let requested_by = normalize_required_text(
            data.requested_by,
            "低毛利申请人不能为空",
            ACTOR_MAX_LEN,
            "低毛利申请人过长",
        )?;
        let evidence_reference_ids = normalize_evidence_references(data.evidence_reference_ids)?;
        Ok(Self {
            base: BaseModel::new(id.to_string()),
            sales_order_id: data.sales_order_id,
            rejected_procurement_confirmation_id: data.rejected_procurement_confirmation_id,
            rejected_submission_id: data.rejected_submission_id,
            low_margin_submission_id: data.low_margin_submission_id,
            acceptance_reason,
            evidence_reference_ids,
            requested_by,
            requested_at: data.requested_at,
            status: LowMarginManagerConfirmationStatus::Pending,
            decided_by: None,
            decided_at: None,
            decision_reason_code: None,
            decision_comment: None,
        })
    }

    /// 追加上级通过决定；该决定不代表采购通过。
    ///
    /// # Errors
    /// 非待处理状态、决定人或意见非法时返回领域错误。
    pub fn approve(&mut self, actor_id: String, comment: Option<String>, at: Instant) -> Result<()> {
        self.ensure_pending()?;
        self.decided_by = Some(normalize_actor(actor_id)?);
        self.decided_at = Some(at);
        self.decision_comment = normalize_optional_text(comment, "低毛利确认意见", COMMENT_MAX_LEN)?;
        self.status = LowMarginManagerConfirmationStatus::Approved;
        Ok(())
    }

    /// 追加上级驳回决定。
    ///
    /// # Errors
    /// 非待处理状态，或原因代码、意见、决定人非法时返回领域错误。
    pub fn reject(
        &mut self,
        actor_id: String,
        reason_code: String,
        comment: String,
        at: Instant,
    ) -> Result<()> {
        self.ensure_pending()?;
        self.decided_by = Some(normalize_actor(actor_id)?);
        self.decided_at = Some(at);
        self.decision_reason_code = Some(normalize_required_text(
            reason_code,
            "低毛利驳回原因代码不能为空",
            DECISION_REASON_CODE_MAX_LEN,
            "低毛利驳回原因代码过长",
        )?);
        self.decision_comment = Some(normalize_required_text(
            comment,
            "低毛利驳回意见不能为空",
            COMMENT_MAX_LEN,
            "低毛利驳回意见过长",
        )?);
        self.status = LowMarginManagerConfirmationStatus::Rejected;
        Ok(())
    }

    fn ensure_pending(&self) -> Result<()> {
        if self.status == LowMarginManagerConfirmationStatus::Pending {
            return Ok(());
        }
        Err(Error::from("低毛利上级确认已形成正式决定"))
    }
}

fn normalize_actor(value: String) -> Result<String> {
    normalize_required_text(value, "决定人不能为空", ACTOR_MAX_LEN, "决定人过长")
}

fn normalize_evidence_references(values: Vec<String>) -> Result<Vec<String>> {
    if values.len() > MAX_EVIDENCE_REFERENCES {
        return Err(Error::from("低毛利证据引用数量超过上限"));
    }
    let mut normalized = Vec::with_capacity(values.len());
    for value in values {
        let value = normalize_required_text(
            value,
            "证据引用不能为空",
            EVIDENCE_REFERENCE_MAX_LEN,
            "证据引用过长",
        )?;
        if normalized.contains(&value) {
            return Err(Error::from("证据引用不得重复"));
        }
        normalized.push(value);
    }
    Ok(normalized)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn data() -> LowMarginManagerConfirmationData {
        LowMarginManagerConfirmationData {
            sales_order_id: SalesOrderId::new("so-1"),
            rejected_procurement_confirmation_id: ProcurementConfirmationId::new("pc-1"),
            rejected_submission_id: SalesOrderSubmissionId::new("sub-1"),
            low_margin_submission_id: SalesOrderSubmissionId::new("sub-2"),
            acceptance_reason: " 维持客户承诺 ".to_string(),
            evidence_reference_ids: vec!["evidence-1".to_string()],
            requested_by: "sales-1".to_string(),
            requested_at: Instant::from_unix_secs(100),
        }
    }

    #[test]
    fn request_normalizes_and_decision_is_terminal() {
        let mut value =
            LowMarginManagerConfirmation::new(LowMarginManagerConfirmationId::new("lmc-1"), data()).unwrap();
        assert_eq!(value.acceptance_reason, "维持客户承诺");
        value
            .approve("leader-1".to_string(), None, Instant::from_unix_secs(200))
            .unwrap();
        assert_eq!(value.status, LowMarginManagerConfirmationStatus::Approved);
        assert!(value
            .reject(
                "leader-2".to_string(),
                "RISK".to_string(),
                "不同意".to_string(),
                Instant::from_unix_secs(300),
            )
            .is_err());
    }

    #[test]
    fn request_rejects_duplicate_evidence_and_reject_requires_comment() {
        let mut duplicate = data();
        duplicate.evidence_reference_ids = vec!["same".to_string(), "same".to_string()];
        assert!(
            LowMarginManagerConfirmation::new(LowMarginManagerConfirmationId::new("lmc-1"), duplicate,)
                .is_err()
        );

        let mut value =
            LowMarginManagerConfirmation::new(LowMarginManagerConfirmationId::new("lmc-2"), data()).unwrap();
        assert!(value
            .reject(
                "leader-1".to_string(),
                "RISK".to_string(),
                " ".to_string(),
                Instant::from_unix_secs(200),
            )
            .is_err());
    }
}
