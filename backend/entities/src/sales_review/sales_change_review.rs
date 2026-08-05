//! `sales_change_review`：销售变更复核记录（数据模型 §6.5）。
//!
//! 保存 `sales_change_submission_id`、`review_stage`、采购或运营的履约影响确认、
//! 财务金额影响复核和审批意见。实物及服务变更走采购影响确认；卡券变更走运营
//! 人工确认商城可执行性；卡券变更完成运营确认后再做财务影响复核。
//! 每次修改拟变更内容都形成新的变更提交并使旧复核失效，所有复核必须引用同一个
//! `sales_change_submission_id`（唯一性由仓储/索引保证）。

use entity_core::BaseModel;
use entity_macros::Entity;
use serde::{Deserialize, Serialize};

use crate::common::stable::StableBase;
use crate::common::state::ensure_transition;
use crate::common::time::Instant;
use crate::errors::{Error, Result};
use crate::ids::{SalesChangeReviewId, SalesChangeSubmissionId};
use crate::validation::{normalize_optional_text, normalize_required_text};

use super::sales_order_review::SalesReviewStatus;

/// 审批人标识最大长度。
const REVIEWER_MAX_LEN: usize = 128;
/// 意见或驳回原因最大长度。
const DECISION_REASON_MAX_LEN: usize = 512;

/// 变更复核阶段（数据模型 §6.5：采购或运营的履约影响确认、财务金额影响复核）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum SalesChangeReviewStage {
    /// 采购履约影响确认（实物及服务变更）。
    ProcurementImpact,
    /// 运营履约影响确认（卡券变更）。
    OperationsImpact,
    /// 财务金额影响复核。
    FinanceReview,
}

impl SalesChangeReviewStage {
    /// 返回阶段的中文展示名。
    ///
    /// # 返回
    /// 返回面向用户的中文标签。
    pub fn label(&self) -> &'static str {
        match self {
            Self::ProcurementImpact => "采购履约影响确认",
            Self::OperationsImpact => "运营履约影响确认",
            Self::FinanceReview => "财务金额影响复核",
        }
    }

    /// 返回阶段的稳定代码。
    ///
    /// # 返回
    /// 返回用于持久化与查询的稳定字符串。
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::ProcurementImpact => "PROCUREMENT_IMPACT",
            Self::OperationsImpact => "OPERATIONS_IMPACT",
            Self::FinanceReview => "FINANCE_REVIEW",
        }
    }
}

/// 变更复核创建数据。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SalesChangeReviewData {
    /// 被复核的不可变变更提交。
    pub sales_change_submission_id: SalesChangeSubmissionId,
    /// 复核阶段。
    pub review_stage: SalesChangeReviewStage,
}

/// 变更复核实体（数据模型 §6.5，状态机与销售审批一致）。
///
/// `StableBase` 是 P0 冻结基元且未派生 `PartialEq`，因此本实体手工实现
/// `PartialEq`/`Eq`（全字段语义相等）以替代约定中的派生写法。
#[derive(Debug, Serialize, Deserialize, Clone, Entity)]
pub struct SalesChangeReview {
    #[serde(flatten)]
    pub base: BaseModel,
    #[serde(flatten)]
    pub stable: StableBase<SalesReviewStatus>,
    /// 被复核的不可变变更提交。
    pub sales_change_submission_id: SalesChangeSubmissionId,
    /// 复核阶段。
    pub review_stage: SalesChangeReviewStage,
    /// 复核人。
    pub reviewer_id: Option<String>,
    /// 复核时间。
    pub reviewed_at: Option<Instant>,
    /// 意见或驳回原因。
    pub decision_reason: Option<String>,
}

impl PartialEq for SalesChangeReview {
    /// 全字段语义相等。
    fn eq(&self, other: &Self) -> bool {
        self.base == other.base
            && self.stable.status == other.stable.status
            && self.stable.current_revision_id == other.stable.current_revision_id
            && self.stable.created_by == other.stable.created_by
            && self.stable.updated_by == other.stable.updated_by
            && self.sales_change_submission_id == other.sales_change_submission_id
            && self.review_stage == other.review_stage
            && self.reviewer_id == other.reviewer_id
            && self.reviewed_at == other.reviewed_at
            && self.decision_reason == other.decision_reason
    }
}

impl Eq for SalesChangeReview {}

impl SalesChangeReview {
    /// 创建变更复核记录（初始 `Pending`；复核人/时间由决策动作写入）。
    ///
    /// # 参数
    /// * `id` - 实体主键（`entities::ids::SalesChangeReviewId`）
    /// * `data` - 创建数据
    /// * `created_by` - 创建人（通常为工作流任务创建者）
    ///
    /// # 返回
    /// 返回新建的变更复核记录。
    ///
    /// # 错误
    /// 创建人标识为空或超长时返回错误。
    pub fn new(
        id: SalesChangeReviewId,
        data: SalesChangeReviewData,
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
            sales_change_submission_id: data.sales_change_submission_id,
            review_stage: data.review_stage,
            reviewer_id: None,
            reviewed_at: None,
            decision_reason: None,
        })
    }

    /// 更新变更复核记录。
    ///
    /// 复核结论只能通过 [`Self::approve`]/[`Self::reject`]/[`Self::invalidate`]
    /// 动作给出，通用更新恒拒绝（避免绕过状态机修改结论）。
    ///
    /// # 参数
    /// * `_data` - 更新数据（被拒绝）
    ///
    /// # 返回
    /// 恒返回 `Err`。
    ///
    /// # 错误
    /// 恒返回「变更复核记录不可直接更新」错误。
    pub fn update(&mut self, _data: SalesChangeReviewData) -> Result<()> {
        Err(Error::from("变更复核记录只能通过复核动作更新"))
    }

    /// 通过复核（`Pending → Approved`）。
    ///
    /// # 参数
    /// * `reviewer_id` - 复核人
    /// * `reviewed_at` - 复核时间
    /// * `decision_reason` - 复核意见（可空）
    ///
    /// # 返回
    /// 迁移成功返回 `Ok(())`。
    ///
    /// # 错误
    /// 非待处理状态，或复核人/意见为空、超长时返回错误。
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

    /// 驳回复核（`Pending → Rejected`；驳回原因必填）。
    ///
    /// # 参数
    /// * `reviewer_id` - 复核人
    /// * `reviewed_at` - 复核时间
    /// * `decision_reason` - 驳回原因
    ///
    /// # 返回
    /// 迁移成功返回 `Ok(())`。
    ///
    /// # 错误
    /// 非待处理状态、驳回原因为空/超长，或复核人为空时返回错误。
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

    /// 标记因内容变化失效（`Pending → Superseded`；修改内容形成新变更提交时，
    /// 旧复核失效，§6.5）。
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

    /// 执行一次复核结论（写入复核人/时间/原因并迁移状态）。
    ///
    /// # 参数
    /// * `to` - 目标状态（通过或驳回）
    /// * `reviewer_id` - 复核人
    /// * `reviewed_at` - 复核时间
    /// * `decision_reason` - 意见或驳回原因
    ///
    /// # 返回
    /// 迁移成功返回 `Ok(())`。
    ///
    /// # 错误
    /// 非待处理状态，或复核人/原因为空、超长时返回错误。
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
            "复核人不能为空",
            REVIEWER_MAX_LEN,
            "复核人过长",
        )?;
        let decision_reason = match decision_reason {
            Some(reason) if to == SalesReviewStatus::Rejected => Some(normalize_required_text(
                reason,
                "驳回原因不能为空",
                DECISION_REASON_MAX_LEN,
                "驳回原因过长",
            )?),
            Some(reason) => normalize_optional_text(Some(reason), "复核意见", DECISION_REASON_MAX_LEN)?,
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
    use crate::common::state::DocumentState;

    fn data() -> SalesChangeReviewData {
        SalesChangeReviewData {
            sales_change_submission_id: SalesChangeSubmissionId::new("cs-1"),
            review_stage: SalesChangeReviewStage::FinanceReview,
        }
    }

    #[test]
    fn new_initializes_pending_without_decision() {
        let review = SalesChangeReview::new(SalesChangeReviewId::new("r-1"), data(), "system-1").unwrap();

        assert_eq!(review.stable.status(), SalesReviewStatus::Pending);
        assert_eq!(review.review_stage, SalesChangeReviewStage::FinanceReview);
        assert!(review.reviewer_id.is_none());
    }

    #[test]
    fn new_rejects_blank_creator() {
        assert!(SalesChangeReview::new(SalesChangeReviewId::new("r-1"), data(), "  ").is_err());
    }

    #[test]
    fn approve_and_reject_write_decision_fields() {
        let mut review = SalesChangeReview::new(SalesChangeReviewId::new("r-1"), data(), "system-1").unwrap();
        review
            .approve(
                " finance-1 ",
                Instant::from_unix_secs(1_800_000_000),
                Some(" 同意 ".to_string()),
            )
            .unwrap();
        assert_eq!(review.stable.status(), SalesReviewStatus::Approved);
        assert_eq!(review.reviewer_id.as_deref(), Some("finance-1"));
        assert_eq!(review.decision_reason.as_deref(), Some("同意"));

        let mut rejected =
            SalesChangeReview::new(SalesChangeReviewId::new("r-2"), data(), "system-1").unwrap();
        rejected
            .reject(
                " finance-2 ",
                Instant::from_unix_secs(1_800_000_100),
                " 金额影响未覆盖 ",
            )
            .unwrap();
        assert_eq!(rejected.stable.status(), SalesReviewStatus::Rejected);
        assert_eq!(rejected.decision_reason.as_deref(), Some("金额影响未覆盖"));
    }

    #[test]
    fn reject_requires_reason_and_invalidate_is_terminal_path() {
        let mut review = SalesChangeReview::new(SalesChangeReviewId::new("r-1"), data(), "system-1").unwrap();
        assert!(review
            .reject("finance-1", Instant::from_unix_secs(1_800_000_000), "   ")
            .is_err());

        review.invalidate(Instant::from_unix_secs(1_800_000_200)).unwrap();
        assert_eq!(review.stable.status(), SalesReviewStatus::Superseded);
        assert!(review
            .approve("finance-1", Instant::from_unix_secs(1_800_000_300), None)
            .is_err());
        assert!(SalesReviewStatus::Rejected.allowed_next().is_empty());
    }

    #[test]
    fn update_is_rejected() {
        let mut review = SalesChangeReview::new(SalesChangeReviewId::new("r-1"), data(), "system-1").unwrap();
        assert!(review.update(data()).is_err());
    }
}
