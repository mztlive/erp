//! 域 D21 `returns` 服务编排（页面：W05 销售单、W09 收货发货、W11 客户往来、
//! W12 供应商往来）。
//!
//! 事务边界只在 Service（conventions §6.1）：
//! - 跨集合写入（处理单 + 明细行、退款/冲正过账）→
//!   `database::Transactional::with_transaction`；
//! - 单集合草稿写入 → `&mut NoTransaction`。
//! - 资金类入口（退款、冲正过账）以业务单号（退款单号/冲正单号）唯一索引 +
//!   状态迁移构成去重机制，重复提交只产生一条正式事实。
//!
//! 跨域只经 `DatabaseExt` 调对方域 Repository：D18 回款/应收分录/核销分配，
//! D19 付款/应付分录/核销分配（退款、冲正事务内写入反向事实与反向核销，
//! §8.3-3）。

use database::{AccessControlExt, NoTransaction, PayableExt, ReceivableExt, ReturnsExt, Transactional};
use entities::common::time::Instant;
use entities::ids::{
    CustomerRefundId, PayableEntryId, PayableEntryOffsetId, PaymentAllocationId, PaymentReversalId,
    PurchaseReturnLineId, PurchaseReturnOrderId, ReceiptAllocationId, ReceiptReversalId, ReceivableEntryId,
    ReceivableEntryOffsetId, SalesReturnCaseId, SalesReturnLineId, SupplierRefundId,
};
use entities::money::Amount;
use entities::payable::{
    AllocationAction as PayableAllocationAction, EntryDirection as PayableEntryDirection, PayableEntry,
    PayableEntryData, PayableEntryOffset, PayableEntryOffsetData, PayableEntryType, PaymentAllocation,
    PaymentAllocationData, SupplierPaymentStatus,
};
use entities::receivable::{
    AllocationAction as ReceivableAllocationAction, CustomerReceiptStatus,
    EntryDirection as ReceivableEntryDirection, ReceiptAllocation, ReceiptAllocationData, ReceivableEntry,
    ReceivableEntryData, ReceivableEntryOffset, ReceivableEntryOffsetData, ReceivableEntryType,
};
use entities::returns::{
    CustomerRefund, CustomerRefundData, CustomerRefundStatus, PaymentReversal, PaymentReversalData,
    PaymentReversalStatus, PurchaseReturnLine, PurchaseReturnLineData, PurchaseReturnOrder,
    PurchaseReturnOrderData, ReceiptReversal, ReceiptReversalData, ReceiptReversalStatus, SalesReturnCase,
    SalesReturnCaseData, SalesReturnLine, SalesReturnLineData, SupplierRefund, SupplierRefundData,
    SupplierRefundStatus,
};
use id_generator::next_id;
use mongodb::Database;
use std::str::FromStr;
use validator::Validate;

use crate::audit::AuditActor;
use crate::errors::{Error, Result};

mod dto;

use self::dto::SortDir;
pub use self::dto::{
    CreateCustomerRefundRequest, CreatePaymentReversalRequest, CreatePurchaseReturnOrderRequest,
    CreateReceiptReversalRequest, CreateSalesReturnCaseRequest, CreateSupplierRefundRequest,
    CustomerRefundListParams, CustomerRefundView, PageView, PaymentReversalView, PostCustomerRefundRequest,
    PostPaymentReversalRequest, PostReceiptReversalRequest, PostSupplierRefundRequest,
    PurchaseReturnOrderListParams, PurchaseReturnOrderView, ReceiptReversalView, SalesReturnCaseListParams,
    SalesReturnCaseView, SupplierRefundView,
};

/// 销售退货处理单列表筛选条件类型（经 `ReturnsExt` 关联类型跨 crate 可达）。
type SalesReturnCaseFilter = <mongodb::Database as ReturnsExt>::SalesReturnCaseFilter;
/// 采购退货单列表筛选条件类型。
type PurchaseReturnOrderFilter = <mongodb::Database as ReturnsExt>::PurchaseReturnOrderFilter;
/// 客户退款列表筛选条件类型。
type CustomerRefundFilter = <mongodb::Database as ReturnsExt>::CustomerRefundFilter;

/// 退货退款服务。
///
/// 提供退货/拒收处理单、采购退货单、客户/供应商退款与回款/付款冲正编排。
pub struct ReturnsService {
    db: Database,
}

impl ReturnsService {
    /// 创建退货退款服务实例。
    ///
    /// # 参数
    /// * `db` - 数据库实例
    ///
    /// # 返回
    /// 返回服务实例。
    pub fn new(db: Database) -> Self {
        Self { db }
    }

    // -----------------------------------------------------------------------
    // 销售退货/拒收处理单
    // -----------------------------------------------------------------------

    /// 分页查询销售退货/拒收处理单列表。
    ///
    /// # 参数
    /// * `params` - 查询参数（`return_no`/`sales_order_id`/`status` 扁平筛选）
    ///
    /// # 返回
    /// 返回契约形状的分页视图。
    pub async fn sales_return_case_list(
        &self,
        params: &SalesReturnCaseListParams,
    ) -> Result<PageView<SalesReturnCaseView>> {
        params.validate()?;
        let query = params.normalized()?;
        let filter = SalesReturnCaseFilter {
            return_no: query.return_no,
            sales_order_id: query.sales_order_id,
            status: query.status,
            page: query.paging.page,
            page_size: query.paging.page_size,
            sort_by: Some(query.paging.sort_by.to_string()),
            sort_ascending: matches!(query.paging.sort_dir, SortDir::Asc),
        };
        let page = self
            .db
            .sales_return_cases()
            .search_sales_return_cases(&filter, &mut NoTransaction)
            .await?;
        let mut views = Vec::with_capacity(page.items.len());
        for row in page.items {
            views.push(self.sales_return_case_view(row.id).await?);
        }
        Ok(PageView {
            items: views,
            total: page.total,
            page: filter.page,
            page_size: filter.page_size,
        })
    }

    /// 查询销售退货/拒收处理单详情（处理单 + 明细行）。
    ///
    /// # 参数
    /// * `id` - 处理单 ID
    ///
    /// # 返回
    /// 返回完整处理单视图。
    ///
    /// # 错误
    /// * `NotFound` - 处理单不存在
    pub async fn sales_return_case_detail(&self, id: &str) -> Result<SalesReturnCaseView> {
        self.sales_return_case_view(id.to_string()).await
    }

    /// 建立销售退货/拒收处理单与明细行（跨集合事务写入）。
    ///
    /// `return_no` 全局唯一（唯一索引）构成幂等去重；同事务写入处理单与
    /// 明细行（`ReturnsRepository::create_sales_return_with_line`）。
    ///
    /// # 参数
    /// * `req` - 创建请求
    /// * `actor` - 已通过鉴权的审计操作人
    ///
    /// # 返回
    /// 返回新建处理单视图。
    ///
    /// # 错误
    /// * `ConflictError` - 退货处理号重复
    pub async fn create_sales_return_case(
        &self,
        req: CreateSalesReturnCaseRequest,
        actor: &AuditActor,
    ) -> Result<SalesReturnCaseView> {
        req.validate()?;
        let case_id = SalesReturnCaseId::new(next_id());
        let case = SalesReturnCase::new(
            case_id.clone(),
            SalesReturnCaseData {
                return_no: req.return_no,
                sales_order_id: req.sales_order_id,
                acceptance_id: req.acceptance_id,
                case_type: req.case_type,
                reason: req.reason,
                discovered_at: req.discovered_at,
                return_route: req.return_route,
            },
            actor.id(),
        )?;
        let line = SalesReturnLine::new(
            SalesReturnLineId::new(next_id()),
            SalesReturnLineData {
                sales_return_case_id: case_id.clone(),
                sales_order_line_id: req.lines[0].sales_order_line_id.clone(),
                requested_quantity: req.lines[0].requested_quantity,
                received_quantity: None,
                quality_result: None,
                restockable_quantity: None,
            },
        )?;
        let audit = actor.clone().resource_log(
            "sales_return_case.create",
            "sales_return_case",
            case_id.to_string(),
        )?;

        let db = self.db.clone();
        let client = db.client().clone();
        client
            .with_transaction(move |session| {
                Box::pin(async move {
                    db.returns()
                        .create_sales_return_with_line(&case, &line, session)
                        .await?;
                    db.audit_logs().create(&audit, session).await?;
                    Ok::<(), crate::errors::Error>(())
                })
            })
            .await?;

        self.sales_return_case_detail(&case_id).await
    }

    // -----------------------------------------------------------------------
    // 采购退货单
    // -----------------------------------------------------------------------

    /// 分页查询采购退货单列表。
    ///
    /// # 参数
    /// * `params` - 查询参数
    ///
    /// # 返回
    /// 返回契约形状的分页视图。
    pub async fn purchase_return_order_list(
        &self,
        params: &PurchaseReturnOrderListParams,
    ) -> Result<PageView<PurchaseReturnOrderView>> {
        params.validate()?;
        let query = params.normalized()?;
        let filter = PurchaseReturnOrderFilter {
            purchase_return_no: query.purchase_return_no,
            purchase_order_id: query.purchase_order_id,
            status: query.status,
            page: query.paging.page,
            page_size: query.paging.page_size,
            sort_by: Some(query.paging.sort_by.to_string()),
            sort_ascending: matches!(query.paging.sort_dir, SortDir::Asc),
        };
        let page = self
            .db
            .purchase_return_orders()
            .search_purchase_return_orders(&filter, &mut NoTransaction)
            .await?;
        let mut views = Vec::with_capacity(page.items.len());
        for row in page.items {
            views.push(self.purchase_return_order_view(row.id).await?);
        }
        Ok(PageView {
            items: views,
            total: page.total,
            page: filter.page,
            page_size: filter.page_size,
        })
    }

    /// 查询采购退货单详情（退货单 + 明细行）。
    ///
    /// # 参数
    /// * `id` - 退货单 ID
    ///
    /// # 返回
    /// 返回完整退货单视图。
    ///
    /// # 错误
    /// * `NotFound` - 退货单不存在
    pub async fn purchase_return_order_detail(&self, id: &str) -> Result<PurchaseReturnOrderView> {
        self.purchase_return_order_view(id.to_string()).await
    }

    /// 建立采购退货单与明细行（跨集合事务写入）。
    ///
    /// `purchase_return_no` 全局唯一（唯一索引）构成幂等去重。
    ///
    /// # 参数
    /// * `req` - 创建请求
    /// * `actor` - 已通过鉴权的审计操作人
    ///
    /// # 返回
    /// 返回新建退货单视图。
    ///
    /// # 错误
    /// * `ConflictError` - 采购退货单号重复
    pub async fn create_purchase_return_order(
        &self,
        req: CreatePurchaseReturnOrderRequest,
        actor: &AuditActor,
    ) -> Result<PurchaseReturnOrderView> {
        req.validate()?;
        let order_id = PurchaseReturnOrderId::new(next_id());
        let order = PurchaseReturnOrder::new(
            order_id.clone(),
            PurchaseReturnOrderData {
                purchase_return_no: req.purchase_return_no,
                purchase_order_id: req.purchase_order_id,
                sales_return_case_id: req.sales_return_case_id,
                return_mode: req.return_mode,
            },
            actor.id(),
        )?;
        let line = PurchaseReturnLine::new(
            PurchaseReturnLineId::new(next_id()),
            PurchaseReturnLineData {
                purchase_return_order_id: order_id.clone(),
                purchase_order_revision_line_id: req.lines[0].purchase_order_revision_line_id.clone(),
                return_quantity: req.lines[0].return_quantity,
                warehouse_id: req.lines[0].warehouse_id.clone(),
            },
        )?;
        let audit = actor.clone().resource_log(
            "purchase_return_order.create",
            "purchase_return_order",
            order_id.to_string(),
        )?;

        let db = self.db.clone();
        let client = db.client().clone();
        client
            .with_transaction(move |session| {
                Box::pin(async move {
                    db.returns()
                        .create_purchase_return_with_line(&order, &line, session)
                        .await?;
                    db.audit_logs().create(&audit, session).await?;
                    Ok::<(), crate::errors::Error>(())
                })
            })
            .await?;

        self.purchase_return_order_detail(&order_id).await
    }

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
        self.db
            .customer_refunds()
            .create(&refund, &mut NoTransaction)
            .await?;
        self.db.audit_logs().create(&audit, &mut NoTransaction).await?;

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
        self.db
            .supplier_refunds()
            .create(&refund, &mut NoTransaction)
            .await?;
        self.db.audit_logs().create(&audit, &mut NoTransaction).await?;

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
    // 回款冲正与付款冲正
    // -----------------------------------------------------------------------

    /// 登记回款冲正草稿（单集合写入，无事务；经办/复核分离）。
    ///
    /// # 参数
    /// * `req` - 创建请求
    /// * `actor` - 已通过鉴权的审计操作人
    ///
    /// # 返回
    /// 返回新建冲正单视图。
    ///
    /// # 错误
    /// * `ConflictError` - 冲正单号重复
    pub async fn create_receipt_reversal(
        &self,
        req: CreateReceiptReversalRequest,
        actor: &AuditActor,
    ) -> Result<ReceiptReversalView> {
        req.validate()?;
        let reversal = ReceiptReversal::new(
            ReceiptReversalId::new(next_id()),
            ReceiptReversalData {
                reversal_no: req.reversal_no,
                original_customer_receipt_id: req.original_customer_receipt_id,
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
            "receipt_reversal.create",
            "receipt_reversal",
            reversal.base.id.clone(),
        )?;
        self.db
            .receipt_reversals()
            .create(&reversal, &mut NoTransaction)
            .await?;
        self.db.audit_logs().create(&audit, &mut NoTransaction).await?;

        self.receipt_reversal_view(reversal.base.id.clone()).await
    }

    /// 回款冲正过账（§8.3-3 事务不变量）。
    ///
    /// 同一事务内：冲正单必须为草稿；按原回款核销分配反向写入 `REVERSE`
    /// 分配并原子冲减子账已核销进度；原回款迁移为已冲正；冲正单迁移为已过账。
    /// 累计有效冲正不得超过原回款金额。
    ///
    /// # 参数
    /// * `id` - 冲正单 ID
    /// * `req` - 过账请求（占位）
    /// * `actor` - 已通过鉴权的审计操作人
    ///
    /// # 返回
    /// 返回过账后冲正单视图。
    ///
    /// # 错误
    /// * `NotFound` - 冲正单或原回款不存在
    /// * `BusinessLogicError` - 累计冲正超原回款或重复过账
    pub async fn post_receipt_reversal(
        &self,
        id: &str,
        req: PostReceiptReversalRequest,
        actor: &AuditActor,
    ) -> Result<ReceiptReversalView> {
        req.validate()?;
        let db = self.db.clone();
        let client = db.client().clone();
        let actor_owned = actor.clone();
        let actor_id = actor.id().to_string();
        let reversal_id = id.to_string();
        let detail_id = reversal_id.clone();
        client
            .with_transaction(move |session| {
                Box::pin(async move {
                    let mut reversal = db
                        .receipt_reversals()
                        .find_by_id(&reversal_id, session)
                        .await?
                        .ok_or_else(|| Error::NotFound("回款冲正单不存在".to_string()))?;
                    if reversal.status != ReceiptReversalStatus::Draft {
                        return Err(Error::BusinessLogicError(
                            "冲正单已过账，请勿重复提交".to_string(),
                        ));
                    }
                    let original_id = reversal.original_customer_receipt_id.clone();
                    let receipt = db
                        .customer_receipts()
                        .find_by_id(&original_id, session)
                        .await?
                        .ok_or_else(|| Error::NotFound("原回款不存在".to_string()))?;
                    if receipt.status != CustomerReceiptStatus::Posted {
                        return Err(Error::BusinessLogicError("只有已过账回款可以冲正".to_string()));
                    }
                    let reversed_before: Amount = db
                        .receipt_reversals()
                        .find_reversals_by_receipts(std::slice::from_ref(&original_id), session)
                        .await?
                        .iter()
                        .filter(|other| other.base.id != reversal.base.id)
                        .fold(zero_amount(), |sum, other| sum.checked_add(other.amount));
                    if reversed_before.checked_add(reversal.amount) > receipt.amount {
                        return Err(Error::BusinessLogicError(
                            "累计冲正金额不得超过原回款金额".to_string(),
                        ));
                    }

                    let allocations = db
                        .receipt_allocations()
                        .find_allocations_by_receipts(&[original_id], session)
                        .await?;
                    let (reverse_rows, chunks) = plan_receipt_reverse(&allocations, reversal.amount)?;
                    let next_seq = allocations
                        .iter()
                        .map(|allocation| allocation.allocation_seq)
                        .max()
                        .unwrap_or(0)
                        + 1;
                    for chunk in &chunks {
                        let entry = db
                            .receivable_entries()
                            .find_by_id(&chunk.increase_entry_id, session)
                            .await?
                            .ok_or_else(|| Error::NotFound("应收分录不存在".to_string()))?;
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
                            return Err(Error::BusinessLogicError("冲正冲减超过已核销金额".to_string()));
                        }
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
                                allocated_at: reversal.occurred_at,
                                reverses_allocation_id: Some(reverse.original_id.clone()),
                            },
                        )?;
                        db.receipt_allocations().create(&allocation, session).await?;
                    }
                    let mut receipt_mut = receipt;
                    receipt_mut.transition(CustomerReceiptStatus::Reversed)?;
                    db.customer_receipts().update(&mut receipt_mut, session).await?;
                    reversal.transition(ReceiptReversalStatus::Posted)?;
                    db.receipt_reversals().update(&mut reversal, session).await?;
                    let audit = actor_owned.clone().resource_log(
                        "receipt_reversal.post",
                        "receipt_reversal",
                        reversal.base.id.clone(),
                    )?;
                    db.audit_logs().create(&audit, session).await?;
                    Ok::<(), crate::errors::Error>(())
                })
            })
            .await?;

        self.receipt_reversal_view(detail_id).await
    }

    /// 登记付款冲正草稿（单集合写入，无事务；经办/复核分离）。
    ///
    /// # 参数
    /// * `req` - 创建请求
    /// * `actor` - 已通过鉴权的审计操作人
    ///
    /// # 返回
    /// 返回新建冲正单视图。
    ///
    /// # 错误
    /// * `ConflictError` - 冲正单号重复
    pub async fn create_payment_reversal(
        &self,
        req: CreatePaymentReversalRequest,
        actor: &AuditActor,
    ) -> Result<PaymentReversalView> {
        req.validate()?;
        let reversal = PaymentReversal::new(
            PaymentReversalId::new(next_id()),
            PaymentReversalData {
                reversal_no: req.reversal_no,
                original_supplier_payment_id: req.original_supplier_payment_id,
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
            "payment_reversal.create",
            "payment_reversal",
            reversal.base.id.clone(),
        )?;
        self.db
            .payment_reversals()
            .create(&reversal, &mut NoTransaction)
            .await?;
        self.db.audit_logs().create(&audit, &mut NoTransaction).await?;

        self.payment_reversal_view(reversal.base.id.clone()).await
    }

    /// 付款冲正过账（§8.3-3 事务不变量，应付侧镜像）。
    ///
    /// 同一事务内：冲正单必须为草稿；按原付款核销分配反向写入 `REVERSE`
    /// 分配并原子冲减应付子账已核销进度；原付款迁移为已冲正；冲正单迁移为
    /// 已过账。累计有效冲正不得超过原付款金额。
    ///
    /// # 参数
    /// * `id` - 冲正单 ID
    /// * `req` - 过账请求（占位）
    /// * `actor` - 已通过鉴权的审计操作人
    ///
    /// # 返回
    /// 返回过账后冲正单视图。
    ///
    /// # 错误
    /// * `NotFound` - 冲正单或原付款不存在
    /// * `BusinessLogicError` - 累计冲正超原付款或重复过账
    pub async fn post_payment_reversal(
        &self,
        id: &str,
        req: PostPaymentReversalRequest,
        actor: &AuditActor,
    ) -> Result<PaymentReversalView> {
        req.validate()?;
        let db = self.db.clone();
        let client = db.client().clone();
        let actor_owned = actor.clone();
        let actor_id = actor.id().to_string();
        let reversal_id = id.to_string();
        let detail_id = reversal_id.clone();
        client
            .with_transaction(move |session| {
                Box::pin(async move {
                    let mut reversal = db
                        .payment_reversals()
                        .find_by_id(&reversal_id, session)
                        .await?
                        .ok_or_else(|| Error::NotFound("付款冲正单不存在".to_string()))?;
                    if reversal.status != PaymentReversalStatus::Draft {
                        return Err(Error::BusinessLogicError(
                            "冲正单已过账，请勿重复提交".to_string(),
                        ));
                    }
                    let original_id = reversal.original_supplier_payment_id.clone();
                    let payment = db
                        .supplier_payments()
                        .find_by_id(&original_id, session)
                        .await?
                        .ok_or_else(|| Error::NotFound("原付款不存在".to_string()))?;
                    if payment.status != SupplierPaymentStatus::Posted {
                        return Err(Error::BusinessLogicError("只有已过账付款可以冲正".to_string()));
                    }
                    let reversed_before: Amount = db
                        .payment_reversals()
                        .find_reversals_by_payments(std::slice::from_ref(&original_id), session)
                        .await?
                        .iter()
                        .filter(|other| other.base.id != reversal.base.id)
                        .fold(zero_amount(), |sum, other| sum.checked_add(other.amount));
                    if reversed_before.checked_add(reversal.amount) > payment.amount {
                        return Err(Error::BusinessLogicError(
                            "累计冲正金额不得超过原付款金额".to_string(),
                        ));
                    }

                    let allocations = db
                        .payment_allocations()
                        .find_allocations_by_payments(&[original_id], session)
                        .await?;
                    let (reverse_rows, chunks) = plan_payment_reverse(&allocations, reversal.amount)?;
                    let next_seq = allocations
                        .iter()
                        .map(|allocation| allocation.allocation_seq)
                        .max()
                        .unwrap_or(0)
                        + 1;
                    for chunk in &chunks {
                        let entry = db
                            .payable_entries()
                            .find_by_id(&chunk.increase_entry_id, session)
                            .await?
                            .ok_or_else(|| Error::NotFound("应付分录不存在".to_string()))?;
                        let reverted = db
                            .payable_accounts()
                            .revert_settlement(&entry.payable_account_id, &chunk.amount, &actor_id, session)
                            .await?;
                        if !reverted {
                            return Err(Error::BusinessLogicError("冲正冲减超过已核销金额".to_string()));
                        }
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
                                allocated_at: reversal.occurred_at,
                                reverses_allocation_id: Some(reverse.original_id.clone()),
                            },
                        )?;
                        db.payment_allocations().create(&allocation, session).await?;
                    }
                    let mut payment_mut = payment;
                    payment_mut.transition(SupplierPaymentStatus::Reversed)?;
                    db.supplier_payments().update(&mut payment_mut, session).await?;
                    reversal.transition(PaymentReversalStatus::Posted)?;
                    db.payment_reversals().update(&mut reversal, session).await?;
                    let audit = actor_owned.clone().resource_log(
                        "payment_reversal.post",
                        "payment_reversal",
                        reversal.base.id.clone(),
                    )?;
                    db.audit_logs().create(&audit, session).await?;
                    Ok::<(), crate::errors::Error>(())
                })
            })
            .await?;

        self.payment_reversal_view(detail_id).await
    }

    // -----------------------------------------------------------------------
    // 私有视图装配
    // -----------------------------------------------------------------------

    /// 装配销售退货/拒收处理单视图。
    ///
    /// # 参数
    /// * `id` - 处理单 ID
    ///
    /// # 返回
    /// 返回完整处理单视图。
    ///
    /// # 错误
    /// * `NotFound` - 处理单不存在
    async fn sales_return_case_view(&self, id: String) -> Result<SalesReturnCaseView> {
        let case = self
            .db
            .sales_return_cases()
            .find_by_id(&id, &mut NoTransaction)
            .await?
            .ok_or_else(|| Error::NotFound("销售退货处理单不存在".to_string()))?;
        let lines = self
            .db
            .sales_return_lines()
            .find_lines_by_cases(&[case.base.id.clone().into()], &mut NoTransaction)
            .await?
            .into_iter()
            .map(|line| crate::returns::dto::SalesReturnLineView {
                id: line.base.id.clone(),
                sales_order_line_id: line.sales_order_line_id.to_string(),
                requested_quantity: line.requested_quantity,
                received_quantity: line.received_quantity,
                quality_result: line.quality_result.map(|result| result.as_str().to_string()),
                restockable_quantity: line.restockable_quantity,
            })
            .collect();
        Ok(SalesReturnCaseView {
            id: case.base.id.clone(),
            return_no: case.return_no,
            sales_order_id: case.sales_order_id.to_string(),
            acceptance_id: case.acceptance_id.map(|id| id.to_string()),
            case_type: case.case_type,
            reason: case.reason,
            discovered_at: case.discovered_at,
            return_route: case.return_route,
            status: case.stable.status(),
            version: case.base.version,
            created_at: case.base.created_at,
            lines,
        })
    }

    /// 装配采购退货单视图。
    ///
    /// # 参数
    /// * `id` - 退货单 ID
    ///
    /// # 返回
    /// 返回完整退货单视图。
    ///
    /// # 错误
    /// * `NotFound` - 退货单不存在
    async fn purchase_return_order_view(&self, id: String) -> Result<PurchaseReturnOrderView> {
        let order = self
            .db
            .purchase_return_orders()
            .find_by_id(&id, &mut NoTransaction)
            .await?
            .ok_or_else(|| Error::NotFound("采购退货单不存在".to_string()))?;
        let lines = self
            .db
            .purchase_return_lines()
            .find_lines_by_orders(&[order.base.id.clone().into()], &mut NoTransaction)
            .await?
            .into_iter()
            .map(|line| crate::returns::dto::PurchaseReturnLineView {
                id: line.base.id.clone(),
                purchase_order_revision_line_id: line.purchase_order_revision_line_id.to_string(),
                return_quantity: line.return_quantity,
                warehouse_id: line.warehouse_id.map(|id| id.to_string()),
            })
            .collect();
        Ok(PurchaseReturnOrderView {
            id: order.base.id.clone(),
            purchase_return_no: order.purchase_return_no,
            purchase_order_id: order.purchase_order_id.to_string(),
            sales_return_case_id: order.sales_return_case_id.map(|id| id.to_string()),
            return_mode: order.return_mode,
            status: order.stable.status(),
            version: order.base.version,
            created_at: order.base.created_at,
            lines,
        })
    }

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

    /// 装配回款冲正单视图。
    ///
    /// # 参数
    /// * `id` - 冲正单 ID
    ///
    /// # 返回
    /// 返回冲正单视图。
    ///
    /// # 错误
    /// * `NotFound` - 冲正单不存在
    async fn receipt_reversal_view(&self, id: String) -> Result<ReceiptReversalView> {
        let reversal = self
            .db
            .receipt_reversals()
            .find_by_id(&id, &mut NoTransaction)
            .await?
            .ok_or_else(|| Error::NotFound("回款冲正单不存在".to_string()))?;
        Ok(ReceiptReversalView {
            id: reversal.base.id.clone(),
            reversal_no: reversal.reversal_no,
            status: reversal.status,
            original_customer_receipt_id: reversal.original_customer_receipt_id.to_string(),
            reason_code: reversal.reason_code,
            reason_text: reversal.reason_text,
            amount: reversal.amount,
            handled_by: reversal.handled_by,
            reviewed_by: reversal.reviewed_by,
            occurred_at: reversal.occurred_at,
            version: reversal.base.version,
            created_at: reversal.base.created_at,
        })
    }

    /// 装配付款冲正单视图。
    ///
    /// # 参数
    /// * `id` - 冲正单 ID
    ///
    /// # 返回
    /// 返回冲正单视图。
    ///
    /// # 错误
    /// * `NotFound` - 冲正单不存在
    async fn payment_reversal_view(&self, id: String) -> Result<PaymentReversalView> {
        let reversal = self
            .db
            .payment_reversals()
            .find_by_id(&id, &mut NoTransaction)
            .await?
            .ok_or_else(|| Error::NotFound("付款冲正单不存在".to_string()))?;
        Ok(PaymentReversalView {
            id: reversal.base.id.clone(),
            reversal_no: reversal.reversal_no,
            status: reversal.status,
            original_supplier_payment_id: reversal.original_supplier_payment_id.to_string(),
            reason_code: reversal.reason_code,
            reason_text: reversal.reason_text,
            amount: reversal.amount,
            handled_by: reversal.handled_by,
            reviewed_by: reversal.reviewed_by,
            occurred_at: reversal.occurred_at,
            version: reversal.base.version,
            created_at: reversal.base.created_at,
        })
    }
}

/// 回款核销反向计划行。
struct ReceiptReversePlanRow {
    /// 被反向分配引用的原 `APPLY` 分配。
    original_id: ReceiptAllocationId,
    /// 反向金额。
    amount: Amount,
    /// 被核销应收分录。
    entry_id: ReceivableEntryId,
}

/// 回款核销冲减计划块（按原分配逐条分摊）。
struct ReceiptReverseChunk {
    /// 被冲减的增加分录。
    increase_entry_id: ReceivableEntryId,
    /// 冲减金额。
    amount: Amount,
}

/// 按原回款核销分配规划反向核销（§8.3-3）。
///
/// 按分配序号顺序分摊退款/冲正金额：每笔原 `APPLY` 分配先扣除既有 `REVERSE`
/// 再分摊；任一时刻累计反向不得超过原有效分配；金额不足时返回错误。
///
/// # 参数
/// * `allocations` - 原回款核销分配（`APPLY` + `REVERSE`）
/// * `amount` - 本次反向金额
///
/// # 返回
/// 返回 `(反向分配行计划, 冲减块计划)`。
///
/// # 错误
/// 原有效分配不足以覆盖反向金额时返回 `BusinessLogicError`。
fn plan_receipt_reverse(
    allocations: &[ReceiptAllocation],
    amount: Amount,
) -> Result<(Vec<ReceiptReversePlanRow>, Vec<ReceiptReverseChunk>)> {
    let mut remaining = amount;
    let mut rows = Vec::new();
    let mut chunks = Vec::new();
    for allocation in allocations {
        if allocation.allocation_action != ReceivableAllocationAction::Apply {
            continue;
        }
        let reversed: Amount = allocations
            .iter()
            .filter(|other| {
                other.allocation_action == ReceivableAllocationAction::Reverse
                    && other.reverses_allocation_id.as_ref() == Some(&allocation.base.id.clone().into())
            })
            .fold(zero_amount(), |sum, other| {
                sum.checked_add(other.allocated_amount)
            });
        if reversed >= allocation.allocated_amount {
            continue;
        }
        let effective = allocation.allocated_amount.checked_sub(reversed);
        let chunk = if effective >= remaining {
            remaining
        } else {
            effective
        };
        if chunk.to_decimal().is_zero() {
            continue;
        }
        rows.push(ReceiptReversePlanRow {
            original_id: allocation.base.id.clone().into(),
            amount: chunk,
            entry_id: allocation.receivable_entry_id.clone(),
        });
        chunks.push(ReceiptReverseChunk {
            increase_entry_id: allocation.receivable_entry_id.clone(),
            amount: chunk,
        });
        remaining = remaining.checked_sub(chunk);
        if remaining.to_decimal().is_zero() {
            break;
        }
    }
    if !remaining.to_decimal().is_zero() {
        return Err(Error::BusinessLogicError(
            "原回款有效分配不足，无法全额反向".to_string(),
        ));
    }
    Ok((rows, chunks))
}

/// 付款核销反向计划行。
struct PaymentReversePlanRow {
    /// 被反向分配引用的原 `APPLY` 分配。
    original_id: PaymentAllocationId,
    /// 反向金额。
    amount: Amount,
    /// 被核销应付分录。
    entry_id: PayableEntryId,
}

/// 付款核销冲减计划块。
struct PaymentReverseChunk {
    /// 被冲减的增加分录。
    increase_entry_id: PayableEntryId,
    /// 冲减金额。
    amount: Amount,
}

/// 按原付款核销分配规划反向核销（§8.3-3，应付侧镜像）。
///
/// # 参数
/// * `allocations` - 原付款核销分配（`APPLY` + `REVERSE`）
/// * `amount` - 本次反向金额
///
/// # 返回
/// 返回 `(反向分配行计划, 冲减块计划)`。
///
/// # 错误
/// 原有效分配不足以覆盖反向金额时返回 `BusinessLogicError`。
fn plan_payment_reverse(
    allocations: &[PaymentAllocation],
    amount: Amount,
) -> Result<(Vec<PaymentReversePlanRow>, Vec<PaymentReverseChunk>)> {
    let mut remaining = amount;
    let mut rows = Vec::new();
    let mut chunks = Vec::new();
    for allocation in allocations {
        if allocation.allocation_action != PayableAllocationAction::Apply {
            continue;
        }
        let reversed: Amount = allocations
            .iter()
            .filter(|other| {
                other.allocation_action == PayableAllocationAction::Reverse
                    && other.reverses_allocation_id.as_ref() == Some(&allocation.base.id.clone().into())
            })
            .fold(zero_amount(), |sum, other| {
                sum.checked_add(other.allocated_amount)
            });
        if reversed >= allocation.allocated_amount {
            continue;
        }
        let effective = allocation.allocated_amount.checked_sub(reversed);
        let chunk = if effective >= remaining {
            remaining
        } else {
            effective
        };
        if chunk.to_decimal().is_zero() {
            continue;
        }
        rows.push(PaymentReversePlanRow {
            original_id: allocation.base.id.clone().into(),
            amount: chunk,
            entry_id: allocation.payable_entry_id.clone(),
        });
        chunks.push(PaymentReverseChunk {
            increase_entry_id: allocation.payable_entry_id.clone(),
            amount: chunk,
        });
        remaining = remaining.checked_sub(chunk);
        if remaining.to_decimal().is_zero() {
            break;
        }
    }
    if !remaining.to_decimal().is_zero() {
        return Err(Error::BusinessLogicError(
            "原付款有效分配不足，无法全额反向".to_string(),
        ));
    }
    Ok((rows, chunks))
}

/// 返回固定零金额。
///
/// # 返回
/// 返回金额 `0.00`。
fn zero_amount() -> Amount {
    Amount::from_str("0.00").expect("固定零金额必须可解析")
}
