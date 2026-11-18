//! 销售单提交启动：加载定义图、构造 `prepare_start` 输入并持久化运行事实。
//!
//! `SalesOrder` 与 `VoucherSalesOrder` 共用本模块，按 `DocumentType` 分派
//! `ProcessKind`、快照与任务归属，不得回退 `CARD_SALES_APPROVAL`。

use std::sync::Arc;

use bpm::engine::{DefinitionGraph, StartAssigneeBinding};
use bpm::model::{ParticipantId, SubjectRef, Timestamp};
use erp_approval_runtime::ApprovalRuntimePort;
use erp_approval_runtime::approval_start::{StartTaskContext, persist_start_runtime, persist_start_tasks};
use erp_core::common::time::Instant;
use erp_identity::AuthorizationService;
use erp_sales::dto::sales_order::SubmissionView;
use erp_sales::entity::sales_order::SalesOrder;
use erp_sales::service::sales_order::SalesOrderService;
use erp_sales::service::sales_order::mapper::submission_view;
use erp_workflow::entity::approval_integration::ApprovalSubjectSnapshotPayload;
use erp_workflow::entity::document_registry::DocumentType;
use erp_workflow::entity::document_registry::business_document::ApprovalDefinitionBinding;
use erp_workflow::repository::prelude::*;
use erp_workflow::service::approval::execution::authorization::AuthorizationFailure;
use erp_workflow::service::approval::execution::idempotency::normalize_idempotency_key;
use erp_workflow::service::approval::execution::start_input::{
    StartInputContext, assemble_start_input, start_bindings,
};
use erp_workflow::service::approval::execution::{
    PlannedWrites, PreparedExecution, StartExecutionInput, map_receipt_first_write_error,
};
use erp_workflow::service::approval::read;
use erp_workflow::service::approval::resubmission::complete_resubmission;
use erp_workflow::{BpmExt, DocumentRegistryExt};
use mongodb::Database;
use persistence_core::Executor;

use super::SalesOrderCommandProcess;
use super::adapter::sales_order_object_readable;
use super::adapters::catalog::CatalogQualificationAdapter;
use super::authorization::SalesCommandAccess;
use crate::order_to_cash::command_event::{SalesCommandEvent, replay_submission_with_executor};
use crate::{Error, Result};

// 使用调用方执行器加载绑定定义图，供创建并提交的同一事务复用。
//
// `binding` 为创建时冻结的定义绑定。返回已转成引擎图的已持久化定义图。定义不存在时返回冲突；仓储失败时返回对应错误。
pub(super) async fn load_bound_definition_graph_with_executor(
    db: &Database,
    binding: &ApprovalDefinitionBinding,
    executor: &mut dyn Executor,
) -> Result<DefinitionGraph> {
    read::bound_definition_graph(db, binding, "销售单绑定的审批定义不存在", executor)
        .await
        .map_err(Error::from)
}

// 销售提交时，按实际单据类型、审批主体、冻结版本和原幂等键在原事务查询已提交启动回执。
// 未命中返回 `None`；幂等键非法或仓储失败返回原错误。
pub(super) async fn load_start_receipt(
    db: &Database,
    document_type: DocumentType,
    subject: &SubjectRef,
    subject_version: u32,
    idempotency_key: &str,
    executor: &mut dyn Executor,
) -> Result<Option<bpm::model::ApprovalCommandReceipt>> {
    read::start_receipt(db, document_type, subject, subject_version, idempotency_key, executor)
        .await
        .map_err(Error::from)
}

/// 销售单启动输入。
///
/// # 用途
/// 收拢 `build_sales_order_start_input` 的定义图、绑定、单据类型与提交人参数。
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
/// 审批人取自已发布节点，不接受客户端选择；`document_type` 必须按业务性质分派。
pub(super) struct SalesOrderStartInput<'a> {
    pub resubmission_reason: Option<String>,
    /// 绑定定义图。
    pub graph: DefinitionGraph,
    /// 冻结绑定。
    pub binding: &'a ApprovalDefinitionBinding,
    /// 按业务性质分派的单据类型。
    pub document_type: DocumentType,
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
    pub receipt: Option<bpm::model::ApprovalCommandReceipt>,
    /// 调用方时间。
    pub now: Instant,
}

// 由定义图与单据组织构造启动输入。
//
// 审批人取自已发布节点，不接受客户端选择。对象读取权失败时收敛为 BLOCKED。
//
// 把启动参数收敛为引擎 `prepare_start` 输入。
//
// `input` 为定义图、绑定、单据类型、主体与提交人。返回可交给 `prepare_start` 的输入。入口缺失、审批人非法、幂等键非法或读取权校验失败时返回错误。
//
// 定义版本必须与冻结绑定一致；对象读取权失败时收敛为 BLOCKED。
pub(super) fn build_sales_order_start_input(input: SalesOrderStartInput<'_>) -> Result<StartExecutionInput> {
    let SalesOrderStartInput {
        resubmission_reason,
        graph,
        binding,
        document_type,
        subject,
        subject_version,
        actor_id,
        organization_id,
        idempotency_key,
        receipt,
        now,
    } = input;
    if graph.definition.definition_version != binding.approval_definition_version {
        return Err(Error::ConflictError("销售单的审批流程版本不一致，请联系管理员核对".to_string()));
    }

    let idempotency_key = normalize_idempotency_key(idempotency_key)?;
    let actor = ParticipantId::new(actor_id)
        .map_err(|_| Error::ValidationError("无法确认提交人账号，请重新登录后再提交".to_string()))?;
    let timestamp = Timestamp::from_utc(now.as_utc());
    let bindings = start_bindings_from_graph(&graph, organization_id)?;
    Ok(assemble_start_input(
        graph,
        bindings,
        StartInputContext {
            resubmission_reason,
            binding,
            document_type,
            subject,
            subject_version,
            actor,
            idempotency_key,
            receipt,
            now: timestamp,
        },
    )?)
}

// 为定义全部节点冻结启动绑定，并按单据组织重验对象读取权。
//
// `graph` 为定义图；`organization_id` 为单据责任组织。返回与节点一一对应的绑定。节点审批人引用非法或显示名为空时返回校验错误。
fn start_bindings_from_graph(
    graph: &DefinitionGraph,
    organization_id: &str,
) -> Result<Vec<StartAssigneeBinding>> {
    Ok(start_bindings(
        graph,
        |assignee| match sales_order_object_readable(organization_id, assignee) {
            Ok(_) => None,
            Err(_) => Some(AuthorizationFailure::CannotReadSubject),
        },
        "审批定义没有节点，无法启动销售单审批",
    )?)
}

/// 销售提交事务内需要一并写入的冻结提交。
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
pub(super) struct SalesOrderStartPersistInput {
    /// 与销售命令共享的身份源，审批运行事实落库前重验分配资格。
    pub identity: Arc<AuthorizationService>,
    pub resubmission_reason: Option<String>,
    /// 已进入审批中的销售单。
    pub order: SalesOrder,
    /// 已锁定的工作副本。
    pub working_copy: erp_sales::entity::sales_order::SalesOrderWorkingCopy,
    /// 冻结提交头。
    pub submission: erp_sales::entity::sales_order::SalesOrderSubmission,
    /// 冻结提交行。
    pub submission_lines: Vec<erp_sales::entity::sales_order::SalesOrderSubmissionLine>,
    /// 工作流动作。
    pub workflow_action: erp_workflow::entity::document_registry::WorkflowAction,
    /// 按业务性质分派的单据类型。
    pub document_type: DocumentType,
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
    pub audit: SalesCommandEvent,
    /// 本次提交附带的草稿替换写入计划。
    pub working_copy_plan: SalesOrderWorkingCopyPersistPlan,
    /// 事务内必须重新确认的商品池精确引用。
    pub sellable_refs: Vec<(String, String)>,
}

/// 销售提交事务内的工作副本写入计划。
///
/// 普通提交不改草稿行；“编辑并提交”会在同一事务替换行；驳回后没有活动副本时
/// 会在同一事务新建并直接锁定副本。
pub(super) use erp_sales::service::sales_order::lifecycle::SalesOrderWorkingCopyPersistPlan;

// 在同一事务中写入提交快照、单据迁移、快照、BPM 运行事实与入口任务。
//
// 提交启动后原子写入销售单、提交快照与运行事实。
//
// `access` 为原命令授权检查器，事务内重验关联、责任与版本；`expected_order_version` 为预读时的销售单版本；`input` 为提交写入集合；`executor` 为原提交事务执行器。
//
// 返回提交快照视图。仓储写入失败或计划不完整时返回错误，事务回滚。
//
// Replay 不得重复写运行事实；Apply 必须写入快照与入口任务。
pub(super) async fn persist_sales_order_start(
    db: &Database,
    access: super::authorization::SalesCommandAccess,
    expected_order_version: u64,
    input: SalesOrderStartPersistInput,
    executor: &mut dyn Executor,
) -> Result<SubmissionView> {
    let view = submission_view(input.submission.clone(), input.submission_lines.clone());
    let replayed = input.persist(db, &access, expected_order_version, executor).await?;
    Ok(replayed.unwrap_or(view))
}

impl SalesOrderStartPersistInput {
    // 事务内先重验关联和回执，未命中才执行首次提交绑定检查。
    async fn validate(
        &self,
        db: &Database,
        access: &SalesCommandAccess,
        expected_order_version: u64,
        executor: &mut dyn Executor,
    ) -> Result<Option<SubmissionView>> {
        access.related_order(&self.order, executor).await?;
        if let Some(view) = replay_submission_with_executor(
            db,
            &self.audit.receipt.base.id,
            (&self.audit.receipt.fingerprint, &self.audit.receipt.idempotency_key_hash),
            &self.order.base.id,
            &self.audit.receipt.actor_id,
            access,
            executor,
        )
        .await?
        {
            return Ok::<Option<SubmissionView>, crate::Error>(Some(view));
        }

        access.revalidate(&self.order.base.id, expected_order_version, executor).await?;
        let current = access.current(&self.order.base.id, executor).await?;
        SalesOrderCommandProcess::check_first_contract_binding(
            db,
            access,
            &current,
            &self.order,
            self.working_copy.contract_revision_id.as_ref(),
            executor,
        )
        .await?;
        Ok(None)
    }

    // 在调用方事务中依次写入收据、守卫、销售事实、运行态与审计。
    async fn persist(
        mut self,
        db: &Database,
        access: &SalesCommandAccess,
        expected_order_version: u64,
        executor: &mut dyn Executor,
    ) -> Result<Option<SubmissionView>> {
        if let Some(view) = self.validate(db, access, expected_order_version, executor).await? {
            return Ok(Some(view));
        }
        let PreparedExecution::Apply(writes) = self.prepared else {
            return Err(Error::Internal("销售提交启动收据缺少领域命令结果".to_string()));
        };

        db.bpm_workflow()
            .insert_command_receipt(&writes.receipt, executor)
            .await
            .map_err(map_receipt_first_write_error)?;
        guard_sales_approval_start(db, &writes, self.document_type, self.now, executor).await?;
        SalesOrderService::new(db.clone())
            .ensure_sellable_refs(
                &self.sellable_refs,
                &CatalogQualificationAdapter::new(access.services.catalog.clone()),
                executor,
            )
            .await?;

        SalesOrderService::new(db.clone())
            .persist_submission_start(
                &mut self.order,
                &mut self.working_copy,
                &self.submission,
                &self.submission_lines,
                self.working_copy_plan,
                executor,
            )
            .await?;
        db.workflow_actions().create(&self.workflow_action, executor).await?;

        persist_runtime_writes(
            db,
            &writes,
            SalesOrderRuntimeWriteInput {
                resubmission_reason: self.resubmission_reason.as_deref(),
                identity: &self.identity,
                approvals: access.services.approvals.as_ref(),
                document_type: self.document_type,
                snapshot_payload: &self.snapshot_payload,
                owner_role: self.owner_role,
                organization_id: &self.organization_id,
                now: self.now,
            },
            executor,
        )
        .await?;

        self.audit.persist(db, executor).await?;
        Ok(None)
    }
}

// 在原写入位置登记审批启动守卫，保留守卫冲突语义。
async fn guard_sales_approval_start(
    db: &Database,
    writes: &PlannedWrites,
    document_type: DocumentType,
    now: Instant,
    executor: &mut dyn Executor,
) -> Result<()> {
    let guarded = db
        .business_documents()
        .mark_approval_started(
            writes.instance.subject.subject_id(),
            document_type,
            &writes.instance.process_definition_id,
            writes.instance.definition_version,
            now,
            executor,
        )
        .await?;
    if guarded.is_none() {
        return Err(Error::ConflictError("销售单的审批状态已变化，未能提交审批，请刷新后核对".to_string()));
    }
    Ok(())
}

/// 销售单启动运行事实写入上下文。
///
/// # 用途
/// 收拢快照、单据类型、责任组织与时间，供 `persist_runtime_writes` 使用。
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
/// `document_type` 必须与提交分派一致，不得在此改写。
pub(super) struct SalesOrderRuntimeWriteInput<'a> {
    /// 审批人资格重验使用的共享身份源。
    pub identity: &'a Arc<AuthorizationService>,
    pub resubmission_reason: Option<&'a str>,
    pub approvals: &'a dyn ApprovalRuntimePort,
    /// 按业务性质分派的单据类型。
    pub document_type: DocumentType,
    /// 冻结快照载荷。
    pub snapshot_payload: &'a ApprovalSubjectSnapshotPayload,
    /// 责任角色。
    pub owner_role: &'a str,
    /// 责任组织。
    pub organization_id: &'a str,
    /// 调用方时间。
    pub now: Instant,
}

// 将启动计划写入 BPM 集合、不可变快照和入口 WorkItem。
//
// 把启动计划落到 BPM 运行事实、不可变快照与入口任务。
//
// `writes` 为启动写入集合；`input` 为快照、单据类型与责任组织；沿用调用方的执行器。计划缺少入口执行或写入失败时返回错误。
//
// 缺少入口执行时不得提交销售单。
pub(super) async fn persist_runtime_writes(
    db: &Database,
    writes: &erp_workflow::service::approval::execution::apply_plan::PlannedWrites,
    input: SalesOrderRuntimeWriteInput<'_>,
    executor: &mut dyn Executor,
) -> Result<()> {
    let SalesOrderRuntimeWriteInput {
        identity,
        resubmission_reason,
        approvals,
        document_type,
        snapshot_payload,
        owner_role,
        organization_id,
        now,
    } = input;

    let mut snapshot = persist_start_runtime(
        db,
        identity,
        writes,
        snapshot_payload,
        now,
        "启动计划缺少入口执行，不得提交销售单",
        executor,
    )
    .await?;

    complete_resubmission(
        db,
        &mut snapshot,
        resubmission_reason,
        writes.instance.started_by.as_str(),
        now,
        executor,
    )
    .await?;
    approvals.persist_start_snapshot(db, snapshot, executor).await?;

    persist_start_tasks(
        db,
        writes,
        StartTaskContext { document_type, owner_role, organization_id, now },
        executor,
    )
    .await
    .map(|_| ())
    .map_err(Error::from)
}

#[cfg(test)]
mod tests {
    use bpm::ids::{ApprovalNodeExecutionId, ApprovalProcessInstanceId};
    use bpm::model::{ApprovalNodeExecution, NewNodeExecution, ParticipantId, Timestamp};
    use erp_core::common::time::Instant;
    use erp_workflow::repository::bpm::ApprovalInstanceListProjection;

    fn execution() -> ApprovalNodeExecution {
        ApprovalNodeExecution::new_active(NewNodeExecution {
            id: ApprovalNodeExecutionId::new("e1"),
            process_instance_id: ApprovalProcessInstanceId::new("inst-1"),
            node_key: "n1".into(),
            node_name: "采购确认".into(),
            round_no: 1,
            execution_no: 1,
            assignment_source: bpm::model::types::ApprovalExecutionAssignmentSource::Definition,
            replaces_execution_id: None,
            assignee_participant_id: ParticipantId::new("u1").unwrap(),
            assignee_name_snapshot: "张三".into(),
            at: Timestamp::from_unix_secs(10).unwrap(),
        })
        .expect("入口执行夹具")
    }

    // 列表投影必须来自入口执行，不得推断未知审批人。
    #[test]
    fn list_projection_copies_entry_assignee() {
        let projection =
            ApprovalInstanceListProjection::from_entry_execution(&execution(), Instant::from_unix_secs(10));
        assert_eq!(projection.current_node_key.as_deref(), Some("n1"));
        assert_eq!(projection.current_assignee_participant_id.as_deref(), Some("u1"));
        assert_eq!(projection.current_assignee_name.as_deref(), Some("张三"));
        assert_eq!(projection.last_status_changed_at, Some(10));
    }
}
