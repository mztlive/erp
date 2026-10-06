//! 采购提交准备：读取草稿与来源事实、调用领域冻结并构造审批持久化输入。

use bpm::SubjectRef;
use erp_audit::AuditActorLogs;
use erp_core::common::time::Instant;
use erp_procurement::dto::purchase_order::{PURCHASE_SUBMIT_ACTION, SavePurchaseOrderLinePatch};
use erp_procurement::entity::purchase_order::{
    PurchaseOrder, PurchaseOrderSubmission, PurchaseOrderSubmissionLine, PurchaseSubmitReceipt,
};
use erp_procurement::repository::PurchaseOrderExt;
use erp_procurement::service::purchase_order::draft_edit::map_draft_edit_violation;
use erp_procurement::service::purchase_order::submission::assign_formal_purchase_no;
use erp_sales::entity::sales_order::SalesOrder;
use erp_sales::repository::SalesOrderExt;
use erp_workflow::entity::approval_integration::ApprovalSubjectSnapshotPayload;
use erp_workflow::entity::document_registry::BusinessDocument;
use erp_workflow::entity::document_registry::business_document::ApprovalDefinitionBinding;
use erp_workflow::service::approval::execution::{PreparedExecution, prepare_start};
use erp_workflow::service::approval::policy::ApprovalDomainAction;
use erp_workflow::service::document_registry::{find_approval_binding, find_registered_document};
use id_generator::next_id;
use persistence_core::NoTransaction;

use super::{PurchaseOrderProcess, PurchaseSubmitCommand};
use crate::procure_to_pay::adapter::{
    RECENT_HISTORY_LIMIT, build_purchase_order_snapshot, execute_purchase_order_domain_action,
    purchase_order_adapter, purchase_order_object_readable, purchase_order_responsible_org_id,
    purchase_order_start_command, purchase_order_subject_ref, require_frozen_binding,
    start_approval_command_kind,
};
use crate::procure_to_pay::start_approval::{
    PurchaseOrderStartInput, PurchaseOrderStartPersistInput, PurchaseSubmitProcurementGuard,
    build_purchase_order_start_input, load_bound_definition_graph, load_start_receipt,
};
use crate::{Error, Result};

/// 尚未冻结的领域草稿事实，读取顺序沿用提交入口合同。
struct PurchaseSubmitDraft {
    order: PurchaseOrder,
    binding: ApprovalDefinitionBinding,
    submission: PurchaseOrderSubmission,
    lines: Vec<PurchaseOrderSubmissionLine>,
}

/// 已核对来源组织并分配正式号的提交上下文。
struct PurchaseSubmitContext {
    draft: PurchaseSubmitDraft,
    sales_order: SalesOrder,
    organization_id: String,
    document: BusinessDocument,
    now: Instant,
    superseded_draft: PurchaseOrderSubmission,
    procurement_guard: Option<PurchaseSubmitProcurementGuard>,
}

/// 同一领域冻结版本的审批计划与展示快照。
struct PurchaseSubmitApproval {
    prepared: PreparedExecution,
    snapshot: ApprovalSubjectSnapshotPayload,
    owner_role: &'static str,
}

/// 事务独占持久化输入；回包和恢复只保留必要的不可变结果身份。
pub(super) struct PreparedPurchaseSubmit {
    pub(super) input: PurchaseOrderStartPersistInput,
    pub(super) output: PurchaseSubmitReceipt,
    pub(super) purchase_order_id: String,
    pub(super) subject_version: u32,
}

impl PurchaseOrderProcess {
    /// 按既有读取顺序准备领域冻结事实、审批计划和命令回执。
    ///
    /// # 参数
    /// * `command` - 已校验请求及当前调用人的同一次提交身份。
    ///
    /// # 返回
    /// 返回由提交入口事务统一应用的持久化输入和冻结结果身份。
    ///
    /// # 错误
    /// 传播范围、版本、草稿、来源、绑定和审批计划的原始错误。
    pub(super) async fn prepare_purchase_submit(
        &self,
        command: &PurchaseSubmitCommand<'_>,
    ) -> Result<PreparedPurchaseSubmit> {
        let adapter = purchase_order_adapter()?;
        let subject = purchase_order_subject_ref(command.purchase_order_id)?;
        let draft = self.load_submit_draft(command).await?;
        let mut context = self.prepare_submit_source(draft, command).await?;
        self.freeze_submit_draft(&mut context, command).await?;
        let approval =
            self.prepare_submit_approval(&mut context, command, subject, adapter.owner_role).await?;
        self.prepare_submit_persistence(context, approval, command)
    }

    /// 读取当前有提交资格的采购单、冻结绑定和仍可编辑的草稿。
    async fn load_submit_draft(&self, command: &PurchaseSubmitCommand<'_>) -> Result<PurchaseSubmitDraft> {
        let order = self
            .command_access(command.actor, "submit")?
            .current(command.purchase_order_id, &mut NoTransaction)
            .await?;
        order
            .ensure_expected_version(command.request.expected_lock_version)
            .map_err(|_| Error::ConflictError("数据已被其他请求修改，请刷新后重试".to_string()))?;
        order
            .ensure_draft_for_submission()
            .map_err(|_| Error::ConflictError("采购单已提交或已生效，请勿重复提交".to_string()))?;
        let binding = find_approval_binding(&self.db, command.purchase_order_id, &mut NoTransaction)
            .await
            .map_err(Error::from)?;
        let binding = require_frozen_binding(binding.as_ref())?.clone();
        let draft_id =
            order.draft_submission_id().map_err(|error| Error::BusinessLogicError(error.to_string()))?;
        let submission = self
            .db
            .purchase_order_submissions()
            .find_by_id(&draft_id, &mut NoTransaction)
            .await?
            .ok_or_else(|| Error::NotFound("草稿提交不存在".to_string()))?;
        submission.ensure_draft().map_err(|_| Error::ConflictError("草稿提交已冻结".to_string()))?;
        let lines = self.db.purchase_order().list_submission_lines(&draft_id, &mut NoTransaction).await?;
        Ok(PurchaseSubmitDraft { order, binding, submission, lines })
    }

    /// 核对来源责任组织，准备正式号和旧草稿失效事实；此处不写库。
    async fn prepare_submit_source(
        &self,
        mut draft: PurchaseSubmitDraft,
        command: &PurchaseSubmitCommand<'_>,
    ) -> Result<PurchaseSubmitContext> {
        let sales_order = self
            .db
            .sales_orders()
            .find_by_id(&draft.order.sales_order_id, &mut NoTransaction)
            .await?
            .ok_or_else(|| Error::NotFound("来源销售单不存在".to_string()))?;
        let organization_id = purchase_order_responsible_org_id(&sales_order)?;
        let _ = purchase_order_object_readable(&organization_id, command.actor.id())?;
        assign_formal_purchase_no(&mut draft.order)?;
        let mut document = find_registered_document(&self.db, command.purchase_order_id, &mut NoTransaction)
            .await?
            .ok_or_else(|| Error::NotFound("业务单据未注册".to_string()))?;
        let now = Instant::now();
        if document.document_no.is_empty() {
            document.assign_document_no(draft.order.purchase_no.clone(), now)?;
        }
        let mut superseded_draft = draft.submission.clone();
        superseded_draft.mark_superseded()?;
        Ok(PurchaseSubmitContext {
            draft,
            sales_order,
            organization_id,
            document,
            now,
            superseded_draft,
            procurement_guard: None,
        })
    }

    /// 补丁合并和冻结仍委托采购领域；覆盖 guard 留给同一启动写事务重验。
    async fn freeze_submit_draft(
        &self,
        context: &mut PurchaseSubmitContext,
        command: &PurchaseSubmitCommand<'_>,
    ) -> Result<()> {
        let draft = &mut context.draft;
        draft.submission = if command.request.line_patches.is_empty() {
            self.domain()
                .freeze_submission(&mut draft.order, &mut draft.submission, &mut draft.lines, command.actor)
                .await?
        } else {
            draft
                .order
                .ensure_payment_term_unchanged(command.request.payment_term_code.as_deref())
                .map_err(map_draft_edit_violation)?;
            let existing_lines = draft.lines.clone();
            let requested_lines =
                SavePurchaseOrderLinePatch::resolve_all(&command.request.line_patches, &existing_lines)?;
            let (submission, lines) = self
                .domain()
                .freeze_submission_from_lines(
                    &draft.order,
                    &draft.submission,
                    &existing_lines,
                    &requested_lines,
                    command.actor,
                )
                .await?;
            draft.lines = lines;
            context.procurement_guard = Some(PurchaseSubmitProcurementGuard {
                requested_lines,
                existing_lines,
                actor_id: command.actor.id().to_string(),
            });
            submission
        };
        Ok(())
    }

    /// 领域动作推进后，依据同一冻结版本读取定义、收据并准备审批计划。
    async fn prepare_submit_approval(
        &self,
        context: &mut PurchaseSubmitContext,
        command: &PurchaseSubmitCommand<'_>,
        subject: SubjectRef,
        owner_role: &'static str,
    ) -> Result<PurchaseSubmitApproval> {
        let draft = &mut context.draft;
        execute_purchase_order_domain_action(
            &mut draft.order,
            ApprovalDomainAction::PurchaseOrderSubmit,
            &draft.submission.base.id,
            command.actor.id(),
        )?;
        let snapshot = build_purchase_order_snapshot(
            &draft.order,
            &context.sales_order,
            &draft.submission,
            &draft.lines,
            command.actor.id(),
            context.now,
        )?;
        let start = purchase_order_start_command(
            command.purchase_order_id,
            draft.order.approval_subject_version,
            command.actor.id(),
            &command.request.idempotency_key,
        );
        let _ = (start_approval_command_kind(&start), RECENT_HISTORY_LIMIT);
        let graph = load_bound_definition_graph(&self.db, &draft.binding).await?;
        let receipt = load_start_receipt(
            &self.db,
            &subject,
            draft.order.approval_subject_version,
            &command.request.idempotency_key,
        )
        .await?;
        let input = build_purchase_order_start_input(PurchaseOrderStartInput {
            graph,
            binding: &draft.binding,
            subject,
            subject_version: draft.order.approval_subject_version,
            actor_id: command.actor.id(),
            organization_id: &context.organization_id,
            idempotency_key: &command.request.idempotency_key,
            receipt,
            now: context.now,
        })?;
        Ok(PurchaseSubmitApproval { prepared: prepare_start(input)?, snapshot, owner_role })
    }

    /// 构造同一启动事务的审计、回执及对象授权重验输入。
    fn prepare_submit_persistence(
        &self,
        context: PurchaseSubmitContext,
        approval: PurchaseSubmitApproval,
        command: &PurchaseSubmitCommand<'_>,
    ) -> Result<PreparedPurchaseSubmit> {
        let draft = context.draft;
        let audit = command
            .actor
            .clone()
            .resource_log_with_id(
                next_id(),
                PURCHASE_SUBMIT_ACTION,
                "purchase_order",
                draft.order.base.id.clone(),
                None,
            )?
            .with_command_id(Some(command.identity.receipt_id().to_string()))?
            .with_resource_number(Some(draft.order.purchase_no.clone()))?;
        let output = PurchaseSubmitReceipt::new(
            draft.order.purchase_no.clone(),
            draft.submission.base.id.clone(),
            draft.submission.submission_no.clone(),
            String::new(),
            draft.order.approval_subject_version.to_string(),
        )
        .with_versions(0, draft.order.base.version);
        let purchase_order_id = draft.order.base.id.clone();
        let subject_version = draft.order.approval_subject_version;
        let input = PurchaseOrderStartPersistInput {
            order: draft.order,
            document: context.document,
            superseded_draft: context.superseded_draft,
            submission: draft.submission,
            submission_lines: draft.lines,
            procurement_guard: context.procurement_guard,
            snapshot_payload: approval.snapshot,
            prepared: approval.prepared,
            owner_role: approval.owner_role,
            organization_id: context.organization_id,
            now: context.now,
            audit,
            object_scope: Some(self.command_access(command.actor, "submit")?),
            receipt: Some((command.identity.clone(), command.fingerprint.to_string(), output.clone())),
        };
        Ok(PreparedPurchaseSubmit { input, output, purchase_order_id, subject_version })
    }
}
