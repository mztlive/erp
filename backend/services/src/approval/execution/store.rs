//! 单事务应用 PlannedWrites：领域动作、BPM CAS、审批任务与 outbox。

use std::collections::HashMap;

use bpm::engine::{TaskCloseReason, TaskIntent};
use bpm::ids::ApprovalNodeExecutionId;
use bpm::model::types::ApprovalCommandKind;
use bpm::model::{
    ApprovalCommandReceipt, ApprovalInstanceAssignee, ApprovalNodeExecution, ApprovalProcessInstance,
};
use entities::approval_integration::{ApprovalNotificationOutbox, ApprovalNotificationTemplateParams};
use entities::common::time::Instant;
use entities::ids::{ApprovalNotificationOutboxId, WorkItemId};
use entities::work_item::work_item::DocumentApprovalWorkItemData;
use entities::work_item::{WorkItem, WorkItemCloseData, WorkItemPriority};

use super::apply_plan::{DomainActionKind, PlannedWrites};
use super::notification_outbox::NotificationIntent;
use crate::errors::{Error, Result};

/// 事务应用失败。调用方必须整体回滚。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ApplyError {
    /// 收据唯一键冲突，事务外重读。
    DuplicateReceipt,
    /// 乐观锁或状态竞争。
    VersionConflict,
    /// 领域动作失败。
    DomainActionFailed(String),
    /// 写入不变量失败。
    Invariant(String),
}

impl From<ApplyError> for Error {
    /// 将应用失败映射为服务错误。
    fn from(error: ApplyError) -> Self {
        match error {
            ApplyError::DuplicateReceipt => super::idempotency::payload_conflict_error(),
            ApplyError::VersionConflict => {
                Error::ConflictError("数据已被其他请求修改，请刷新后重试".to_string())
            }
            ApplyError::DomainActionFailed(message) | ApplyError::Invariant(message) => {
                Error::BusinessLogicError(message)
            }
        }
    }
}

/// 创建审批任务所需的业务上下文。
#[derive(Debug, Clone)]
pub struct TaskApplyContext {
    /// 新任务主键。
    pub work_item_id: String,
    /// 单据类型稳定代码。
    pub business_object_type: String,
    /// 单据主键。
    pub business_object_id: String,
    /// 冻结提交版本。
    pub subject_version: String,
    /// 合同签署的责任角色。
    pub owner_role: String,
    /// 责任组织。
    pub owner_organization_id: String,
    /// 当前操作人。
    pub actor_id: String,
    /// 调用方时间。
    pub now: Instant,
}

/// 强类型领域动作执行器。
pub trait DomainActionExecutor {
    /// 在写入 BPM 终态前执行签署动作。
    ///
    /// # 错误
    /// 领域动作失败时返回错误，调用方必须回滚。
    fn execute(&self, kind: DomainActionKind) -> std::result::Result<(), ApplyError>;
}

/// 始终成功的测试动作。
#[derive(Debug, Default)]
pub struct RecordingDomainActions {
    /// 已执行动作。
    pub executed: std::cell::RefCell<Vec<DomainActionKind>>,
    /// 预设失败。
    pub fail: bool,
}

impl DomainActionExecutor for RecordingDomainActions {
    fn execute(&self, kind: DomainActionKind) -> std::result::Result<(), ApplyError> {
        if self.fail {
            return Err(ApplyError::DomainActionFailed("领域动作失败".to_string()));
        }
        self.executed.borrow_mut().push(kind);
        Ok(())
    }
}

/// 内存事务会话。begin 后失败必须 rollback，禁止半提交。
#[derive(Debug, Default, Clone)]
pub struct MemoryRuntimeStore {
    receipts: HashMap<String, ApprovalCommandReceipt>,
    instances: HashMap<String, ApprovalProcessInstance>,
    executions: HashMap<String, ApprovalNodeExecution>,
    assignees: HashMap<String, ApprovalInstanceAssignee>,
    work_items: HashMap<String, WorkItem>,
    outbox: HashMap<String, ApprovalNotificationOutbox>,
    snapshot: Option<Box<MemoryRuntimeStore>>,
}

impl MemoryRuntimeStore {
    /// 开启事务快照。
    ///
    /// # 返回
    /// 无。
    pub fn begin(&mut self) {
        let mut clone = self.clone();
        clone.snapshot = None;
        self.snapshot = Some(Box::new(clone));
    }

    /// 提交事务并丢弃快照。
    ///
    /// # 返回
    /// 无。
    pub fn commit(&mut self) {
        self.snapshot = None;
    }

    /// 回滚到 begin 时的快照。
    ///
    /// # 返回
    /// 无。
    pub fn rollback(&mut self) {
        if let Some(snapshot) = self.snapshot.take() {
            *self = *snapshot;
        }
    }

    /// 按收据键查找。
    ///
    /// # 参数
    /// * `kind` - 命令种类
    /// * `scope_id` - 作用域
    /// * `idempotency_key` - 幂等键
    ///
    /// # 返回
    /// 命中时返回收据。
    pub fn find_receipt(
        &self,
        kind: ApprovalCommandKind,
        scope_id: &str,
        idempotency_key: &str,
    ) -> Option<&ApprovalCommandReceipt> {
        self.receipts.get(&receipt_key(kind, scope_id, idempotency_key))
    }

    /// 返回已持久化实例。
    ///
    /// # 参数
    /// * `id` - 实例主键
    ///
    /// # 返回
    /// 命中时返回实例。
    pub fn instance(&self, id: &str) -> Option<&ApprovalProcessInstance> {
        self.instances.get(id)
    }

    /// 返回指定执行的开放任务数。
    ///
    /// # 参数
    /// * `execution_id` - 节点执行
    ///
    /// # 返回
    /// 返回 OPEN 任务数量。
    pub fn open_task_count(&self, execution_id: &ApprovalNodeExecutionId) -> usize {
        self.work_items
            .values()
            .filter(|item| {
                item.status == entities::work_item::WorkItemStatus::Open
                    && item.approval_node_execution_id.as_ref() == Some(execution_id)
            })
            .count()
    }

    /// 返回全部任务。
    ///
    /// # 返回
    /// 返回任务切片。
    pub fn work_items(&self) -> impl Iterator<Item = &WorkItem> {
        self.work_items.values()
    }

    /// 返回 outbox 条目。
    ///
    /// # 返回
    /// 返回通知记录。
    pub fn outbox_items(&self) -> impl Iterator<Item = &ApprovalNotificationOutbox> {
        self.outbox.values()
    }
}

/// 在同一事务快照内应用计划。任一失败回滚。
///
/// 顺序：领域动作 → 收据 → 实例 → 执行 → 绑定 → 任务 → outbox。
///
/// # 参数
/// * `store` - 事务会话
/// * `writes` - 计划写入
/// * `ctx` - 任务映射上下文
/// * `domain` - 领域动作
///
/// # 错误
/// 重复收据、版本冲突或领域动作失败时回滚并返回错误。
pub fn commit_writes(
    store: &mut MemoryRuntimeStore,
    writes: &PlannedWrites,
    ctx: &TaskApplyContext,
    domain: &dyn DomainActionExecutor,
) -> std::result::Result<(), ApplyError> {
    store.begin();
    if let Err(error) = apply_all(store, writes, ctx, domain) {
        store.rollback();
        return Err(error);
    }
    store.commit();
    Ok(())
}

/// 收据 duplicate-key 后按已提交收据回读。
///
/// # 参数
/// * `store` - 已提交会话
/// * `kind` - 命令种类
/// * `scope_id` - 作用域
/// * `idempotency_key` - 幂等键
/// * `digest` - 本次摘要
///
/// # 返回
/// 同载荷返回已提交收据，异载荷冲突。
pub fn replay_after_duplicate(
    store: &MemoryRuntimeStore,
    kind: ApprovalCommandKind,
    scope_id: &str,
    idempotency_key: &str,
    digest: &str,
) -> Result<ApprovalCommandReceipt> {
    let receipt = store
        .find_receipt(kind, scope_id, idempotency_key)
        .ok_or_else(|| Error::ConflictError("命令收据不存在".to_string()))?;
    match super::idempotency::classify_receipt(Some(receipt), digest) {
        super::idempotency::ReceiptBranch::SamePayload(item) => Ok(item.clone()),
        _ => Err(super::idempotency::payload_conflict_error()),
    }
}

fn apply_all(
    store: &mut MemoryRuntimeStore,
    writes: &PlannedWrites,
    ctx: &TaskApplyContext,
    domain: &dyn DomainActionExecutor,
) -> std::result::Result<(), ApplyError> {
    if let Some(kind) = writes.domain_action {
        domain.execute(kind)?;
    }
    insert_receipt(store, &writes.receipt)?;
    persist_instance(store, &writes.instance)?;
    for execution in &writes.updated_executions {
        replace_execution(store, execution)?;
    }
    for execution in &writes.created_executions {
        insert_execution(store, execution)?;
    }
    for assignee in &writes.created_assignees {
        store.assignees.insert(assignee_key(assignee), assignee.clone());
    }
    for assignee in &writes.updated_assignees {
        store.assignees.insert(assignee_key(assignee), assignee.clone());
    }
    apply_task_intents(store, writes, ctx)?;
    enqueue_notifications(store, &writes.notifications, ctx)?;
    Ok(())
}

fn insert_receipt(
    store: &mut MemoryRuntimeStore,
    receipt: &ApprovalCommandReceipt,
) -> std::result::Result<(), ApplyError> {
    let key = receipt_key(receipt.command_kind, &receipt.scope_id, &receipt.idempotency_key);
    if store.receipts.contains_key(&key) {
        return Err(ApplyError::DuplicateReceipt);
    }
    store.receipts.insert(key, receipt.clone());
    Ok(())
}

fn persist_instance(
    store: &mut MemoryRuntimeStore,
    instance: &ApprovalProcessInstance,
) -> std::result::Result<(), ApplyError> {
    store.instances.insert(instance.base.id.clone(), instance.clone());
    Ok(())
}

fn insert_execution(
    store: &mut MemoryRuntimeStore,
    execution: &ApprovalNodeExecution,
) -> std::result::Result<(), ApplyError> {
    if store.executions.contains_key(&execution.base.id) {
        return Err(ApplyError::Invariant("执行主键重复".to_string()));
    }
    store
        .executions
        .insert(execution.base.id.clone(), execution.clone());
    Ok(())
}

fn replace_execution(
    store: &mut MemoryRuntimeStore,
    execution: &ApprovalNodeExecution,
) -> std::result::Result<(), ApplyError> {
    if !store.executions.contains_key(&execution.base.id) {
        return Err(ApplyError::VersionConflict);
    }
    store
        .executions
        .insert(execution.base.id.clone(), execution.clone());
    Ok(())
}

fn apply_task_intents(
    store: &mut MemoryRuntimeStore,
    writes: &PlannedWrites,
    ctx: &TaskApplyContext,
) -> std::result::Result<(), ApplyError> {
    for intent in &writes.create_tasks {
        let TaskIntent::HumanTaskRequested {
            execution_id,
            assignee,
            ..
        } = intent
        else {
            continue;
        };
        if assignee.as_str().trim().is_empty() {
            return Err(ApplyError::Invariant("审批任务责任人不能为空".to_string()));
        }
        let item = WorkItem::new_document_approval(
            WorkItemId::new(ctx.work_item_id.clone()),
            DocumentApprovalWorkItemData {
                approval_node_execution_id: execution_id.clone(),
                business_object_type: ctx.business_object_type.clone(),
                business_object_id: ctx.business_object_id.clone(),
                subject_version: ctx.subject_version.clone(),
                owner_role: ctx.owner_role.clone(),
                owner_organization_id: ctx.owner_organization_id.clone(),
                owner_user_id: assignee.as_str().to_string(),
                priority: WorkItemPriority::Normal,
                due_at: None,
            },
            ctx.now,
        )
        .map_err(|error| ApplyError::Invariant(error.to_string()))?;
        store.work_items.insert(item.base.id.clone(), item);
    }
    for execution_id in &writes.complete_tasks {
        complete_open_task(store, execution_id, &ctx.actor_id, ctx.now)?;
    }
    for (execution_id, reason) in &writes.close_tasks {
        close_open_tasks(store, execution_id, reason, &ctx.actor_id, ctx.now)?;
    }
    Ok(())
}

fn complete_open_task(
    store: &mut MemoryRuntimeStore,
    execution_id: &ApprovalNodeExecutionId,
    actor_id: &str,
    now: Instant,
) -> std::result::Result<(), ApplyError> {
    let ids: Vec<String> = store
        .work_items
        .values()
        .filter(|item| {
            item.status == entities::work_item::WorkItemStatus::Open
                && item.approval_node_execution_id.as_ref() == Some(execution_id)
        })
        .map(|item| item.base.id.clone())
        .collect();
    for id in ids {
        let item = store.work_items.get_mut(&id).ok_or(ApplyError::VersionConflict)?;
        item.complete_by_approval_runtime(actor_id, now)
            .map_err(|error| ApplyError::Invariant(error.to_string()))?;
    }
    Ok(())
}

fn close_open_tasks(
    store: &mut MemoryRuntimeStore,
    execution_id: &ApprovalNodeExecutionId,
    reason: &TaskCloseReason,
    actor_id: &str,
    now: Instant,
) -> std::result::Result<(), ApplyError> {
    let ids: Vec<String> = store
        .work_items
        .values()
        .filter(|item| {
            item.status == entities::work_item::WorkItemStatus::Open
                && item.approval_node_execution_id.as_ref() == Some(execution_id)
        })
        .map(|item| item.base.id.clone())
        .collect();
    for id in ids {
        let item = store.work_items.get_mut(&id).ok_or(ApplyError::VersionConflict)?;
        item.close_by_approval_runtime(
            actor_id,
            WorkItemCloseData {
                close_reason: reason.as_str().to_string(),
            },
            now,
        )
        .map_err(|error| ApplyError::Invariant(error.to_string()))?;
    }
    Ok(())
}

fn enqueue_notifications(
    store: &mut MemoryRuntimeStore,
    intents: &[NotificationIntent],
    ctx: &TaskApplyContext,
) -> std::result::Result<(), ApplyError> {
    for intent in intents {
        if store
            .outbox
            .values()
            .any(|item| item.dedup_key == intent.dedup_key)
        {
            continue;
        }
        let record = ApprovalNotificationOutbox::enqueue(
            ApprovalNotificationOutboxId::new(intent.dedup_key.clone()),
            intent.dedup_key.clone(),
            intent.event_kind,
            vec![ctx.actor_id.clone()],
            ApprovalNotificationTemplateParams {
                document_type_label: "库存调整单".into(),
                document_no: ctx.business_object_id.clone(),
                current_node_name: "审批节点".into(),
                current_approver_display_name: ctx.actor_id.clone(),
                round_no: 1,
                reject_reason_summary: None,
            },
            ctx.now,
        )
        .map_err(|error| ApplyError::Invariant(error.to_string()))?;
        store.outbox.insert(record.base.id.clone(), record);
    }
    Ok(())
}

fn receipt_key(kind: ApprovalCommandKind, scope_id: &str, idempotency_key: &str) -> String {
    format!("{}:{scope_id}:{idempotency_key}", kind.as_str())
}

fn assignee_key(assignee: &ApprovalInstanceAssignee) -> String {
    format!("{}:{}", assignee.process_instance_id.as_ref(), assignee.node_key)
}
