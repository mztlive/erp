use application_core::AuditActor;
use erp_audit::{AuditActorLogs, AuditExt};
use erp_core::common::time::Instant;
use erp_core::ids::{
    BusinessDocumentId, SalesOrderId, SalesOrderSubmissionId, SalesOrderWorkingCopyId, WorkflowActionId,
};
use erp_sales::dto::sales_order::{SubmissionView, SubmitSalesOrderRequest};
use erp_sales::entity::sales_order::{
    SalesOrder, SalesOrderWorkingCopy, SalesOrderWorkingCopyLine, WorkingPurpose,
};
use erp_sales::repository::SalesOrderExt;
use erp_sales::repository::prelude::*;
use erp_sales::service::sales_order::command::identity::{
    sales_submission_audit_id, sales_submission_fingerprint,
};
use erp_sales::service::sales_order::mapper::{build_submission, build_submission_lines, submission_view};
use erp_workflow::entity::document_registry::{WorkflowAction, WorkflowActionData, WorkflowActionType};
use erp_workflow::service::approval::execution::{command_recovery_delay, prepare_start};
use erp_workflow::service::document_registry::find_approval_binding;
use id_generator::next_id;
use persistence_core::{NoTransaction, Transactional};
use validator::Validate;

use super::super::SalesOrderCommandProcess;
use super::super::adapter::{
    RECENT_HISTORY_LIMIT, SalesOrderStartCommand, build_sales_order_snapshot,
    execute_sales_order_domain_action, reject_legacy_card_sales_decision, reject_legacy_card_sales_work_item,
    require_frozen_binding, sales_approval_ports, sales_order_object_readable,
    sales_order_responsible_org_id, sales_order_start_command, start_approval_command_kind,
};
use super::super::start_approval::{
    ReplaySalesOrderStartInput, SalesOrderStartInput, SalesOrderStartPersistInput,
    SalesOrderWorkingCopyPersistPlan, build_sales_order_start_input, load_bound_definition_graph,
    load_start_receipt, persist_sales_order_start, replay_sales_order_start_with_executor,
};
use crate::{Error, Result};

/// 销售提交启动恢复入参。
struct RecoverSalesSubmissionStartInput<'a> {
    /// 销售单主键。
    sales_order_id: &'a str,
    /// 提交时冻结的单据类型。
    document_type: erp_workflow::entity::document_registry::DocumentType,
    /// 提交时冻结的审批主题版本。
    subject_version: u32,
    /// 提交幂等键。
    idempotency_key: &'a str,
    /// 提交人。
    actor: &'a AuditActor,
    /// 提交审计收据 ID。
    audit_id: &'a str,
    /// 命令载荷指纹。
    fingerprint: &'a str,
    /// 触发恢复的原始错误。
    original_error: Error,
}

/// 销售单提交并启动审批所需的单据集合。
///
/// # 用途
/// 将销售单、工作副本与冻结提交打包。
///
/// # 参数
/// 无
///
/// # 返回
/// 无
///
/// # 错误
/// 无
///
/// # 关键业务约束
/// 提交行必须由当前工作副本冻结。
struct ApprovalSubmissionStart<'a> {
    /// 销售单主键。
    id: &'a str,
    /// 已进入审批中的销售单。
    order: SalesOrder,
    /// 已锁定的工作副本。
    working_copy: SalesOrderWorkingCopy,
    /// 冻结提交头。
    submission: erp_sales::entity::sales_order::SalesOrderSubmission,
    /// 冻结提交行。
    submission_lines: Vec<erp_sales::entity::sales_order::SalesOrderSubmissionLine>,
    /// 工作副本行，用于可售引用重验。
    copy_lines: Vec<SalesOrderWorkingCopyLine>,
    /// 草稿替换与补开副本的事务写入计划。
    working_copy_plan: SalesOrderWorkingCopyPersistPlan,
}

/// 禁止回退 `CARD_SALES_APPROVAL` 或卡券专用工作项。
///
/// # 错误
/// 主体种类为旧卡券定义时返回冲突。
pub(super) fn ensure_unified_start_command(start: &SalesOrderStartCommand) -> Result<()> {
    reject_legacy_card_sales_work_item("DOCUMENT_APPROVAL")?;
    if start.subject_kind == "CARD_SALES_APPROVAL" {
        return reject_legacy_card_sales_decision();
    }
    Ok(())
}

/// 读取销售单最新提交号，作为撤回查找实例的 `subject_version`。
///
/// # 错误
/// 没有提交时返回冲突。
pub(super) async fn latest_submission_no(db: &mongodb::Database, sales_order_id: &str) -> Result<u32> {
    db.sales_order_submissions()
        .find_latest_by_order(&SalesOrderId::new(sales_order_id), &mut NoTransaction)
        .await?
        .map(|submission| submission.submission_no)
        .ok_or_else(|| Error::ConflictError("销售单没有可撤回的提交版本".to_string()))
}

impl SalesOrderCommandProcess {
    /// 提交销售单并冻结提交快照。
    ///
    /// `GoodsService` 与 `Voucher` 均直接调用统一 `start_approval`，以
    /// `submission_no` 冻结 `subject_version` 与快照；禁止经 `sales_review`
    /// 准入、`CARD_SALES_APPROVAL` 或第二条启动路径。
    ///
    /// # 参数
    /// * `id` - 销售单 ID
    /// * `req` - 提交请求（含期望草稿版本与幂等键）
    /// * `actor` - 已通过鉴权的审计操作人
    ///
    /// # 返回
    /// 返回提交快照视图；已提交过的草稿幂等返回既有提交。
    ///
    /// # 错误
    /// * `NotFound` - 销售单或有效工作副本不存在
    /// * `ConflictError` - 期望版本与当前版本不一致
    #[tracing::instrument(
        name = "sales_order.submit",
        skip_all,
        fields(layer = "service", domain = "sales_order", operation = "submit")
    )]
    pub async fn submit_sales_order(
        &self,
        id: &str,
        req: SubmitSalesOrderRequest,
        actor: &AuditActor,
    ) -> Result<SubmissionView> {
        req.validate()?;
        let access = self.command_access(actor, "submit")?;
        let authorized_order = access.current(id, &mut NoTransaction).await?;
        let idempotency_key = req.idempotency_key.trim().to_string();
        if idempotency_key.is_empty() {
            return Err(Error::ValidationError("幂等键不能为空".to_string()));
        }
        let audit_id = sales_submission_audit_id(actor.id(), id, &idempotency_key);
        let fingerprint = sales_submission_fingerprint(actor.id(), id, &req)?;
        if let Some(existing) = self.replay_sales_submission(&audit_id, &fingerprint, id, actor).await? {
            return Ok(existing);
        }
        let (customer_id, settlement_party_id, draft) = self
            .resolve_sales_command_draft(&access, &req.contract_id, req.draft, &mut NoTransaction)
            .await?;
        let order = authorized_order;
        order
            .ensure_first_submission_working_copy_editable()
            .map_err(|error| Error::ConflictError(error.to_string()))?;
        if !order.matches_contract_context(&req.contract_id, &customer_id, &settlement_party_id) {
            return Err(Error::ConflictError("销售单合同归属已变化，请刷新后重试".to_string()));
        }
        self.sales().ensure_sellable_draft_lines(&draft.lines, &self.catalog()).await?;
        let order_id = SalesOrderId::new(order.base.id.clone());
        let active_working_copy = self
            .db
            .sales_order_working_copies()
            .find_active_by_order_and_purpose(&order_id, WorkingPurpose::FirstSubmission, &mut NoTransaction)
            .await?;
        let stable = self.sales().collect_stable_lines_for_draft(&order_id, &draft.lines).await?;
        let (mut working_copy, copy_lines, working_copy_plan) = self
            .sales()
            .prepare_submission_copy(&order, active_working_copy, stable, &draft, req.version, actor)
            .await?;

        if let Some(existing) = self
            .db
            .sales_order_submissions()
            .find_by_working_copy(
                &SalesOrderWorkingCopyId::new(working_copy.base.id.clone()),
                &mut NoTransaction,
            )
            .await?
        {
            let existing_id = SalesOrderSubmissionId::new(existing.base.id.clone());
            let existing_lines = self
                .db
                .sales_order_submission_lines()
                .list_lines_by_submissions(&[existing_id], &mut NoTransaction)
                .await?;
            return Ok(submission_view(existing, existing_lines));
        }
        self.sales().ensure_sellable_working_copy_lines(&copy_lines, &self.catalog()).await?;
        self.ensure_procurement_responsibility_before_submit(&order, &copy_lines).await?;
        let latest_submission_no = self
            .db
            .sales_order_submissions()
            .find_latest_by_order(&order_id, &mut NoTransaction)
            .await?
            .map(|submission| submission.submission_no)
            .unwrap_or(0);
        let submission_no =
            erp_sales::entity::sales_order::SalesOrderSubmission::next_submission_no(latest_submission_no)?;
        let submission = build_submission(&working_copy, &copy_lines, submission_no, actor)?;
        let submission_lines = build_submission_lines(&submission, &copy_lines)?;
        working_copy.submit()?;
        self.start_approval_submission(
            ApprovalSubmissionStart {
                id,
                order,
                working_copy,
                submission,
                submission_lines,
                copy_lines,
                working_copy_plan,
            },
            actor,
            &idempotency_key,
            audit_id,
            fingerprint,
        )
        .await
    }

    /// 销售单提交并启动统一审批。
    ///
    /// # 用途
    /// 冻结提交快照并启动统一审批。
    ///
    /// # 参数
    /// * `start` - 销售单、工作副本与提交快照
    /// * `actor` - 审计操作人
    /// * `idempotency_key` - 客户端幂等键
    /// * `audit_id` - 幂等审计主键
    /// * `fingerprint` - 请求摘要
    ///
    /// # 返回
    /// 返回提交快照视图。
    ///
    /// # 错误
    /// 无绑定、定义缺失、状态不允许或写入失败时返回错误。
    async fn start_approval_submission(
        &self,
        start: ApprovalSubmissionStart<'_>,
        actor: &AuditActor,
        idempotency_key: &str,
        audit_id: String,
        fingerprint: String,
    ) -> Result<SubmissionView> {
        let ApprovalSubmissionStart {
            id,
            mut order,
            working_copy,
            submission,
            submission_lines,
            copy_lines,
            working_copy_plan,
        } = start;
        let access = self.command_access(actor, "submit")?;
        let expected_order_version = order.base.version;
        let ports = sales_approval_ports(order.business_type)?;
        let subject = crate::order_to_cash::subject_ref_for_sales_business(order.business_type, id)
            .map_err(|error| Error::ValidationError(error.to_string()))?;
        let binding =
            find_approval_binding(&self.db, id, &mut NoTransaction).await.map_err(crate::Error::from)?;
        let binding = require_frozen_binding(binding.as_ref())?.clone();
        execute_sales_order_domain_action(&mut order, ports.on_approval_start, actor.id())?;
        let now = Instant::now();
        let snapshot = build_sales_order_snapshot(&order, &submission, &submission_lines, actor.id(), now)?;
        let start = sales_order_start_command(
            ports.document_type,
            id,
            submission.submission_no,
            actor.id(),
            idempotency_key,
        );
        ensure_unified_start_command(&start)?;
        let _ = (start_approval_command_kind(&start), RECENT_HISTORY_LIMIT);
        let organization_id = sales_order_responsible_org_id(&order)?;
        let graph = load_bound_definition_graph(&self.db, &binding).await?;
        let existing_receipt = load_start_receipt(
            &self.db,
            ports.document_type,
            &subject,
            submission.submission_no,
            idempotency_key,
        )
        .await?;
        let start_input = build_sales_order_start_input(SalesOrderStartInput {
            graph,
            binding: &binding,
            document_type: ports.document_type,
            subject,
            subject_version: submission.submission_no,
            actor_id: actor.id(),
            organization_id: &organization_id,
            idempotency_key,
            receipt: existing_receipt,
            now,
        })?;
        let prepared = prepare_start(start_input)?;
        let workflow_action = WorkflowAction::new(
            WorkflowActionId::new(next_id()),
            WorkflowActionData {
                document_id: BusinessDocumentId::new(id.to_string()),
                action_type: WorkflowActionType::Submit,
                from_status: "DRAFT".to_string(),
                to_status: "PENDING_REVIEW".to_string(),
                actor_id: actor.id().to_string(),
                actor_role: "role-sales".to_string(),
                comment: None,
            },
        )?;
        let audit = actor.clone().resource_log_with_id(
            audit_id.clone(),
            "sales_order.submit",
            "sales_order_submission",
            submission.base.id.clone(),
            Some(format!("command_sha256={fingerprint}")),
        )?;
        let sellable_refs =
            erp_sales::service::sales_order::SalesOrderService::sellable_working_copy_refs(&copy_lines)?;
        erp_sales::service::sales_order::SalesOrderService::new(self.db.clone())
            .ensure_sellable_refs(
                &sellable_refs,
                &crate::order_to_cash::adapters::catalog::CatalogQualificationAdapter::new(self.db.clone()),
                &mut NoTransaction,
            )
            .await?;
        let recovery_subject_version = submission.submission_no;
        let persisted = persist_sales_order_start(
            &self.db,
            access,
            expected_order_version,
            SalesOrderStartPersistInput {
                order,
                working_copy,
                submission,
                submission_lines,
                workflow_action,
                document_type: ports.document_type,
                snapshot_payload: snapshot,
                prepared,
                owner_role: ports.owner_role,
                organization_id,
                now,
                audit,
                working_copy_plan,
                sellable_refs,
            },
        )
        .await;
        match persisted {
            Ok(view) => Ok(view),
            Err(error) if error.command_may_have_committed() => {
                self.recover_sales_submission_start(RecoverSalesSubmissionStartInput {
                    sales_order_id: id,
                    document_type: ports.document_type,
                    subject_version: recovery_subject_version,
                    idempotency_key,
                    actor,
                    audit_id: &audit_id,
                    fingerprint: &fingerprint,
                    original_error: error,
                })
                .await
            },
            Err(error) => Err(error),
        }
    }

    /// 按稳定审计收据重放已提交的销售快照。
    async fn replay_sales_submission(
        &self,
        audit_id: &str,
        expected_fingerprint: &str,
        sales_order_id: &str,
        actor: &AuditActor,
    ) -> Result<Option<SubmissionView>> {
        let Some(audit) = self.db.audit_logs().find_by_id(audit_id, &mut NoTransaction).await? else {
            return Ok(None);
        };
        if audit.message.as_deref() != Some(&format!("command_sha256={expected_fingerprint}")) {
            return Err(Error::ConflictError("同一幂等键已用于不同的销售提交".to_string()));
        }
        let submission_id = audit
            .resource_id
            .as_deref()
            .ok_or_else(|| Error::Internal("销售提交幂等收据缺少结果引用".to_string()))?;
        let submission = self
            .db
            .sales_order_submissions()
            .find_by_id(submission_id, &mut NoTransaction)
            .await?
            .ok_or_else(|| Error::Internal("销售提交幂等收据对应快照缺失".to_string()))?;
        if !submission.matches_receipt_identity(sales_order_id, actor.id()) {
            return Err(Error::Internal("销售提交幂等收据与业务对象不一致".to_string()));
        }
        let submission_id = SalesOrderSubmissionId::new(submission.base.id.clone());
        let lines = self
            .db
            .sales_order_submission_lines()
            .list_lines_by_submissions(&[submission_id], &mut NoTransaction)
            .await?;
        let access = self.command_access(actor, "submit")?;
        let id = sales_order_id.to_string();
        self.db
            .client()
            .clone()
            .with_transaction(move |executor| Box::pin(async move { access.current(&id, executor).await }))
            .await?;
        Ok(Some(submission_view(submission, lines)))
    }

    /// receipt 唯一竞争、瞬态事务或提交结果未知后，以 fresh session 有界回读。
    ///
    /// # 参数
    /// * `input` - 销售提交恢复所需的单据类型、主题版本与审计身份
    ///
    /// # 返回
    /// 回读到已提交结果时返回提交视图；否则返回原始错误。
    ///
    /// # 错误
    /// 有界回读仍无法确认提交结果时返回原始错误，或传播不可恢复的冲突。
    ///
    /// # 关键业务约束
    /// 仅在 `command_may_have_committed` 场景进入；不得在确认未提交时吞掉错误。
    async fn recover_sales_submission_start(
        &self,
        input: RecoverSalesSubmissionStartInput<'_>,
    ) -> Result<SubmissionView> {
        const RECOVERY_ATTEMPTS: usize = 8;
        for attempt in 0..RECOVERY_ATTEMPTS {
            let db = self.db.clone();
            let sales_order_id_owned = input.sales_order_id.to_string();
            let idempotency_key_owned = input.idempotency_key.to_string();
            let actor_id = input.actor.id().to_string();
            let document_type = input.document_type;
            let subject_version = input.subject_version;
            let access = self.command_access(input.actor, "submit")?;
            let recovered = self
                .db
                .client()
                .with_transaction(move |executor| {
                    Box::pin(async move {
                        let order = access.current(&sales_order_id_owned, executor).await?;
                        let current_ports = sales_approval_ports(order.business_type)?;
                        if current_ports.document_type != document_type {
                            return Err(Error::ConflictError(
                                "销售单业务类型在提交恢复期间已变化".to_string(),
                            ));
                        }
                        let organization_id = sales_order_responsible_org_id(&order)?;
                        let _ = sales_order_object_readable(&organization_id, &actor_id)?;
                        let binding = find_approval_binding(&db, &sales_order_id_owned, executor)
                            .await
                            .map_err(crate::Error::from)?;
                        let binding = require_frozen_binding(binding.as_ref())?;
                        let subject = crate::order_to_cash::subject_ref_for_sales_business(
                            order.business_type,
                            &sales_order_id_owned,
                        )
                        .map_err(|error| Error::ValidationError(error.to_string()))?;
                        replay_sales_order_start_with_executor(
                            &db,
                            ReplaySalesOrderStartInput {
                                document_type,
                                subject: &subject,
                                subject_version,
                                idempotency_key: &idempotency_key_owned,
                                binding,
                                actor_id: &actor_id,
                            },
                            executor,
                        )
                        .await
                    })
                })
                .await;
            match recovered {
                Ok(Some(_)) => {
                    if let Some(view) = self
                        .replay_sales_submission(
                            input.audit_id,
                            input.fingerprint,
                            input.sales_order_id,
                            input.actor,
                        )
                        .await?
                    {
                        return Ok(view);
                    }
                },
                Ok(None) => {},
                Err(error) if error.command_may_have_committed() => {},
                Err(error) => return Err(error),
            }
            if attempt + 1 < RECOVERY_ATTEMPTS {
                tokio::time::sleep(command_recovery_delay(attempt)).await;
            }
        }
        Err(input.original_error)
    }
}
