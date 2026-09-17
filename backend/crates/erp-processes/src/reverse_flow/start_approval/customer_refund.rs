use application_core::AuditActor;
use bpm::engine::{DefinitionGraph, TaskIntent};
use bpm::ids::ApprovalProcessInstanceId;
use bpm::model::SubjectRef;
use erp_audit::{AuditActorLogs, AuditExt};
use erp_core::common::time::Instant;
use erp_core::ids::{ApprovalSubjectSnapshotId, WorkItemId};
use erp_returns::entity::returns::CustomerRefund;
use erp_workflow::entity::approval_integration::{ApprovalSubjectSnapshot, ApprovalSubjectSnapshotPayload};
use erp_workflow::entity::document_registry::DocumentType;
use erp_workflow::entity::document_registry::business_document::ApprovalDefinitionBinding;
use erp_workflow::entity::work_item::{DocumentApprovalWorkItemData, WorkItem, WorkItemPriority};
use erp_workflow::service::approval::execution::{
    PreparedExecution, StartExecutionInput, map_receipt_first_write_error,
};
use erp_workflow::{ApprovalIntegrationExt, BpmExt, DocumentRegistryExt, WorkItemExt};
use id_generator::next_id;
use mongodb::Database;
use persistence_core::Transactional;

use super::super::adapter::customer_refund_object_readable;
use super::common::{ReverseStartContracts, ReverseStartInput, build_reverse_start_input};
use super::mapping::list_projection_from_execution;
use super::prepare::load_start_receipt_for_document_type;
use crate::{Error, Result};

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
pub async fn load_start_receipt(
    db: &Database,
    subject: &SubjectRef,
    subject_version: u32,
    idempotency_key: &str,
) -> Result<Option<bpm::model::ApprovalCommandReceipt>> {
    load_start_receipt_for_document_type(
        db,
        DocumentType::CustomerRefund,
        subject,
        subject_version,
        idempotency_key,
    )
    .await
}

/// 客户退款启动输入。
///
/// # 用途
/// 收拢 `build_customer_refund_start_input` 的定义图、绑定与提交人参数。
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
pub struct CustomerRefundStartInput<'a> {
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
    pub receipt: Option<bpm::model::ApprovalCommandReceipt>,
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
pub fn build_customer_refund_start_input(input: CustomerRefundStartInput<'_>) -> Result<StartExecutionInput> {
    let CustomerRefundStartInput {
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
    build_reverse_start_input(
        ReverseStartInput {
            graph,
            binding,
            subject,
            subject_version,
            actor_id,
            organization_id,
            idempotency_key,
            receipt,
            now,
        },
        &ReverseStartContracts {
            document_type: DocumentType::CustomerRefund,
            readable: customer_refund_object_readable,
            version_mismatch: "客户退款单绑定定义版本与已加载定义不一致",
            empty_definition: "审批定义没有节点，无法启动客户退款审批",
        },
    )
}

/// 客户退款启动事务写入集合。
///
/// # 用途
/// 收拢提交后需一并写入的退款单、快照、启动计划与审计身份。
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
/// 运行事实、不可变快照与入口任务必须与单据迁移同事务。
pub struct CustomerRefundStartPersistInput {
    /// 已进入 `IN_APPROVAL` 的退款单。
    pub refund: CustomerRefund,
    /// 审计操作人。
    pub actor: AuditActor,
    /// 退款单主键。
    pub id: String,
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
}

/// 在同一事务中写入单据迁移、快照、BPM 运行事实与入口任务。
///
/// # 用途
/// 提交启动后原子写入退款单、快照与运行事实。
///
/// # 参数
/// * `db` - 数据库
/// * `input` - 退款单、快照与启动计划
///
/// # 返回
/// 返回提交后的退款单实体，由调用方装配视图。
///
/// # 错误
/// 仓储写入失败或计划不完整时返回错误，事务回滚。
///
/// # 关键业务约束
/// Replay 不得重复写运行事实；Apply 必须写入快照与入口任务。
pub async fn persist_customer_refund_start(
    db: &Database,
    input: CustomerRefundStartPersistInput,
) -> Result<CustomerRefund> {
    let CustomerRefundStartPersistInput {
        refund,
        actor,
        id,
        snapshot_payload,
        prepared,
        owner_role,
        organization_id,
        now,
    } = input;
    let PreparedExecution::Apply(writes) = prepared else {
        return Ok(refund);
    };
    let audit = actor.resource_log("customer_refund.submit", "customer_refund", id)?;
    let db = db.clone();
    let client = db.client().clone();
    client
        .with_transaction(move |session| {
            Box::pin(async move {
                db.bpm_workflow()
                    .insert_command_receipt(&writes.receipt, session)
                    .await
                    .map_err(map_receipt_first_write_error)?;
                let guarded = db
                    .business_documents()
                    .mark_approval_started(
                        writes.instance.subject.subject_id(),
                        DocumentType::CustomerRefund,
                        &writes.instance.process_definition_id,
                        writes.instance.definition_version,
                        now,
                        session,
                    )
                    .await?;
                if guarded.is_none() {
                    return Err(Error::ConflictError("客户退款单审批启动守卫冲突，请刷新后重试".to_string()));
                }
                let mut refund = refund;
                erp_returns::service::ReturnsService::new(db.clone())
                    .persist_customer_refund(&mut refund, session)
                    .await?;
                persist_runtime_writes(
                    &db,
                    &writes,
                    &snapshot_payload,
                    owner_role,
                    &organization_id,
                    now,
                    session,
                )
                .await?;
                db.audit_logs().create(&audit, session).await?;
                Ok::<CustomerRefund, crate::Error>(refund)
            })
        })
        .await
}

/// 将启动计划写入 BPM 集合、不可变快照和入口 WorkItem。
///
/// # 错误
/// 计划缺少入口执行或写入失败时返回错误。
pub async fn persist_runtime_writes(
    db: &Database,
    writes: &erp_workflow::service::approval::execution::apply_plan::PlannedWrites,
    snapshot_payload: &ApprovalSubjectSnapshotPayload,
    owner_role: &str,
    organization_id: &str,
    now: Instant,
    session: &mut mongodb::ClientSession,
) -> Result<()> {
    let first = writes
        .created_executions
        .first()
        .ok_or_else(|| Error::Internal("启动计划缺少入口执行，不得提交客户退款".to_string()))?;
    db.bpm_workflow()
        .create_bpm_runtime_after_receipt(
            &writes.instance,
            &writes.created_assignees,
            first,
            &list_projection_from_execution(first, now),
            session,
        )
        .await?;
    let mut snapshot = ApprovalSubjectSnapshot::new(
        ApprovalSubjectSnapshotId::new(next_id()),
        ApprovalProcessInstanceId::new(writes.instance.base.id.clone()),
        DocumentType::CustomerRefund,
        writes.instance.subject.subject_id(),
        writes.instance.subject_version,
        snapshot_payload.clone(),
    )
    .map_err(|error| Error::ValidationError(error.to_string()))?;
    snapshot.display = Some(
        erp_read_models::workbench::capture_approval_display(
            db,
            snapshot.document_type,
            &snapshot.business_object_id,
            session,
        )
        .await?,
    );
    db.approval_subject_snapshots().create_immutable_snapshot(&snapshot, session).await?;
    persist_open_tasks(db, writes, owner_role, organization_id, now, session).await
}

/// 将 `HumanTaskRequested` 映射为 `DOCUMENT_APPROVAL` 任务并写入。
///
/// # 错误
/// 责任人为空或仓储失败时返回错误。
async fn persist_open_tasks(
    db: &Database,
    writes: &erp_workflow::service::approval::execution::apply_plan::PlannedWrites,
    owner_role: &str,
    organization_id: &str,
    now: Instant,
    session: &mut mongodb::ClientSession,
) -> Result<()> {
    for intent in &writes.create_tasks {
        let TaskIntent::HumanTaskRequested { execution_id, assignee, .. } = intent else {
            continue;
        };
        let item = WorkItem::new_document_approval(
            WorkItemId::new(next_id()),
            DocumentApprovalWorkItemData {
                approval_node_execution_id: execution_id.clone(),
                business_object_type: DocumentType::CustomerRefund.as_str().to_string(),
                business_object_id: writes.instance.subject.subject_id().to_string(),
                subject_version: writes.instance.subject_version.to_string(),
                owner_role: owner_role.to_string(),
                owner_organization_id: organization_id.to_string(),
                owner_user_id: assignee.as_str().to_string(),
                priority: WorkItemPriority::Normal,
                due_at: None,
            },
            now,
        )?;
        db.work_items().create(&item, session).await?;
    }
    Ok(())
}
