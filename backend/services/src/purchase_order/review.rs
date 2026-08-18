//! 采购财务审核、应付与成本事实编排。

use database::{
    AccessControlExt, CostExt, Executor, NoTransaction, PayableExt, PurchaseOrderExt, SalesOrderExt,
    SalesReviewExt, SupplierOfferingExt, Transactional, WorkItemExt,
};
use entities::common::time::Instant;
use entities::ids::{CostEntryId, PayableEntryId};
use entities::purchase_order::{
    PurchaseLineType, PurchaseOrder, PurchaseOrderReviewDecision, PurchaseOrderStatus,
    PurchaseOrderSubmission, PurchaseOrderSubmissionLine, SubmissionStatus,
};
use entities::work_item::{WorkItem, WorkItemStatus, WorkItemType};
use id_generator::next_id;
use sha2::{Digest, Sha256};
use validator::Validate;

use super::dto::{
    PurchaseOrderReviewDecisionCommand, PurchaseOrderReviewDecisionResult, PurchaseReviewResult,
    ReviewPurchaseOrderCommand,
};
use super::shared::{zero_amount, zero_rate};
use super::PurchaseOrderService;
use crate::audit::AuditActor;
use crate::errors::{Error, Result};
use crate::iam::SharedRbacService;
use crate::work_item::WorkItemService;

const PURCHASE_REVIEW_RECEIPT_PREFIX: &str = "purchase-review-command-";
const COMMAND_FINGERPRINT_PREFIX: &str = "command_sha256=";

#[derive(Clone, Copy)]
struct PurchaseReviewContext<'a> {
    purchase_order_id: &'a str,
    work_item_id: &'a str,
    expected_task_version: u64,
    expected_subject_version: &'a str,
    decision: &'a PurchaseOrderReviewDecisionCommand,
    idempotency_key: &'a str,
    actor: &'a AuditActor,
}

impl PurchaseReviewContext<'_> {
    fn fingerprint(self, review_result: &str, decision_note: Option<&str>) -> String {
        command_fingerprint(&[
            self.purchase_order_id,
            &self.decision.submission_id,
            self.work_item_id,
            &self.expected_task_version.to_string(),
            self.expected_subject_version,
            &self.decision.expected_purchase_order_lock_version.to_string(),
            review_result,
            decision_note.unwrap_or_default(),
        ])
    }
}

impl PurchaseOrderService {
    /// 旧财务审核旁路。审批改造后立即失败关闭。
    ///
    /// # 错误
    /// 恒返回冲突，不得再写入 `PurchaseReviewStatus`。
    pub async fn review_purchase_order(
        &self,
        _path_purchase_order_id: &str,
        _command: ReviewPurchaseOrderCommand,
        _actor: &AuditActor,
        _rbac: SharedRbacService,
    ) -> Result<PurchaseReviewResult> {
        Err(Error::ConflictError(
            "采购单必须走统一审批，禁止写入财务审核旁路".to_string(),
        ))
    }

    /// 最终通过并生效：形成采购版本、应付与成本事实。
    ///
    /// 仅由合同 §4.4.4 `on_final_approve` 调用，不得再作为人工财务审核旁路。
    ///
    /// # 参数
    /// * `id` - 采购单主键
    /// * `actor` - 已通过鉴权的审计操作人
    ///
    /// # 返回
    /// 返回生效结果。
    ///
    /// # 错误
    /// 非审批中、缺少提交、来源复验失败或仓储失败时返回错误。
    pub async fn formalize_approved_order(
        &self,
        id: &str,
        actor: &AuditActor,
    ) -> Result<PurchaseReviewResult> {
        use super::adapter::{execute_purchase_order_domain_action, purchase_order_adapter};
        use crate::approval::policy::ApprovalDomainAction;

        let adapter = purchase_order_adapter()?;
        let order = self
            .db
            .purchase_orders()
            .find_by_id(id, &mut NoTransaction)
            .await?
            .ok_or_else(|| Error::NotFound("采购单不存在".to_string()))?;
        execute_purchase_order_domain_action(
            &mut order.clone(),
            adapter.on_final_approve,
            order.current_submission_id.clone().unwrap_or_default().as_str(),
            actor.id(),
        )?;
        let submission_id = order
            .current_submission_id
            .clone()
            .ok_or_else(|| Error::BusinessLogicError("采购单缺少待生效提交".to_string()))?;
        let submission = self
            .db
            .purchase_order_submissions()
            .find_by_id(&submission_id, &mut NoTransaction)
            .await?
            .ok_or_else(|| Error::NotFound("待审核提交不存在".to_string()))?;
        if submission.status != SubmissionStatus::Pending {
            return Err(Error::ConflictError(
                "提交已审核或已失效，请勿重复生效".to_string(),
            ));
        }
        let submission_lines = self
            .db
            .purchase_order_submission_lines()
            .find_many(
                mongodb::bson::doc! { "purchase_order_submission_id": &submission_id },
                &mut NoTransaction,
            )
            .await?;
        let revision_no = self.next_revision_no(&order).await?;
        let (revision, revision_lines) = self
            .build_effective_revision(&order, &submission, &submission_lines, revision_no)
            .await?;
        let payable = self.build_payable(&order, &submission).await?;
        let cost_entries = self
            .build_confirmed_cost_entries(&submission, &submission_lines, revision_no)
            .await?;
        let subject_version = order.approval_subject_version.to_string();
        let lock_version = order.base.version;
        let revision_id = revision.base.id.clone();
        let payable_entry_id = payable.1.base.id.clone();
        persist_formalized_order(
            &self.db,
            order,
            submission,
            submission_lines,
            revision,
            revision_lines,
            payable,
            cost_entries,
            actor,
        )
        .await?;
        let _ = ApprovalDomainAction::PurchaseOrderFormalizeApprovedOrder;
        Ok(PurchaseReviewResult {
            work_item_id: String::new(),
            work_item_status: WorkItemStatus::Completed.as_str().to_string(),
            task_version: "0".to_string(),
            subject_version,
            review_result: "APPROVED".to_string(),
            revision_id: Some(revision_id),
            revision_no: Some(revision_no),
            payable_entry_id: Some(payable_entry_id),
            lock_version,
            reference: format!("PO-V{revision_no}"),
        })
    }

    /// 财务审核通过（§8.1.4 事务不变量）。
    ///
    /// 单事务：锁定提交 → 逐行复验采购确认来源 → 复制为生效版本与版本行 →
    /// 形成销售分配 → 推进采购状态与版本指针 → 应付原始分录与 `CONFIRMED`
    /// 成本事实 → 完成审核待办 → 审计。任一失败整体回滚。
    ///
    /// # 返回
    /// 返回审核结果（版本与应付分录）。
    ///
    /// # 错误
    /// * `NotFound` - 采购单/提交/待办不存在
    /// * `ConflictError` - 版本不一致或重复审核
    /// * `BusinessLogicError` - 状态机或来源校验失败
    async fn review_approve(
        &self,
        review: PurchaseReviewContext<'_>,
        rbac: SharedRbacService,
    ) -> Result<PurchaseReviewResult> {
        let action = "purchase_order.review";
        let fingerprint = review.fingerprint(
            PurchaseOrderReviewDecisionResult::Approved.as_str(),
            review.decision.comment.as_deref(),
        );
        let PurchaseReviewContext {
            purchase_order_id: id,
            work_item_id,
            expected_task_version,
            expected_subject_version,
            decision,
            idempotency_key,
            actor,
        } = review;
        let audit_id = purchase_review_audit_id(actor.id(), action, work_item_id, idempotency_key);
        if let Some(result) = self
            .replay_purchase_review(
                &audit_id,
                &fingerprint,
                id,
                work_item_id,
                expected_subject_version,
            )
            .await?
        {
            return Ok(result);
        }
        let order = self
            .db
            .purchase_orders()
            .find_by_id(id, &mut NoTransaction)
            .await?
            .ok_or_else(|| Error::NotFound("采购单不存在".to_string()))?;
        self.ensure_version(&order, decision.expected_purchase_order_lock_version)?;
        if order.stable.status != PurchaseOrderStatus::PendingFinanceReview {
            return Err(Error::ConflictError(
                "采购单不在待财务审核状态，请勿重复审核".to_string(),
            ));
        }
        if order.current_submission_id.as_deref() != Some(&decision.submission_id) {
            return Err(Error::ConflictError("提交与采购单当前待审提交不一致".to_string()));
        }
        let submission = self
            .db
            .purchase_order_submissions()
            .find_by_id(&decision.submission_id, &mut NoTransaction)
            .await?
            .ok_or_else(|| Error::NotFound("待审核提交不存在".to_string()))?;
        if submission.status != SubmissionStatus::Pending {
            return Err(Error::ConflictError(
                "提交已审核或已失效，请勿重复审核".to_string(),
            ));
        }
        let submission_lines = self
            .db
            .purchase_order_submission_lines()
            .find_many(
                mongodb::bson::doc! { "purchase_order_submission_id": &decision.submission_id },
                &mut NoTransaction,
            )
            .await?;

        // 生效版本号：当前版本 + 1。
        let revision_no = self.next_revision_no(&order).await?;
        let (revision, revision_lines) = self
            .build_effective_revision(&order, &submission, &submission_lines, revision_no)
            .await?;

        let mut work_item = self
            .db
            .work_items()
            .find_by_id(work_item_id, &mut NoTransaction)
            .await?
            .ok_or_else(|| Error::NotFound("审核待办不存在".to_string()))?;
        validate_purchase_review_work_item(
            &work_item,
            id,
            &submission.base.id,
            expected_task_version,
            expected_subject_version,
            actor,
        )?;
        let decision_at = Instant::now();
        work_item.record_activity(actor.id(), decision_at)?;
        work_item.complete_by_domain_command(actor.id(), decision_at)?;

        let payable = self.build_payable(&order, &submission).await?;
        let cost_entries = self
            .build_confirmed_cost_entries(&submission, &submission_lines, revision_no)
            .await?;

        let actor_id = actor.id().to_string();
        let audit_actor = actor.clone();
        let audit_id_for_tx = audit_id.clone();
        let fingerprint_for_tx = fingerprint.clone();
        let db = self.db.clone();
        let client = db.client().clone();
        let order_for_tx = order.clone();
        let submission_for_tx = submission.clone();
        let submission_lines_for_tx = submission_lines.clone();
        let work_item_for_tx = work_item.clone();
        let revision_for_tx = revision.clone();
        let payable_for_tx = payable.clone();
        let payable_entry_id = payable.1.base.id.clone();
        let cost_entries_for_tx = cost_entries.clone();
        let review_comment_for_tx = decision.comment.clone();
        let result_work_item_id = work_item_id.to_string();
        let result_subject_version = expected_subject_version.to_string();
        let rbac_for_tx = rbac.clone();
        let transaction_result = client
            .with_transaction(move |session| {
                Box::pin(async move {
                    WorkItemService::new(db.clone(), rbac_for_tx)
                        .ensure_domain_decision_access(&audit_actor, &work_item_for_tx, session)
                        .await?;
                    ensure_purchase_reviewer_eligible(
                        &db,
                        &work_item_for_tx,
                        &submission_for_tx,
                        &actor_id,
                        session,
                    )
                    .await?;
                    ensure_purchase_review_sources(
                        &db,
                        &order_for_tx,
                        &submission_for_tx,
                        &submission_lines_for_tx,
                        session,
                    )
                    .await?;
                    db.purchase_order()
                        .create_effective_revision(&revision_for_tx, &revision_lines, session)
                        .await?;
                    let mut order_mut = order_for_tx.clone();
                    order_mut.apply_finance_review(true, &actor_id)?;
                    order_mut.stable.current_revision_id = Some(revision_for_tx.base.id.clone());
                    let mut submission_mut = submission_for_tx.clone();
                    submission_mut.record_review(
                        PurchaseOrderReviewDecision::Approved {
                            comment: review_comment_for_tx,
                        },
                        decision_at,
                        &actor_id,
                    )?;
                    db.purchase_order_submissions()
                        .update(&mut submission_mut, session)
                        .await?;
                    db.purchase_orders().update(&mut order_mut, session).await?;
                    db.payable()
                        .create_payable_with_entry(&payable_for_tx.0, &payable_for_tx.1, session)
                        .await?;
                    for entry in &cost_entries_for_tx {
                        db.cost()
                            .create_cost_entry_with_allocations(entry, Vec::new(), session)
                            .await?;
                    }
                    let mut completed_work_item = work_item_for_tx;
                    db.work_items().update(&mut completed_work_item, session).await?;
                    let receipt_message = purchase_review_receipt_message(
                        &fingerprint_for_tx,
                        PurchaseReviewReceipt::Approved {
                            lock_version: order_mut.base.version,
                            task_version: completed_work_item.base.version,
                            revision_id: revision_for_tx.base.id.clone(),
                            revision_no: revision_for_tx.revision.revision_no,
                            payable_entry_id: payable_entry_id.clone(),
                        },
                    );
                    let audit = audit_actor.resource_log_with_id(
                        audit_id_for_tx,
                        action,
                        "purchase_order",
                        order_mut.base.id.clone(),
                        Some(receipt_message),
                    )?;
                    db.audit_logs().create(&audit, session).await?;
                    Ok::<(u64, u64, String), crate::errors::Error>((
                        order_mut.base.version,
                        completed_work_item.base.version,
                        payable_entry_id,
                    ))
                })
            })
            .await;
        let (lock_version, task_version, payable_entry_id) = match transaction_result {
            Ok(result) => result,
            Err(error) => {
                if let Some(result) = self
                    .replay_purchase_review(
                        &audit_id,
                        &fingerprint,
                        id,
                        work_item_id,
                        expected_subject_version,
                    )
                    .await?
                {
                    return Ok(result);
                }
                return Err(error);
            }
        };

        Ok(PurchaseReviewResult {
            work_item_id: result_work_item_id,
            work_item_status: WorkItemStatus::Completed.as_str().to_string(),
            task_version: task_version.to_string(),
            subject_version: result_subject_version,
            review_result: "APPROVED".to_string(),
            revision_id: Some(revision.base.id.clone()),
            revision_no: Some(revision_no),
            payable_entry_id: Some(payable_entry_id),
            lock_version,
            reference: format!("REVIEW-V{lock_version}"),
        })
    }

    /// 财务审核驳回（采购返回可编辑草稿）。
    ///
    /// 单事务：提交记录驳回结论、采购回草稿、完成审核待办、审计。
    ///
    /// # 返回
    /// 返回审核结果（`REJECTED`）。
    ///
    /// # 错误
    /// * `NotFound` - 采购单/提交/待办不存在
    /// * `ConflictError` - 版本不一致或重复审核
    async fn review_reject(
        &self,
        review: PurchaseReviewContext<'_>,
        rbac: SharedRbacService,
    ) -> Result<PurchaseReviewResult> {
        let action = "purchase_order.review";
        let decision = review.decision;
        let reason_code = decision
            .reason_code
            .as_deref()
            .ok_or_else(|| Error::ValidationError("审核驳回分支缺少原因代码".to_string()))?;
        let decision_note = format!(
            "{}\u{1f}{}",
            reason_code,
            decision.comment.as_deref().unwrap_or_default()
        );
        let fingerprint = review.fingerprint(
            PurchaseOrderReviewDecisionResult::Rejected.as_str(),
            Some(&decision_note),
        );
        let PurchaseReviewContext {
            purchase_order_id: id,
            work_item_id,
            expected_task_version,
            expected_subject_version,
            decision: _,
            idempotency_key,
            actor,
        } = review;
        let audit_id = purchase_review_audit_id(actor.id(), action, work_item_id, idempotency_key);
        if let Some(result) = self
            .replay_purchase_review(
                &audit_id,
                &fingerprint,
                id,
                work_item_id,
                expected_subject_version,
            )
            .await?
        {
            return Ok(result);
        }
        let order = self
            .db
            .purchase_orders()
            .find_by_id(id, &mut NoTransaction)
            .await?
            .ok_or_else(|| Error::NotFound("采购单不存在".to_string()))?;
        self.ensure_version(&order, decision.expected_purchase_order_lock_version)?;
        if order.stable.status != PurchaseOrderStatus::PendingFinanceReview {
            return Err(Error::ConflictError(
                "采购单不在待财务审核状态，请勿重复审核".to_string(),
            ));
        }
        if order.current_submission_id.as_deref() != Some(&decision.submission_id) {
            return Err(Error::ConflictError("提交与采购单当前待审提交不一致".to_string()));
        }
        let submission = self
            .db
            .purchase_order_submissions()
            .find_by_id(&decision.submission_id, &mut NoTransaction)
            .await?
            .ok_or_else(|| Error::NotFound("待审核提交不存在".to_string()))?;
        if submission.status != SubmissionStatus::Pending {
            return Err(Error::ConflictError(
                "提交已审核或已失效，请勿重复审核".to_string(),
            ));
        }
        let mut work_item = self
            .db
            .work_items()
            .find_by_id(work_item_id, &mut NoTransaction)
            .await?
            .ok_or_else(|| Error::NotFound("审核待办不存在".to_string()))?;
        validate_purchase_review_work_item(
            &work_item,
            id,
            &submission.base.id,
            expected_task_version,
            expected_subject_version,
            actor,
        )?;
        let decision_at = Instant::now();
        work_item.record_activity(actor.id(), decision_at)?;
        work_item.complete_by_domain_command(actor.id(), decision_at)?;

        let actor_id = actor.id().to_string();
        let audit_actor = actor.clone();
        let audit_id_for_tx = audit_id.clone();
        let fingerprint_for_tx = fingerprint.clone();
        tracing::info!(
            purchase_order_id = %id,
            submission_id = %decision.submission_id,
            reason_code = %reason_code,
            "采购财务审核驳回已记录"
        );
        let db = self.db.clone();
        let client = db.client().clone();
        let order_for_tx = order.clone();
        let submission_for_tx = submission.clone();
        let work_item_for_tx = work_item.clone();
        let reject_reason_for_tx = reason_code.to_string();
        let review_comment_for_tx = decision.comment.clone();
        let result_work_item_id = work_item_id.to_string();
        let result_subject_version = expected_subject_version.to_string();
        let rbac_for_tx = rbac.clone();
        let transaction_result = client
            .with_transaction(move |session| {
                Box::pin(async move {
                    WorkItemService::new(db.clone(), rbac_for_tx)
                        .ensure_domain_decision_access(&audit_actor, &work_item_for_tx, session)
                        .await?;
                    ensure_purchase_reviewer_eligible(
                        &db,
                        &work_item_for_tx,
                        &submission_for_tx,
                        &actor_id,
                        session,
                    )
                    .await?;
                    let mut order_mut = order_for_tx.clone();
                    order_mut.apply_finance_review(false, &actor_id)?;
                    order_mut.current_submission_id = None;
                    let mut submission_mut = submission_for_tx.clone();
                    submission_mut.record_review(
                        PurchaseOrderReviewDecision::Rejected {
                            reason_code: reject_reason_for_tx,
                            comment: review_comment_for_tx,
                        },
                        decision_at,
                        &actor_id,
                    )?;
                    db.purchase_order_submissions()
                        .update(&mut submission_mut, session)
                        .await?;
                    db.purchase_orders().update(&mut order_mut, session).await?;
                    let mut completed_work_item = work_item_for_tx;
                    db.work_items().update(&mut completed_work_item, session).await?;
                    let receipt_message = purchase_review_receipt_message(
                        &fingerprint_for_tx,
                        PurchaseReviewReceipt::Rejected {
                            lock_version: order_mut.base.version,
                            task_version: completed_work_item.base.version,
                        },
                    );
                    let audit = audit_actor.resource_log_with_id(
                        audit_id_for_tx,
                        action,
                        "purchase_order",
                        order_mut.base.id.clone(),
                        Some(receipt_message),
                    )?;
                    db.audit_logs().create(&audit, session).await?;
                    Ok::<(u64, u64), crate::errors::Error>((
                        order_mut.base.version,
                        completed_work_item.base.version,
                    ))
                })
            })
            .await;
        let (lock_version, task_version) = match transaction_result {
            Ok(result) => result,
            Err(error) => {
                if let Some(result) = self
                    .replay_purchase_review(
                        &audit_id,
                        &fingerprint,
                        id,
                        work_item_id,
                        expected_subject_version,
                    )
                    .await?
                {
                    return Ok(result);
                }
                return Err(error);
            }
        };

        Ok(PurchaseReviewResult {
            work_item_id: result_work_item_id,
            work_item_status: WorkItemStatus::Completed.as_str().to_string(),
            task_version: task_version.to_string(),
            subject_version: result_subject_version,
            review_result: "REJECTED".to_string(),
            revision_id: None,
            revision_no: None,
            payable_entry_id: None,
            lock_version,
            reference: format!("REVIEW-V{lock_version}"),
        })
    }

    /// 重放已提交的采购审核强类型命令，并拒绝同一幂等键混用不同载荷。
    async fn replay_purchase_review(
        &self,
        audit_id: &str,
        expected_fingerprint: &str,
        purchase_order_id: &str,
        work_item_id: &str,
        subject_version: &str,
    ) -> Result<Option<PurchaseReviewResult>> {
        let Some(audit) = self
            .db
            .audit_logs()
            .find_by_id(audit_id, &mut NoTransaction)
            .await?
        else {
            return Ok(None);
        };
        if audit.resource_id.as_deref() != Some(purchase_order_id) {
            return Err(Error::Internal("采购审核幂等收据与业务对象不一致".to_string()));
        }
        let message = audit
            .message
            .as_deref()
            .ok_or_else(|| Error::Internal("采购审核幂等收据缺少结果".to_string()))?;
        let receipt = parse_purchase_review_receipt(message, expected_fingerprint)?;
        Ok(Some(receipt.into_result(work_item_id, subject_version)))
    }

    /// 构建应付子账与原始应付分录（D19；子账按采购单维度）。
    async fn build_payable(
        &self,
        order: &PurchaseOrder,
        submission: &PurchaseOrderSubmission,
    ) -> Result<(entities::payable::PayableAccount, entities::payable::PayableEntry)> {
        let account = entities::payable::PayableAccount::new(
            entities::ids::PayableAccountId::new(next_id()),
            entities::payable::PayableAccountData {
                source_document_id: order.base.id.clone(),
                supplier_id: order.supplier_id.clone(),
                source_type: entities::payable::PayableSourceType::PurchaseOrder,
                gross_total: submission.gross_amount,
                settled_total: zero_amount(),
                invoiceable_total: submission.gross_amount,
                invoiced_total: zero_amount(),
            },
            "system",
        )?;
        let entry = entities::payable::PayableEntry::new(
            PayableEntryId::new(next_id()),
            entities::payable::PayableEntryData {
                payable_account_id: account.base.id.clone().into(),
                entry_type: entities::payable::PayableEntryType::Original,
                direction: entities::payable::EntryDirection::Increase,
                amount: submission.gross_amount,
                due_date: entities::common::time::BusinessDate::today(),
                source_fact_type: "purchase_order".to_string(),
                source_document_id: order.base.id.clone(),
                source_revision_id: submission.base.id.clone(),
                source_sequence: 1,
                posted_at: Instant::now(),
            },
        )?;
        Ok((account, entry))
    }

    /// 构建 `CONFIRMED` 成本事实（D20；逐采购行一个成本事实）。
    async fn build_confirmed_cost_entries(
        &self,
        submission: &PurchaseOrderSubmission,
        lines: &[PurchaseOrderSubmissionLine],
        revision_no: u32,
    ) -> Result<Vec<entities::cost::CostEntry>> {
        let mut entries = Vec::new();
        for line in lines {
            let tax_rate = line.input_tax_rate.unwrap_or_else(zero_rate);
            entries.push(entities::cost::CostEntry::new(
                CostEntryId::new(next_id()),
                entities::cost::CostEntryData {
                    cost_type: if line.line_type == PurchaseLineType::LogisticsFee {
                        entities::cost::CostType::Logistics
                    } else {
                        entities::cost::CostType::Product
                    },
                    cost_stage: entities::cost::CostStage::Confirmed,
                    cost_scope: entities::cost::CostScope::NonVoucherFulfillment,
                    cost_basis: None,
                    supplier_id: Some(submission.supplier_id.clone()),
                    gross_amount: line.gross_amount,
                    net_amount: line.net_amount,
                    tax_amount: line.tax_amount,
                    tax_inclusion: true,
                    input_tax_rate: tax_rate,
                    occurred_at: Instant::now(),
                    source_fact_type: "purchase_order".to_string(),
                    source_document_id: submission.purchase_order_id.to_string(),
                    source_line_id: line.base.id.clone(),
                    source_version: revision_no.to_string(),
                    adjusts_cost_entry_id: None,
                    evidence_attachment_id: None,
                },
            )?);
        }
        Ok(entries)
    }
}

/// 在同一事务内写入生效版本、采购单、提交结论、应付与成本。
///
/// # 错误
/// 状态不允许、来源复验失败或仓储失败时返回错误。
#[allow(clippy::too_many_arguments)]
async fn persist_formalized_order(
    db: &mongodb::Database,
    order: PurchaseOrder,
    submission: PurchaseOrderSubmission,
    submission_lines: Vec<PurchaseOrderSubmissionLine>,
    revision: entities::purchase_order::PurchaseOrderRevision,
    revision_lines: Vec<entities::purchase_order::PurchaseOrderRevisionLine>,
    payable: (entities::payable::PayableAccount, entities::payable::PayableEntry),
    cost_entries: Vec<entities::cost::CostEntry>,
    actor: &AuditActor,
) -> Result<()> {
    let actor_id = actor.id().to_string();
    let audit = actor.clone().resource_log(
        "purchase_order.formalize",
        "purchase_order",
        order.base.id.clone(),
    )?;
    let db = db.clone();
    let client = db.client().clone();
    client
        .with_transaction(move |session| {
            Box::pin(async move {
                ensure_purchase_review_sources(&db, &order, &submission, &submission_lines, session).await?;
                db.purchase_order()
                    .create_effective_revision(&revision, &revision_lines, session)
                    .await?;
                let mut order_mut = order;
                order_mut.formalize_approved(&actor_id)?;
                order_mut.stable.current_revision_id = Some(revision.base.id.clone());
                let mut submission_mut = submission;
                submission_mut.record_review(
                    PurchaseOrderReviewDecision::Approved { comment: None },
                    Instant::now(),
                    &actor_id,
                )?;
                db.purchase_order_submissions()
                    .update(&mut submission_mut, session)
                    .await?;
                db.purchase_orders().update(&mut order_mut, session).await?;
                db.payable()
                    .create_payable_with_entry(&payable.0, &payable.1, session)
                    .await?;
                for entry in &cost_entries {
                    db.cost()
                        .create_cost_entry_with_allocations(entry, Vec::new(), session)
                        .await?;
                }
                db.audit_logs().create(&audit, session).await?;
                Ok::<(), crate::errors::Error>(())
            })
        })
        .await
}

/// 审核命令写入审计唯一键的最小结果收据。
#[derive(Debug, Clone, PartialEq, Eq)]
enum PurchaseReviewReceipt {
    Approved {
        lock_version: u64,
        task_version: u64,
        revision_id: String,
        revision_no: u32,
        payable_entry_id: String,
    },
    Rejected {
        lock_version: u64,
        task_version: u64,
    },
}

impl PurchaseReviewReceipt {
    /// 把持久化收据恢复为公开命令结果。
    fn into_result(self, work_item_id: &str, subject_version: &str) -> PurchaseReviewResult {
        let (review_result, lock_version, task_version, revision_id, revision_no, payable_entry_id) =
            match self {
                Self::Approved {
                    lock_version,
                    task_version,
                    revision_id,
                    revision_no,
                    payable_entry_id,
                } => (
                    "APPROVED",
                    lock_version,
                    task_version,
                    Some(revision_id),
                    Some(revision_no),
                    Some(payable_entry_id),
                ),
                Self::Rejected {
                    lock_version,
                    task_version,
                } => ("REJECTED", lock_version, task_version, None, None, None),
            };
        PurchaseReviewResult {
            work_item_id: work_item_id.to_string(),
            work_item_status: WorkItemStatus::Completed.as_str().to_string(),
            task_version: task_version.to_string(),
            subject_version: subject_version.to_string(),
            review_result: review_result.to_string(),
            revision_id,
            revision_no,
            payable_entry_id,
            lock_version,
            reference: format!("REVIEW-V{lock_version}"),
        }
    }
}

/// 生成不暴露原始幂等键的稳定审计主键。
fn purchase_review_audit_id(actor_id: &str, action: &str, work_item_id: &str, key: &str) -> String {
    format!(
        "{PURCHASE_REVIEW_RECEIPT_PREFIX}{}",
        stable_digest(&format!("{actor_id}|{action}|{work_item_id}|{key}"))
    )
}

/// 将审核结果编码为受长度约束的审计消息。
fn purchase_review_receipt_message(fingerprint: &str, receipt: PurchaseReviewReceipt) -> String {
    let result = match receipt {
        PurchaseReviewReceipt::Approved {
            lock_version,
            task_version,
            revision_id,
            revision_no,
            payable_entry_id,
        } => format!("A|{lock_version}|{task_version}|{revision_id}|{revision_no}|{payable_entry_id}"),
        PurchaseReviewReceipt::Rejected {
            lock_version,
            task_version,
        } => format!("R|{lock_version}|{task_version}"),
    };
    format!("{COMMAND_FINGERPRINT_PREFIX}{fingerprint};result={result}")
}

/// 解析并校验采购审核命令收据。
fn parse_purchase_review_receipt(message: &str, expected_fingerprint: &str) -> Result<PurchaseReviewReceipt> {
    let (fingerprint, result) = message
        .strip_prefix(COMMAND_FINGERPRINT_PREFIX)
        .and_then(|value| value.split_once(";result="))
        .ok_or_else(|| Error::Internal("采购审核幂等收据格式非法".to_string()))?;
    if fingerprint != expected_fingerprint {
        return Err(Error::ConflictError("幂等键已用于不同的采购审核命令".to_string()));
    }
    let fields = result.split('|').collect::<Vec<_>>();
    match fields.as_slice() {
        ["A", lock_version, task_version, revision_id, revision_no, payable_entry_id] => {
            Ok(PurchaseReviewReceipt::Approved {
                lock_version: parse_receipt_number(lock_version, "采购单版本")?,
                task_version: parse_receipt_number(task_version, "待办版本")?,
                revision_id: (*revision_id).to_string(),
                revision_no: parse_receipt_number(revision_no, "采购版本号")?,
                payable_entry_id: (*payable_entry_id).to_string(),
            })
        }
        ["R", lock_version, task_version] => Ok(PurchaseReviewReceipt::Rejected {
            lock_version: parse_receipt_number(lock_version, "采购单版本")?,
            task_version: parse_receipt_number(task_version, "待办版本")?,
        }),
        _ => Err(Error::Internal("采购审核幂等收据结果非法".to_string())),
    }
}

/// 解析收据中的整数版本字段。
fn parse_receipt_number<T>(value: &str, field: &str) -> Result<T>
where
    T: std::str::FromStr,
{
    value
        .parse()
        .map_err(|_| Error::Internal(format!("采购审核幂等收据{field}非法")))
}

/// 对各字段分别加长度前缀后计算命令摘要，避免拼接歧义。
fn command_fingerprint(parts: &[&str]) -> String {
    let mut digest = Sha256::new();
    for part in parts {
        digest.update((part.len() as u64).to_be_bytes());
        digest.update(part.as_bytes());
    }
    format!("{:x}", digest.finalize())
}

/// 计算稳定 SHA-256 十六进制摘要。
fn stable_digest(value: &str) -> String {
    format!("{:x}", Sha256::digest(value.as_bytes()))
}

/// 校验采购审核强类型命令锁定的待办、提交与个人责任。
fn validate_purchase_review_work_item(
    item: &WorkItem,
    purchase_order_id: &str,
    submission_id: &str,
    expected_task_version: u64,
    expected_subject_version: &str,
    actor: &AuditActor,
) -> Result<()> {
    if item.base.version != expected_task_version {
        return Err(Error::ConflictError(
            "待办责任或版本已变化，请刷新后重试".to_string(),
        ));
    }
    if expected_subject_version != submission_id || item.subject_version != submission_id {
        return Err(Error::ConflictError(
            "采购提交版本已变化，请刷新后重试".to_string(),
        ));
    }
    if item.approval_step_instance_id.is_some()
        || item.work_item_type != WorkItemType::PurchaseOrderReview
        || item.business_object_type != "purchase_order"
        || item.business_object_id != purchase_order_id
    {
        return Err(Error::BusinessLogicError("待办与当前采购审核不匹配".to_string()));
    }
    if !item.is_owned_by(actor.id()) {
        return Err(Error::Forbidden(
            "当前账号不是该待办责任人，或处理权已变化".to_string(),
        ));
    }
    Ok(())
}

/// 在审核通过事务内重验冻结金额、销售分配与采购确认的精确供给来源。
async fn ensure_purchase_review_sources(
    db: &mongodb::Database,
    order: &PurchaseOrder,
    submission: &PurchaseOrderSubmission,
    lines: &[PurchaseOrderSubmissionLine],
    executor: &mut dyn Executor,
) -> Result<()> {
    let mut gross = zero_amount();
    let mut net = zero_amount();
    let mut tax = zero_amount();
    for line in lines {
        gross = gross.checked_add(line.gross_amount);
        net = net.checked_add(line.net_amount);
        tax = tax.checked_add(line.tax_amount);
        if line.line_type != PurchaseLineType::ItemService {
            continue;
        }
        let confirmation_line_id = line
            .procurement_confirmation_line_id
            .as_ref()
            .ok_or_else(|| Error::BusinessLogicError("采购明细缺少采购确认分行引用".to_string()))?;
        let confirmation_line = db
            .procurement_confirmation_lines()
            .find_by_id(confirmation_line_id, executor)
            .await?
            .ok_or_else(|| Error::BusinessLogicError("采购明细引用的采购确认分行不存在".to_string()))?;
        let confirmation = db
            .procurement_confirmations()
            .find_by_id(&confirmation_line.procurement_confirmation_id, executor)
            .await?
            .ok_or_else(|| Error::BusinessLogicError("采购确认不存在".to_string()))?;
        if confirmation.stable.status != entities::sales_review::ProcurementConfirmationStatus::Approved
            || confirmation.sales_order_id != order.sales_order_id
            || confirmation_line.supplier_id != submission.supplier_id
            || line.sales_order_submission_line_id.as_ref()
                != Some(&confirmation_line.sales_order_submission_line_id)
            || line.unit_cost_gross != Some(confirmation_line.latest_cost_gross)
            || line.input_tax_rate != Some(confirmation_line.input_tax_rate)
            || line.expected_delivery_date != Some(confirmation_line.expected_delivery_date)
            || line.allocated_quantity.is_none_or(|quantity| {
                quantity.to_decimal() > confirmation_line.confirmed_quantity.to_decimal()
            })
        {
            return Err(Error::BusinessLogicError(
                "采购明细与已通过采购确认的冻结来源不一致".to_string(),
            ));
        }
        let offering_revision_id = confirmation_line
            .supplier_offering_revision_id
            .as_ref()
            .ok_or_else(|| Error::BusinessLogicError("采购确认分行缺少冻结供给版本".to_string()))?;
        let offering_revision = db
            .supplier_offering_revisions()
            .find_by_id(offering_revision_id, executor)
            .await?
            .ok_or_else(|| Error::BusinessLogicError("采购确认分行引用的供给版本不存在".to_string()))?;
        let offering = db
            .supplier_offerings()
            .find_by_id(&offering_revision.supplier_offering_id, executor)
            .await?
            .ok_or_else(|| Error::BusinessLogicError("采购确认分行引用的供给不存在".to_string()))?;
        if offering.supplier_id != submission.supplier_id || line.sku_id.as_ref() != Some(&offering.sku_id) {
            return Err(Error::BusinessLogicError(
                "采购确认冻结的供给版本与采购供应商或SKU不一致".to_string(),
            ));
        }
        let sales_line = db
            .sales_order_submission_lines()
            .find_by_id(&confirmation_line.sales_order_submission_line_id, executor)
            .await?
            .ok_or_else(|| Error::BusinessLogicError("采购明细引用的销售提交行不存在".to_string()))?;
        if sales_line.submission_id != confirmation.submission_id {
            return Err(Error::BusinessLogicError(
                "采购确认分行与销售提交版本不一致".to_string(),
            ));
        }
    }
    if gross != submission.gross_amount || net != submission.net_amount || tax != submission.tax_amount {
        return Err(Error::BusinessLogicError(
            "采购提交表头金额与冻结明细汇总不一致".to_string(),
        ));
    }
    Ok(())
}

/// 在采购审核事实持久化前重验角色、数据范围与岗位分离。
async fn ensure_purchase_reviewer_eligible(
    db: &mongodb::Database,
    item: &WorkItem,
    submission: &PurchaseOrderSubmission,
    actor_id: &str,
    executor: &mut dyn Executor,
) -> Result<()> {
    let resolver = crate::approval::ApprovalAssigneeResolver::new(db.clone());
    if !resolver
        .user_is_eligible_for_assignment(actor_id, &item.owner_role, &item.owner_organization_id, executor)
        .await?
    {
        return Err(Error::Forbidden(
            "当前账号已不具备该待办的角色或数据范围".to_string(),
        ));
    }
    if submission.submitted_by.as_deref() == Some(actor_id) {
        return Err(Error::Forbidden("采购提交人不得审核自己的提交".to_string()));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use entities::common::time::Instant;
    use entities::ids::WorkItemId;
    use entities::work_item::{
        AssignmentMode, AssignmentSource, WorkItem, WorkItemData, WorkItemPriority, WorkItemType,
    };

    use super::{
        parse_purchase_review_receipt, purchase_review_audit_id, purchase_review_receipt_message,
        validate_purchase_review_work_item, PurchaseReviewReceipt,
    };
    use crate::audit::AuditActor;

    fn actor(id: &str) -> AuditActor {
        AuditActor::new(
            id.to_string(),
            format!("{id}@example.test"),
            entities::AccountKind::Admin,
        )
    }

    fn owned_review_task() -> WorkItem {
        let mut task = WorkItem::new_at(
            WorkItemId::new("wi-1"),
            WorkItemData {
                work_item_type: WorkItemType::PurchaseOrderReview,
                approval_step_instance_id: None,
                business_object_type: "purchase_order".to_string(),
                business_object_id: "po-1".to_string(),
                subject_version: "submission-1".to_string(),
                assignment_mode: AssignmentMode::Pool,
                owner_role: "role-finance".to_string(),
                owner_organization_id: "company".to_string(),
                owner_user_id: None,
                assignment_source: AssignmentSource::SystemRule,
                priority: WorkItemPriority::Normal,
                due_at: None,
                reason_code: None,
                impact_summary: None,
            },
            Instant::from_unix_secs(1),
        )
        .unwrap();
        task.reassign("reviewer-1", Instant::from_unix_secs(2)).unwrap();
        task.base.version = 2;
        task
    }

    #[test]
    fn strong_review_task_requires_task_and_subject_versions() {
        let task = owned_review_task();
        let reviewer = actor("reviewer-1");

        assert!(validate_purchase_review_work_item(
            &task,
            "po-1",
            "submission-1",
            2,
            "submission-1",
            &reviewer,
        )
        .is_ok());
        assert!(validate_purchase_review_work_item(
            &task,
            "po-1",
            "submission-1",
            1,
            "submission-1",
            &reviewer,
        )
        .is_err());
        assert!(validate_purchase_review_work_item(
            &task,
            "po-1",
            "submission-1",
            2,
            "submission-old",
            &reviewer,
        )
        .is_err());
    }

    #[test]
    fn strong_review_task_requires_current_owner() {
        let task = owned_review_task();

        assert!(validate_purchase_review_work_item(
            &task,
            "po-1",
            "submission-1",
            2,
            "submission-1",
            &actor("reviewer-2"),
        )
        .is_err());
    }

    #[test]
    fn review_receipt_round_trips_and_never_embeds_raw_idempotency_key() {
        let fingerprint = "a".repeat(64);
        let receipt = PurchaseReviewReceipt::Approved {
            lock_version: 4,
            task_version: 3,
            revision_id: "revision-1".to_string(),
            revision_no: 2,
            payable_entry_id: "payable-1".to_string(),
        };
        let message = purchase_review_receipt_message(&fingerprint, receipt.clone());

        assert_eq!(
            parse_purchase_review_receipt(&message, &fingerprint).unwrap(),
            receipt
        );
        let audit_id = purchase_review_audit_id("actor-1", "purchase_order.review", "wi-1", "raw-secret-key");
        assert!(!audit_id.contains("raw-secret-key"));
        assert!(message.len() <= 256);
    }
}
