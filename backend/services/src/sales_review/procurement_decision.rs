//! 采购确认完成命令。新写入已失败关闭；旧符号保留至 P0-D 删除。
#![allow(dead_code)]

use std::collections::HashMap;
use std::str::FromStr;

use database::{
    AccessControlExt, DocumentRegistryExt, Executor, ReceivableExt, SalesOrderExt, SalesReviewExt,
    SupplierExt, SupplierOfferingExt, Transactional, WorkItemExt,
};
use entities::common::time::{BusinessDate, Instant};
use entities::document_registry::{DocumentType, WorkflowAction, WorkflowActionData, WorkflowActionType};
use entities::ids::{BusinessDocumentId, WorkflowActionId};
use entities::money::Quantity;
use entities::sales_order::{LineType, RevisionSource, SalesOrder, SalesOrderSubmissionLine};
use entities::sales_review::ProcurementConfirmationLine;
use entities::supplier::{CapabilityCode, CapabilityStatus};
use entities::supplier_offering::{
    OfferingStatus, SupplierOffering, SupplierOfferingAvailability, SupplierOfferingRevision,
};

use super::formalization::{build_receivable_account, build_receivable_entry, build_revision};
use super::procurement_confirmation::{
    command_audit_id, command_fingerprint, ensure_pending_confirmation,
    ensure_procurement_confirmation_actor_eligible, ensure_submission_identity,
    load_procurement_confirmation_work_item, parse_task_version, replace_pending_confirmation_lines,
    ProcurementConfirmationTaskGuard, DECISION_COMMAND_ACTION,
};
use super::{
    CompleteProcurementConfirmationCommand, CompleteProcurementConfirmationResult,
    ProcurementConfirmationBusinessResult, ProcurementSalesResolution, SalesReviewService,
};
use crate::audit::AuditActor;
use crate::errors::{Error, Result};
use crate::iam::SharedRbacService;
use crate::work_item::WorkItemService;

impl SalesReviewService {
    /// 执行 W07 唯一采购确认完成命令。
    ///
    /// # 错误
    /// 命令分支、对象身份、任务/对象版本或当前访问事实不满足时失败关闭。
    pub async fn complete_procurement_confirmation(
        &self,
        id: &str,
        command: CompleteProcurementConfirmationCommand,
        actor: &AuditActor,
        rbac: SharedRbacService,
    ) -> Result<CompleteProcurementConfirmationResult> {
        let _ = (self, id, command, actor, rbac);
        Err(Error::ConflictError(
            "采购二次确认已停止新写入，必须走销售单统一审批".to_string(),
        ))
    }

    /// 以 W07 唯一强类型完成命令通过采购确认。
    ///
    /// 单事务重验命令身份、任务/确认版本、当前责任、资格和岗位分离，形成销售
    /// 正式版本、应收、采购创建依据语义、按供应商/履约方式拆出的采购草稿、
    /// 追加式工作流动作及审计收据，并完成原待办。不创建后继采购建单任务。
    ///
    /// # 错误
    /// 任一身份、状态、版本或准入事实漂移时整体回滚并返回稳定业务错误。
    async fn complete_approved_procurement_confirmation(
        &self,
        id: &str,
        req: CompleteProcurementConfirmationCommand,
        actor: &AuditActor,
        rbac: SharedRbacService,
    ) -> Result<CompleteProcurementConfirmationResult> {
        let expected_task_version = parse_task_version(&req.expected_task_version)?;
        let fingerprint = command_fingerprint(DECISION_COMMAND_ACTION, id, actor.id(), &req)?;
        let audit_id = command_audit_id(DECISION_COMMAND_ACTION, actor.id(), &req.idempotency_key);
        if let Some(receipt) = self.replay_decision_command(&audit_id, id, &fingerprint).await? {
            return receipt.into_result(&req.work_item_id);
        }

        let db = self.db.clone();
        let client = db.client().clone();
        let command = req.clone();
        let actor_owned = actor.clone();
        let actor_id = actor.id().to_string();
        let confirmation_id = id.to_string();
        let audit_id_for_tx = audit_id.clone();
        let fingerprint_for_tx = fingerprint.clone();
        let rbac_for_tx = rbac.clone();
        let transaction_result = client
            .with_transaction(move |session| {
                Box::pin(async move {
                    let mut confirmation = db
                        .procurement_confirmations()
                        .find_by_id(&confirmation_id, session)
                        .await?
                        .ok_or_else(|| Error::NotFound("采购确认不存在".to_string()))?;
                    ensure_pending_confirmation(&confirmation)?;
                    if confirmation.base.version != command.decision.expected_confirmation_edit_version() {
                        return Err(Error::ConflictError(
                            "采购确认工作数据已变化，请刷新后重试".to_string(),
                        ));
                    }
                    ensure_submission_identity(
                        &confirmation,
                        command.decision.submission_id(),
                        &command.expected_subject_version,
                    )?;
                    let mut work_item = load_procurement_confirmation_work_item(
                        &db,
                        ProcurementConfirmationTaskGuard::new(
                            &confirmation_id,
                            &confirmation.submission_id,
                            &command.work_item_id,
                            expected_task_version,
                            &command.expected_subject_version,
                            &actor_id,
                        ),
                        session,
                    )
                    .await?;
                    WorkItemService::new(db.clone(), rbac_for_tx)
                        .ensure_domain_decision_access(&actor_owned, &work_item, session)
                        .await?;
                    let mut submission = db
                        .sales_order_submissions()
                        .find_by_id(&confirmation.submission_id, session)
                        .await?
                        .ok_or_else(|| Error::NotFound("销售提交不存在".to_string()))?;
                    ensure_procurement_confirmation_actor_eligible(
                        &db,
                        &work_item,
                        &submission.submitted_by,
                        &actor_id,
                        session,
                    )
                    .await?;
                    let submission_lines = db
                        .sales_order_submission_lines()
                        .list_lines_by_submissions(std::slice::from_ref(&confirmation.submission_id), session)
                        .await?;
                    let incoming_lines = command.decision.approved_lines().ok_or_else(|| {
                        Error::ValidationError("通过采购确认时必须提交确认分行".to_string())
                    })?;
                    let confirmation_lines =
                        replace_pending_confirmation_lines(&db, &confirmation, incoming_lines, session)
                            .await?;
                    SalesReviewService::new(db.clone())
                        .ensure_confirmation_sources(&confirmation_lines, &submission_lines, session)
                        .await?;
                    ensure_confirmation_coverage(&submission_lines, &confirmation_lines)?;
                    let mut order = db
                        .sales_orders()
                        .find_by_id(&confirmation.sales_order_id, session)
                        .await?
                        .ok_or_else(|| Error::NotFound("销售单不存在".to_string()))?;
                    ensure_order_awaiting_confirmation(&order)?;
                    let mut document = db
                        .business_documents()
                        .find_by_id(&order.base.id, session)
                        .await?
                        .ok_or_else(|| Error::NotFound("销售单业务单据注册不存在".to_string()))?;
                    ensure_unformalized_sales_document(&document, &order)?;

                    let now = Instant::now();
                    let revision = build_revision(
                        &order,
                        &submission,
                        &submission_lines,
                        RevisionSource::ErpApproval,
                        now,
                        &actor_owned,
                    )?;
                    let from_status = order.commercial_status.as_str().to_string();
                    order.approve(now, &actor_id)?;
                    order.attach_revision(revision.revision.base.id.clone(), &actor_id);
                    submission.approve(&actor_id)?;
                    document.formalize(now);
                    confirmation.approve(&actor_id, now)?;
                    work_item.record_activity(&actor_id, now)?;
                    work_item.complete_by_domain_command(&actor_id, now)?;
                    let account = build_receivable_account(&order, &revision);
                    let entry = build_receivable_entry(&account, &revision, now)?;
                    let workflow = WorkflowAction::new(
                        WorkflowActionId::new(id_generator::next_id()),
                        WorkflowActionData {
                            document_id: BusinessDocumentId::new(order.base.id.clone()),
                            action_type: WorkflowActionType::Approve,
                            from_status,
                            to_status: order.commercial_status.as_str().to_string(),
                            actor_id: actor_id.clone(),
                            actor_role: work_item.owner_role.clone(),
                            comment: Some("采购二次确认通过".to_string()),
                        },
                    )?;
                    db.sales_order()
                        .formalize_submission(
                            &mut order,
                            &revision.revision,
                            &revision.lines,
                            &revision.goods_lines,
                            &revision.voucher_lines,
                            session,
                        )
                        .await?;
                    db.sales_order_submissions()
                        .update(&mut submission, session)
                        .await?;
                    db.receivable()
                        .create_receivable_with_entry(&account, &entry, session)
                        .await?;
                    db.procurement_confirmations()
                        .update(&mut confirmation, session)
                        .await?;
                    db.business_documents().update(&mut document, session).await?;
                    db.workflow_actions().create(&workflow, session).await?;
                    db.work_items().update(&mut work_item, session).await?;
                    let drafts = crate::purchase_order::create_drafts_from_confirmation_lines(
                        &db,
                        &confirmation.sales_order_id,
                        &confirmation_lines,
                        &actor_owned,
                        session,
                    )
                    .await?;
                    let receipt = DecisionCommandReceipt::Approved {
                        confirmation_id: confirmation.base.id.clone(),
                        sales_order_id: confirmation.sales_order_id.to_string(),
                        submission_id: confirmation.submission_id.to_string(),
                        sales_order_revision_id: revision.revision.base.id.clone(),
                        receivable_account_id: account.base.id.clone(),
                        purchase_orders: drafts
                            .into_iter()
                            .map(|draft| super::ApprovedPurchaseOrderDraftView {
                                purchase_order_id: draft.purchase_order_id,
                                purchase_no: draft.purchase_no,
                            })
                            .collect(),
                    };
                    let audit = actor_owned.resource_log_with_id(
                        audit_id_for_tx,
                        DECISION_COMMAND_ACTION,
                        "procurement_confirmation",
                        confirmation_id,
                        Some(decision_receipt_message(&fingerprint_for_tx, &receipt)),
                    )?;
                    db.audit_logs().create(&audit, session).await?;
                    Ok::<DecisionCommandReceipt, crate::errors::Error>(receipt)
                })
            })
            .await;

        let receipt = match transaction_result {
            Ok(receipt) => receipt,
            Err(error) => {
                if let Some(receipt) = self.replay_decision_command(&audit_id, id, &fingerprint).await? {
                    return receipt.into_result(&req.work_item_id);
                }
                return Err(error);
            }
        };
        receipt.into_result(&req.work_item_id)
    }

    /// 以 W07 唯一强类型完成命令驳回采购确认。
    ///
    /// 单事务追加正式驳回事实和 `workflow_action`、退回销售草稿并完成原待办；
    /// 不创建后继任务，只返回三条固定销售出路。
    ///
    /// # 错误
    /// 任一身份、状态、版本、责任、资格或岗位分离事实漂移时整体回滚。
    async fn complete_rejected_procurement_confirmation(
        &self,
        id: &str,
        req: CompleteProcurementConfirmationCommand,
        actor: &AuditActor,
        rbac: SharedRbacService,
    ) -> Result<CompleteProcurementConfirmationResult> {
        let (reject_reason_code, reject_comment) = req
            .decision
            .rejection()
            .ok_or_else(|| Error::ValidationError("采购确认驳回命令分支非法".to_string()))?;
        let reject_comment = reject_comment.to_string();
        let expected_task_version = parse_task_version(&req.expected_task_version)?;
        let fingerprint = command_fingerprint(DECISION_COMMAND_ACTION, id, actor.id(), &req)?;
        let audit_id = command_audit_id(DECISION_COMMAND_ACTION, actor.id(), &req.idempotency_key);
        if let Some(receipt) = self.replay_decision_command(&audit_id, id, &fingerprint).await? {
            return receipt.into_result(&req.work_item_id);
        }
        let db = self.db.clone();
        let client = db.client().clone();
        let command = req.clone();
        let actor_owned = actor.clone();
        let actor_id = actor.id().to_string();
        let confirmation_id = id.to_string();
        let audit_id_for_tx = audit_id.clone();
        let fingerprint_for_tx = fingerprint.clone();
        let rbac_for_tx = rbac.clone();
        let transaction_result = client
            .with_transaction(move |session| {
                Box::pin(async move {
                    let mut confirmation = db
                        .procurement_confirmations()
                        .find_by_id(&confirmation_id, session)
                        .await?
                        .ok_or_else(|| Error::NotFound("采购确认不存在".to_string()))?;
                    ensure_pending_confirmation(&confirmation)?;
                    if confirmation.base.version != command.decision.expected_confirmation_edit_version() {
                        return Err(Error::ConflictError(
                            "采购确认工作数据已变化，请刷新后重试".to_string(),
                        ));
                    }
                    ensure_submission_identity(
                        &confirmation,
                        command.decision.submission_id(),
                        &command.expected_subject_version,
                    )?;
                    let mut work_item = load_procurement_confirmation_work_item(
                        &db,
                        ProcurementConfirmationTaskGuard::new(
                            &confirmation_id,
                            &confirmation.submission_id,
                            &command.work_item_id,
                            expected_task_version,
                            &command.expected_subject_version,
                            &actor_id,
                        ),
                        session,
                    )
                    .await?;
                    WorkItemService::new(db.clone(), rbac_for_tx)
                        .ensure_domain_decision_access(&actor_owned, &work_item, session)
                        .await?;
                    let mut submission = db
                        .sales_order_submissions()
                        .find_by_id(&confirmation.submission_id, session)
                        .await?
                        .ok_or_else(|| Error::NotFound("销售提交不存在".to_string()))?;
                    ensure_procurement_confirmation_actor_eligible(
                        &db,
                        &work_item,
                        &submission.submitted_by,
                        &actor_id,
                        session,
                    )
                    .await?;
                    let mut order = db
                        .sales_orders()
                        .find_by_id(&confirmation.sales_order_id, session)
                        .await?
                        .ok_or_else(|| Error::NotFound("销售单不存在".to_string()))?;
                    ensure_order_awaiting_confirmation(&order)?;
                    let document = db
                        .business_documents()
                        .find_by_id(&order.base.id, session)
                        .await?
                        .ok_or_else(|| Error::NotFound("销售单业务单据注册不存在".to_string()))?;
                    ensure_unformalized_sales_document(&document, &order)?;

                    let now = Instant::now();
                    let from_status = order.commercial_status.as_str().to_string();
                    confirmation.reject(&actor_id, now, reject_reason_code, Some(reject_comment.clone()))?;
                    order.return_to_draft(&actor_id)?;
                    submission.reject(&actor_id)?;
                    work_item.record_activity(&actor_id, now)?;
                    work_item.complete_by_domain_command(&actor_id, now)?;
                    let workflow = WorkflowAction::new(
                        WorkflowActionId::new(id_generator::next_id()),
                        WorkflowActionData {
                            document_id: BusinessDocumentId::new(order.base.id.clone()),
                            action_type: WorkflowActionType::Reject,
                            from_status,
                            to_status: order.commercial_status.as_str().to_string(),
                            actor_id: actor_id.clone(),
                            actor_role: work_item.owner_role.clone(),
                            comment: rejection_workflow_comment(reject_reason_code, &reject_comment)?,
                        },
                    )?;
                    db.procurement_confirmations()
                        .update(&mut confirmation, session)
                        .await?;
                    db.sales_orders().update(&mut order, session).await?;
                    db.sales_order_submissions()
                        .update(&mut submission, session)
                        .await?;
                    db.workflow_actions().create(&workflow, session).await?;
                    db.work_items().update(&mut work_item, session).await?;
                    let receipt = DecisionCommandReceipt::Rejected {
                        confirmation_id: confirmation.base.id.clone(),
                        sales_order_id: confirmation.sales_order_id.to_string(),
                        submission_id: confirmation.submission_id.to_string(),
                        workflow_action_id: workflow.base.id.clone(),
                    };
                    let audit = actor_owned.resource_log_with_id(
                        audit_id_for_tx,
                        DECISION_COMMAND_ACTION,
                        "procurement_confirmation",
                        confirmation_id,
                        Some(decision_receipt_message(&fingerprint_for_tx, &receipt)),
                    )?;
                    db.audit_logs().create(&audit, session).await?;
                    Ok::<DecisionCommandReceipt, crate::errors::Error>(receipt)
                })
            })
            .await;

        let receipt = match transaction_result {
            Ok(receipt) => receipt,
            Err(error) => {
                if let Some(receipt) = self.replay_decision_command(&audit_id, id, &fingerprint).await? {
                    return receipt.into_result(&req.work_item_id);
                }
                return Err(error);
            }
        };
        receipt.into_result(&req.work_item_id)
    }

    /// 按稳定审计收据严格重放已提交的 W07 正式决定。
    async fn replay_decision_command(
        &self,
        audit_id: &str,
        confirmation_id: &str,
        expected_fingerprint: &str,
    ) -> Result<Option<DecisionCommandReceipt>> {
        let Some(audit) = self
            .db
            .audit_logs()
            .find_by_id(audit_id, &mut database::NoTransaction)
            .await?
        else {
            return Ok(None);
        };
        if audit.action != DECISION_COMMAND_ACTION
            || audit.resource_type != "procurement_confirmation"
            || audit.resource_id.as_deref() != Some(confirmation_id)
            || !audit.success
        {
            return Err(Error::Internal("采购确认决定幂等收据身份非法".to_string()));
        }
        let receipt = parse_decision_receipt(
            audit
                .message
                .as_deref()
                .ok_or_else(|| Error::Internal("采购确认决定幂等收据为空".to_string()))?,
            expected_fingerprint,
        )?;
        Ok(Some(receipt))
    }

    /// 重新校验确认行引用的供给修订与能力修订仍是当前有效版本。
    ///
    /// # 参数
    /// * `lines` - 待保存或待审批的采购确认分行
    ///
    /// # 返回
    /// 全部分行来源与供应商匹配且仍有效时返回 `Ok(())`。
    ///
    /// # 错误
    /// 来源缺失、供应商不匹配或版本已更新时返回 `ValidationError`。
    pub(super) async fn ensure_confirmation_sources(
        &self,
        lines: &[ProcurementConfirmationLine],
        submission_lines: &[SalesOrderSubmissionLine],
        executor: &mut dyn Executor,
    ) -> Result<()> {
        let today = BusinessDate::today();
        let zero = Quantity::from_str("0").expect("静态零值必须合法");
        let mut quantities_by_revision: HashMap<String, (Option<Quantity>, Quantity)> = HashMap::new();
        let mut resolved = Vec::with_capacity(lines.len());
        for line in lines {
            let (offering, revision, availability) =
                self.current_confirmation_offering(line, executor).await?;
            ensure_confirmation_line_sku(line, &offering, submission_lines)?;
            let total = quantities_by_revision
                .entry(revision.base.id.clone())
                .or_insert((availability.available_quantity, zero));
            total.1 = Quantity::try_from(total.1.to_decimal() + line.confirmed_quantity.to_decimal())?;
            resolved.push((line, revision, availability));
        }
        for (line, revision, availability) in resolved {
            let procurement_quantity = quantities_by_revision
                .get(&revision.base.id)
                .map(|(_, quantity)| *quantity)
                .ok_or_else(|| Error::BusinessLogicError("采购数量汇总结果缺失".to_string()))?;
            ensure_confirmation_line_terms(
                line,
                &revision,
                &availability,
                procurement_quantity,
                submission_lines,
                today,
            )?;
            self.ensure_confirmation_capability(line, today, executor).await?;
        }
        ensure_confirmation_capacity(&quantities_by_revision)?;
        Ok(())
    }

    /// 加载并校验确认行引用的当前供给。
    async fn current_confirmation_offering(
        &self,
        line: &ProcurementConfirmationLine,
        executor: &mut dyn Executor,
    ) -> Result<(
        SupplierOffering,
        SupplierOfferingRevision,
        SupplierOfferingAvailability,
    )> {
        let revision_id = line
            .supplier_offering_revision_id
            .as_ref()
            .ok_or_else(|| Error::ValidationError(format!("采购确认第 {} 行缺少供给版本", line.line_no)))?;
        let revision = self
            .db
            .supplier_offering_revisions()
            .find_by_id(revision_id, executor)
            .await?
            .ok_or_else(|| Error::ValidationError(format!("采购确认第 {} 行供给版本不存在", line.line_no)))?;
        let offering = self
            .db
            .supplier_offerings()
            .find_by_id(&revision.supplier_offering_id, executor)
            .await?
            .ok_or_else(|| Error::ValidationError(format!("采购确认第 {} 行供给不存在", line.line_no)))?;
        let is_current = offering.stable.current_revision_id.as_deref() == Some(revision_id.as_ref());
        if offering.supplier_id != line.supplier_id
            || offering.stable.status != OfferingStatus::Active
            || !is_current
        {
            return Err(Error::ValidationError(format!(
                "采购确认第 {} 行供给已变化，请重新选择",
                line.line_no
            )));
        }
        let availability = self
            .db
            .supplier_offering_availabilities()
            .find_by_offering_id(&revision.supplier_offering_id, executor)
            .await?
            .ok_or_else(|| Error::ValidationError(format!("采购确认第 {} 行可供状态不存在", line.line_no)))?;
        Ok((offering, revision, availability))
    }

    /// 校验确认行引用的供应商能力仍为当前启用版本。
    async fn ensure_confirmation_capability(
        &self,
        line: &ProcurementConfirmationLine,
        today: BusinessDate,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        let revision = self
            .db
            .supplier_capability_revisions()
            .find_by_id(&line.supplier_capability_revision_id, executor)
            .await?
            .ok_or_else(|| Error::ValidationError(format!("采购确认第 {} 行能力版本不存在", line.line_no)))?;
        let capability = self
            .db
            .supplier_capabilities()
            .find_by_supplier_and_code(&line.supplier_id, revision.capability_code, executor)
            .await?
            .ok_or_else(|| {
                Error::ValidationError(format!("采购确认第 {} 行供应商能力不存在", line.line_no))
            })?;
        let is_current = capability.stable.current_revision_id.as_deref()
            == Some(line.supplier_capability_revision_id.as_ref());
        let in_window = revision.valid_from <= today && revision.valid_to.is_none_or(|date| today <= date);
        let capability_matches_mode =
            revision.capability_code == capability_code_for_fulfillment(line.fulfillment_mode);
        if revision.supplier_id != line.supplier_id
            || revision.status != CapabilityStatus::Active
            || capability.stable.status != CapabilityStatus::Active
            || !in_window
            || !capability_matches_mode
            || !is_current
        {
            return Err(Error::ValidationError(format!(
                "采购确认第 {} 行能力版本已变化，请重新选择",
                line.line_no
            )));
        }
        let supplier = self
            .db
            .supplier_accounts()
            .find_by_id(&line.supplier_id, executor)
            .await?
            .ok_or_else(|| Error::ValidationError(format!("采购确认第 {} 行供应商不存在", line.line_no)))?;
        if !supplier.is_active() {
            return Err(Error::BusinessLogicError(format!(
                "采购确认第 {} 行供应商已停用",
                line.line_no
            )));
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum DecisionCommandReceipt {
    Approved {
        confirmation_id: String,
        sales_order_id: String,
        submission_id: String,
        sales_order_revision_id: String,
        receivable_account_id: String,
        purchase_orders: Vec<super::ApprovedPurchaseOrderDraftView>,
    },
    Rejected {
        confirmation_id: String,
        sales_order_id: String,
        submission_id: String,
        workflow_action_id: String,
    },
}

impl DecisionCommandReceipt {
    fn into_result(self, work_item_id: &str) -> Result<CompleteProcurementConfirmationResult> {
        let business_result = match self {
            Self::Approved {
                confirmation_id,
                sales_order_id,
                submission_id,
                sales_order_revision_id,
                receivable_account_id,
                purchase_orders,
            } => ProcurementConfirmationBusinessResult::ApprovedAndSalesEffective {
                procurement_confirmation_id: confirmation_id.clone(),
                sales_order_id,
                submission_id,
                sales_order_revision_id,
                receivable_account_id,
                procurement_creation_basis_id: confirmation_id,
                purchase_orders,
            },
            Self::Rejected {
                confirmation_id,
                sales_order_id,
                submission_id,
                workflow_action_id,
            } => ProcurementConfirmationBusinessResult::RejectedToSales {
                procurement_confirmation_id: confirmation_id,
                sales_order_id,
                rejected_submission_id: submission_id,
                workflow_action_id,
                next_sales_resolutions: [
                    ProcurementSalesResolution::ResubmitChangedTerms,
                    ProcurementSalesResolution::RequestLowMarginAcceptance,
                    ProcurementSalesResolution::VoidAfterRejection,
                ],
            },
        };
        Ok(CompleteProcurementConfirmationResult {
            work_item_id: work_item_id.to_string(),
            work_item_status: entities::work_item::WorkItemStatus::Completed,
            business_result,
        })
    }
}

fn ensure_unformalized_sales_document(
    document: &entities::document_registry::BusinessDocument,
    order: &SalesOrder,
) -> Result<()> {
    if document.document_type != DocumentType::SalesOrder
        || document.document_no != order.order_no
        || document.formalized_at.is_some()
    {
        return Err(Error::ConflictError(
            "销售单业务单据注册与待正式化对象不一致".to_string(),
        ));
    }
    Ok(())
}

fn rejection_workflow_comment(
    reason_code: entities::sales_review::ProcurementRejectReasonCode,
    comment: &str,
) -> Result<Option<String>> {
    let mut value = format!("reason={}", reason_code.as_str());
    value.push_str("; ");
    value.push_str(comment.trim());
    if value.chars().count() > 512 {
        return Err(Error::ValidationError(
            "采购确认驳回工作流意见不能超过512个字符".to_string(),
        ));
    }
    Ok(Some(value))
}

fn decision_receipt_message(fingerprint: &str, receipt: &DecisionCommandReceipt) -> String {
    match receipt {
        DecisionCommandReceipt::Approved {
            confirmation_id,
            sales_order_id,
            submission_id,
            sales_order_revision_id,
            receivable_account_id,
            purchase_orders,
        } => {
            let drafts = encode_purchase_order_receipt(purchase_orders);
            format!(
                "fp={fingerprint};approved={confirmation_id}|{sales_order_id}|{submission_id}|{sales_order_revision_id}|{receivable_account_id}|{drafts}"
            )
        }
        DecisionCommandReceipt::Rejected {
            confirmation_id,
            sales_order_id,
            submission_id,
            workflow_action_id,
        } => {
            format!(
                "fp={fingerprint};rejected={confirmation_id}|{sales_order_id}|{submission_id}|{workflow_action_id}"
            )
        }
    }
}

/// 把通过事务生成的采购草稿编码进幂等收据。
///
/// # 参数
/// * `purchase_orders` - 草稿身份列表
///
/// # 返回
/// 返回 `id:no,id:no` 形式的稳定文本；空列表返回空串。
///
/// # 错误
/// 无。
fn encode_purchase_order_receipt(purchase_orders: &[super::ApprovedPurchaseOrderDraftView]) -> String {
    purchase_orders
        .iter()
        .map(|order| format!("{}:{}", order.purchase_order_id, order.purchase_no))
        .collect::<Vec<_>>()
        .join(",")
}

/// 从幂等收据解析采购草稿身份。
///
/// # 参数
/// * `value` - `id:no,id:no` 文本；空串表示无草稿
///
/// # 返回
/// 返回解析出的草稿身份；无法识别的片段会被跳过。
///
/// # 错误
/// 无。
fn decode_purchase_order_receipt(value: &str) -> Vec<super::ApprovedPurchaseOrderDraftView> {
    if value.is_empty() {
        return Vec::new();
    }
    value
        .split(',')
        .filter_map(|item| {
            let (purchase_order_id, purchase_no) = item.split_once(':')?;
            if purchase_order_id.is_empty() || purchase_no.is_empty() {
                return None;
            }
            Some(super::ApprovedPurchaseOrderDraftView {
                purchase_order_id: purchase_order_id.to_string(),
                purchase_no: purchase_no.to_string(),
            })
        })
        .collect()
}

fn parse_decision_receipt(message: &str, expected_fingerprint: &str) -> Result<DecisionCommandReceipt> {
    let value = message
        .strip_prefix("fp=")
        .ok_or_else(|| Error::Internal("采购确认决定幂等收据格式非法".to_string()))?;
    let (fingerprint, result) = value
        .split_once(';')
        .ok_or_else(|| Error::Internal("采购确认决定幂等收据结果缺失".to_string()))?;
    if fingerprint != expected_fingerprint {
        return Err(Error::ConflictError(
            "幂等键已用于不同的采购确认决定命令".to_string(),
        ));
    }
    if let Some(value) = result.strip_prefix("approved=") {
        let fields = value.split('|').collect::<Vec<_>>();
        if fields.len() != 5 && fields.len() != 6 {
            return Err(Error::Internal("采购确认通过收据结果非法".to_string()));
        }
        return Ok(DecisionCommandReceipt::Approved {
            confirmation_id: fields[0].to_string(),
            sales_order_id: fields[1].to_string(),
            submission_id: fields[2].to_string(),
            sales_order_revision_id: fields[3].to_string(),
            receivable_account_id: fields[4].to_string(),
            purchase_orders: fields
                .get(5)
                .map(|value| decode_purchase_order_receipt(value))
                .unwrap_or_default(),
        });
    }
    if let Some(value) = result.strip_prefix("rejected=") {
        let fields = value.split('|').collect::<Vec<_>>();
        let [confirmation_id, sales_order_id, submission_id, workflow_action_id] = fields.as_slice() else {
            return Err(Error::Internal("采购确认驳回收据结果非法".to_string()));
        };
        return Ok(DecisionCommandReceipt::Rejected {
            confirmation_id: (*confirmation_id).to_string(),
            sales_order_id: (*sales_order_id).to_string(),
            submission_id: (*submission_id).to_string(),
            workflow_action_id: (*workflow_action_id).to_string(),
        });
    }
    Err(Error::Internal("采购确认决定收据结论非法".to_string()))
}

/// 校验确认分行选择的供给属于对应销售提交商品。
fn ensure_confirmation_line_sku(
    line: &ProcurementConfirmationLine,
    offering: &SupplierOffering,
    submission_lines: &[SalesOrderSubmissionLine],
) -> Result<()> {
    let submission_line = submission_lines
        .iter()
        .find(|row| row.base.id == line.sales_order_submission_line_id.as_ref())
        .ok_or_else(|| Error::ValidationError(format!("采购确认第 {} 行销售明细不存在", line.line_no)))?;
    if submission_line.sku_id.as_ref() == Some(&offering.sku_id) {
        return Ok(());
    }
    Err(Error::ValidationError(format!(
        "采购确认第 {} 行供给不属于该销售商品，请重新选择",
        line.line_no
    )))
}

/// 校验确认行价格、税率、可供状态、有效期与销售承诺交期。
fn ensure_confirmation_line_terms(
    line: &ProcurementConfirmationLine,
    revision: &SupplierOfferingRevision,
    availability: &SupplierOfferingAvailability,
    procurement_quantity: Quantity,
    submission_lines: &[SalesOrderSubmissionLine],
    today: BusinessDate,
) -> Result<()> {
    if !availability.is_available()
        || revision.valid_from > today
        || revision.valid_to.is_some_and(|valid_to| valid_to < today)
    {
        return Err(Error::ValidationError(format!(
            "采购确认第 {} 行供给当前不可用，请重新计算方案",
            line.line_no
        )));
    }
    let expected_price = if procurement_quantity >= revision.bulk_minimum_order_quantity {
        revision.bulk_supply_price_gross
    } else {
        revision.dropship_supply_price_gross
    };
    if line.latest_cost_gross != expected_price || line.input_tax_rate != revision.input_tax_rate {
        return Err(Error::ValidationError(format!(
            "采购确认第 {} 行价格或税率已变化，请重新计算方案",
            line.line_no
        )));
    }
    let submission_line = submission_lines
        .iter()
        .find(|submission| submission.base.id == line.sales_order_submission_line_id.as_ref())
        .ok_or_else(|| Error::ValidationError(format!("采购确认第 {} 行销售明细不存在", line.line_no)))?;
    let due_at = submission_line
        .fulfillment_due_at
        .ok_or_else(|| Error::ValidationError(format!("采购确认第 {} 行销售承诺交期缺失", line.line_no)))?;
    let due_date = BusinessDate::from_str(&due_at.as_utc().date_naive().to_string())?;
    if line.expected_delivery_date > due_date {
        return Err(Error::ValidationError(format!(
            "采购确认第 {} 行预计交期晚于销售承诺日期 {}",
            line.line_no, due_date
        )));
    }
    Ok(())
}

/// 校验同一供给修订的多条拆分行合计不超过当前可供数量。
fn ensure_confirmation_capacity(
    quantities_by_revision: &HashMap<String, (Option<Quantity>, Quantity)>,
) -> Result<()> {
    for (revision_id, (available, confirmed)) in quantities_by_revision {
        if available.is_some_and(|quantity| *confirmed > quantity) {
            return Err(Error::ValidationError(format!(
                "供给版本 {} 的确认数量合计超过当前可供数量，请重新计算方案",
                revision_id
            )));
        }
    }
    Ok(())
}

/// 将确认履约方式映射到供应商能力类别。
fn capability_code_for_fulfillment(mode: entities::sales_review::types::FulfillmentMode) -> CapabilityCode {
    match mode {
        entities::sales_review::types::FulfillmentMode::CompanyWarehouse
        | entities::sales_review::types::FulfillmentMode::SupplierDirect => CapabilityCode::Physical,
        entities::sales_review::types::FulfillmentMode::ElectronicDelivery => CapabilityCode::Virtual,
        entities::sales_review::types::FulfillmentMode::OfflineService => CapabilityCode::OfflineService,
    }
}

/// 校验采购确认覆盖全部需外采明细（§6.5 跨行断言）。
///
/// # 参数
/// * `submission_lines` - 销售提交明细
/// * `confirmation_lines` - 采购确认分行
///
/// # 返回
/// 全部行确认数量覆盖承诺数量时返回 `Ok(())`。
///
/// # 错误
/// 任一提交明细确认数量不足时返回 `ValidationError`。
fn ensure_confirmation_coverage(
    submission_lines: &[SalesOrderSubmissionLine],
    confirmation_lines: &[ProcurementConfirmationLine],
) -> Result<()> {
    for sub_line in submission_lines {
        if sub_line.line_type == LineType::Voucher {
            continue;
        }
        let required = sub_line
            .quantity
            .ok_or_else(|| Error::ValidationError(format!("第 {} 行缺少承诺数量", sub_line.line_no)))?;
        let mut confirmed = Quantity::from_str("0").expect("静态零值必须合法");
        for line in confirmation_lines {
            if line.sales_order_submission_line_id.to_string() == sub_line.base.id {
                if line.supplier_offering_revision_id.is_none() {
                    return Err(Error::ValidationError(format!(
                        "第 {} 行缺少有效供给版本，请重新选择供应商供给",
                        sub_line.line_no
                    )));
                }
                confirmed = Quantity::try_from(confirmed.to_decimal() + line.confirmed_quantity.to_decimal())
                    .map_err(|error| Error::ValidationError(error.to_string()))?;
            }
        }
        if confirmed.to_decimal() < required.to_decimal() {
            return Err(Error::ValidationError(format!(
                "第 {} 行确认数量不足：需 {required}，已确认 {confirmed}",
                sub_line.line_no
            )));
        }
    }
    Ok(())
}

/// 校验销售单处于待采购确认状态。
///
/// # 参数
/// * `order` - 销售单
///
/// # 返回
/// 状态合法时返回 `Ok(())`。
///
/// # 错误
/// 状态非法时返回 `ValidationError`。
fn ensure_order_awaiting_confirmation(order: &SalesOrder) -> Result<()> {
    if order.commercial_status != entities::sales_order::CommercialStatus::PendingReview
        || order.review_status != entities::sales_order::ReviewStatus::PendingProcurementConfirmation
    {
        return Err(Error::ValidationError(
            "销售单不在待采购确认状态，无法通过采购确认".to_string(),
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use serde_json::Value;

    use super::{parse_decision_receipt, DecisionCommandReceipt};

    #[test]
    fn decision_receipt_rejects_same_key_with_different_payload() {
        let receipt = parse_decision_receipt(
            "fp=abc;approved=confirmation-1|sales-1|submission-1|revision-1|account-1",
            "abc",
        )
        .unwrap();
        assert_eq!(
            receipt,
            DecisionCommandReceipt::Approved {
                confirmation_id: "confirmation-1".to_string(),
                sales_order_id: "sales-1".to_string(),
                submission_id: "submission-1".to_string(),
                sales_order_revision_id: "revision-1".to_string(),
                receivable_account_id: "account-1".to_string(),
                purchase_orders: Vec::new(),
            }
        );
        assert!(parse_decision_receipt(
            "fp=abc;approved=confirmation-1|sales-1|submission-1|revision-1|account-1",
            "other"
        )
        .is_err());
    }

    #[test]
    fn rejected_result_has_fixed_resolutions_and_no_successor_task() {
        let result = DecisionCommandReceipt::Rejected {
            confirmation_id: "confirmation-1".to_string(),
            sales_order_id: "sales-1".to_string(),
            submission_id: "submission-1".to_string(),
            workflow_action_id: "workflow-1".to_string(),
        }
        .into_result("work-1")
        .unwrap();
        let value = serde_json::to_value(result).unwrap();
        let business = value.get("business_result").and_then(Value::as_object).unwrap();

        assert_eq!(value["work_item_status"], "COMPLETED");
        assert_eq!(business["outcome"], "REJECTED_TO_SALES");
        assert_eq!(business["next_sales_resolutions"].as_array().unwrap().len(), 3);
        assert!(!business.contains_key("successor_work_item_id"));
    }

    #[test]
    fn approved_result_exposes_confirmation_as_creation_basis() {
        let result = DecisionCommandReceipt::Approved {
            confirmation_id: "confirmation-1".to_string(),
            sales_order_id: "sales-1".to_string(),
            submission_id: "submission-1".to_string(),
            sales_order_revision_id: "revision-1".to_string(),
            receivable_account_id: "account-1".to_string(),
            purchase_orders: vec![super::super::ApprovedPurchaseOrderDraftView {
                purchase_order_id: "po-1".to_string(),
                purchase_no: "PO-1".to_string(),
            }],
        }
        .into_result("work-1")
        .unwrap();
        let value = serde_json::to_value(result).unwrap();

        assert_eq!(
            value["business_result"]["procurement_creation_basis_id"],
            "confirmation-1"
        );
        assert_eq!(
            value["business_result"]["purchase_orders"][0]["purchase_order_id"],
            "po-1"
        );
        assert_eq!(
            value["business_result"]["purchase_orders"][0]["purchase_no"],
            "PO-1"
        );
    }

    #[test]
    fn approved_receipt_round_trips_purchase_orders() {
        let receipt = parse_decision_receipt(
            "fp=abc;approved=confirmation-1|sales-1|submission-1|revision-1|account-1|po-1:PO-1,po-2:PO-2",
            "abc",
        )
        .unwrap();
        assert_eq!(
            receipt,
            DecisionCommandReceipt::Approved {
                confirmation_id: "confirmation-1".to_string(),
                sales_order_id: "sales-1".to_string(),
                submission_id: "submission-1".to_string(),
                sales_order_revision_id: "revision-1".to_string(),
                receivable_account_id: "account-1".to_string(),
                purchase_orders: vec![
                    super::super::ApprovedPurchaseOrderDraftView {
                        purchase_order_id: "po-1".to_string(),
                        purchase_no: "PO-1".to_string(),
                    },
                    super::super::ApprovedPurchaseOrderDraftView {
                        purchase_order_id: "po-2".to_string(),
                        purchase_no: "PO-2".to_string(),
                    },
                ],
            }
        );
    }
}
