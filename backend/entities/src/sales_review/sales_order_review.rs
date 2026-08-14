//! `sales_order_review`：销售单审批记录（数据模型 §6.5）。
//!
//! 审批对象是被审批的不可变提交快照（`submission_id`）；本表只保存已经形成的
//! 正式决定。审批等待与当前步骤属于 D03 `approval_step_instance`，不得在本表
//! 建立 `PENDING` 占位记录或反推审批运行状态。

use entity_core::BaseModel;
use entity_macros::Entity;
use serde::{Deserialize, Serialize};

use crate::common::state::DocumentState;
use crate::common::time::Instant;
use crate::errors::{Error, Result};
use crate::ids::{SalesOrderId, SalesOrderReviewId, SalesOrderSubmissionId};
use crate::validation::{normalize_optional_text, normalize_required_text};

/// 审批人标识最大长度。
const REVIEWER_MAX_LEN: usize = 128;
/// 意见或驳回原因最大长度。
const DECISION_REASON_MAX_LEN: usize = 512;

/// 审批阶段（数据模型 §6.5：销售领导审批、运营审批、低毛利上级确认）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum SalesReviewStage {
    /// 销售领导审批。
    SalesLeader,
    /// 运营审批。
    Operations,
    /// 低毛利上级确认。
    LowMarginSuperior,
}

impl SalesReviewStage {
    /// 返回阶段的中文展示名。
    ///
    /// # 返回
    /// 返回面向用户的中文标签。
    pub fn label(&self) -> &'static str {
        match self {
            Self::SalesLeader => "销售领导审批",
            Self::Operations => "运营审批",
            Self::LowMarginSuperior => "低毛利上级确认",
        }
    }

    /// 返回阶段的稳定代码。
    ///
    /// # 返回
    /// 返回用于持久化与查询的稳定字符串。
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::SalesLeader => "SALES_LEADER",
            Self::Operations => "OPERATIONS",
            Self::LowMarginSuperior => "LOW_MARGIN_SUPERIOR",
        }
    }
}

/// 销售变更等其它 D14 记录沿用的运行状态。
///
/// `sales_order_review` 不使用本枚举；其状态固定使用
/// [`SalesOrderReviewDecision`]，避免把等待态写入正式决定表。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum SalesReviewStatus {
    /// 等待处理。
    Pending,
    /// 通过。
    Approved,
    /// 驳回。
    Rejected,
    /// 因内容变化失效。
    Superseded,
}

impl SalesReviewStatus {
    /// 返回状态的中文展示名。
    ///
    /// # 返回
    /// 返回面向用户的中文标签。
    pub fn label(&self) -> &'static str {
        match self {
            Self::Pending => "待处理",
            Self::Approved => "通过",
            Self::Rejected => "驳回",
            Self::Superseded => "因内容变化失效",
        }
    }

    /// 返回状态的稳定代码。
    ///
    /// # 返回
    /// 返回用于持久化与查询的稳定字符串。
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Pending => "PENDING",
            Self::Approved => "APPROVED",
            Self::Rejected => "REJECTED",
            Self::Superseded => "SUPERSEDED",
        }
    }
}

impl DocumentState for SalesReviewStatus {
    fn allowed_next(self) -> &'static [Self] {
        match self {
            Self::Pending => &[Self::Approved, Self::Rejected, Self::Superseded],
            Self::Approved | Self::Rejected | Self::Superseded => &[],
        }
    }
}

/// 销售单审批正式决定。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum SalesOrderReviewDecision {
    /// 通过。
    Approved,
    /// 驳回。
    Rejected,
}

impl SalesOrderReviewDecision {
    /// 返回持久化稳定代码。
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Approved => "APPROVED",
            Self::Rejected => "REJECTED",
        }
    }
}

/// 销售审批记录创建数据。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SalesOrderReviewData {
    /// 销售单。
    pub sales_order_id: SalesOrderId,
    /// 被审批的不可变提交快照。
    pub submission_id: SalesOrderSubmissionId,
    /// 审批阶段。
    pub review_stage: SalesReviewStage,
    /// 正式审批决定。
    pub status: SalesOrderReviewDecision,
    /// 审批人。
    pub reviewer_id: String,
    /// 审批时间。
    pub reviewed_at: Instant,
    /// 意见或驳回原因。
    pub decision_reason: Option<String>,
}

/// 销售审批记录实体（数据模型 §6.5：`(submission_id, review_stage)` 唯一）。
#[derive(Debug, Serialize, Deserialize, Clone, Entity, PartialEq, Eq)]
pub struct SalesOrderReview {
    #[serde(flatten)]
    pub base: BaseModel,
    /// 销售单。
    pub sales_order_id: SalesOrderId,
    /// 被审批的不可变提交快照。
    pub submission_id: SalesOrderSubmissionId,
    /// 审批阶段。
    pub review_stage: SalesReviewStage,
    /// 正式审批决定。
    pub status: SalesOrderReviewDecision,
    /// 审批人。
    pub reviewer_id: String,
    /// 审批时间。
    pub reviewed_at: Instant,
    /// 意见或驳回原因。
    pub decision_reason: Option<String>,
}

impl SalesOrderReview {
    /// 创建不可变审批决定记录。
    ///
    /// # 参数
    /// * `id` - 实体主键（`entities::ids::SalesOrderReviewId`）
    /// * `data` - 已形成的审批决定数据
    ///
    /// # 返回
    /// 返回已经包含正式决定、审批人和审批时间的记录。
    ///
    /// # 错误
    /// 审批人或决定原因不满足约束时返回错误。
    pub fn new(id: SalesOrderReviewId, data: SalesOrderReviewData) -> Result<Self> {
        let reviewer_id =
            normalize_required_text(data.reviewer_id, "审批人不能为空", REVIEWER_MAX_LEN, "审批人过长")?;
        let decision_reason = match data.decision_reason {
            Some(reason) if data.status == SalesOrderReviewDecision::Rejected => {
                Some(normalize_required_text(
                    reason,
                    "驳回原因不能为空",
                    DECISION_REASON_MAX_LEN,
                    "驳回原因过长",
                )?)
            }
            Some(reason) => normalize_optional_text(Some(reason), "审批意见", DECISION_REASON_MAX_LEN)?,
            None if data.status == SalesOrderReviewDecision::Rejected => {
                return Err(Error::from("驳回原因不能为空"));
            }
            None => None,
        };
        Ok(Self {
            base: BaseModel::new(id.to_string()),
            sales_order_id: data.sales_order_id,
            submission_id: data.submission_id,
            review_stage: data.review_stage,
            status: data.status,
            reviewer_id,
            reviewed_at: data.reviewed_at,
            decision_reason,
        })
    }

    /// 更新审批记录。
    ///
    /// 记录创建时已经包含正式决定，审批人不得修改销售单内容，后续任何更新
    /// 恒拒绝。
    ///
    /// # 参数
    /// * `_data` - 更新数据（被拒绝）
    ///
    /// # 返回
    /// 恒返回 `Err`。
    ///
    /// # 错误
    /// 恒返回「审批记录不可直接更新」错误。
    pub fn update(&mut self, _data: SalesOrderReviewData) -> Result<()> {
        Err(Error::from("审批决定记录不可更新"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn data(status: SalesOrderReviewDecision) -> SalesOrderReviewData {
        SalesOrderReviewData {
            sales_order_id: SalesOrderId::new("o-1"),
            submission_id: SalesOrderSubmissionId::new("s-1"),
            review_stage: SalesReviewStage::SalesLeader,
            status,
            reviewer_id: " leader-1 ".to_string(),
            reviewed_at: Instant::from_unix_secs(1_800_000_000),
            decision_reason: Some(" 同意 ".to_string()),
        }
    }

    #[test]
    fn new_creates_immutable_approved_decision() {
        let review = SalesOrderReview::new(
            SalesOrderReviewId::new("r-1"),
            data(SalesOrderReviewDecision::Approved),
        )
        .unwrap();

        assert_eq!(review.status, SalesOrderReviewDecision::Approved);
        assert_eq!(review.review_stage, SalesReviewStage::SalesLeader);
        assert_eq!(review.reviewer_id, "leader-1");
        assert_eq!(review.reviewed_at.unix_secs(), 1_800_000_000);
        assert_eq!(review.decision_reason.as_deref(), Some("同意"));
    }

    #[test]
    fn rejected_decision_requires_reason() {
        let mut rejected = data(SalesOrderReviewDecision::Rejected);
        rejected.decision_reason = None;
        assert!(SalesOrderReview::new(SalesOrderReviewId::new("r-2"), rejected).is_err());
    }

    #[test]
    fn new_rejects_blank_reviewer() {
        let mut decision = data(SalesOrderReviewDecision::Approved);
        decision.reviewer_id = "   ".to_string();
        assert!(SalesOrderReview::new(SalesOrderReviewId::new("r-1"), decision).is_err());
    }

    #[test]
    fn update_is_rejected() {
        let decision = data(SalesOrderReviewDecision::Approved);
        let mut review = SalesOrderReview::new(SalesOrderReviewId::new("r-1"), decision.clone()).unwrap();
        assert!(review.update(decision).is_err());
    }
}
