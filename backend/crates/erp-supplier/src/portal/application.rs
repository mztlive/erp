use application_core::AuditActor;
use entity_core::BaseModel;
use entity_macros::Entity;
use erp_core::AccountKind;
use erp_core::validation::normalize_required_text;
use serde::{Deserialize, Serialize};

use super::{CooperationRequest, CooperationResult};
use crate::{Error, Result};

/// 合作条款申请的独立状态。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CooperationStatus {
    Draft,
    Submitted,
    Returned,
    Withdrawn,
    Effective,
}

/// 冻结供应商原始提交；处理决定另外追加。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FrozenSubmission {
    pub submission_no: u32,
    pub proposal: CooperationRequest,
    pub submitted_by: String,
    pub submitted_at: u64,
    pub procurement_owner_id: String,
    pub task_id: String,
}

/// 对应一次冻结提交的撤回、退回或确认事实。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CooperationDecision {
    pub submission_no: u32,
    pub status: CooperationStatus,
    pub actor_id: String,
    pub actor_kind: AccountKind,
    pub reason: Option<String>,
    pub decided_at: u64,
}

/// 供应商商务档案合作申请；申请人和内部确认人独立保存。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Entity)]
pub struct CooperationApplication {
    #[serde(flatten)]
    pub base: BaseModel,
    pub supplier_id: String,
    pub created_by: String,
    pub status: CooperationStatus,
    pub proposal: CooperationRequest,
    pub submissions: Vec<FrozenSubmission>,
    pub decisions: Vec<CooperationDecision>,
    pub result: Option<CooperationResult>,
}

impl CooperationApplication {
    /// 创建当前已鉴权供应商的草稿。
    ///
    /// # 参数
    /// * `id` - 服务器分配的稳定申请标识。
    /// * `supplier_id` - 已鉴权会话中的供应商绑定。
    /// * `proposal` - 允许列表内容。
    /// * `actor` - 真实供应商操作人。
    /// # 返回
    /// 返回尚无正式商务结果的草稿。
    /// # 错误
    /// 身份、目标或商务参数非法时拒绝。
    pub fn new(
        id: String,
        supplier_id: &str,
        proposal: CooperationRequest,
        actor: &AuditActor,
    ) -> Result<Self> {
        ensure_actor(actor, AccountKind::Supplier)?;
        let id = normalize_required_text(id, "申请编号不能为空", 128, "申请编号过长")?;
        let supplier_id =
            normalize_required_text(supplier_id.to_string(), "供应商绑定不能为空", 128, "供应商绑定过长")?;
        Ok(Self {
            base: BaseModel::new(id),
            supplier_id,
            created_by: actor.id().into(),
            status: CooperationStatus::Draft,
            proposal: proposal.normalized()?,
            submissions: Vec::new(),
            decisions: Vec::new(),
            result: None,
        })
    }

    /// 编辑草稿、退回或撤回申请，保留原提交历史。
    ///
    /// # 参数
    /// * `supplier_id` - 当前有效绑定。
    /// * `version` - 当前申请版本。
    /// * `proposal` - 重新核对后的申请内容。
    /// * `actor` - 供应商操作人。
    /// # 返回
    /// 成功时替换可编辑内容。
    /// # 错误
    /// 范围、身份、版本或状态不允许时拒绝。
    pub fn edit(
        &mut self,
        supplier_id: &str,
        version: u64,
        proposal: CooperationRequest,
        actor: &AuditActor,
    ) -> Result<()> {
        self.ensure_supplier(supplier_id, version, actor)?;
        self.ensure_editable()?;
        self.proposal = proposal.normalized()?;
        Ok(())
    }

    /// 冻结一次提交并关联服务器选择的采购任务。
    ///
    /// # 参数
    /// * `supplier_id` - 当前有效绑定。
    /// * `version` - 当前申请版本。
    /// * `owner_id` - 已验证采购处理人。
    /// * `task_id` - 同事务创建的任务标识。
    /// * `actor` - 原始供应商提交人。
    /// * `now` - 提交时间。
    /// # 返回
    /// 追加冻结快照并进入待确认。
    /// # 错误
    /// 身份、版本、状态或任务字段非法时拒绝。
    pub fn submit(
        &mut self,
        supplier_id: &str,
        version: u64,
        owner_id: &str,
        task_id: &str,
        actor: &AuditActor,
        now: u64,
    ) -> Result<()> {
        self.ensure_supplier(supplier_id, version, actor)?;
        self.ensure_editable()?;
        let owner = normalize_required_text(owner_id.into(), "采购处理人不能为空", 128, "处理人过长")?;
        let task = normalize_required_text(task_id.into(), "采购任务不能为空", 128, "采购任务过长")?;
        let submission_no = u32::try_from(self.submissions.len())
            .ok()
            .and_then(|n| n.checked_add(1))
            .ok_or_else(|| Error::ConflictError("提交次数超限".into()))?;
        let proposal = self.proposal.clone().normalized()?;
        self.submissions.push(FrozenSubmission {
            submission_no,
            proposal,
            submitted_by: actor.id().into(),
            submitted_at: now,
            procurement_owner_id: owner,
            task_id: task,
        });
        self.status = CooperationStatus::Submitted;
        Ok(())
    }

    /// 供应商撤回尚未确认的冻结提交。
    ///
    /// # 参数
    /// * `supplier_id` - 当前绑定。
    /// * `version` - 当前申请版本。
    /// * `actor` - 供应商操作人。
    /// * `now` - 撤回时间。
    /// # 返回
    /// 追加撤回事实，原快照保持不变。
    /// # 错误
    /// 身份、范围、状态或版本不允许时拒绝。
    pub fn withdraw(&mut self, supplier_id: &str, version: u64, actor: &AuditActor, now: u64) -> Result<()> {
        self.ensure_supplier(supplier_id, version, actor)?;
        self.finish(CooperationStatus::Withdrawn, actor, None, now)
    }

    /// 内部采购退回并追加供应商可见原因。
    ///
    /// # 参数
    /// * `version` - 当前申请版本。
    /// * `actor` - 内部确认人。
    /// * `reason` - 必填退回原因。
    /// * `now` - 决定时间。
    /// # 返回
    /// 追加退回事实。
    /// # 错误
    /// 身份、版本、状态或原因非法时拒绝。
    pub fn return_to_supplier(
        &mut self,
        version: u64,
        actor: &AuditActor,
        reason: String,
        now: u64,
    ) -> Result<()> {
        self.ensure_version(version)?;
        self.ensure_confirmer(actor)?;
        let reason = normalize_required_text(reason, "退回原因不能为空", 500, "退回原因过长")?;
        self.finish(CooperationStatus::Returned, actor, Some(reason), now)
    }

    /// 在正式写入准备完成后记录内部确认结果。
    ///
    /// # 参数
    /// * `version` - 当前申请版本。
    /// * `actor` - 内部确认人。
    /// * `result` - 同事务正式商务结果。
    /// # 返回
    /// 追加确认事实并形成终态。
    /// # 错误
    /// 身份、状态、版本或结果身份不一致时拒绝。
    pub fn activate(&mut self, version: u64, actor: &AuditActor, result: CooperationResult) -> Result<()> {
        self.ensure_version(version)?;
        self.ensure_confirmer(actor)?;
        if result.confirmed_by != actor.id()
            || result.profile_id.trim().is_empty()
            || result.profile_revision_no == 0
            || self.proposal.expected_supplier_version.checked_add(1) != Some(result.supplier_version)
            || result.profile_id == self.proposal.expected_profile_id
        {
            return Err(Error::ValidationError("合作条款确认结果无效".into()));
        }
        self.finish(CooperationStatus::Effective, actor, None, result.confirmed_at)?;
        self.result = Some(result);
        Ok(())
    }

    /// 读取当前待确认的冻结提交，不采用可编辑草稿作为业务事实。
    ///
    /// # 参数
    /// 无。
    /// # 返回
    /// 返回最后一次冻结提交。
    /// # 错误
    /// 非待确认、无提交或提交内容被改写时拒绝。
    pub fn pending_submission(&self) -> Result<&FrozenSubmission> {
        if self.status != CooperationStatus::Submitted || self.base.is_deleted() {
            return Err(Error::ConflictError("申请已不处于待确认状态".into()));
        }
        let submission = self.submissions.last().ok_or_else(|| Error::Internal("申请缺少冻结提交".into()))?;
        if submission.proposal != self.proposal {
            return Err(Error::ConflictError("待确认提交内容不得修改".into()));
        }
        Ok(submission)
    }

    /// 校验仓储更新保持供应商归属和既有历史不变。
    ///
    /// # 参数
    /// * `before` - 同执行器读取的原申请。
    /// # 返回
    /// 不变式满足时返回空结果。
    /// # 错误
    /// 归属、原提交或历史决定被改写时拒绝。
    pub fn ensure_history_preserved(&self, before: &Self) -> Result<()> {
        if self.base.id != before.base.id
            || self.base.version != before.base.version
            || self.base.created_at != before.base.created_at
            || self.base.deleted_at != before.base.deleted_at
            || self.supplier_id != before.supplier_id
            || self.created_by != before.created_by
            || !self.submissions.starts_with(&before.submissions)
            || !self.decisions.starts_with(&before.decisions)
            || (before.status == CooperationStatus::Effective && self != before)
        {
            return Err(Error::ConflictError("供应商归属及已提交历史不可修改".into()));
        }
        self.ensure_history_transition(before)
    }

    /// 校验真实内部身份及待确认状态；当前任务归属由 Process 重验。
    ///
    /// # 参数
    /// * `actor` - 本次已鉴权内部操作人。
    /// # 返回
    /// 内部身份且处于待确认状态时返回空结果。
    /// # 错误
    /// 外部身份或非待确认时拒绝。
    pub fn ensure_confirmer(&self, actor: &AuditActor) -> Result<()> {
        ensure_actor(actor, AccountKind::Admin)?;
        self.pending_submission()?;
        Ok(())
    }

    /// 供应商身份必须匹配且版本未变；范围外按不存在拒绝。
    fn ensure_supplier(&self, supplier_id: &str, version: u64, actor: &AuditActor) -> Result<()> {
        ensure_actor(actor, AccountKind::Supplier)?;
        if self.supplier_id != supplier_id {
            return Err(Error::NotFound("合作条款申请不可见".into()));
        }
        self.ensure_version(version)
    }

    /// 已删除或版本不一致时拒绝，避免覆盖并发修改。
    fn ensure_version(&self, version: u64) -> Result<()> {
        if self.base.is_deleted() || self.base.version != version {
            return Err(Error::ConflictError("申请已改变，请重新核对后提交".into()));
        }
        Ok(())
    }

    /// 仅草稿、退回、撤回可改内容或再次提交。
    fn ensure_editable(&self) -> Result<()> {
        if !matches!(
            self.status,
            CooperationStatus::Draft | CooperationStatus::Returned | CooperationStatus::Withdrawn
        ) {
            return Err(Error::ConflictError("当前申请不可编辑或再次提交".into()));
        }
        Ok(())
    }

    /// 只允许原地保存，或按当前状态追加一次提交或决定。
    fn ensure_history_transition(&self, before: &Self) -> Result<()> {
        let submissions = self.submissions.len().checked_sub(before.submissions.len());
        let decisions = self.decisions.len().checked_sub(before.decisions.len());
        let allowed = match before.status {
            CooperationStatus::Draft | CooperationStatus::Returned | CooperationStatus::Withdrawn => {
                (self.status == before.status && submissions == Some(0) && decisions == Some(0))
                    || (self.status == CooperationStatus::Submitted
                        && submissions == Some(1)
                        && decisions == Some(0))
            },
            CooperationStatus::Submitted => {
                matches!(
                    self.status,
                    CooperationStatus::Returned | CooperationStatus::Withdrawn | CooperationStatus::Effective
                ) && submissions == Some(0)
                    && decisions == Some(1)
                    && self.proposal == before.proposal
            },
            CooperationStatus::Effective => self == before,
        };
        if !allowed || (self.result.is_some() != (self.status == CooperationStatus::Effective)) {
            return Err(Error::ConflictError("合作条款申请状态与历史不一致".into()));
        }
        if self.status == CooperationStatus::Submitted {
            self.pending_submission()?;
        }
        if before.status == CooperationStatus::Submitted {
            self.ensure_last_decision(before)?;
        }
        Ok(())
    }

    /// 最后一条决定必须对应当前冻结提交，且操作人种类与状态一致。
    fn ensure_last_decision(&self, before: &Self) -> Result<()> {
        let submission = before.pending_submission()?;
        let decision =
            self.decisions.last().ok_or_else(|| Error::ConflictError("申请缺少处理决定".into()))?;
        let internal = matches!(self.status, CooperationStatus::Returned | CooperationStatus::Effective);
        if decision.submission_no != submission.submission_no
            || decision.status != self.status
            || (internal && decision.actor_kind != AccountKind::Admin)
            || (!internal && decision.actor_kind != AccountKind::Supplier)
            || (self.status == CooperationStatus::Returned
                && decision.reason.as_ref().is_none_or(|r| r.trim().is_empty()))
            || (self.status == CooperationStatus::Effective
                && self.result.as_ref().is_none_or(|r| r.confirmed_by != decision.actor_id))
        {
            return Err(Error::ConflictError("合作条款申请处理事实无效".into()));
        }
        Ok(())
    }

    /// 以当前冻结提交号追加决定并切换状态。
    fn finish(
        &mut self,
        status: CooperationStatus,
        actor: &AuditActor,
        reason: Option<String>,
        now: u64,
    ) -> Result<()> {
        let submission_no = self.pending_submission()?.submission_no;
        self.decisions.push(CooperationDecision {
            submission_no,
            status,
            actor_id: actor.id().into(),
            actor_kind: actor.kind(),
            reason,
            decided_at: now,
        });
        self.status = status;
        Ok(())
    }
}

/// 要求操作人种类与账号标识符合本次动作。
///
/// # 参数
/// * `actor` - 已鉴权操作人
/// * `expected` - 允许的账号种类
///
/// # 返回
/// 种类匹配且账号标识去空白后非空时返回 `Ok(())`。
///
/// # 错误
/// 种类不符或账号标识为空时返回 `Forbidden`。
pub(super) fn ensure_actor(actor: &AuditActor, expected: AccountKind) -> Result<()> {
    if actor.kind() != expected || actor.id().trim().is_empty() {
        return Err(Error::Forbidden("操作人身份不允许执行合作条款动作".into()));
    }
    Ok(())
}
