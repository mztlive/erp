use super::dto::{CreateSupplierRefundRequest, PostSupplierRefundRequest, SupplierRefundView};
use super::reversal_plan::{plan_payment_reverse, zero_amount};
use super::ReturnsService;
use crate::audit::AuditActor;
use crate::document_registry::{new_registered_document, persist_registered_document};
use crate::errors::{Error, Result};
use database::{AccessControlExt, NoTransaction, PayableExt, ReturnsExt, Transactional};
use entities::document_registry::DocumentType;
use entities::ids::{PayableEntryId, PayableEntryOffsetId, PaymentAllocationId, SupplierRefundId};
use entities::money::Amount;
use entities::payable::{
    AllocationAction as PayableAllocationAction, EntryDirection as PayableEntryDirection, PayableEntry,
    PayableEntryData, PayableEntryOffset, PayableEntryOffsetData, PayableEntryType, PaymentAllocation,
    PaymentAllocationData, SupplierPaymentStatus,
};
use entities::returns::{SupplierRefund, SupplierRefundData, SupplierRefundStatus};
use id_generator::next_id;
use validator::Validate;

impl ReturnsService {
    // -----------------------------------------------------------------------
    // 供应商退款
    // -----------------------------------------------------------------------

    /// 查询供应商退款详情。
    ///
    /// # 参数
    /// * `id` - 退款单 ID
    ///
    /// # 返回
    /// 返回退款单视图。
    ///
    /// # 错误
    /// * `NotFound` - 退款单不存在
    pub async fn supplier_refund_detail(&self, id: &str) -> Result<SupplierRefundView> {
        self.supplier_refund_view(id.to_string()).await
    }

    /// 登记供应商退款草稿（单集合写入，无事务）。
    ///
    /// 退款单号全局唯一（唯一索引）构成幂等去重；经办人与复核人必须不同。
    ///
    /// # 参数
    /// * `req` - 创建请求
    /// * `actor` - 已通过鉴权的审计操作人
    ///
    /// # 返回
    /// 返回新建退款单视图。
    ///
    /// # 错误
    /// * `ConflictError` - 退款单号重复
    pub async fn create_supplier_refund(
        &self,
        req: CreateSupplierRefundRequest,
        actor: &AuditActor,
    ) -> Result<SupplierRefundView> {
        req.validate()?;
        let refund = SupplierRefund::new(
            SupplierRefundId::new(next_id()),
            SupplierRefundData {
                refund_no: req.refund_no,
                purchase_return_order_id: req.purchase_return_order_id,
                supplier_id: req.supplier_id,
                original_payment_id: req.original_payment_id,
                original_payable_entry_id: req.original_payable_entry_id,
                reason_code: req.reason_code,
                reason_text: req.reason_text,
                amount: req.amount,
                handled_by: req.handled_by,
                reviewed_by: req.reviewed_by,
                occurred_at: req.occurred_at,
                evidence_attachment_id: None,
            },
        )?;
        let audit = actor.clone().resource_log(
            "supplier_refund.create",
            "supplier_refund",
            refund.base.id.clone(),
        )?;
        let document = new_registered_document(
            &refund.base.id,
            DocumentType::SupplierRefund,
            refund.refund_no.clone(),
        )?;
        let db = self.db.clone();
        let client = db.client().clone();
        let refund_for_tx = refund.clone();
        client
            .with_transaction(move |session| {
                Box::pin(async move {
                    db.supplier_refunds().create(&refund_for_tx, session).await?;
                    persist_registered_document(&db, &document, session).await?;
                    db.audit_logs().create(&audit, session).await?;
                    Ok::<(), crate::errors::Error>(())
                })
            })
            .await?;

        self.supplier_refund_detail(&refund.base.id).await
    }

    /// 供应商退款过账（§8.3-3 事务不变量，应付侧镜像）。
    ///
    /// 同一事务内：退款单必须为草稿；按原付款（或其核销分配）反向写入
    /// `REVERSE` 付款核销分配；按条件原子冲减应付子账已核销进度；写反向应付
    /// 分录（减少）与分录抵销；退款单迁移为已过账。任一校验失败整体回滚。
    ///
    /// # 参数
    /// * `id` - 退款单 ID
    /// * `req` - 过账请求（占位）
    /// * `actor` - 已通过鉴权的审计操作人
    ///
    /// # 返回
    /// 返回过账后退款单视图。
    ///
    /// # 错误
    /// * `NotFound` - 退款单或原付款不存在
    /// * `BusinessLogicError` - 累计退款超原付款、重复过账或超额冲减
    pub async fn post_supplier_refund(
        &self,
        id: &str,
        req: PostSupplierRefundRequest,
        actor: &AuditActor,
    ) -> Result<SupplierRefundView> {
        req.validate()?;
        let db = self.db.clone();
        let client = db.client().clone();
        let actor_owned = actor.clone();
        let actor_id = actor.id().to_string();
        let refund_id = id.to_string();
        let detail_id = refund_id.clone();
        client
            .with_transaction(move |session| {
                Box::pin(async move {
                    let mut refund = db
                        .supplier_refunds()
                        .find_by_id(&refund_id, session)
                        .await?
                        .ok_or_else(|| Error::NotFound("供应商退款单不存在".to_string()))?;
                    if refund.status != SupplierRefundStatus::Draft {
                        return Err(Error::BusinessLogicError(
                            "退款单已过账，请勿重复提交".to_string(),
                        ));
                    }
                    let original_payment_id = refund.original_payment_id.clone().ok_or_else(|| {
                        Error::BusinessLogicError("按原应付分录退款由冲减分录完成".to_string())
                    })?;
                    let payment = db
                        .supplier_payments()
                        .find_by_id(&original_payment_id, session)
                        .await?
                        .ok_or_else(|| Error::NotFound("原付款不存在".to_string()))?;
                    if payment.status != SupplierPaymentStatus::Posted {
                        return Err(Error::BusinessLogicError("只有已过账付款可以退款".to_string()));
                    }
                    let refunded_before: Amount = db
                        .supplier_refunds()
                        .find_refunds_by_originals(std::slice::from_ref(&original_payment_id), &[], session)
                        .await?
                        .iter()
                        .filter(|other| other.base.id != refund.base.id)
                        .fold(zero_amount(), |sum, other| sum.checked_add(other.amount));
                    if refunded_before.checked_add(refund.amount) > payment.amount {
                        return Err(Error::BusinessLogicError(
                            "累计退款金额不得超过原付款金额".to_string(),
                        ));
                    }

                    let allocations = db
                        .payment_allocations()
                        .find_allocations_by_payments(&[original_payment_id], session)
                        .await?;
                    let (reverse_rows, chunks) = plan_payment_reverse(&allocations, refund.amount)?;
                    let next_seq = allocations
                        .iter()
                        .map(|allocation| allocation.allocation_seq)
                        .max()
                        .unwrap_or(0)
                        + 1;

                    let mut decrease_entry: Option<PayableEntry> = None;
                    for (offset_index, chunk) in chunks.iter().enumerate() {
                        let entry = db
                            .payable_entries()
                            .find_by_id(&chunk.increase_entry_id, session)
                            .await?
                            .ok_or_else(|| Error::NotFound("应付分录不存在".to_string()))?;
                        let account = db
                            .payable_accounts()
                            .find_by_id(&entry.payable_account_id, session)
                            .await?
                            .ok_or_else(|| Error::NotFound("应付往来子账不存在".to_string()))?;
                        if account.supplier_id != payment.supplier_id {
                            return Err(Error::BusinessLogicError("禁止跨供应商退款".to_string()));
                        }
                        let reverted = db
                            .payable_accounts()
                            .revert_settlement(&entry.payable_account_id, &chunk.amount, &actor_id, session)
                            .await?;
                        if !reverted {
                            return Err(Error::BusinessLogicError("退款冲减超过已核销金额".to_string()));
                        }
                        if decrease_entry.is_none() {
                            decrease_entry = Some(PayableEntry::new(
                                PayableEntryId::new(next_id()),
                                PayableEntryData {
                                    payable_account_id: entry.payable_account_id.clone(),
                                    entry_type: PayableEntryType::SupplierRefund,
                                    direction: PayableEntryDirection::Decrease,
                                    amount: refund.amount,
                                    due_date: entities::common::time::BusinessDate::today(),
                                    source_fact_type: "supplier_refund".to_string(),
                                    source_document_id: refund.base.id.clone(),
                                    source_revision_id: refund.base.id.clone(),
                                    source_sequence: 1,
                                    posted_at: refund.occurred_at,
                                },
                            )?);
                        }
                        db.payable_entry_offsets()
                            .create(
                                &PayableEntryOffset::new(
                                    PayableEntryOffsetId::new(next_id()),
                                    PayableEntryOffsetData {
                                        decrease_entry_id: decrease_entry
                                            .as_ref()
                                            .map(|entry| entry.base.id.clone().into())
                                            .expect("减少分录已创建"),
                                        increase_entry_id: chunk.increase_entry_id.clone(),
                                        offset_sequence: offset_index as u32 + 1,
                                        offset_amount: chunk.amount,
                                    },
                                )?,
                                session,
                            )
                            .await?;
                    }
                    if let Some(entry) = decrease_entry {
                        db.payable_entries().create(&entry, session).await?;
                    }
                    for (reverse_index, reverse) in reverse_rows.iter().enumerate() {
                        let allocation = PaymentAllocation::new(
                            PaymentAllocationId::new(next_id()),
                            PaymentAllocationData {
                                supplier_payment_id: payment.base.id.clone().into(),
                                payable_entry_id: reverse.entry_id.clone(),
                                allocation_seq: next_seq + reverse_index as u32,
                                allocation_action: PayableAllocationAction::Reverse,
                                allocated_amount: reverse.amount,
                                allocated_at: refund.occurred_at,
                                reverses_allocation_id: Some(reverse.original_id.clone()),
                            },
                        )?;
                        db.payment_allocations().create(&allocation, session).await?;
                    }
                    refund.transition(SupplierRefundStatus::Posted)?;
                    db.supplier_refunds().update(&mut refund, session).await?;
                    let audit = actor_owned.clone().resource_log(
                        "supplier_refund.post",
                        "supplier_refund",
                        refund.base.id.clone(),
                    )?;
                    db.audit_logs().create(&audit, session).await?;
                    Ok::<(), crate::errors::Error>(())
                })
            })
            .await?;

        self.supplier_refund_detail(&detail_id).await
    }

    // -----------------------------------------------------------------------
    // 私有视图装配
    // -----------------------------------------------------------------------

    /// 装配供应商退款单视图。
    ///
    /// # 参数
    /// * `id` - 退款单 ID
    ///
    /// # 返回
    /// 返回退款单视图。
    ///
    /// # 错误
    /// * `NotFound` - 退款单不存在
    async fn supplier_refund_view(&self, id: String) -> Result<SupplierRefundView> {
        let refund = self
            .db
            .supplier_refunds()
            .find_by_id(&id, &mut NoTransaction)
            .await?
            .ok_or_else(|| Error::NotFound("供应商退款单不存在".to_string()))?;
        Ok(SupplierRefundView {
            id: refund.base.id.clone(),
            refund_no: refund.refund_no,
            status: refund.status,
            purchase_return_order_id: refund.purchase_return_order_id.map(|id| id.to_string()),
            supplier_id: refund.supplier_id.to_string(),
            original_payment_id: refund.original_payment_id.map(|id| id.to_string()),
            original_payable_entry_id: refund.original_payable_entry_id.map(|id| id.to_string()),
            reason_code: refund.reason_code,
            reason_text: refund.reason_text,
            amount: refund.amount,
            handled_by: refund.handled_by,
            reviewed_by: refund.reviewed_by,
            occurred_at: refund.occurred_at,
            version: refund.base.version,
            created_at: refund.base.created_at,
        })
    }
}
