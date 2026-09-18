//! 采购启动事务持久化：冻结提交、BPM 运行事实与入口任务。
//!
//! 本模块只做同一事务内的顺序写入与审计；`create` 并提交路径与单纯
//! `submit` 路径共享同一启动装配。

use async_trait::async_trait;
use bpm::engine::TaskIntent;
use bpm::ids::ApprovalProcessInstanceId;
use bpm::model::ApprovalNodeExecution;
use erp_audit::AuditExt;
use erp_core::common::time::Instant;
use erp_core::ids::{ApprovalSubjectSnapshotId, WorkItemId};
use erp_procurement::dto::purchase_order::SavePurchaseOrderLine;
use erp_procurement::entity::purchase_order::{
    PurchaseCommandReceipt, PurchaseOrder, PurchaseOrderSubmission, PurchaseOrderSubmissionLine,
    validate_draft_line_edits,
};
use erp_workflow::entity::approval_integration::{ApprovalSubjectSnapshot, ApprovalSubjectSnapshotPayload};
use erp_workflow::entity::document_registry::{BusinessDocument, DocumentType};
use erp_workflow::entity::work_item::{DocumentApprovalWorkItemData, WorkItem, WorkItemPriority};
use erp_workflow::repository::bpm::ApprovalInstanceListProjection;
use erp_workflow::repository::prelude::*;
use erp_workflow::service::approval::execution::{PreparedExecution, map_receipt_first_write_error};
use erp_workflow::{ApprovalIntegrationExt, BpmExt, DocumentRegistryExt, WorkItemExt};
use id_generator::next_id;
use mongodb::Database;
use persistence_core::Executor;

use crate::{Error, Result};

/// 采购单提交事务内需要一并写入的冻结提交。
///
/// # 用途
/// 收拢正式号、提交快照、启动计划与审计，供同一事务写入。
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
pub(crate) struct PurchaseOrderStartPersistInput {
    /// 已进入审批中的采购单。
    pub order: PurchaseOrder,
    /// 已同步正式号的注册行。
    pub document: BusinessDocument,
    /// 已失效的旧草稿提交。
    pub superseded_draft: PurchaseOrderSubmission,
    /// 冻结提交头。
    pub submission: PurchaseOrderSubmission,
    /// 冻结提交行。
    pub submission_lines: Vec<PurchaseOrderSubmissionLine>,
    /// 提交时携带草稿补丁时，在同一事务推进 guard 并复核采购覆盖。
    pub procurement_guard: Option<PurchaseSubmitProcurementGuard>,
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
    /// 已构造审计（结果消息留空，事务内任务写入后回填）。
    pub audit: erp_audit::AuditLog,
    /// 采购提交收据：请求指纹与结果载荷；任务身份在事务内回填后编码进审计消息。
    pub receipt: Option<(String, super::super::submission::PurchaseSubmitReceipt)>,
    /// 提交写事务内重验的对象范围；创建并提交路径已在同一事务检查建单范围。
    pub object_scope: Option<super::super::authorization::PurchaseCommandAccess>,
}

/// 提交时草稿补丁的采购覆盖校验上下文。
pub(crate) struct PurchaseSubmitProcurementGuard {
    /// 服务端合并后的完整目标行。
    pub requested_lines: Vec<SavePurchaseOrderLine>,
    /// 补丁合并前的当前草稿行。
    pub existing_lines: Vec<PurchaseOrderSubmissionLine>,
    /// 推进销售采购 guard 的操作人。
    pub actor_id: String,
}

/// 在调用方事务会话中写入正式号、提交快照、运行事实与入口任务。
///
/// # 参数
/// * `db` - 数据库
/// * `input` - 提交写入集合
/// * `executor` - 数据访问执行器
///
/// # 返回
/// 返回首个入口任务身份，无任务时为空。
///
/// # 错误
/// 仓储写入失败或计划不完整时返回错误，由调用方事务回滚。
///
/// # 关键业务约束
/// 独立提交通过本模块新建事务，因此审批收据是整笔事务第一写。创建并提交时，
/// 外层创建命令已经完成自身幂等仲裁并持有同一事务；本方法不得再开事务，且其
/// 审批写段仍必须按“启动收据 -> 带正式编号的注册行启动守卫 -> 业务提交 ->
/// BPM 运行事实”顺序执行。写事务必须重验采购对象范围，历史参与不授予提交。
pub(crate) async fn persist_purchase_order_start(
    db: &Database,
    input: PurchaseOrderStartPersistInput,
    executor: &mut dyn Executor,
) -> Result<Option<(String, u64)>> {
    if !matches!(&input.prepared, PreparedExecution::Apply(_)) {
        return Ok(None);
    }
    if let Some(scope) = &input.object_scope {
        scope.current(&input.order.base.id, executor).await?;
    }
    let mut posting = StartPosting { db, input, first_task: None };
    execute_start_steps(&mut posting, executor).await?;
    Ok(posting.first_task)
}

/// 采购启动写段的既有副作用边界；顺序不得早于审批收据或跳过旧草稿失效。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum StartStep {
    Receipt,
    DocumentGuard,
    ProcurementGuard,
    SupplierQualification,
    Submission,
    SupersededDraft,
    Runtime,
    Audit,
}

#[async_trait]
pub(super) trait StartSteps: Send {
    /// 复用调用方执行器应用一个原步骤，失败时保留原错误。
    async fn apply(&mut self, step: StartStep, executor: &mut dyn Executor) -> Result<()>;
}

/// 生产写序由此唯一入口执行；任一步失败后不再推进后续步骤。
pub(super) async fn execute_start_steps(
    steps: &mut impl StartSteps,
    executor: &mut dyn Executor,
) -> Result<()> {
    use StartStep::*;
    for step in [
        Receipt,
        DocumentGuard,
        ProcurementGuard,
        SupplierQualification,
        Submission,
        SupersededDraft,
        Runtime,
        Audit,
    ] {
        steps.apply(step, executor).await?;
    }
    Ok(())
}

struct StartPosting<'a> {
    db: &'a Database,
    input: PurchaseOrderStartPersistInput,
    first_task: Option<(String, u64)>,
}

#[async_trait]
impl StartSteps for StartPosting<'_> {
    async fn apply(&mut self, step: StartStep, executor: &mut dyn Executor) -> Result<()> {
        let input = &mut self.input;
        let PreparedExecution::Apply(writes) = &input.prepared else {
            return Ok(());
        };
        match step {
            StartStep::Receipt => {
                self.db
                    .bpm_workflow()
                    .insert_command_receipt(&writes.receipt, executor)
                    .await
                    .map_err(map_receipt_first_write_error)?;
            },
            StartStep::DocumentGuard => {
                let guarded = self
                    .db
                    .business_documents()
                    .mark_loaded_approval_started(
                        &mut input.document,
                        DocumentType::PurchaseOrder,
                        &writes.instance.process_definition_id,
                        writes.instance.definition_version,
                        input.now,
                        executor,
                    )
                    .await?;
                if !guarded {
                    return Err(Error::ConflictError("采购单审批启动守卫冲突，请刷新后重试".to_string()));
                }
            },
            StartStep::ProcurementGuard => {
                if let Some(guard) = input.procurement_guard.take() {
                    let coverage = super::super::draft_edit::advance_guard_and_load_coverage(
                        self.db,
                        &input.order,
                        &guard.actor_id,
                        executor,
                    )
                    .await?;
                    let requested_edits = guard
                        .requested_lines
                        .iter()
                        .map(SavePurchaseOrderLine::to_draft_edit)
                        .collect::<Vec<_>>();
                    validate_draft_line_edits(&requested_edits, &guard.existing_lines, &coverage.lines)
                        .map_err(
                            erp_procurement::service::purchase_order::draft_edit::map_draft_edit_violation,
                        )?;
                }
            },
            StartStep::SupplierQualification => {
                use erp_procurement::ports::creation_basis::CreationBasisSupplierPort;
                super::super::creation_basis::supplier::CreationBasisSupplierAdapter::new(self.db.clone())
                    .ensure_qualified(
                        &input.submission.supplier_id,
                        input.submission.purchase_type,
                        erp_core::common::time::BusinessDate::today(),
                        executor,
                    )
                    .await?;
            },
            StartStep::Submission => {
                erp_procurement::service::purchase_order::start_approval::persist_started_submission(
                    self.db,
                    &mut input.order,
                    &input.submission,
                    &input.submission_lines,
                    executor,
                )
                .await?;
            },
            StartStep::SupersededDraft => {
                erp_procurement::service::purchase_order::start_approval::persist_superseded_draft(
                    self.db,
                    &mut input.superseded_draft,
                    executor,
                )
                .await?;
            },
            StartStep::Runtime => {
                self.first_task = persist_runtime_writes(
                    self.db,
                    writes,
                    &input.snapshot_payload,
                    input.owner_role,
                    &input.organization_id,
                    input.now,
                    executor,
                )
                .await?;
            },
            StartStep::Audit => {
                if let Some((fingerprint, receipt)) = input.receipt.take() {
                    let receipt = receipt.with_first_task(self.first_task.as_ref());
                    input.audit.message =
                        Some(PurchaseCommandReceipt::new(fingerprint, receipt).encode_message()?);
                }
                self.db.audit_logs().create(&input.audit, executor).await?;
            },
        }
        Ok(())
    }
}

/// 将启动计划写入 BPM 集合、不可变快照和入口 WorkItem。
///
/// # 错误
/// 计划缺少入口执行或写入失败时返回错误。
async fn persist_runtime_writes(
    db: &Database,
    writes: &erp_workflow::service::approval::execution::apply_plan::PlannedWrites,
    snapshot_payload: &ApprovalSubjectSnapshotPayload,
    owner_role: &str,
    organization_id: &str,
    now: Instant,
    executor: &mut dyn Executor,
) -> Result<Option<(String, u64)>> {
    let first = writes
        .created_executions
        .first()
        .ok_or_else(|| Error::Internal("启动计划缺少入口执行，不得提交采购单".to_string()))?;
    db.bpm_workflow()
        .create_bpm_runtime_after_receipt(
            &writes.instance,
            &writes.created_assignees,
            first,
            &list_projection_from_execution(first, now),
            executor,
        )
        .await?;
    let snapshot = ApprovalSubjectSnapshot::new(
        ApprovalSubjectSnapshotId::new(next_id()),
        ApprovalProcessInstanceId::new(writes.instance.base.id.clone()),
        DocumentType::PurchaseOrder,
        writes.instance.subject.subject_id(),
        writes.instance.subject_version,
        snapshot_payload.clone(),
    )
    .map_err(|error| Error::ValidationError(error.to_string()))?;
    db.approval_subject_snapshots().create_immutable_snapshot(&snapshot, executor).await?;
    persist_open_tasks(db, writes, owner_role, organization_id, now, executor).await
}

/// 由入口执行构造有界列表投影。
///
/// # 参数
/// * `execution` - 入口执行
/// * `now` - 状态变更时间
///
/// # 返回
/// 返回启动时的列表投影。
fn list_projection_from_execution(
    execution: &ApprovalNodeExecution,
    now: Instant,
) -> ApprovalInstanceListProjection {
    ApprovalInstanceListProjection {
        current_node_key: Some(execution.node_key.clone()),
        current_node_name: Some(execution.node_name.clone()),
        current_assignee_participant_id: Some(execution.assignee_participant_id.as_str().to_string()),
        current_assignee_name: Some(execution.assignee_name_snapshot.clone()),
        latest_rejected_execution_id: None,
        latest_rejection_summary: None,
        last_status_changed_at: Some(now.unix_secs()),
    }
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
    executor: &mut dyn Executor,
) -> Result<Option<(String, u64)>> {
    let mut first_task = None;
    for intent in &writes.create_tasks {
        let TaskIntent::HumanTaskRequested { execution_id, assignee, .. } = intent else {
            continue;
        };
        let item = WorkItem::new_document_approval(
            WorkItemId::new(next_id()),
            DocumentApprovalWorkItemData {
                approval_node_execution_id: execution_id.clone(),
                business_object_type: DocumentType::PurchaseOrder.as_str().to_string(),
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
        if first_task.is_none() {
            first_task = Some((item.base.id.clone(), item.base.version));
        }
        db.work_items().create(&item, executor).await?;
    }
    Ok(first_task)
}
