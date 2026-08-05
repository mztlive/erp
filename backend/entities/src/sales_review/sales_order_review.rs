//! `sales_order_review`：销售单审批记录（数据模型 §6.5）。
//!
//! 审批对象是被审批的不可变提交快照（`submission_id`）；审批人不得在审批动作中
//! 修改销售单内容。销售修改被驳回内容后，旧审批记录改为「因内容变化失效」，
//! 新提交从第一步开始。采购二次确认的唯一状态源是 `procurement_confirmation`，
//! 不重复写成 `review_stage`。

use entity_core::BaseModel;
use entity_macros::Entity;
use serde::{Deserialize, Serialize};

use crate::common::stable::StableBase;
use crate::common::state::{ensure_transition, DocumentState};
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

/// 审批状态（数据模型 §6.5：待处理、通过、驳回、因内容变化失效）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum SalesReviewStatus {
    /// 待处理。
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
    /// 待处理可被通过、驳回或因内容变化失效；其余为终态（§6.5）。
    fn allowed_next(self) -> &'static [Self] {
        match self {
            Self::Pending => &[Self::Approved, Self::Rejected, Self::Superseded],
            Self::Approved | Self::Rejected | Self::Superseded => &[],
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
}

/// 销售审批记录实体（数据模型 §6.5：`(submission_id, review_stage)` 唯一）。
///
/// `StableBase` 是 P0 冻结基元且未派生 `PartialEq`，因此本实体手工实现
/// `PartialEq`/`Eq`（全字段语义相等）以替代约定中的派生写法。
#[derive(Debug, Serialize, Deserialize, Clone, Entity)]
pub struct SalesOrderReview {
    #[serde(flatten)]
    pub base: BaseModel,
    #[serde(flatten)]
    pub stable: StableBase<SalesReviewStatus>,
    /// 销售单。
    pub sales_order_id: SalesOrderId,
    /// 被审批的不可变提交快照。
    pub submission_id: SalesOrderSubmissionId,
    /// 审批阶段。
    pub review_stage: SalesReviewStage,
    /// 审批人。
    pub reviewer_id: Option<String>,
    /// 审批时间。
    pub reviewed_at: Option<Instant>,
    /// 意见或驳回原因。
    pub decision_reason: Option<String>,
}

impl PartialEq for SalesOrderReview {
    /// 全字段语义相等。
    fn eq(&self, other: &Self) -> bool {
        self.base == other.base
            && self.stable.status == other.stable.status
            && self.stable.current_revision_id == other.stable.current_revision_id
            && self.stable.created_by == other.stable.created_by
            && self.stable.updated_by == other.stable.updated_by
            && self.sales_order_id == other.sales_order_id
            && self.submission_id == other.submission_id
            && self.review_stage == other.review_stage
            && self.reviewer_id == other.reviewer_id
            && self.reviewed_at == other.reviewed_at
            && self.decision_reason == other.decision_reason
    }
}

impl Eq for SalesOrderReview {}

impl SalesOrderReview {
    /// 创建审批记录（初始 `Pending`；审批人/时间由决策动作写入）。
    ///
    /// # 参数
    /// * `id` - 实体主键（`entities::ids::SalesOrderReviewId`）
    /// * `data` - 创建数据
    /// * `created_by` - 创建人（通常为工作流任务创建者）
    ///
    /// # 返回
    /// 返回新建的审批记录。
    ///
    /// # 错误
    /// 创建人标识为空或超长时返回错误。
    pub fn new(
        id: SalesOrderReviewId,
        data: SalesOrderReviewData,
        created_by: impl Into<String>,
    ) -> Result<Self> {
        let created_by = normalize_required_text(
            created_by.into(),
            "创建人不能为空",
            REVIEWER_MAX_LEN,
            "创建人过长",
        )?;
        Ok(Self {
            base: BaseModel::new(id.to_string()),
            stable: StableBase::new(SalesReviewStatus::Pending, created_by),
            sales_order_id: data.sales_order_id,
            submission_id: data.submission_id,
            review_stage: data.review_stage,
            reviewer_id: None,
            reviewed_at: None,
            decision_reason: None,
        })
    }

    /// 更新审批记录。
    ///
    /// 审批人不得在审批动作中修改销售单内容；审批结论只能通过
    /// [`Self::approve`]/[`Self::reject`]/[`Self::invalidate`] 动作给出，
    /// 通用更新恒拒绝（审批字段与结论绑定，避免绕过状态机）。
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
        Err(Error::from("审批记录只能通过审批动作更新"))
    }

    /// 通过审批（`Pending → Approved`）。
    ///
    /// # 参数
    /// * `reviewer_id` - 审批人
    /// * `reviewed_at` - 审批时间
    /// * `decision_reason` - 审批意见（可空）
    ///
    /// # 返回
    /// 迁移成功返回 `Ok(())`。
    ///
    /// # 错误
    /// 非待处理状态，或审批人/意见为空、超长时返回错误。
    pub fn approve(
        &mut self,
        reviewer_id: impl Into<String>,
        reviewed_at: Instant,
        decision_reason: Option<String>,
    ) -> Result<()> {
        self.decide(
            SalesReviewStatus::Approved,
            reviewer_id,
            reviewed_at,
            decision_reason,
        )
    }

    /// 驳回审批（`Pending → Rejected`；驳回原因必填）。
    ///
    /// # 参数
    /// * `reviewer_id` - 审批人
    /// * `reviewed_at` - 审批时间
    /// * `decision_reason` - 驳回原因
    ///
    /// # 返回
    /// 迁移成功返回 `Ok(())`。
    ///
    /// # 错误
    /// 非待处理状态、驳回原因为空/超长，或审批人为空时返回错误。
    pub fn reject(
        &mut self,
        reviewer_id: impl Into<String>,
        reviewed_at: Instant,
        decision_reason: impl Into<String>,
    ) -> Result<()> {
        self.decide(
            SalesReviewStatus::Rejected,
            reviewer_id,
            reviewed_at,
            Some(decision_reason.into()),
        )
    }

    /// 标记因内容变化失效（`Pending → Superseded`；新提交从第一步开始，§6.5）。
    ///
    /// # 参数
    /// * `reviewed_at` - 失效时间
    ///
    /// # 返回
    /// 迁移成功返回 `Ok(())`。
    ///
    /// # 错误
    /// 非待处理状态时返回 [`Error::InvalidStateTransition`]。
    pub fn invalidate(&mut self, reviewed_at: Instant) -> Result<()> {
        ensure_transition(self.stable.status, SalesReviewStatus::Superseded)?;
        self.stable.status = SalesReviewStatus::Superseded;
        self.reviewed_at = Some(reviewed_at);
        Ok(())
    }

    /// 执行一次审批结论（写入审批人/时间/原因并迁移状态）。
    ///
    /// # 参数
    /// * `to` - 目标状态（通过或驳回）
    /// * `reviewer_id` - 审批人
    /// * `reviewed_at` - 审批时间
    /// * `decision_reason` - 意见或驳回原因
    ///
    /// # 返回
    /// 迁移成功返回 `Ok(())`。
    ///
    /// # 错误
    /// 非待处理状态，或审批人/原因为空、超长时返回错误。
    fn decide(
        &mut self,
        to: SalesReviewStatus,
        reviewer_id: impl Into<String>,
        reviewed_at: Instant,
        decision_reason: Option<String>,
    ) -> Result<()> {
        ensure_transition(self.stable.status, to)?;
        let reviewer_id = normalize_required_text(
            reviewer_id.into(),
            "审批人不能为空",
            REVIEWER_MAX_LEN,
            "审批人过长",
        )?;
        let decision_reason = match decision_reason {
            Some(reason) if to == SalesReviewStatus::Rejected => Some(normalize_required_text(
                reason,
                "驳回原因不能为空",
                DECISION_REASON_MAX_LEN,
                "驳回原因过长",
            )?),
            Some(reason) => normalize_optional_text(Some(reason), "审批意见", DECISION_REASON_MAX_LEN)?,
            None => None,
        };
        self.stable.status = to;
        self.reviewer_id = Some(reviewer_id.clone());
        self.reviewed_at = Some(reviewed_at);
        self.decision_reason = decision_reason;
        self.stable.touch(reviewer_id);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn data() -> SalesOrderReviewData {
        SalesOrderReviewData {
            sales_order_id: SalesOrderId::new("o-1"),
            submission_id: SalesOrderSubmissionId::new("s-1"),
            review_stage: SalesReviewStage::SalesLeader,
        }
    }

    #[test]
    fn new_initializes_pending_without_decision() {
        let review = SalesOrderReview::new(SalesOrderReviewId::new("r-1"), data(), "system-1").unwrap();

        assert_eq!(review.stable.status(), SalesReviewStatus::Pending);
        assert_eq!(review.review_stage, SalesReviewStage::SalesLeader);
        assert!(review.reviewer_id.is_none());
        assert!(review.reviewed_at.is_none());
    }

    #[test]
    fn new_rejects_blank_creator() {
        assert!(SalesOrderReview::new(SalesOrderReviewId::new("r-1"), data(), "   ").is_err());
    }

    #[test]
    fn approve_and_reject_write_decision_fields() {
        let mut review = SalesOrderReview::new(SalesOrderReviewId::new("r-1"), data(), "system-1").unwrap();
        review
            .approve(
                " leader-1 ",
                Instant::from_unix_secs(1_800_000_000),
                Some(" 同意 ".to_string()),
            )
            .unwrap();

        assert_eq!(review.stable.status(), SalesReviewStatus::Approved);
        assert_eq!(review.reviewer_id.as_deref(), Some("leader-1"));
        assert_eq!(review.reviewed_at.unwrap().unix_secs(), 1_800_000_000);
        assert_eq!(review.decision_reason.as_deref(), Some("同意"));

        let mut rejected = SalesOrderReview::new(SalesOrderReviewId::new("r-2"), data(), "system-1").unwrap();
        rejected
            .reject(
                " leader-2 ",
                Instant::from_unix_secs(1_800_000_100),
                " 价格超出预算 ",
            )
            .unwrap();
        assert_eq!(rejected.stable.status(), SalesReviewStatus::Rejected);
        assert_eq!(rejected.decision_reason.as_deref(), Some("价格超出预算"));
    }

    #[test]
    fn reject_requires_reason_and_allows_legal_transitions() {
        let mut review = SalesOrderReview::new(SalesOrderReviewId::new("r-1"), data(), "system-1").unwrap();
        assert!(
            review
                .reject("leader-1", Instant::from_unix_secs(1_800_000_000), "   ")
                .is_err(),
            "驳回原因必填"
        );

        review.invalidate(Instant::from_unix_secs(1_800_000_200)).unwrap();
        assert_eq!(review.stable.status(), SalesReviewStatus::Superseded);
        assert!(review
            .approve("leader-1", Instant::from_unix_secs(1_800_000_300), None)
            .is_err());
        assert!(ensure_transition(SalesReviewStatus::Rejected, SalesReviewStatus::Approved).is_err());
        assert!(SalesReviewStatus::Approved.allowed_next().is_empty());
    }

    #[test]
    fn update_is_rejected() {
        let mut review = SalesOrderReview::new(SalesOrderReviewId::new("r-1"), data(), "system-1").unwrap();
        assert!(review.update(data()).is_err());
    }
}
