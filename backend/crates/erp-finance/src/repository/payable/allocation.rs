use entity_core::NOT_DELETED_TIMESTAMP_BSON;
use erp_core::ids::{PayableAccountId, PayableEntryId};
use mongodb::bson::{Document, doc};
use mongodb::options::FindOptions;
use persistence_core::{Executor, PageResult, Pagination, QueryFilter, Result, mongo_ops};

use crate::entity::payable::{PaymentAllocation, PurchaseInvoiceAllocation};
use crate::repository::owned::{PaymentAllocationRepository, PurchaseInvoiceAllocationRepository};

/// 进项发票分配服务端分页筛选条件（FIN-R06）。
#[derive(Debug, Clone)]
pub struct PurchaseInvoiceAllocationFilter {
    /// 应付往来子账；`None` 表示不筛选。
    pub payable_account_id: Option<PayableAccountId>,
    /// 进项发票；`None` 表示不筛选。
    pub invoice_id: Option<erp_core::ids::InvoiceId>,
    /// 页码（1 起）。
    pub page: u64,
    /// 单页条数。
    pub page_size: u32,
    /// 是否升序；`false` 表示降序（默认）。
    pub sort_ascending: bool,
}

impl QueryFilter for PurchaseInvoiceAllocationFilter {
    /// 转换为 MongoDB 查询条件（自动追加未删除过滤）。
    ///
    /// # 返回
    /// 返回查询条件文档。
    fn to_doc(&self) -> Document {
        let mut filter = doc! { "deleted_at": NOT_DELETED_TIMESTAMP_BSON };
        if let Some(account_id) = &self.payable_account_id {
            filter.insert("payable_account_id", account_id.to_string());
        }
        if let Some(invoice_id) = &self.invoice_id {
            filter.insert("invoice_id", invoice_id.to_string());
        }
        filter
    }
}

impl Pagination for PurchaseInvoiceAllocationFilter {
    /// 返回页码与单页条数。
    ///
    /// # 返回
    /// 返回 `(page, page_size)` 元组。
    fn page_and_size(&self) -> (u64, u64) {
        (self.page, u64::from(self.page_size))
    }
}

impl<'a> PaymentAllocationRepository<'a> {
    /// 批量按付款单集合取回核销分配（`$in` 一次取回，禁止 N+1）。
    ///
    /// # 参数
    /// * `payment_ids` - 付款单 ID 集合
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 返回全部匹配分配。
    ///
    /// # 错误
    /// 当 MongoDB 查询或游标读取失败时返回错误。
    pub async fn find_allocations_by_payments(
        &self,
        payment_ids: &[erp_core::ids::SupplierPaymentId],
        executor: &mut dyn Executor,
    ) -> Result<Vec<PaymentAllocation>> {
        if payment_ids.is_empty() {
            return Ok(Vec::new());
        }
        let payment_ids: Vec<String> = payment_ids.iter().map(ToString::to_string).collect();
        self.find_many(doc! { "supplier_payment_id": { "$in": payment_ids } }, executor).await
    }

    /// 批量按应付分录集合取回核销分配（`$in`，用于反向核销锁定）。
    ///
    /// # 参数
    /// * `entry_ids` - 应付分录 ID 集合
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 返回全部匹配分配。
    ///
    /// # 错误
    /// 当 MongoDB 查询或游标读取失败时返回错误。
    pub async fn find_allocations_by_entries(
        &self,
        entry_ids: &[PayableEntryId],
        executor: &mut dyn Executor,
    ) -> Result<Vec<PaymentAllocation>> {
        if entry_ids.is_empty() {
            return Ok(Vec::new());
        }
        let entry_ids: Vec<String> = entry_ids.iter().map(ToString::to_string).collect();
        self.find_many(doc! { "payable_entry_id": { "$in": entry_ids } }, executor).await
    }
}

impl<'a> PurchaseInvoiceAllocationRepository<'a> {
    /// 批量按发票集合取回进项发票分配（`$in` 一次取回，禁止 N+1）。
    ///
    /// # 参数
    /// * `invoice_ids` - 发票 ID 集合
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 返回全部匹配分配。
    ///
    /// # 错误
    /// 当 MongoDB 查询或游标读取失败时返回错误。
    pub async fn find_allocations_by_invoices(
        &self,
        invoice_ids: &[erp_core::ids::InvoiceId],
        executor: &mut dyn Executor,
    ) -> Result<Vec<PurchaseInvoiceAllocation>> {
        if invoice_ids.is_empty() {
            return Ok(Vec::new());
        }
        let invoice_ids: Vec<String> = invoice_ids.iter().map(ToString::to_string).collect();
        self.find_many(doc! { "invoice_id": { "$in": invoice_ids } }, executor).await
    }

    /// 按账户/发票条件服务端分页检索进项发票分配（FIN-R06）。
    ///
    /// 过滤、稳定排序（`(created_at, id)` 同方向）、`skip/limit` 与总数均在
    /// 数据库完成，只装载当前页；无条件时按集合全量分页。排序方向由调用方
    /// 传入，两个键始终同方向，保证同秒记录跨页无重复、无遗漏。
    /// 查询形状由 `idx_purchase_invoice_allocations_account_page` 与
    /// `idx_purchase_invoice_allocations_invoice_page` 覆盖。
    ///
    /// # 参数
    /// * `filter` - 账户/发票条件与分页排序
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 返回当前页分配实体与满足筛选条件的总数。
    ///
    /// # 错误
    /// 当 MongoDB 查询、游标读取或计数失败时返回错误。
    pub async fn search_purchase_invoice_allocations(
        &self,
        filter: &PurchaseInvoiceAllocationFilter,
        executor: &mut dyn Executor,
    ) -> Result<PageResult<PurchaseInvoiceAllocation>> {
        let direction = if filter.sort_ascending { 1 } else { -1 };
        let options = FindOptions::builder()
            .sort(doc! { "created_at": direction, "id": direction })
            .skip(filter.skip())
            .limit(filter.limit())
            .build();
        let items = mongo_ops::find_many(&self.collection(), filter.to_doc(), options, executor).await?;
        let total = mongo_ops::count_documents(&self.collection(), filter.to_doc(), executor).await?;
        Ok(PageResult { items, total: total as i64 })
    }

    /// 批量按应付子账集合取回进项发票分配（`$in`，用于收票进度校验）。
    ///
    /// # 参数
    /// * `account_ids` - 应付往来子账 ID 集合
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 返回全部匹配分配。
    ///
    /// # 错误
    /// 当 MongoDB 查询或游标读取失败时返回错误。
    pub async fn find_allocations_by_accounts(
        &self,
        account_ids: &[PayableAccountId],
        executor: &mut dyn Executor,
    ) -> Result<Vec<PurchaseInvoiceAllocation>> {
        if account_ids.is_empty() {
            return Ok(Vec::new());
        }
        let account_ids: Vec<String> = account_ids.iter().map(ToString::to_string).collect();
        self.find_many(doc! { "payable_account_id": { "$in": account_ids } }, executor).await
    }
}

#[cfg(test)]
mod tests {
    use erp_core::ids::PayableAccountId;
    use persistence_core::{Pagination, QueryFilter};

    use super::PurchaseInvoiceAllocationFilter;

    #[test]
    fn invoice_allocation_filter_applies_account_invoice_and_deleted_filter() {
        use erp_core::ids::InvoiceId;
        let filter = PurchaseInvoiceAllocationFilter {
            payable_account_id: Some(PayableAccountId::new("acct-1")),
            invoice_id: Some(InvoiceId::new("inv-1")),
            page: 2,
            page_size: 20,
            sort_ascending: true,
        };
        let document = filter.to_doc();
        assert_eq!(document.get_i64("deleted_at").unwrap(), 0);
        assert_eq!(document.get_str("payable_account_id").unwrap(), "acct-1");
        assert_eq!(document.get_str("invoice_id").unwrap(), "inv-1");
        assert_eq!(filter.page_and_size(), (2, 20));
        assert_eq!(filter.skip(), 20);
    }

    #[test]
    fn invoice_allocation_filter_supports_unscoped_pagination() {
        let filter = PurchaseInvoiceAllocationFilter {
            payable_account_id: None,
            invoice_id: None,
            page: 1,
            page_size: 10,
            sort_ascending: false,
        };
        let document = filter.to_doc();
        assert_eq!(document.get_i64("deleted_at").unwrap(), 0);
        assert!(!document.contains_key("payable_account_id"));
        assert!(!document.contains_key("invoice_id"));
    }
}
