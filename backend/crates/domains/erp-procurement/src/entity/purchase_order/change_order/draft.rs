//! 采购变更草稿恢复时的固定来源与冻结目标一致性规则。

use erp_core::{Error, Result};

use super::{
    PurchaseChangeOrder, PurchaseChangeOrderUpdate, PurchaseChangeSubmission, PurchaseChangeSubmissionLine,
};
use crate::entity::purchase_order::{
    PurchaseOrder, PurchaseOrderRevision, PurchaseOrderRevisionLine, SubmissionStatus,
};

impl PurchaseChangeOrder {
    /// 切换独立编辑副本，保留已驳回状态、审批版本和历史提交。
    /// # 参数
    /// * `submission` / `lines` - 使用新身份的完整草稿头、行。
    /// * `reason` / `content_hash` - 变更原因及完整目标摘要。
    /// * `actor_id` - 保存操作人。
    /// # 返回
    /// 仅切换编辑目标和更新审计，审批版本不变。
    /// # 错误
    /// 非可编辑状态、非草稿提交、来源或行身份不一致时拒绝。
    pub fn save_draft_target(
        &mut self,
        submission: &PurchaseChangeSubmission,
        lines: &[PurchaseChangeSubmissionLine],
        reason: Option<String>,
        content_hash: String,
        actor_id: &str,
    ) -> Result<()> {
        self.ensure_draft_for_submission()?;
        if submission.status != SubmissionStatus::Draft
            || submission.submitted_at.is_some()
            || submission.submitted_by.is_some()
            || submission.purchase_change_order_id.as_ref() != self.base.id
            || submission.base_revision_id != self.base_revision_id
            || self.current_submission_id.as_ref().map(AsRef::as_ref) == Some(submission.base.id.as_str())
            || lines.is_empty()
            || lines.iter().any(|line| line.purchase_change_submission_id.as_ref() != submission.base.id)
        {
            return Err(Error::from("采购变更保存必须使用独立的完整草稿提交"));
        }
        self.update(
            PurchaseChangeOrderUpdate {
                reason,
                current_submission_id: Some(submission.base.id.clone().into()),
                target_content_hash: Some(content_hash),
                ..Default::default()
            },
            actor_id,
        )
    }
    /// 校验编辑仍属于原采购单且基准仍为当前版本。
    ///
    /// # 参数
    /// * `order` - 已由调用方证明更新和提交资格的来源采购单。
    /// # 返回
    /// 草稿及来源身份、当前基准一致时成功。
    /// # 错误
    /// 非草稿、来源不同或当前版本已变化时拒绝。
    pub fn ensure_draft_origin(&self, order: &PurchaseOrder) -> Result<()> {
        self.ensure_draft_for_submission()?;
        if order.base.id != self.purchase_order_id.as_ref() {
            return Err(Error::from("采购变更来源采购单不一致"));
        }
        self.ensure_base_revision_current(order.stable.current_revision_id.as_ref().map(AsRef::as_ref))
    }

    /// 校验初次编辑使用的基准表头和全部行均属于原采购版本。
    ///
    /// # 参数
    /// * `revision` - 变更基准版本表头。
    /// * `lines` - 按稳定顺序读取的完整版本行。
    /// # 返回
    /// 固定来源、版本及非空行集合一致时成功。
    /// # 错误
    /// 版本、采购单或行来源不一致，或缺少明细时拒绝。
    pub fn ensure_draft_base(
        &self,
        revision: &PurchaseOrderRevision,
        lines: &[PurchaseOrderRevisionLine],
    ) -> Result<()> {
        self.ensure_draft_for_submission()?;
        if self.current_submission_id.is_some()
            || revision.base.id != self.base_revision_id.as_ref()
            || revision.purchase_order_id != self.purchase_order_id
            || lines.is_empty()
            || lines.iter().any(|line| line.purchase_order_revision_id != self.base_revision_id)
        {
            return Err(Error::from("采购变更基准版本或明细不一致"));
        }
        Ok(())
    }

    /// 校验撤回后恢复的是当前冻结提交及其完整目标行。
    ///
    /// # 参数
    /// * `submission` - 当前冻结的采购变更提交。
    /// * `lines` - 按稳定顺序读取的完整提交行。
    /// # 返回
    /// 当前指针、变更身份、基准及非空行集合一致时成功。
    /// # 错误
    /// 误用历史提交、跨变更来源或明细缺失时拒绝，禁止回退为基准内容。
    pub fn ensure_draft_target(
        &self,
        submission: &PurchaseChangeSubmission,
        lines: &[PurchaseChangeSubmissionLine],
    ) -> Result<()> {
        self.ensure_draft_for_submission()?;
        if self.current_submission_id.as_ref().map(AsRef::as_ref) != Some(submission.base.id.as_str())
            || submission.purchase_change_order_id.as_ref() != self.base.id
            || submission.base_revision_id != self.base_revision_id
            || lines.is_empty()
            || lines.iter().any(|line| line.purchase_change_submission_id.as_ref() != submission.base.id)
        {
            return Err(Error::from("采购变更当前冻结提交或明细不一致"));
        }
        Ok(())
    }
}

impl PurchaseChangeSubmission {
    /// 将新构建的未提交目标标记为独立编辑副本，不消费正式 CS 序号。
    /// # 参数
    /// 无。
    /// # 返回
    /// 将草稿编号设置为独立 DRAFT 身份。
    /// # 错误
    /// 已提交或已处理的历史目标不得转为编辑副本。
    pub fn mark_editing_copy(&mut self) -> Result<()> {
        if self.status != SubmissionStatus::Draft
            || self.submitted_at.is_some()
            || self.submitted_by.is_some()
        {
            return Err(Error::from("历史采购变更提交不得改为编辑副本"));
        }
        self.submission_no = format!("DRAFT-{}", self.base.id);
        Ok(())
    }
}
