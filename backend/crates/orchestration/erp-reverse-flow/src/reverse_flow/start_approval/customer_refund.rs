use application_core::{AuditActor, CommandReceipt};
use bpm::model::SubjectRef;
use erp_approval_runtime::approval_start::{StartTaskContext, persist_start_runtime, persist_start_tasks};
use erp_audit::AuditActorLogs;
use erp_core::common::time::Instant;
use erp_returns::entity::returns::CustomerRefund;
use erp_returns::service::ReturnsService;
use erp_workflow::entity::approval_integration::ApprovalSubjectSnapshotPayload;
use erp_workflow::entity::document_registry::DocumentType;
use erp_workflow::service::approval::execution::apply_plan::PlannedWrites;
use erp_workflow::service::approval::execution::{PreparedExecution, StartExecutionInput};
use erp_workflow::service::approval::resubmission::complete_resubmission;
use mongodb::Database;
use persistence_core::Executor;

use super::super::adapter::customer_refund_object_readable;
use super::common::{ReverseStartContracts, ReverseStartInput, build_reverse_start_input};
use super::prepare::load_start_receipt_for_document_type;
use super::reversal_submit::{committed_reversal_submit, persist_start_guard, persist_submit_receipt};
use crate::{Error, Result, ReverseFlowPorts};

/// 读取同载荷启动收据；不存在时返回 `None`。
///
/// # 参数
/// * `db` - 数据库
/// * `subject` - 业务对象引用
/// * `subject_version` - 冻结提交版本
/// * `idempotency_key` - 调用方幂等键
/// * `executor` - 本次提交事务执行器。
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
    executor: &mut dyn Executor,
) -> Result<Option<bpm::model::ApprovalCommandReceipt>> {
    load_start_receipt_for_document_type(
        db,
        DocumentType::CustomerRefund,
        subject,
        subject_version,
        idempotency_key,
        executor,
    )
    .await
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
pub fn build_customer_refund_start_input(input: ReverseStartInput<'_>) -> Result<StartExecutionInput> {
    build_reverse_start_input(
        input,
        &ReverseStartContracts {
            document_type: DocumentType::CustomerRefund,
            readable: customer_refund_object_readable,
            version_mismatch: "客户退款单的审批流程版本不一致，请联系管理员核对",
            empty_definition: "客户退款审批流程没有配置审批步骤，请联系管理员补全",
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
    /// 绑定原请求版本和重提说明的不可变提交收据。
    pub command_receipt: CommandReceipt,
    /// 本次重新提交说明。
    pub resubmission_reason: Option<String>,
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
/// * `executor` - 用例入口的原事务执行器。
///
/// # 返回
/// 完成启动写入或命中精确提交回执后成功。
///
/// # 错误
/// 仓储写入失败或计划不完整时返回错误，事务回滚。
///
/// # 关键业务约束
/// Replay 不得重复写运行事实；Apply 必须写入快照与入口任务。
pub async fn persist_customer_refund_start(
    db: &Database,
    rbac: &ReverseFlowPorts,
    input: CustomerRefundStartPersistInput,
    executor: &mut dyn Executor,
) -> Result<()> {
    if replay_committed_start(db, rbac, &input, executor).await?.is_some() {
        return Ok(());
    }

    let CustomerRefundStartPersistInput {
        command_receipt,
        resubmission_reason,
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
        return Err(Error::ConflictError("原提交记录缺少当时的单据版本，请刷新后先核对审批状态".into()));
    };

    let audit = actor.resource_log("customer_refund.submit", "customer_refund", id.clone())?;
    persist_start_guard(db, DocumentType::CustomerRefund, &writes, now, executor).await?;
    let mut refund = refund;
    ReturnsService::new(db.clone()).persist_customer_refund(&mut refund, executor).await?;

    persist_runtime_writes(
        resubmission_reason.as_deref(),
        db,
        rbac,
        &writes,
        &snapshot_payload,
        StartTaskContext {
            document_type: DocumentType::CustomerRefund,
            owner_role,
            organization_id: &organization_id,
            now,
        },
        executor,
    )
    .await?;

    persist_submit_receipt(db, &command_receipt, id, &audit, executor).await?;
    Ok(())
}

/// 在读取或写入业务状态前，以原事务重验真实单据并恢复精确命令。
async fn replay_committed_start(
    db: &Database,
    rbac: &ReverseFlowPorts,
    input: &CustomerRefundStartPersistInput,
    executor: &mut dyn Executor,
) -> Result<Option<CustomerRefund>> {
    super::authorization::ensure_replay_authorized(
        db,
        rbac,
        &input.actor,
        DocumentType::CustomerRefund,
        &input.refund.base.id,
        executor,
    )
    .await?;
    if committed_reversal_submit(db, &input.id, &input.command_receipt, executor).await?.is_some() {
        return Ok(Some(ReturnsService::new(db.clone()).load_customer_refund(&input.id, executor).await?));
    }
    Ok(None)
}

/// 将启动计划写入 BPM 集合、不可变快照和入口 WorkItem。
///
/// # 参数
/// * `db` - 数据库。
/// * `writes` - 已准备的启动写入计划。
/// * `snapshot_payload` - 冻结的主体快照。
/// * `rbac` - 原调用方身份与审批运行能力。
/// * `context` - 原提交的单据类型、入口任务责任和采集时间。
/// * `executor` - 调用方事务执行器。
///
/// # 返回
/// 成功时无返回值。
///
/// # 错误
/// 计划缺少入口执行或写入失败时返回错误。
pub async fn persist_runtime_writes(
    resubmission_reason: Option<&str>,
    db: &Database,
    rbac: &ReverseFlowPorts,
    writes: &PlannedWrites,
    snapshot_payload: &ApprovalSubjectSnapshotPayload,
    context: StartTaskContext<'_>,
    executor: &mut dyn Executor,
) -> Result<()> {
    let context = StartTaskContext { document_type: DocumentType::CustomerRefund, ..context };
    let mut snapshot = persist_start_runtime(
        db,
        &rbac.identity,
        writes,
        snapshot_payload,
        context.now,
        "启动计划缺少入口执行，不得提交客户退款",
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
    complete_resubmission(
        db,
        &mut snapshot,
        resubmission_reason,
        writes.instance.started_by.as_str(),
        context.now,
        executor,
    )
    .await?;
    rbac.runtime.persist_start_snapshot(db, snapshot, executor).await?;

    persist_start_tasks(db, writes, context, executor).await.map(|_| ()).map_err(Into::into)
}
