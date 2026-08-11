//! 采购财务审核、应付与成本事实编排。

use database::{
    AccessControlExt, CostExt, NoTransaction, PayableExt, PurchaseOrderExt, Transactional, WorkItemExt,
};
use entities::common::time::Instant;
use entities::ids::{CostEntryId, PayableEntryId};
use entities::purchase_order::{
    PurchaseLineType, PurchaseOrder, PurchaseOrderStatus, PurchaseOrderSubmission,
    PurchaseOrderSubmissionLine, SubmissionStatus,
};
use id_generator::next_id;
use validator::Validate;

use super::dto::{ApprovePurchaseOrderRequest, PurchaseReviewResult, RejectPurchaseOrderRequest};
use super::shared::{zero_amount, zero_rate};
use super::PurchaseOrderService;
use crate::audit::AuditActor;
use crate::errors::{Error, Result};

impl PurchaseOrderService {
    /// 财务审核通过（§8.1.4 事务不变量）。
    ///
    /// 单事务：锁定提交 → 逐行复验采购确认来源 → 复制为生效版本与版本行 →
    /// 形成销售分配 → 推进采购状态与版本指针 → 应付原始分录与 `CONFIRMED`
    /// 成本事实 → 完成审核待办 → 审计。任一失败整体回滚。
    ///
    /// # 参数
    /// * `id` - 采购单 ID
    /// * `req` - 审核通过请求
    /// * `actor` - 已通过鉴权的审计操作人
    ///
    /// # 返回
    /// 返回审核结果（版本与应付分录）。
    ///
    /// # 错误
    /// * `NotFound` - 采购单/提交/待办不存在
    /// * `ConflictError` - 版本不一致或重复审核
    /// * `BusinessLogicError` - 状态机或来源校验失败
    pub async fn review_approve(
        &self,
        id: &str,
        req: ApprovePurchaseOrderRequest,
        actor: &AuditActor,
    ) -> Result<PurchaseReviewResult> {
        req.validate()?;
        let order = self
            .db
            .purchase_orders()
            .find_by_id(id, &mut NoTransaction)
            .await?
            .ok_or_else(|| Error::NotFound("采购单不存在".to_string()))?;
        self.ensure_version(&order, req.expected_lock_version)?;
        if order.stable.status != PurchaseOrderStatus::PendingFinanceReview {
            return Err(Error::ConflictError(
                "采购单不在待财务审核状态，请勿重复审核".to_string(),
            ));
        }
        if order.current_submission_id.as_deref() != Some(&req.submission_id) {
            return Err(Error::ConflictError("提交与采购单当前待审提交不一致".to_string()));
        }
        let submission = self
            .db
            .purchase_order_submissions()
            .find_by_id(&req.submission_id, &mut NoTransaction)
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
                mongodb::bson::doc! { "purchase_order_submission_id": &req.submission_id },
                &mut NoTransaction,
            )
            .await?;

        // 生效版本号：当前版本 + 1。
        let revision_no = self.next_revision_no(&order).await?;
        let (revision, revision_lines) = self
            .build_effective_revision(&order, &submission, &submission_lines, revision_no, &req, actor)
            .await?;

        let mut work_item = self
            .db
            .work_items()
            .find_by_id(&req.work_item_id, &mut NoTransaction)
            .await?
            .ok_or_else(|| Error::NotFound("审核待办不存在".to_string()))?;
        if work_item.status != entities::work_item::WorkItemStatus::InProgress
            && work_item.status != entities::work_item::WorkItemStatus::Unclaimed
        {
            return Err(Error::ConflictError("审核待办已终结，请勿重复审核".to_string()));
        }
        work_item.claim(actor.id())?;
        work_item.complete(actor.id(), Instant::now())?;

        let payable = self.build_payable(&order, &submission).await?;
        let cost_entries = self
            .build_confirmed_cost_entries(&submission, &submission_lines, revision_no)
            .await?;

        let audit =
            actor
                .clone()
                .resource_log("purchase_order.approve", "purchase_order", order.base.id.clone())?;
        let actor_id = actor.id().to_string();
        let db = self.db.clone();
        let client = db.client().clone();
        let order_for_tx = order.clone();
        let submission_for_tx = submission.clone();
        let work_item_for_tx = work_item.clone();
        let revision_for_tx = revision.clone();
        let payable_for_tx = payable.clone();
        let payable_entry_id = payable.1.base.id.clone();
        let cost_entries_for_tx = cost_entries.clone();
        client
            .with_transaction(move |session| {
                Box::pin(async move {
                    db.purchase_order()
                        .create_effective_revision(&revision_for_tx, &revision_lines, session)
                        .await?;
                    let mut order_mut = order_for_tx.clone();
                    order_mut.apply_finance_review(true, &actor_id)?;
                    order_mut.stable.current_revision_id = Some(revision_for_tx.base.id.clone());
                    let mut submission_mut = submission_for_tx.clone();
                    submission_mut.mark_reviewed(true)?;
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
                    db.work_items()
                        .update(&mut work_item_for_tx.clone(), session)
                        .await?;
                    db.audit_logs().create(&audit, session).await?;
                    Ok::<(), crate::errors::Error>(())
                })
            })
            .await?;

        Ok(PurchaseReviewResult {
            review_result: "APPROVED".to_string(),
            revision_id: Some(revision.base.id.clone()),
            revision_no: Some(revision_no),
            payable_entry_id: Some(payable_entry_id),
            lock_version: order.base.version,
            reference: format!("REVIEW-V{}", order.base.version),
        })
    }

    /// 财务审核驳回（采购返回可编辑草稿）。
    ///
    /// 单事务：提交记录驳回结论、采购回草稿、完成审核待办、审计。
    ///
    /// # 参数
    /// * `id` - 采购单 ID
    /// * `req` - 驳回请求（结构化原因代码必填）
    /// * `actor` - 已通过鉴权的审计操作人
    ///
    /// # 返回
    /// 返回审核结果（`REJECTED`）。
    ///
    /// # 错误
    /// * `NotFound` - 采购单/提交/待办不存在
    /// * `ConflictError` - 版本不一致或重复审核
    pub async fn review_reject(
        &self,
        id: &str,
        req: RejectPurchaseOrderRequest,
        actor: &AuditActor,
    ) -> Result<PurchaseReviewResult> {
        req.validate()?;
        let order = self
            .db
            .purchase_orders()
            .find_by_id(id, &mut NoTransaction)
            .await?
            .ok_or_else(|| Error::NotFound("采购单不存在".to_string()))?;
        self.ensure_version(&order, req.expected_lock_version)?;
        if order.stable.status != PurchaseOrderStatus::PendingFinanceReview {
            return Err(Error::ConflictError(
                "采购单不在待财务审核状态，请勿重复审核".to_string(),
            ));
        }
        let submission = self
            .db
            .purchase_order_submissions()
            .find_by_id(&req.submission_id, &mut NoTransaction)
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
            .find_by_id(&req.work_item_id, &mut NoTransaction)
            .await?
            .ok_or_else(|| Error::NotFound("审核待办不存在".to_string()))?;
        if work_item.status != entities::work_item::WorkItemStatus::InProgress
            && work_item.status != entities::work_item::WorkItemStatus::Unclaimed
        {
            return Err(Error::ConflictError("审核待办已终结，请勿重复审核".to_string()));
        }
        work_item.claim(actor.id())?;
        work_item.complete(actor.id(), Instant::now())?;

        let audit =
            actor
                .clone()
                .resource_log("purchase_order.reject", "purchase_order", order.base.id.clone())?;
        let actor_id = actor.id().to_string();
        tracing::info!(
            purchase_order_id = %id,
            submission_id = %req.submission_id,
            reason_code = %req.reason_code,
            comment = ?req.comment,
            "采购财务审核驳回已记录"
        );
        let db = self.db.clone();
        let client = db.client().clone();
        let order_for_tx = order.clone();
        let submission_for_tx = submission.clone();
        let work_item_for_tx = work_item.clone();
        client
            .with_transaction(move |session| {
                Box::pin(async move {
                    let mut order_mut = order_for_tx.clone();
                    order_mut.apply_finance_review(false, &actor_id)?;
                    order_mut.current_submission_id = None;
                    let mut submission_mut = submission_for_tx.clone();
                    submission_mut.mark_reviewed(false)?;
                    db.purchase_order_submissions()
                        .update(&mut submission_mut, session)
                        .await?;
                    db.purchase_orders().update(&mut order_mut, session).await?;
                    db.work_items()
                        .update(&mut work_item_for_tx.clone(), session)
                        .await?;
                    db.audit_logs().create(&audit, session).await?;
                    Ok::<(), crate::errors::Error>(())
                })
            })
            .await?;

        Ok(PurchaseReviewResult {
            review_result: "REJECTED".to_string(),
            revision_id: None,
            revision_no: None,
            payable_entry_id: None,
            lock_version: order.base.version,
            reference: format!("REVIEW-V{}", order.base.version),
        })
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
