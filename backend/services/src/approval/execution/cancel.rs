//! 业务撤回与受阻取消编排。

use bpm::engine::{cancel, CancelCommand};
use bpm::ids::{ApprovalCommandReceiptId, ApprovalNodeExecutionId, ApprovalProcessInstanceId};
use bpm::model::types::{ApprovalCommandKind, ApprovalNodeExecutionStatus, ApprovalProcessInstanceStatus};
use bpm::model::{
    ApprovalCommandReceipt, ApprovalNodeExecution, ApprovalProcessInstance, IdempotencyKey, ParticipantId,
    SubjectRef,
};
use database::{BpmExt, Executor, WorkItemExt};
use entities::work_item::{WorkItem, WorkItemStatus, WorkItemType};
use mongodb::Database;

use super::apply_plan::{apply_plan, DomainActionKind};
use super::authorization::requires_blocked_cancel;
use super::idempotency::{
    cancel_blocked_identity, cancel_identity, document_cancel_identity, map_receipt_first_write_error,
    normalize_idempotency_key, payload_conflict_error, PreparedCommandIdentity, ReceiptBranch,
};
use super::start::map_engine_error;
use super::{ExecutionCommandInput, PlannedWrites, PreparedExecution};
use crate::errors::{Error, Result};

const EXECUTION_HISTORY_PAGE_SIZE: u32 = 50;
const MAX_EXECUTION_HISTORY_PAGES: usize = 32;

/// 已在查库前规范化的业务单据撤回命令。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DocumentCancelCommand {
    subject: SubjectRef,
    subject_version: u32,
    expected_document_version: u64,
    reason: String,
    actor: ParticipantId,
    idempotency_key: IdempotencyKey,
}

impl DocumentCancelCommand {
    /// 在任何仓储读取前冻结普通撤回的完整业务身份。
    ///
    /// # 错误
    /// 主体版本、业务版本、原因、操作人或幂等键不合法时返回校验错误。
    pub fn new(
        subject: SubjectRef,
        subject_version: u32,
        expected_document_version: u64,
        reason: &str,
        actor_id: &str,
        idempotency_key: &str,
    ) -> Result<Self> {
        if subject_version == 0 {
            return Err(Error::ValidationError("审批主题版本必须从 1 开始".to_string()));
        }
        if expected_document_version == 0 {
            return Err(Error::ValidationError("业务单据版本必须从 1 开始".to_string()));
        }
        let reason = normalize_document_cancel_reason(reason)?;
        let actor =
            ParticipantId::new(actor_id).map_err(|_| Error::ValidationError("撤回人引用无效".to_string()))?;
        let idempotency_key = normalize_idempotency_key(idempotency_key)?;
        Ok(Self {
            subject,
            subject_version,
            expected_document_version,
            reason,
            actor,
            idempotency_key,
        })
    }

    /// 返回精确业务主体。
    pub fn subject(&self) -> &SubjectRef {
        &self.subject
    }

    /// 返回冻结提交版本。
    pub fn subject_version(&self) -> u32 {
        self.subject_version
    }

    /// 返回强业务乐观锁版本。
    pub fn expected_document_version(&self) -> u64 {
        self.expected_document_version
    }

    /// 返回规范化撤回原因。
    pub fn reason(&self) -> &str {
        &self.reason
    }

    /// 返回规范化撤回人。
    pub fn actor(&self) -> &ParticipantId {
        &self.actor
    }

    /// 返回规范化幂等键。
    pub fn idempotency_key(&self) -> &IdempotencyKey {
        &self.idempotency_key
    }
}

/// 已由不可变 receipt 与完整终态事实共同证明的普通撤回结果。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DocumentCancelReplayProof {
    receipt: ApprovalCommandReceipt,
    instance: ApprovalProcessInstance,
}

impl DocumentCancelReplayProof {
    /// 返回撤回命令的稳定结果引用。
    pub fn result_ref(&self) -> &str {
        &self.receipt.result_ref
    }

    /// 返回已取消的精确审批实例。
    pub fn instance(&self) -> &ApprovalProcessInstance {
        &self.instance
    }

    /// 返回已分类的命令收据。
    pub fn receipt(&self) -> &ApprovalCommandReceipt {
        &self.receipt
    }
}

/// 规范化业务单据撤回原因。
///
/// # 错误
/// 去除首尾空白后为空时返回校验错误。
pub fn normalize_document_cancel_reason(reason: &str) -> Result<String> {
    let reason = reason.trim();
    if reason.is_empty() {
        return Err(Error::ValidationError("撤回原因不能为空".to_string()));
    }
    Ok(reason.to_string())
}

/// 按 V3 与已知历史 scope 的精确顺序读取普通撤回收据。
pub async fn find_document_cancel_receipt(
    db: &Database,
    identity: &PreparedCommandIdentity,
    executor: &mut dyn Executor,
) -> Result<Option<ApprovalCommandReceipt>> {
    for scope in identity.scope_candidates() {
        let receipt = db
            .bpm_workflow()
            .find_command_receipt(
                ApprovalCommandKind::CancelApproval,
                scope,
                identity.idempotency_key(),
                executor,
            )
            .await?;
        if receipt.is_some() {
            return Ok(receipt);
        }
    }
    Ok(None)
}

/// 在同一新会话快照内回读并证明一个已经提交的业务单据撤回。
///
/// 本函数只读取，不重跑 Fresh 命令。调用方必须在同一个事务快照内先完成当前
/// 授权重验，再调用本函数；失败事务后的恢复每次都必须创建新会话。
pub async fn replay_committed_document_cancel(
    db: &Database,
    command: &DocumentCancelCommand,
    executor: &mut dyn Executor,
) -> Result<Option<DocumentCancelReplayProof>> {
    let Some(instance) = db
        .bpm_workflow()
        .cancellation_instance_by_subject(command.subject(), command.subject_version(), executor)
        .await?
    else {
        return Ok(None);
    };
    if instance.subject != *command.subject() || instance.subject_version != command.subject_version() {
        return Err(document_cancel_terminal_conflict());
    }
    if instance.status != ApprovalProcessInstanceStatus::Cancelled {
        return Ok(None);
    }
    if instance.current_node_execution_id.is_some() || instance.ended_at.is_none() {
        return Err(document_cancel_terminal_conflict());
    }

    let ended_execution = cancelled_execution(db, &instance, executor).await?;
    let tasks = db
        .work_items()
        .approval_tasks_for_execution(
            &ApprovalNodeExecutionId::new(ended_execution.base.id.clone()),
            executor,
        )
        .await?;
    let expected_task_version = terminal_task_version(&tasks, &instance, &ended_execution, command)?;
    let expected_instance_version = instance
        .base
        .version
        .checked_sub(1)
        .ok_or_else(document_cancel_terminal_conflict)?;
    let expected_execution_version = ended_execution
        .base
        .version
        .checked_sub(1)
        .ok_or_else(document_cancel_terminal_conflict)?;
    let identity = document_cancel_identity(
        command.idempotency_key().clone(),
        &instance.base.id,
        command.subject_version(),
        command.expected_document_version(),
        expected_instance_version,
        expected_execution_version,
        expected_task_version,
        command.reason(),
        command.actor().as_str(),
    )?;
    let Some(receipt) = find_document_cancel_receipt(db, &identity, executor).await? else {
        return Ok(None);
    };
    if !matches!(identity.classify(Some(&receipt)), ReceiptBranch::SamePayload(_)) {
        return Err(payload_conflict_error());
    }
    if receipt.result_ref != instance.base.id {
        return Err(document_cancel_terminal_conflict());
    }
    Ok(Some(DocumentCancelReplayProof { receipt, instance }))
}

/// 以命令收据作为事务第一笔物理写，并持久化取消运行事实与关闭任务。
///
/// 调用方只能在本函数返回后写业务单据、通知和审计；所有写入必须复用同一事务
/// executor。
pub async fn claim_and_persist_document_cancel_runtime(
    db: &Database,
    writes: &PlannedWrites,
    closed_tasks: &[WorkItem],
    executor: &mut dyn Executor,
) -> Result<()> {
    if writes.receipt.command_kind != ApprovalCommandKind::CancelApproval
        || !writes.receipt.scope_id.starts_with("v3:")
        || !writes.receipt.payload_digest.starts_with("v3:")
        || writes.receipt.result_ref != writes.instance.base.id
        || writes.instance.status != ApprovalProcessInstanceStatus::Cancelled
    {
        return Err(Error::Internal("业务单据撤回计划身份非法".to_string()));
    }
    db.bpm_workflow()
        .insert_command_receipt(&writes.receipt, executor)
        .await
        .map_err(map_receipt_first_write_error)?;
    db.bpm_workflow()
        .persist_cancelled_runtime_after_receipt(&writes.instance, &writes.updated_executions, executor)
        .await?;
    db.work_items()
        .persist_cancelled_approval_tasks(closed_tasks, executor)
        .await?;
    Ok(())
}

async fn cancelled_execution(
    db: &Database,
    instance: &ApprovalProcessInstance,
    executor: &mut dyn Executor,
) -> Result<ApprovalNodeExecution> {
    let instance_id = ApprovalProcessInstanceId::new(instance.base.id.clone());
    let mut after_execution_no = None;
    let mut candidates = Vec::new();
    for page_index in 0..MAX_EXECUTION_HISTORY_PAGES {
        let page = db
            .bpm_workflow()
            .list_execution_history(
                &instance_id,
                after_execution_no,
                EXECUTION_HISTORY_PAGE_SIZE,
                executor,
            )
            .await?;
        if page.is_empty() {
            break;
        }
        let next_after = page.last().map(|row| row.execution_no);
        candidates.extend(
            page.iter()
                .filter(|row| {
                    row.status == ApprovalNodeExecutionStatus::Cancelled
                        && row.round_no == instance.current_round_no
                        && row.ended_at == instance.ended_at
                        && row.process_instance_id.as_ref() == instance.base.id
                })
                .cloned(),
        );
        if page.len() < EXECUTION_HISTORY_PAGE_SIZE as usize {
            break;
        }
        if next_after == after_execution_no || page_index + 1 == MAX_EXECUTION_HISTORY_PAGES {
            return Err(document_cancel_terminal_conflict());
        }
        after_execution_no = next_after;
    }
    let [execution] = candidates.as_slice() else {
        return Err(document_cancel_terminal_conflict());
    };
    Ok(execution.clone())
}

fn terminal_task_version(
    tasks: &[WorkItem],
    instance: &ApprovalProcessInstance,
    execution: &ApprovalNodeExecution,
    command: &DocumentCancelCommand,
) -> Result<Option<u64>> {
    match tasks {
        [] => {
            let Some(blocker) = execution.blocker_code else {
                return Err(document_cancel_terminal_conflict());
            };
            if requires_blocked_cancel(blocker) {
                return Err(document_cancel_terminal_conflict());
            }
            Ok(None)
        }
        [task]
            if execution.blocker_code.is_none()
                && task.work_item_type == WorkItemType::DocumentApproval
                && task.status == WorkItemStatus::Closed
                && task.approval_node_execution_id.as_ref().map(AsRef::as_ref)
                    == Some(execution.base.id.as_str())
                && task.business_object_type == instance.subject.subject_kind()
                && task.business_object_id == instance.subject.subject_id()
                && task.subject_version == instance.subject_version.to_string()
                && task.closed_by.as_deref() == Some(command.actor().as_str())
                && task.close_reason.as_deref() == Some(command.reason()) =>
        {
            task.base
                .version
                .checked_sub(1)
                .map(Some)
                .ok_or_else(document_cancel_terminal_conflict)
        }
        _ => Err(document_cancel_terminal_conflict()),
    }
}

fn document_cancel_terminal_conflict() -> Error {
    Error::ConflictError("业务单据撤回收据与终态事实不一致".to_string())
}

/// 取消编排输入。
#[derive(Debug, Clone)]
pub struct CancelExecutionInput {
    /// 公共输入。
    pub command: ExecutionCommandInput,
    /// 当前实例。
    pub instance: ApprovalProcessInstance,
    /// 当前执行。
    pub current: ApprovalNodeExecution,
    /// 提交版本。
    pub subject_version: u32,
    /// 期望实例版本。
    pub expected_instance_version: u64,
    /// 期望执行版本。
    pub expected_execution_version: u64,
    /// 可空任务版本。
    pub expected_task_version: Option<u64>,
    /// 非空原因。
    pub reason: String,
    /// 取消人。
    pub actor: ParticipantId,
    /// 是否关闭开放任务。
    pub close_open_task: bool,
    /// 是否受阻取消端口。
    pub blocked_port: bool,
    /// 收据主键。
    pub receipt_id: ApprovalCommandReceiptId,
}

/// 规划取消。允许原审批人恢复的 blocker 走业务取消，其余 blocker 必须走受阻取消。
///
/// # 参数
/// * `input` - 取消输入
///
/// # 错误
/// 端口与 blocker 类别不匹配、异载荷冲突或引擎失败时返回错误。
pub fn prepare_cancel(input: CancelExecutionInput) -> Result<PreparedExecution> {
    prepare_cancel_with_document_version(input, None)
}

/// 规划业务单据普通撤回，并把业务乐观锁版本纳入命令收据摘要。
///
/// # 参数
/// * `input` - 普通撤回输入；不得标记为受阻取消端口
/// * `expected_document_version` - 调用方已重验的业务单据版本
///
/// # 错误
/// 输入误用受阻端口、端口与 blocker 类别不匹配、异载荷冲突或引擎失败时
/// 返回错误。
pub fn prepare_document_cancel(
    input: CancelExecutionInput,
    expected_document_version: u64,
) -> Result<PreparedExecution> {
    if input.blocked_port {
        return Err(Error::ValidationError(
            "业务单据普通撤回不得使用受阻取消端口".to_string(),
        ));
    }
    prepare_cancel_with_document_version(input, Some(expected_document_version))
}

/// 使用可选业务单据版本形成取消计划。
fn prepare_cancel_with_document_version(
    input: CancelExecutionInput,
    expected_document_version: Option<u64>,
) -> Result<PreparedExecution> {
    let identity = if input.blocked_port {
        let blocker = input
            .instance
            .blocker_code
            .or(input.current.blocker_code)
            .ok_or_else(|| Error::ValidationError("受阻取消缺少 blocker".to_string()))?;
        if !requires_blocked_cancel(blocker) {
            return Err(Error::ValidationError(
                "原审批人可恢复时不得走受阻取消".to_string(),
            ));
        }
        cancel_blocked_identity(
            input.command.idempotency_key.clone(),
            &input.instance.base.id,
            blocker.as_str(),
            input.expected_instance_version,
            input.expected_execution_version,
            input.expected_task_version,
            &input.reason,
            input.actor.as_str(),
        )
    } else {
        if input.instance.blocker_code.is_some_and(requires_blocked_cancel) {
            return Err(Error::ValidationError(
                "不可恢复原审批人的阻塞只能走受阻取消".to_string(),
            ));
        }
        match expected_document_version {
            Some(version) => document_cancel_identity(
                input.command.idempotency_key.clone(),
                &input.instance.base.id,
                input.subject_version,
                version,
                input.expected_instance_version,
                input.expected_execution_version,
                input.expected_task_version,
                &input.reason,
                input.actor.as_str(),
            ),
            None => cancel_identity(
                input.command.idempotency_key.clone(),
                &input.instance.base.id,
                input.subject_version,
                input.expected_instance_version,
                input.expected_execution_version,
                input.expected_task_version,
                &input.reason,
                input.actor.as_str(),
            ),
        }
    }?;
    match identity.classify(input.command.receipt.as_ref()) {
        ReceiptBranch::PayloadConflict => return Err(super::idempotency::payload_conflict_error()),
        ReceiptBranch::SamePayload(receipt) => {
            return Ok(PreparedExecution::Replay {
                receipt: receipt.clone(),
            });
        }
        ReceiptBranch::Fresh => {}
    }
    let plan = cancel(
        input.instance,
        input.current,
        CancelCommand {
            actor: input.actor,
            reason: input.reason,
            close_open_task: input.close_open_task,
            now: input.command.now,
        },
    )
    .map_err(map_engine_error)?;
    let receipt = ApprovalCommandReceipt::new(
        input.receipt_id,
        identity.current(),
        plan.instance.base.id.clone(),
        input.command.now,
    )
    .map_err(|error| Error::ValidationError(error.to_string()))?;
    Ok(PreparedExecution::Apply(Box::new(apply_plan(
        plan,
        receipt,
        Some(DomainActionKind::Cancel),
    ))))
}
