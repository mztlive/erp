use application_core::AuditActor;
use bpm::model::SubjectRef;
use erp_audit::AuditActorLogs;
use erp_core::common::time::Instant;
use erp_identity::SharedRbacService;
use erp_returns::entity::returns::SupplierRefund;
use erp_workflow::entity::approval_integration::ApprovalSubjectSnapshotPayload;
use erp_workflow::entity::document_registry::DocumentType;
use erp_workflow::repository::prelude::*;
use erp_workflow::service::approval::execution::apply_plan::PlannedWrites;
use erp_workflow::service::approval::execution::{
    PreparedExecution, StartExecutionInput, map_receipt_first_write_error,
};
use erp_workflow::{BpmExt, DocumentRegistryExt};
use mongodb::Database;
use persistence_core::{Executor, Transactional};

use super::super::adapter::supplier_refund_object_readable;
use super::common::{ReverseStartContracts, ReverseStartInput, build_reverse_start_input};
use super::prepare::load_start_receipt_for_document_type;
use crate::approval_start::{
    StartTaskContext, persist_start_runtime, persist_start_snapshot, persist_start_tasks,
};
use crate::audit::persist_log;
use crate::{Error, Result};

/// 读取供应商退款同载荷启动收据；不存在时返回 `None`。
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
pub async fn load_supplier_refund_start_receipt(
    db: &Database,
    subject: &SubjectRef,
    subject_version: u32,
    idempotency_key: &str,
) -> Result<Option<bpm::model::ApprovalCommandReceipt>> {
    load_start_receipt_for_document_type(
        db,
        DocumentType::SupplierRefund,
        subject,
        subject_version,
        idempotency_key,
    )
    .await
}

/// 由定义图与单据组织构造供应商退款启动输入。
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
pub fn build_supplier_refund_start_input(input: ReverseStartInput<'_>) -> Result<StartExecutionInput> {
    build_reverse_start_input(
        input,
        &ReverseStartContracts {
            document_type: DocumentType::SupplierRefund,
            readable: supplier_refund_object_readable,
            version_mismatch: "供应商退款单绑定定义版本与已加载定义不一致",
            empty_definition: "审批定义没有节点，无法启动供应商退款审批",
        },
    )
}

/// 供应商退款启动事务写入集合。
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
pub struct SupplierRefundStartPersistInput {
    /// 已进入 `IN_APPROVAL` 的退款单。
    pub refund: SupplierRefund,
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

/// 在同一事务中写入供应商退款迁移、快照、BPM 运行事实与入口任务。
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
pub async fn persist_supplier_refund_start(
    db: &Database,
    rbac: &SharedRbacService,
    input: SupplierRefundStartPersistInput,
) -> Result<SupplierRefund> {
    let db = db.clone();
    let rbac = rbac.clone();
    db.client()
        .clone()
        .with_transaction(move |executor| {
            Box::pin(async move { persist_start(db, rbac, input, executor).await })
        })
        .await
}

/// 启动写入在同一事务中重新授权，回放不产生额外写入。
async fn persist_start(
    db: Database,
    rbac: SharedRbacService,
    input: SupplierRefundStartPersistInput,
    executor: &mut dyn Executor,
) -> Result<SupplierRefund> {
    let SupplierRefundStartPersistInput {
        refund,
        actor,
        id,
        snapshot_payload,
        prepared,
        owner_role,
        organization_id,
        now,
    } = input;
    super::authorization::ensure_replay_authorized(
        &db,
        &rbac,
        &actor,
        DocumentType::SupplierRefund,
        "supplier_refund:submit",
        &refund.base.id,
        executor,
    )
    .await?;
    let PreparedExecution::Apply(writes) = prepared else {
        return Ok(refund);
    };
    super::super::return_settlement::validate_settlement(&db, &refund, executor).await?;
    let audit = actor.resource_log("supplier_refund.submit", "supplier_refund", id)?;
    persist_start_guard(&db, &writes, now, executor).await?;
    let mut refund = refund;
    erp_returns::service::ReturnsService::persist_supplier_refund(&db, &mut refund, executor).await?;
    persist_supplier_refund_runtime(
        &db,
        &writes,
        &snapshot_payload,
        owner_role,
        &organization_id,
        now,
        executor,
    )
    .await?;
    persist_log(&db, &audit, executor).await?;
    Ok::<SupplierRefund, crate::Error>(refund)
}

/// 回执和注册单据守卫先写入，后续失败由外层事务整体回滚。
async fn persist_start_guard(
    db: &Database,
    writes: &PlannedWrites,
    now: Instant,
    executor: &mut dyn Executor,
) -> Result<()> {
    db.bpm_workflow()
        .insert_command_receipt(&writes.receipt, executor)
        .await
        .map_err(map_receipt_first_write_error)?;
    let guarded = db
        .business_documents()
        .mark_approval_started(
            writes.instance.subject.subject_id(),
            DocumentType::SupplierRefund,
            &writes.instance.process_definition_id,
            writes.instance.definition_version,
            now,
            executor,
        )
        .await?;
    if guarded.is_none() {
        return Err(Error::ConflictError("供应商退款单审批启动守卫冲突，请刷新后重试".to_string()));
    }
    Ok(())
}

/// 将供应商退款启动计划写入 BPM 集合、不可变快照和入口 WorkItem。
///
/// # 参数
/// * `db` - 数据库。
/// * `writes` - 已准备的启动写入计划。
/// * `snapshot_payload` - 冻结的主体快照。
/// * `owner_role` - 入口任务责任角色。
/// * `organization_id` - 入口任务责任组织。
/// * `now` - 状态变更与任务创建时间。
/// * `executor` - 调用方事务执行器。
///
/// # 返回
/// 成功时无返回值。
///
/// # 错误
/// 计划缺少入口执行或写入失败时返回错误。
pub async fn persist_supplier_refund_runtime(
    db: &Database,
    writes: &PlannedWrites,
    snapshot_payload: &ApprovalSubjectSnapshotPayload,
    owner_role: &str,
    organization_id: &str,
    now: Instant,
    executor: &mut dyn Executor,
) -> Result<()> {
    let mut snapshot = persist_start_runtime(
        db,
        writes,
        DocumentType::SupplierRefund,
        snapshot_payload,
        now,
        "启动计划缺少入口执行，不得提交供应商退款",
        executor,
    )
    .await?;
    snapshot.display = Some(
        erp_read_models::workbench::capture_approval_display(
            db,
            snapshot.document_type,
            &snapshot.business_object_id,
            executor,
        )
        .await?,
    );
    persist_start_snapshot(db, snapshot, executor).await?;
    persist_start_tasks(
        db,
        writes,
        StartTaskContext { document_type: DocumentType::SupplierRefund, owner_role, organization_id, now },
        executor,
    )
    .await
    .map(|_| ())
}
