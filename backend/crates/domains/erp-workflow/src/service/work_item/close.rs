//! W29 异常任务的授权、重放与关闭共用同一写事务。
use std::result::Result as StdResult;
use std::sync::Arc;

use application_core::{AuditActor, CommandReceipt};
use erp_core::common::time::Instant;
use mongodb::Database;
use persistence_core::{Executor, Transactional};
use validator::Validate;

use super::write::{
    IDEMPOTENCY_AUDIT_PREFIX, WorkItemWriteError, WorkItemWriteOutcome, expected_task_version, required_text,
    work_item_update_error,
};
use super::{CloseWorkItemRequest, WorkItemMutationOutcome, WorkItemService};
use crate::entity::work_item::{WorkItem, WorkItemCloseData};
use crate::entity::work_item_command::{WorkItemCommandReceipt, WorkItemCommandResult};
use crate::error::{Error, Result};
use crate::ports::{PreparedWorkflowAudit, W29CloseFact, WorkflowAuditOperation, WorkflowAuthorizationPort};
use crate::repository::{WorkItemCommandExt, WorkItemExt};

/// 服务端规范化的关闭命令；不保存事务外的任务或授权快照。
struct CloseCommand {
    id: String,
    actor: AuditActor,
    expected_version: u64,
    decision: W29CloseFact,
    receipt: CommandReceipt,
    audit: PreparedWorkflowAudit,
}

impl<A: WorkflowAuthorizationPort> WorkItemService<A> {
    /// 在同一事务授权并关闭重复、误派或已有有效替代任务。
    /// # 参数
    /// id 为真实任务，req 为关闭原因、预期版本和幂等键，actor 为当前操作人。
    /// # 返回
    /// 成功或同载荷回放返回已应用结果；版本变化返回冲突结果。
    /// # 错误
    /// 无管理资格、任务不支持通用关闭、替代任务非法或持久化失败时拒绝。
    pub async fn close(
        self,
        id: String,
        req: CloseWorkItemRequest,
        actor: AuditActor,
    ) -> Result<WorkItemMutationOutcome> {
        let command = Arc::new(self.close_command(id, req, actor)?);
        let transaction_command = command.clone();
        let service = self.clone();
        let result = self
            .db
            .client()
            .with_transaction(move |executor| {
                Box::pin(async move { service.apply_close(&transaction_command, executor).await })
            })
            .await;
        self.finish_command(&command.receipt, &command.actor, &command.audit, result).await
    }

    /// 只规范化输入和审计；决定关闭的读取全部留在原事务。
    fn close_command(
        &self,
        id: String,
        req: CloseWorkItemRequest,
        actor: AuditActor,
    ) -> Result<CloseCommand> {
        req.validate()?;
        let key = required_text(&req.idempotency_key, "幂等键不能为空")?;
        let reason = required_text(&req.reason_code, "关闭原因代码不能为空")?;
        let replacement = req
            .replacement_work_item_id
            .as_deref()
            .map(|value| required_text(value, "替代任务ID不能为空"))
            .transpose()?;
        let decision =
            self.facts.prepare_w29_close(&reason, req.comment.as_deref(), replacement.as_deref())?;
        let expected_version = expected_task_version(&req.expected_task_version)?;
        let receipt = close_command_receipt(&id, &actor, &key, expected_version, &decision)?;
        let audit = self.prepare_close_audit(&actor, &receipt, &id)?;
        Ok(CloseCommand { id, actor, expected_version, decision, receipt, audit })
    }

    /// 首次执行和回放都从原快照中的真实任务取证。
    async fn apply_close(
        &self,
        command: &CloseCommand,
        executor: &mut dyn Executor,
    ) -> StdResult<WorkItemWriteOutcome, WorkItemWriteError> {
        let item = self.managed_item(&command.id, &command.actor, executor).await?;
        if let Some(replayed) = self.idempotent_replay(&command.receipt, &item, executor).await? {
            return Ok(WorkItemWriteOutcome::Updated(Box::new(replayed)));
        }
        if item.base.version != command.expected_version {
            return Ok(WorkItemWriteOutcome::VersionConflict);
        }
        if !item.is_w29_closable() {
            return Err(Error::BusinessLogicError("只有 W29 登记的异常任务允许受控关闭".into()).into());
        }
        if let Some(id) = command.decision.replacement_work_item_id.as_deref() {
            self.ensure_w29_replacement(&item, id, &command.actor, executor).await?;
        }
        let item = self.close_with_domain_evidence(item, command, executor).await?;
        Ok(WorkItemWriteOutcome::Updated(Box::new(item)))
    }

    /// 替代任务必须来自同一快照、仍开放、同类并处于当前管理范围。
    async fn ensure_w29_replacement(
        &self,
        current: &WorkItem,
        replacement_id: &str,
        actor: &AuditActor,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        if replacement_id == current.base.id {
            return Err(Error::ValidationError("替代任务不能引用自身".into()));
        }
        let replacement = self.managed_item(replacement_id, actor, executor).await?;
        if !replacement.is_w29_replacement_for(current) {
            return Err(Error::ConflictError("替代任务必须是同一 W29 对象类别的开放正式任务".into()));
        }
        Ok(())
    }

    /// 沿原执行器原子保存领域证据、任务终态、审计与强类型回执。
    async fn close_with_domain_evidence(
        &self,
        mut item: WorkItem,
        command: &CloseCommand,
        executor: &mut dyn Executor,
    ) -> StdResult<WorkItem, WorkItemWriteError> {
        let closed_at = Instant::now();
        item.close(
            command.actor.id(),
            WorkItemCloseData { close_reason: command.decision.close_reason.clone() },
            closed_at,
        )?;
        let evidence = command.decision.evidence_reference(&item.base.id, command.receipt.id());
        self.facts
            .persist_w29_close(
                &item,
                &command.decision,
                &evidence,
                command.actor.id(),
                command.receipt.id(),
                closed_at,
                executor,
            )
            .await?;
        self.db.work_items().update(&mut item, executor).await.map_err(work_item_update_error)?;
        self.audit.persist(&command.audit, executor).await?;
        persist_close_receipt(&self.db, &command.receipt, &item, &evidence, &command.audit.id, executor)
            .await?;
        Ok(item)
    }

    /// 写入前冻结并校验关闭事件的安全上下文。
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

#[cfg(test)]
mod tests {
    use erp_core::AccountKind;
    use persistence_core::NoTransaction;
    use persistence_core::testing::SnapshotExecutor;

    use super::super::testing::unavailable_service;
    use super::*;

    #[tokio::test]
    async fn close_uses_the_callers_snapshot_and_stops_before_task_or_receipt_reads_on_denial() {
        let service = unavailable_service();
        let actor = AuditActor::new("manager".into(), "manager".into(), AccountKind::Admin);
        let decision = W29CloseFact { close_reason: "误派".into(), replacement_work_item_id: None };
        let receipt = close_command_receipt("task", &actor, "key", 1, &decision).unwrap();
        let audit = service.prepare_close_audit(&actor, &receipt, "task").unwrap();
        let command =
            CloseCommand { id: "task".into(), actor, expected_version: 1, decision, receipt, audit };
        let error = service.apply_close(&command, &mut NoTransaction).await.err().unwrap();
        assert!(matches!(error, WorkItemWriteError::Service(Error::ValidationError(_))));
        let error = service.apply_close(&command, &mut SnapshotExecutor::default()).await.err().unwrap();
        assert!(
            matches!(error, WorkItemWriteError::Service(Error::Internal(reason)) if reason == "测试授权设施不可用")
        );
    }
}
