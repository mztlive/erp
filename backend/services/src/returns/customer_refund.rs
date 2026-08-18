use super::dto::{
    CreateCustomerRefundRequest, CustomerRefundListParams, CustomerRefundView, PageView,
    PostCustomerRefundRequest, SortDir,
};
use super::reversal_plan::{plan_receipt_reverse, zero_amount};
use super::ReturnsService;
use crate::audit::AuditActor;
use crate::document_registry::{new_registered_document, persist_registered_document};
use crate::errors::{Error, Result};
use database::{AccessControlExt, NoTransaction, ReceivableExt, ReturnsExt, Transactional};
use entities::common::time::Instant;
use entities::document_registry::DocumentType;
use entities::ids::{CustomerRefundId, ReceiptAllocationId, ReceivableEntryId, ReceivableEntryOffsetId};
use entities::money::Amount;
use entities::receivable::{
    AllocationAction as ReceivableAllocationAction, CustomerReceiptStatus,
    EntryDirection as ReceivableEntryDirection, ReceiptAllocation, ReceiptAllocationData, ReceivableEntry,
    ReceivableEntryData, ReceivableEntryOffset, ReceivableEntryOffsetData, ReceivableEntryType,
};
use entities::returns::{CustomerRefund, CustomerRefundData, CustomerRefundStatus};
use id_generator::next_id;
use validator::Validate;

/// 客户退款列表筛选条件类型。
type CustomerRefundFilter = <mongodb::Database as ReturnsExt>::CustomerRefundFilter;

impl ReturnsService {
    // -----------------------------------------------------------------------
    // 客户退款
    // -----------------------------------------------------------------------

    /// 分页查询客户退款列表。
    ///
    /// # 参数
    /// * `params` - 查询参数
    ///
    /// # 返回
    /// 返回契约形状的分页视图。
    pub async fn customer_refund_list(
        &self,
        params: &CustomerRefundListParams,
    ) -> Result<PageView<CustomerRefundView>> {
        params.validate()?;
        let query = params.normalized()?;
        let filter = CustomerRefundFilter {
            refund_no: query.refund_no,
            customer_id: query.customer_id,
            status: query.status,
            page: query.paging.page,
            page_size: query.paging.page_size,
            sort_by: Some(query.paging.sort_by.to_string()),
            sort_ascending: matches!(query.paging.sort_dir, SortDir::Asc),
        };
        let page = self
            .db
            .customer_refunds()
            .search_customer_refunds(&filter, &mut NoTransaction)
            .await?;
        Ok(PageView {
            items: page
                .items
                .into_iter()
                .map(|row| CustomerRefundView {
                    id: row.id,
                    refund_no: row.refund_no,
                    status: row.status,
                    sales_return_case_id: row.sales_return_case_id,
                    customer_id: row.customer_id,
                    original_receipt_id: None,
                    original_receivable_entry_id: None,
                    reason_code: None,
                    reason_text: row.reason_text,
                    amount: row.amount,
                    handled_by: String::new(),
                    reviewed_by: String::new(),
                    occurred_at: Instant::from_unix_secs(row.occurred_at as i64),
                    version: row.version,
                    created_at: row.created_at,
                })
                .collect(),
            total: page.total,
            page: filter.page,
            page_size: filter.page_size,
        })
    }

    /// 查询客户退款详情。
    ///
    /// # 参数
    /// * `id` - 退款单 ID
    ///
    /// # 返回
    /// 返回退款单视图。
    ///
    /// # 错误
    /// * `NotFound` - 退款单不存在
    pub async fn customer_refund_detail(&self, id: &str) -> Result<CustomerRefundView> {
        self.customer_refund_view(id.to_string()).await
    }

    /// 登记客户退款草稿（单集合写入，无事务）。
    ///
    /// 退款单号全局唯一（唯一索引）构成幂等去重。经办人与复核人必须不同
    /// （岗位分离，W11 纠错强制复核）。
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
    pub async fn create_customer_refund(
        &self,
        req: CreateCustomerRefundRequest,
        actor: &AuditActor,
    ) -> Result<CustomerRefundView> {
        req.validate()?;
        let refund = CustomerRefund::new(
            CustomerRefundId::new(next_id()),
            CustomerRefundData {
                refund_no: req.refund_no,
                sales_return_case_id: req.sales_return_case_id,
                customer_id: req.customer_id,
                original_receipt_id: req.original_receipt_id,
                original_receivable_entry_id: req.original_receivable_entry_id,
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
            "customer_refund.create",
            "customer_refund",
            refund.base.id.clone(),
        )?;
        let document = new_registered_document(
            &refund.base.id,
            DocumentType::CustomerRefund,
            refund.refund_no.clone(),
        )?;
        let db = self.db.clone();
        let client = db.client().clone();
        let refund_for_tx = refund.clone();
        client
            .with_transaction(move |session| {
                Box::pin(async move {
                    db.customer_refunds().create(&refund_for_tx, session).await?;
                    persist_registered_document(&db, &document, session).await?;
                    db.audit_logs().create(&audit, session).await?;
                    Ok::<(), crate::errors::Error>(())
                })
            })
            .await?;

        self.customer_refund_detail(&refund.base.id).await
    }

    /// 客户退款过账（§8.3-3 事务不变量）。
    ///
    /// 同一事务内：退款单必须为草稿；按原回款（或其核销分配）反向写入
    /// `REVERSE` 回款核销分配；按条件原子冲减子账已核销进度
    /// （`revert_settlement` 不产生负已核销）；写反向应收分录（减少）与
    /// 分录抵销；退款单迁移为已过账。任一校验失败整体回滚，保留原事实。
    /// 退款单号唯一 + 状态迁移构成重复提交去重。
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
    /// * `NotFound` - 退款单或原回款不存在
    /// * `BusinessLogicError` - 累计退款超原回款、重复过账或超额冲减
    pub async fn post_customer_refund(
        &self,
        id: &str,
        req: PostCustomerRefundRequest,
        actor: &AuditActor,
    ) -> Result<CustomerRefundView> {
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
                        .customer_refunds()
                        .find_by_id(&refund_id, session)
                        .await?
                        .ok_or_else(|| Error::NotFound("客户退款单不存在".to_string()))?;
                    if refund.status != CustomerRefundStatus::Draft {
                        return Err(Error::BusinessLogicError(
                            "退款单已过账，请勿重复提交".to_string(),
                        ));
                    }
                    let original_receipt_id = refund.original_receipt_id.clone().ok_or_else(|| {
                        Error::BusinessLogicError("按原应收分录退款由冲减分录完成".to_string())
                    })?;
                    let receipt = db
                        .customer_receipts()
                        .find_by_id(&original_receipt_id, session)
                        .await?
                        .ok_or_else(|| Error::NotFound("原回款不存在".to_string()))?;
                    if receipt.status != CustomerReceiptStatus::Posted {
                        return Err(Error::BusinessLogicError("只有已过账回款可以退款".to_string()));
                    }
                    let refunded_before: Amount = db
                        .customer_refunds()
                        .find_refunds_by_originals(std::slice::from_ref(&original_receipt_id), &[], session)
                        .await?
                        .iter()
                        .filter(|other| other.base.id != refund.base.id)
                        .fold(zero_amount(), |sum, other| sum.checked_add(other.amount));
                    if refunded_before.checked_add(refund.amount) > receipt.amount {
                        return Err(Error::BusinessLogicError(
                            "累计退款金额不得超过原回款金额".to_string(),
                        ));
                    }

                    let allocations = db
                        .receipt_allocations()
                        .find_allocations_by_receipts(&[original_receipt_id], session)
                        .await?;
                    let (reverse_rows, chunks) = plan_receipt_reverse(&allocations, refund.amount)?;
                    let next_seq = allocations
                        .iter()
                        .map(|allocation| allocation.allocation_seq)
                        .max()
                        .unwrap_or(0)
                        + 1;

                    let mut decrease_entry: Option<ReceivableEntry> = None;
                    for (offset_index, chunk) in chunks.iter().enumerate() {
                        let entry = db
                            .receivable_entries()
                            .find_by_id(&chunk.increase_entry_id, session)
                            .await?
                            .ok_or_else(|| Error::NotFound("应收分录不存在".to_string()))?;
                        let account = db
                            .receivable_accounts()
                            .find_by_id(&entry.receivable_account_id, session)
                            .await?
                            .ok_or_else(|| Error::NotFound("应收往来子账不存在".to_string()))?;
                        if account.counterparty_party_id != receipt.counterparty_party_id {
                            return Err(Error::BusinessLogicError("禁止跨往来主体退款".to_string()));
                        }
                        let reverted = db
                            .receivable_accounts()
                            .revert_settlement(
                                &entry.receivable_account_id,
                                &chunk.amount,
                                &actor_id,
                                session,
                            )
                            .await?;
                        if !reverted {
                            return Err(Error::BusinessLogicError("退款冲减超过已核销金额".to_string()));
                        }
                        if decrease_entry.is_none() {
                            decrease_entry = Some(ReceivableEntry::new(
                                ReceivableEntryId::new(next_id()),
                                ReceivableEntryData {
                                    receivable_account_id: entry.receivable_account_id.clone(),
                                    entry_type: ReceivableEntryType::Refund,
                                    direction: ReceivableEntryDirection::Decrease,
                                    amount: refund.amount,
                                    due_date: entities::common::time::BusinessDate::today(),
                                    source_fact_type: "customer_refund".to_string(),
                                    source_document_id: refund.base.id.clone(),
                                    source_revision_id: refund.base.id.clone(),
                                    source_sequence: 1,
                                    posted_at: refund.occurred_at,
                                },
                            )?);
                        }
                        db.receivable_entry_offsets()
                            .create(
                                &ReceivableEntryOffset::new(
                                    ReceivableEntryOffsetId::new(next_id()),
                                    ReceivableEntryOffsetData {
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
                        db.receivable_entries().create(&entry, session).await?;
                    }
                    for (reverse_index, reverse) in reverse_rows.iter().enumerate() {
                        let allocation = ReceiptAllocation::new(
                            ReceiptAllocationId::new(next_id()),
                            ReceiptAllocationData {
                                customer_receipt_id: receipt.base.id.clone().into(),
                                receivable_entry_id: reverse.entry_id.clone(),
                                allocation_seq: next_seq + reverse_index as u32,
                                allocation_action: ReceivableAllocationAction::Reverse,
                                allocated_amount: reverse.amount,
                                allocated_at: refund.occurred_at,
                                reverses_allocation_id: Some(reverse.original_id.clone()),
                            },
                        )?;
                        db.receipt_allocations().create(&allocation, session).await?;
                    }
                    refund.transition(CustomerRefundStatus::Posted)?;
                    db.customer_refunds().update(&mut refund, session).await?;
                    let audit = actor_owned.clone().resource_log(
                        "customer_refund.post",
                        "customer_refund",
                        refund.base.id.clone(),
                    )?;
                    db.audit_logs().create(&audit, session).await?;
                    Ok::<(), crate::errors::Error>(())
                })
            })
            .await?;

        self.customer_refund_detail(&detail_id).await
    }

    // -----------------------------------------------------------------------
    // 私有视图装配
    // -----------------------------------------------------------------------

    /// 装配客户退款单视图。
    ///
    /// # 参数
    /// * `id` - 退款单 ID
    ///
    /// # 返回
    /// 返回退款单视图。
    ///
    /// # 错误
    /// * `NotFound` - 退款单不存在
    async fn customer_refund_view(&self, id: String) -> Result<CustomerRefundView> {
        let refund = self
            .db
            .customer_refunds()
            .find_by_id(&id, &mut NoTransaction)
            .await?
            .ok_or_else(|| Error::NotFound("客户退款单不存在".to_string()))?;
        Ok(CustomerRefundView {
            id: refund.base.id.clone(),
            refund_no: refund.refund_no,
            status: refund.status,
            sales_return_case_id: refund.sales_return_case_id.map(|id| id.to_string()),
            customer_id: refund.customer_id.to_string(),
            original_receipt_id: refund.original_receipt_id.map(|id| id.to_string()),
            original_receivable_entry_id: refund.original_receivable_entry_id.map(|id| id.to_string()),
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
