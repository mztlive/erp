//! W29 异常任务受控关闭。

use std::result::Result as StdResult;
use std::sync::Arc;

use application_core::{AuditActor, CommandReceipt};
use erp_core::common::time::Instant;
use mongodb::Database;
use persistence_core::{Executor, Transactional};
use validator::Validate;

use super::access::{ActorAccess, ensure_generic_work_item_mutation, ensure_item_in_managed_scope};
use super::write::{
    IDEMPOTENCY_AUDIT_PREFIX, WorkItemWriteError, WorkItemWriteOutcome, expected_task_version,
    record_command_attempt, recover_command_error, required_text, work_item_update_error,
};
use super::{CloseWorkItemRequest, WorkItemConflictKind, WorkItemMutationOutcome, WorkItemService};
use crate::entity::work_item::{WorkItem, WorkItemCloseData};
use crate::entity::work_item_command::{WorkItemCommandReceipt, WorkItemCommandResult};
use crate::error::{Error, Result};
use crate::ports::{PreparedWorkflowAudit, W29CloseFact, WorkflowAuditOperation};
use crate::repository::{WorkItemCommandExt, WorkItemExt};

/// W29 关闭命令的领域证据与审计输入。
///
/// # 用途
/// 将关闭事务所需字段打包，供 [`WorkItemService::close_with_domain_evidence`] 使用。
///
/// # 参数
/// 无
///
/// # 返回
/// 无
///
/// # 错误
/// 无
///
/// # 关键业务约束
/// 关闭必须在同一事务内写入领域证据与任务终态。
struct CloseDomainEvidenceInput<'a> {
    /// 待关闭任务。
    item: WorkItem,
    /// 操作人。
    actor: &'a AuditActor,
    /// 已规范化的 W29 关闭决策。
    decision: W29CloseFact,
    /// 强类型幂等命令收据。
    receipt: CommandReceipt,
}

impl<A: crate::ports::WorkflowAuthorizationPort + Send + Sync + 'static> WorkItemService<A> {
    /// 关闭重复、误派或已有有效替代任务。
    ///
    /// # 参数
    /// * `id` - 待关闭任务 ID
    /// * `req` - 关闭原因、预期版本与幂等键
    /// * `actor` - 当前操作人
    ///
    /// # 返回
    /// 关闭成功或同载荷回放时返回已应用结果。预期版本与当前版本不一致，或写入遇到版本冲突时，返回版本冲突结果。
    ///
    /// # 错误
    /// 缺少管理权限、任务类型禁止通用关闭、原因或替代任务非法、对象不可访问或仓储失败时返回错误。版本陈旧本身不是错误。
    pub async fn close(
        self,
        id: String,
        req: CloseWorkItemRequest,
        actor: AuditActor,
    ) -> Result<WorkItemMutationOutcome> {
        let managed_access = self.managed_access(&actor).await?;
        let item = self.load(id.clone()).await?;
        ensure_generic_work_item_mutation(&item)?;
        req.validate()?;
        let idempotency_key = required_text(&req.idempotency_key, "幂等键不能为空")?;
        let reason_code = required_text(&req.reason_code, "关闭原因代码不能为空")?;
        let replacement_id = req
            .replacement_work_item_id
            .as_deref()
            .map(|value| required_text(value, "替代任务ID不能为空"))
            .transpose()?;
        let decision =
            self.facts.prepare_w29_close(&reason_code, req.comment.as_deref(), replacement_id.as_deref())?;
        let expected_task_version = expected_task_version(&req.expected_task_version)?;
        let receipt = close_command_receipt(&id, &actor, &idempotency_key, expected_task_version, &decision)?;
        if let Some(replayed) = self.idempotent_replay(&receipt, &id, &actor).await? {
            ensure_generic_work_item_mutation(&replayed)?;
            ensure_item_in_managed_scope(&replayed, &managed_access)?;
            self.ensure_object_participation(&actor, &replayed).await?;
            return self.applied_outcome(replayed, &actor).await;
        }
        if item.base.version != expected_task_version {
            return self.conflict_outcome(&id, WorkItemConflictKind::Version, &actor).await;
        }
        ensure_item_in_managed_scope(&item, &managed_access)?;
        self.ensure_object_participation(&actor, &item).await?;
        if !item.is_w29_closable() {
            return Err(Error::BusinessLogicError("只有 W29 登记的异常任务允许受控关闭".to_string()));
        }
        if let Some(replacement_id) = decision.replacement_work_item_id.as_deref() {
            self.ensure_w29_replacement(&item, replacement_id, &actor, &managed_access).await?;
        }
        let updated = self
            .close_with_domain_evidence(CloseDomainEvidenceInput { item, actor: &actor, decision, receipt })
            .await?;
        match updated {
            WorkItemWriteOutcome::Updated(item) => self.applied_outcome(*item, &actor).await,
            WorkItemWriteOutcome::VersionConflict => {
                self.conflict_outcome(&id, WorkItemConflictKind::Version, &actor).await
            },
        }
    }

    /// 校验重复关闭所引用的替代任务仍是同类、开放且位于当前管理范围。
    async fn ensure_w29_replacement(
        &self,
        current: &WorkItem,
        replacement_id: &str,
        actor: &AuditActor,
        access: &ActorAccess,
    ) -> Result<()> {
        if replacement_id == current.base.id {
            return Err(Error::ValidationError("替代任务不能引用自身".to_string()));
        }
        let replacement = self.load(replacement_id.to_string()).await?;
        if !replacement.is_w29_replacement_for(current) {
            return Err(Error::ConflictError("替代任务必须是同一 W29 对象类别的开放正式任务".to_string()));
        }
        ensure_item_in_managed_scope(&replacement, access)?;
        self.ensure_object_participation(actor, &replacement).await
    }

    /// 在同一事务内写入 W29 领域证据、关闭任务并登记审计。
    ///
    /// # 用途
    /// 将任务关闭与领域对象证据写入同一事务。
    ///
    /// # 参数
    /// * `input` - 任务、关闭原因与审计字段
    ///
    /// # 返回
    /// 返回写入结果或版本冲突。
    ///
    /// # 错误
    /// 替代任务非法、领域对象不存在或仓储失败时返回错误。
    ///
    /// # 关键业务约束
    /// 仅 W29 可关闭任务允许走此路径；替代任务必须是同类开放正式任务。
    async fn close_with_domain_evidence(
        &self,
        input: CloseDomainEvidenceInput<'_>,
    ) -> Result<WorkItemWriteOutcome> {
        let CloseDomainEvidenceInput { mut item, actor, decision, receipt } = input;
        let closed_at = Instant::now();
        item.close(actor.id(), WorkItemCloseData { close_reason: decision.close_reason.clone() }, closed_at)?;
        let evidence_reference = decision.evidence_reference(&item.base.id, receipt.id());
        let replay_receipt = receipt.clone();
        let replay_item_id = item.base.id.clone();
        let audit = self.prepare_close_audit(actor, &receipt, &item.base.id)?;
        let actor_id = actor.id().to_string();
        let receipt_id = receipt.id().to_string();
        let db = self.db.clone();
        let facts = Arc::clone(&self.facts);
        let attempt_audit = audit.clone();
        let audit_port = Arc::clone(&self.audit);
        let client = db.client().clone();
        let result = client
            .with_transaction(move |executor| {
                Box::pin(async move {
                    facts
                        .persist_w29_close(
                            &item,
                            &decision,
                            &evidence_reference,
                            &actor_id,
                            &receipt_id,
                            closed_at,
                            executor,
                        )
                        .await?;
                    db.work_items().update(&mut item, executor).await.map_err(work_item_update_error)?;
                    audit_port.persist(&audit, executor).await?;
                    persist_close_receipt(&db, &receipt, &item, &evidence_reference, &audit.id, executor)
                        .await?;
                    Ok::<WorkItem, WorkItemWriteError>(item)
                })
            })
            .await;
        self.record_close_attempt(&attempt_audit, &result).await;
        match result {
            Ok(item) => Ok(WorkItemWriteOutcome::Updated(Box::new(item))),
            Err(WorkItemWriteError::VersionConflict) => Ok(WorkItemWriteOutcome::VersionConflict),
            Err(WorkItemWriteError::Service(error)) => recover_command_error(
                error,
                self.idempotent_replay(&replay_receipt, &replay_item_id, actor).await,
            )
            .map(|item| WorkItemWriteOutcome::Updated(Box::new(item))),
        }
    }

    /// 在关闭写入前冻结并校验安全审计上下文。
    /// # 参数
    /// * `actor` - 命令操作人及调用上下文。
    /// * `receipt` - 已规范化的命令身份。
    /// * `item_id` - 原任务编号。
    /// # 返回
    /// 返回保留调用上下文的关闭审计事实。
    /// # 错误
    /// 审计动作或上下文校验失败时返回原错误。
    fn prepare_close_audit(
        &self,
        actor: &AuditActor,
        receipt: &CommandReceipt,
        item_id: &str,
    ) -> Result<PreparedWorkflowAudit> {
        let audit = PreparedWorkflowAudit::resource_with_message(
            actor.clone(),
            receipt.action(),
            receipt.resource_type(),
            item_id.to_string(),
            Some("异常任务已按正式关闭决定终结".to_string()),
        )?
        .for_command(receipt.id().to_string(), WorkflowAuditOperation::Closed)?;
        self.audit.validate(&audit)?;
        Ok(audit)
    }

    /// 在关闭事务结束后按原错误分类记录一次尝试。
    /// # 参数
    /// * `audit` - 写入前冻结的安全上下文。
    /// * `result` - 关闭事务的原始结果。
    /// # 返回
    /// 无；保留原事务结果，尝试失败不覆盖业务错误。
    /// # 错误
    /// 无；尝试持久化错误由统一记录入口处理。
    async fn record_close_attempt(
        &self,
        audit: &PreparedWorkflowAudit,
        result: &StdResult<WorkItem, WorkItemWriteError>,
    ) {
        let version_error = Error::ConflictError("任务版本已变化".into());
        if let Err(error) = result {
            let error = match error {
                WorkItemWriteError::Service(error) => error,
                WorkItemWriteError::VersionConflict => &version_error,
            };
            record_command_attempt(self.audit.as_ref(), audit, error).await;
        }
    }
}

/// 由已校验关闭输入构造原指纹与稳定命令身份。
/// # 参数
/// * `id` - 原任务编号。
/// * `actor` - 命令操作人。
/// * `idempotency_key` - 已校验幂等键。
/// * `expected_task_version` - 原请求任务版本。
/// * `decision` - 已规范化的正式关闭决定。
/// # 返回
/// 返回保留原字段次序与规范化规则的命令身份。
/// # 错误
/// 命令身份或指纹输入非法时返回原错误。
fn close_command_receipt(
    id: &str,
    actor: &AuditActor,
    idempotency_key: &str,
    expected_task_version: u64,
    decision: &W29CloseFact,
) -> Result<CommandReceipt> {
    Ok(CommandReceipt::from_resource_parts(
        IDEMPOTENCY_AUDIT_PREFIX,
        actor.id(),
        "work_item.close",
        "work_item",
        id,
        idempotency_key,
        [
            expected_task_version.to_string(),
            decision.close_reason.clone(),
            decision.replacement_work_item_id.clone().unwrap_or_default(),
        ],
    )?)
}

/// 在原关闭事务内保存正式终态对应的强类型结果。
/// # 参数
/// * `db` - 原事务使用的数据库。
/// * `receipt` - 已规范化的命令身份。
/// * `item` - 已写入关闭终态的任务。
/// * `evidence_reference` - 原领域证据引用。
/// * `audit_id` - 同事务关闭事件编号。
/// * `executor` - 原调用方事务执行器。
/// # 返回
/// 成功保存唯一命令结果时返回 `Ok(())`。
/// # 错误
/// 结果校验或仓储写入失败时返回原错误。
async fn persist_close_receipt(
    db: &Database,
    receipt: &CommandReceipt,
    item: &WorkItem,
    evidence_reference: &str,
    audit_id: &str,
    executor: &mut dyn Executor,
) -> Result<()> {
    let committed = WorkItemCommandReceipt::new(
        receipt,
        WorkItemCommandResult::Closed {
            work_item_id: item.base.id.clone(),
            task_version: item.base.version,
            evidence_reference: evidence_reference.to_string(),
        },
        audit_id.to_string(),
    )?;
    db.work_item_command_receipts().create(&committed, executor).await?;
    Ok(())
}
