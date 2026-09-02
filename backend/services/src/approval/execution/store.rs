//! 单事务应用 PlannedWrites：领域动作、BPM CAS、审批任务与 outbox。

use std::collections::HashMap;

use bpm::engine::{TaskCloseReason, TaskIntent};
use bpm::ids::ApprovalNodeExecutionId;
use bpm::model::types::ApprovalCommandKind;
use bpm::model::{
    ApprovalCommandReceipt, ApprovalInstanceAssignee, ApprovalNodeExecution, ApprovalProcessInstance,
    IdempotencyKey,
};
use entities::approval_integration::{ApprovalNotificationOutbox, ApprovalNotificationTemplateParams};
use entities::common::time::Instant;
use entities::ids::{ApprovalNotificationOutboxId, WorkItemId};
use entities::work_item::DocumentApprovalWorkItemData;
use entities::work_item::{ApprovalRuntimeTaskEnding, WorkItem, WorkItemPriority};

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
    receipts: HashMap<(ApprovalCommandKind, String, IdempotencyKey), ApprovalCommandReceipt>,
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
        idempotency_key: &IdempotencyKey,
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

    /// 向内存适配器注入已有任务，供重复开放任务契约测试使用。
    ///
    /// # 参数
    /// * `item` - 已构造的待办
    ///
    /// # 返回
    /// 无。
    ///
    /// # 错误
    /// 无。
    ///
    /// # 关键业务约束
    /// 仅测试适配器可预置脏数据；生产仓储不得提供绕过 CAS 的注入入口。
    pub fn insert_work_item(&mut self, item: WorkItem) {
        self.work_items.insert(item.base.id.clone(), item);
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
    idempotency_key: &IdempotencyKey,
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
    apply_task_intents(store, writes, ctx)?;
    enqueue_notifications(store, &writes.notifications, ctx)?;
    Ok(())
}

pub(super) fn insert_receipt(
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

/// 按引擎自增后的版本对实例做 CAS 写入。
///
/// # 参数
/// * `store` - 内存事务会话
/// * `instance` - 引擎变更后的实例
///
/// # 返回
/// 新建或版本恰好前进 1 时写入成功。
///
/// # 错误
/// 已存在实例的版本不是 `instance.version - 1` 时返回版本冲突。
///
/// # 关键业务约束
/// 冲突不得覆盖已提交快照，调用方必须回滚。
fn persist_instance(
    store: &mut MemoryRuntimeStore,
    instance: &ApprovalProcessInstance,
) -> std::result::Result<(), ApplyError> {
    if let Some(existing) = store.instances.get(&instance.base.id) {
        let expected = instance
            .base
            .version
            .checked_sub(1)
            .ok_or(ApplyError::VersionConflict)?;
        if existing.base.version != expected {
            return Err(ApplyError::VersionConflict);
        }
    }
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

/// 按引擎自增后的版本替换已有节点执行。
///
/// # 参数
/// * `store` - 内存事务会话
/// * `execution` - 引擎变更后的执行
///
/// # 返回
/// 版本恰好前进 1 时替换成功。
///
/// # 错误
/// 执行不存在或版本不匹配时返回版本冲突。
///
/// # 关键业务约束
/// 不得插入新执行或覆盖并发写入。
fn replace_execution(
    store: &mut MemoryRuntimeStore,
    execution: &ApprovalNodeExecution,
) -> std::result::Result<(), ApplyError> {
    let Some(existing) = store.executions.get(&execution.base.id) else {
        return Err(ApplyError::VersionConflict);
    };
    let expected = execution
        .base
        .version
        .checked_sub(1)
        .ok_or(ApplyError::VersionConflict)?;
    if existing.base.version != expected {
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

/// 完成指定执行下全部开放审批任务。
///
/// # 参数
/// * `store` - 内存事务会话
/// * `execution_id` - 当前结束的节点执行
/// * `actor_id` - 操作人
/// * `now` - 调用方时间
///
/// # 返回
/// 全部开放任务已完成时返回 `Ok(())`。
///
/// # 错误
/// 实体状态变更失败时返回不变量错误。
///
/// # 关键业务约束
/// 重复开放任务必须全部完成。
fn complete_open_task(
    store: &mut MemoryRuntimeStore,
    execution_id: &ApprovalNodeExecutionId,
    actor_id: &str,
    now: Instant,
) -> std::result::Result<(), ApplyError> {
    end_open_tasks(
        store,
        execution_id,
        actor_id,
        &ApprovalRuntimeTaskEnding::Complete,
        now,
    )
}

/// 关闭指定执行下全部开放审批任务。
///
/// # 参数
/// * `store` - 内存事务会话
/// * `execution_id` - 当前结束的节点执行
/// * `reason` - 关闭原因
/// * `actor_id` - 操作人
/// * `now` - 调用方时间
///
/// # 返回
/// 全部开放任务已关闭时返回 `Ok(())`。
///
/// # 错误
/// 实体状态变更失败时返回不变量错误。
///
/// # 关键业务约束
/// 重复开放任务必须全部关闭，语义与生产 Mongo CAS 路径一致。
pub(super) fn close_open_tasks(
    store: &mut MemoryRuntimeStore,
    execution_id: &ApprovalNodeExecutionId,
    reason: &TaskCloseReason,
    actor_id: &str,
    now: Instant,
) -> std::result::Result<(), ApplyError> {
    end_open_tasks(
        store,
        execution_id,
        actor_id,
        &ApprovalRuntimeTaskEnding::Close {
            reason: reason.as_str().to_string(),
        },
        now,
    )
}

/// 结束指定执行下全部开放审批任务，语义与生产 Mongo CAS 路径一致。
///
/// # 参数
/// * `store` - 内存事务会话
/// * `execution_id` - 当前结束的节点执行
/// * `actor_id` - 操作人
/// * `ending` - 完成或关闭
/// * `now` - 调用方时间
///
/// # 返回
/// 全部开放任务已终结时返回 `Ok(())`。
///
/// # 错误
/// 实体状态变更失败时返回不变量错误。
///
/// # 关键业务约束
/// 同一执行的重复开放任务必须全部关闭；不得只处理第一条。
fn end_open_tasks(
    store: &mut MemoryRuntimeStore,
    execution_id: &ApprovalNodeExecutionId,
    actor_id: &str,
    ending: &ApprovalRuntimeTaskEnding,
    now: Instant,
) -> std::result::Result<(), ApplyError> {
    let open: Vec<WorkItem> = store
        .work_items
        .values()
        .filter(|item| {
            item.status == entities::work_item::WorkItemStatus::Open
                && item.approval_node_execution_id.as_ref() == Some(execution_id)
        })
        .cloned()
        .collect();
    let ended = WorkItem::end_all_for_approval_execution(open, execution_id, actor_id, ending, now)
        .map_err(|error| ApplyError::Invariant(error.to_string()))?;
    persist_ended_tasks(store, &ended)
}

/// 按加载版本 CAS 写回已终结任务，语义对齐生产 `persist_ended_approval_tasks`。
///
/// # 参数
/// * `store` - 内存事务会话
/// * `items` - 已由实体方法终结、仍持有加载版本的任务
///
/// # 返回
/// 全部任务仍为开放且版本匹配时写入终结快照。
///
/// # 错误
/// 任一任务缺失、非开放或版本不匹配时返回版本冲突，不写入任何任务。
///
/// # 关键业务约束
/// 先校验再写入，避免只关闭第一条；调用方失败时必须 rollback。
pub(super) fn persist_ended_tasks(
    store: &mut MemoryRuntimeStore,
    items: &[WorkItem],
) -> std::result::Result<(), ApplyError> {
    for item in items {
        let stored = store
            .work_items
            .get(&item.base.id)
            .ok_or(ApplyError::VersionConflict)?;
        if stored.base.version != item.base.version
            || stored.status != entities::work_item::WorkItemStatus::Open
        {
            return Err(ApplyError::VersionConflict);
        }
    }
    for item in items {
        store.work_items.insert(item.base.id.clone(), item.clone());
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

fn receipt_key(
    kind: ApprovalCommandKind,
    scope_id: &str,
    idempotency_key: &IdempotencyKey,
) -> (ApprovalCommandKind, String, IdempotencyKey) {
    (kind, scope_id.to_string(), idempotency_key.clone())
}

fn assignee_key(assignee: &ApprovalInstanceAssignee) -> String {
    format!("{}:{}", assignee.process_instance_id.as_ref(), assignee.node_key)
}

#[cfg(test)]
mod tests {
    use super::*;
    use bpm::engine::TaskCloseReason;
    use bpm::ids::ApprovalNodeExecutionId;
    use entities::ids::WorkItemId;
    use entities::work_item::WorkItemStatus;

    fn open_task(id: &str, execution_id: &str) -> WorkItem {
        WorkItem::new_document_approval(
            WorkItemId::new(id),
            DocumentApprovalWorkItemData {
                approval_node_execution_id: ApprovalNodeExecutionId::new(execution_id),
                business_object_type: "stock_adjustment".into(),
                business_object_id: "adj-1".into(),
                subject_version: "1".into(),
                owner_role: "stock_adjustment_approver".into(),
                owner_organization_id: "org-1".into(),
                owner_user_id: "u1".into(),
                priority: WorkItemPriority::Normal,
                due_at: None,
            },
            Instant::from_unix_secs(10),
        )
        .expect("开放任务")
    }

    /// 同一执行两个 OPEN 任务必须全部关闭，OPEN 数为 0。
    #[test]
    fn memory_store_closes_duplicate_open_tasks() {
        let mut store = MemoryRuntimeStore::default();
        let execution_id = ApprovalNodeExecutionId::new("e1");
        store.insert_work_item(open_task("wi-1", "e1"));
        store.insert_work_item(open_task("wi-2", "e1"));
        assert_eq!(store.open_task_count(&execution_id), 2);
        close_open_tasks(
            &mut store,
            &execution_id,
            &TaskCloseReason::ApprovalRuntimeBlocked,
            "u1",
            Instant::from_unix_secs(11),
        )
        .expect("重复开放任务必须全部关闭");
        assert_eq!(store.open_task_count(&execution_id), 0);
        assert!(store
            .work_items()
            .all(|item| item.status == WorkItemStatus::Closed));
    }

    /// 实例版本跳跃必须 CAS 失败且不覆盖已提交快照。
    #[test]
    fn memory_store_instance_cas_rejects_version_gap() {
        use bpm::ids::{ApprovalProcessDefinitionId, ApprovalProcessInstanceId};
        use bpm::model::{NewProcessInstance, ParticipantId, ProcessKind, SubjectRef, Timestamp};

        let instance = bpm::model::ApprovalProcessInstance::start_running(NewProcessInstance {
            id: ApprovalProcessInstanceId::new("inst"),
            process_definition_id: ApprovalProcessDefinitionId::new("def"),
            definition_version: 1,
            process_kind: ProcessKind::StockAdjustment,
            subject: SubjectRef::new("stock_adjustment", "adj-1").unwrap(),
            subject_version: 1,
            started_by: ParticipantId::new("starter").unwrap(),
            at: Timestamp::from_unix_secs(10).unwrap(),
        })
        .unwrap();
        let mut store = MemoryRuntimeStore::default();
        persist_instance(&mut store, &instance).unwrap();
        let mut drifted = instance.clone();
        drifted.base.version = 3;
        assert_eq!(
            persist_instance(&mut store, &drifted),
            Err(ApplyError::VersionConflict)
        );
        assert_eq!(
            store.instance("inst").unwrap().base.version,
            instance.base.version
        );
    }

    /// 同载荷收据回放、异载荷冲突。
    #[test]
    fn memory_store_receipt_replay_matches_duplicate_semantics() {
        use bpm::model::types::ApprovalCommandKind;
        use bpm::model::{ApprovalCommandIdentity, CanonicalCommandPayload, CommandPayloadField, Timestamp};

        let mut store = MemoryRuntimeStore::default();
        let kind = ApprovalCommandKind::StartApproval;
        let key = IdempotencyKey::parse("key-1").unwrap();
        let identity = ApprovalCommandIdentity::new(
            kind,
            "approval.runtime.start",
            key.clone(),
            CanonicalCommandPayload::new().field(CommandPayloadField::Text("stock_adjustment")),
            CanonicalCommandPayload::new().field(CommandPayloadField::Text("start")),
        )
        .unwrap();
        let receipt = ApprovalCommandReceipt::new(
            bpm::ids::ApprovalCommandReceiptId::new("r1"),
            &identity,
            "result-1",
            Timestamp::from_unix_secs(10).unwrap(),
        )
        .unwrap();
        insert_receipt(&mut store, &receipt).unwrap();
        assert_eq!(
            insert_receipt(&mut store, &receipt),
            Err(ApplyError::DuplicateReceipt)
        );
        let same = replay_after_duplicate(
            &store,
            kind,
            receipt.scope_id.as_str(),
            &key,
            receipt.payload_digest.as_str(),
        )
        .unwrap();
        assert_eq!(same.payload_digest, receipt.payload_digest);
        let conflict =
            replay_after_duplicate(&store, kind, receipt.scope_id.as_str(), &key, "other").unwrap_err();
        assert!(conflict
            .to_string()
            .contains("APPROVAL_IDEMPOTENCY_PAYLOAD_CONFLICT"));
    }

    /// Mongo 契约已上收到 `runtime_persistence_contract`；本文件只保留内存原语。
    #[test]
    fn memory_close_is_covered_by_shared_persistence_contract() {
        assert!(include_str!("runtime_persistence_contract.rs")
            .contains("run_memory_runtime_persistence_contract"));
        assert!(include_str!("runtime_persistence_contract.rs")
            .contains("mongo_adapter_satisfies_runtime_persistence_contract"));
    }
}
