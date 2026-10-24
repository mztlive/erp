//! 销售变更提交启动：加载定义图、构造 `prepare_start` 输入并持久化运行事实。

use application_core::AuditActor;
use bpm::engine::{DefinitionGraph, StartAssigneeBinding};
use bpm::model::{ApprovalCommandReceipt, ParticipantId, SubjectRef, Timestamp};
use erp_audit::AuditLog;
use erp_core::common::time::Instant;
use erp_identity::SharedRbacService;
use erp_read_models::sales_center::access::SalesAccess;
use erp_sales::service::sales_review::SalesChangeSubmissionWrite;
use erp_workflow::entity::approval_integration::{ApprovalStartIdentity, ApprovalSubjectSnapshotPayload};
use erp_workflow::entity::document_registry::business_document::ApprovalDefinitionBinding;
use erp_workflow::entity::document_registry::{DocumentType, WorkflowAction};
use erp_workflow::repository::prelude::*;
use erp_workflow::service::approval::execution::apply_plan::PlannedWrites;
use erp_workflow::service::approval::execution::authorization::AuthorizationFailure;
use erp_workflow::service::approval::execution::idempotency::normalize_idempotency_key;
use erp_workflow::service::approval::execution::start_input::{
    StartInputContext, assemble_start_input, start_bindings,
};
use erp_workflow::service::approval::execution::{
    PreparedExecution, StartExecutionInput, map_receipt_first_write_error,
};
use erp_workflow::service::approval::process_kind::process_kind_of;
use erp_workflow::service::approval::read;
use erp_workflow::{BpmExt, DocumentRegistryExt};
use mongodb::Database;
use persistence_core::{Executor, NoTransaction, Transactional};

use super::adapter::sales_change_order_object_readable;
use crate::approval_start::{
    StartReplayErrors, StartTaskContext, persist_start_runtime, persist_start_snapshot, persist_start_tasks,
    replay_start,
};
use crate::audit::persist_log;
use crate::{Error, Result};

/// 加载绑定定义图。缺失时失败关闭，不得用空图启动。
///
/// # 参数
/// * `db` - 数据库
/// * `binding` - 创建时冻结的定义绑定
///
/// # 返回
/// 返回已持久化的定义图。
///
/// # 错误
/// 定义不存在或仓储失败时返回冲突或仓储错误。
pub(super) async fn load_bound_definition_graph(
    db: &Database,
    binding: &ApprovalDefinitionBinding,
) -> Result<DefinitionGraph> {
    read::bound_definition_graph(db, binding, "销售变更单绑定的审批定义不存在", &mut NoTransaction)
        .await
        .map_err(Error::from)
}

/// 读取同载荷启动收据；不存在时返回 `None`。
///
/// # 参数
/// * `db` - 数据库
/// * `subject` - 业务对象引用
/// * `subject_version` - 冻结提交版本
/// * `idempotency_key` - 调用方幂等键
///
/// # 返回
/// 已提交收据或空。
///
/// # 错误
/// 幂等键非法或仓储失败时返回错误。
pub(super) async fn load_start_receipt(
    db: &Database,
    subject: &SubjectRef,
    subject_version: u32,
    idempotency_key: &str,
) -> Result<Option<ApprovalCommandReceipt>> {
    read::start_receipt(
        db,
        DocumentType::SalesChangeOrder,
        subject,
        subject_version,
        idempotency_key,
        &mut NoTransaction,
    )
    .await
    .map_err(Error::from)
}

/// 在 fresh 事务快照内按完整 V3/legacy 身份回读已提交的销售变更启动结果。
///
/// # 参数
/// * `db` - 数据库。
/// * `subject` - 业务对象引用。
/// * `subject_version` - 冻结提交版本。
/// * `idempotency_key` - 调用方幂等键。
/// * `binding` - 创建时冻结的定义绑定。
/// * `actor_id` - 启动人。
/// * `executor` - 当前事务执行器。
///
/// # 返回
/// 已提交且与冻结事实一致的审批实例 ID。没有同载荷收据时为 `None`。
///
/// # 错误
/// 幂等键非法、收据载荷冲突、收据指向的实例不存在、实例与冻结事实不一致，或仓储读取失败时返回错误。
pub(super) async fn replay_sales_change_start_with_executor(
    db: &Database,
    subject: &SubjectRef,
    subject_version: u32,
    idempotency_key: &str,
    binding: &ApprovalDefinitionBinding,
    actor_id: &str,
    executor: &mut dyn Executor,
) -> Result<Option<String>> {
    replay_start(
        db,
        ApprovalStartIdentity {
            process_kind: process_kind_of(DocumentType::SalesChangeOrder),
            subject,
            subject_version,
            binding,
            actor_id,
        },
        idempotency_key,
        StartReplayErrors {
            missing_instance: "销售变更启动收据引用的审批实例不存在",
            identity_mismatch: "销售变更启动收据与冻结运行事实不一致",
        },
        executor,
    )
    .await
}

/// 销售变更启动输入。
///
/// # 用途
/// 收拢 `build_sales_change_start_input` 的定义图、绑定与提交人参数。
///
/// # 参数
/// 无。
///
/// # 返回
/// 无。
///
/// # 错误
/// 无。
///
/// # 关键业务约束
/// 审批人取自已发布节点，不接受客户端选择。
pub(super) struct SalesChangeStartInput<'a> {
    /// 绑定定义图。
    pub graph: DefinitionGraph,
    /// 冻结绑定。
    pub binding: &'a ApprovalDefinitionBinding,
    /// 业务对象引用。
    pub subject: SubjectRef,
    /// 冻结提交版本。
    pub subject_version: u32,
    /// 提交人。
    pub actor_id: &'a str,
    /// 单据责任组织。
    pub organization_id: &'a str,
    /// 规范化前的幂等键。
    pub idempotency_key: &'a str,
    /// 已存在收据。
    pub receipt: Option<ApprovalCommandReceipt>,
    /// 调用方时间。
    pub now: Instant,
}

/// 由定义图与单据组织构造启动输入。
///
/// 审批人取自已发布节点，不接受客户端选择。对象读取权失败时收敛为 BLOCKED。
///
/// # 用途
/// 把启动参数收敛为引擎 `prepare_start` 输入。
///
/// # 参数
/// * `input` - 定义图、绑定、主体与提交人
///
/// # 返回
/// 返回可交给 `prepare_start` 的输入。
///
/// # 错误
/// 入口缺失、审批人非法、幂等键非法或读取权校验失败时返回错误。
///
/// # 关键业务约束
/// 定义版本必须与冻结绑定一致；对象读取权失败时收敛为 BLOCKED。
pub(super) fn build_sales_change_start_input(
    input: SalesChangeStartInput<'_>,
) -> Result<StartExecutionInput> {
    let SalesChangeStartInput {
        graph,
        binding,
        subject,
        subject_version,
        actor_id,
        organization_id,
        idempotency_key,
        receipt,
        now,
    } = input;
    if graph.definition.definition_version != binding.approval_definition_version {
        return Err(Error::ConflictError("销售变更单绑定定义版本与已加载定义不一致".to_string()));
    }
    let idempotency_key = normalize_idempotency_key(idempotency_key)?;
    let actor =
        ParticipantId::new(actor_id).map_err(|_| Error::ValidationError("提交人引用无效".to_string()))?;
    let timestamp = Timestamp::from_utc(now.as_utc());
    let bindings = start_bindings_from_graph(&graph, organization_id)?;
    Ok(assemble_start_input(
        graph,
        bindings,
        StartInputContext {
            binding,
            document_type: DocumentType::SalesChangeOrder,
            subject,
            subject_version,
            actor,
            idempotency_key,
            receipt,
            now: timestamp,
        },
    )?)
}

/// 为定义全部节点冻结启动绑定，并按单据组织重验对象读取权。
///
/// # 参数
/// * `graph` - 定义图
/// * `organization_id` - 单据责任组织
///
/// # 返回
/// 返回与节点一一对应的绑定。
///
/// # 错误
/// 节点审批人引用非法或显示名为空时返回校验错误。
fn start_bindings_from_graph(
    graph: &DefinitionGraph,
    organization_id: &str,
) -> Result<Vec<StartAssigneeBinding>> {
    Ok(start_bindings(
        graph,
        |assignee| match sales_change_order_object_readable(organization_id, assignee) {
            Ok(_) => None,
            Err(_) => Some(AuthorizationFailure::CannotReadSubject),
        },
        "审批定义没有节点，无法启动销售变更审批",
    )?)
}

/// 销售变更提交事务内需要一并写入的冻结提交。
///
/// # 用途
/// 收拢提交快照、启动计划、责任组织与审计，供同一事务写入。
///
/// # 参数
/// 无。
///
/// # 返回
/// 无。
///
/// # 错误
/// 无。
///
/// # 关键业务约束
/// 运行事实、不可变快照与入口任务必须与提交快照同事务。
pub(super) struct SalesChangeStartPersistInput {
    /// 销售本域冻结提交与副本写入计划。
    pub sales_write: SalesChangeSubmissionWrite,
    /// 工作流动作。
    pub workflow_action: WorkflowAction,
    /// 冻结快照载荷。
    pub snapshot_payload: ApprovalSubjectSnapshotPayload,
    /// `prepare_start` 结果。
    pub prepared: PreparedExecution,
    /// 合同签署的责任角色。
    pub owner_role: &'static str,
    /// 责任组织。
    pub organization_id: String,
    /// 调用方时间。
    pub now: Instant,
    /// 已构造审计。
    pub audit: AuditLog,
    /// 已认证操作人，用于事务内重验来源销售单范围。
    pub actor: AuditActor,
    /// 与写事务绑定的授权源。
    pub rbac: SharedRbacService,
}

/// 在同一事务中写入提交快照、单据迁移、快照、BPM 运行事实与入口任务。
///
/// # 用途
/// 提交启动后原子写入变更单、提交快照与运行事实。
///
/// # 参数
/// * `db` - 数据库
/// * `input` - 提交写入集合
///
/// # 返回
/// 成功时无返回值。
///
/// # 错误
/// 仓储写入失败或计划不完整时返回错误，事务回滚。
///
/// # 关键业务约束
/// Replay 不得重复写运行事实；Apply 必须写入快照与入口任务。
pub(super) async fn persist_sales_change_start(
    db: &Database,
    input: SalesChangeStartPersistInput,
) -> Result<()> {
    let db = db.clone();
    let client = db.client().clone();
    if !matches!(input.prepared, PreparedExecution::Apply(_)) {
        return Ok(());
    }
    client.with_transaction(move |executor| Box::pin(async move { input.persist(&db, executor).await })).await
}

impl SalesChangeStartPersistInput {
    /// 在入口事务中重验来源授权，并依原顺序写收据、业务事实、运行态和审计。
    async fn persist(mut self, db: &Database, executor: &mut dyn Executor) -> Result<()> {
        let PreparedExecution::Apply(writes) = self.prepared else {
            return Ok(());
        };
        SalesAccess::new(db.clone(), self.rbac)
            .require_object(
                &self.actor,
                "submit",
                self.sales_write.change().sales_order_id.as_ref(),
                &[],
                executor,
            )
            .await
            .map_err(Error::from)?;
        db.bpm_workflow()
            .insert_command_receipt(&writes.receipt, executor)
            .await
            .map_err(map_receipt_first_write_error)?;
        let guarded = db
            .business_documents()
            .mark_approval_started(
                writes.instance.subject.subject_id(),
                DocumentType::SalesChangeOrder,
                &writes.instance.process_definition_id,
                writes.instance.definition_version,
                self.now,
                executor,
            )
            .await?;
        if guarded.is_none() {
            return Err(Error::ConflictError("销售变更单审批启动守卫冲突，请刷新后重试".to_string()));
        }
        self.sales_write.persist(db, executor).await?;
        db.workflow_actions().create(&self.workflow_action, executor).await?;
        persist_runtime_writes(
            db,
            &writes,
            &self.snapshot_payload,
            self.owner_role,
            &self.organization_id,
            self.now,
            executor,
        )
        .await?;
        persist_log(db, &self.audit, executor).await?;

        Ok(())
    }
}

/// 将启动计划写入 BPM 集合、不可变快照和入口 WorkItem。
///
/// # 错误
/// 计划缺少入口执行或写入失败时返回错误。
async fn persist_runtime_writes(
    db: &Database,
    writes: &PlannedWrites,
    snapshot_payload: &ApprovalSubjectSnapshotPayload,
    owner_role: &str,
    organization_id: &str,
    now: Instant,
    executor: &mut dyn Executor,
) -> Result<()> {
    let snapshot = persist_start_runtime(
        db,
        writes,
        DocumentType::SalesChangeOrder,
        snapshot_payload,
        now,
        "启动计划缺少入口执行，不得提交销售变更单",
        executor,
    )
    .await?;
    persist_start_snapshot(db, snapshot, executor).await?;
    persist_start_tasks(
        db,
        writes,
        StartTaskContext { document_type: DocumentType::SalesChangeOrder, owner_role, organization_id, now },
        executor,
    )
    .await
    .map(|_| ())
}
