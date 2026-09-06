use crate::repository::owned::{
    CustomerReceiptRepository, InvoiceRepository, ReceiptAllocationRepository, ReceivableAccountRepository,
    ReceivableEntryRepository, SalesInvoiceAllocationRepository,
};
use entities::receivable::{
    CustomerReceiptStatus, InvoiceDirection, InvoiceKind, InvoiceStatus, ReceiptAllocation,
    ReceivableAccount, ReceivableEntry, ReceivableFundsReview, SalesInvoiceAllocation,
};
use erp_core::ids::{PartyId, ReceivableAccountId};
use mongodb::bson::doc;
use mongodb::options::FindOptions;

use super::super::extensions::ReceivableExt;
use super::invoice::{InvoiceFilter, InvoiceRow};
use super::receipt::{CustomerReceiptFilter, CustomerReceiptRow};
use super::{ReceivableRepository, RECEIVABLE_ENTRIES, RECEIVABLE_FUNDS_REVIEWS};
use persistence_core::Executor;
use persistence_core::PageResult;
use persistence_core::{mongo_ops, Result};

/// 回款/发票列表的账户关联作用域（FIN-R08）。
///
/// `sales_order_id` 与 `receivable_account_id` 同时给出时取交集；
/// 两者均为空表示无作用域，不得触发关联扫描。
#[derive(Debug, Clone, Default)]
pub struct ReceivableListScope {
    /// 来源销售单；`None` 表示不按销售单约束。
    pub sales_order_id: Option<String>,
    /// 应收子账；`None` 表示不按子账约束。
    pub receivable_account_id: Option<ReceivableAccountId>,
}

impl ReceivableListScope {
    /// 无作用域时返回 `true`（不得触发关联扫描）。
    pub fn is_empty(&self) -> bool {
        self.sales_order_id.is_none() && self.receivable_account_id.is_none()
    }
}

/// 客户回款单作用域分页查询（FIN-R08）。
///
/// 作用域解析与分页搜索合并在仓储内完成；禁止向 Service 返回无界中间 ID。
#[derive(Debug, Clone)]
pub struct ScopedCustomerReceiptQuery {
    /// 回款单号模糊匹配；`None` 表示不筛选。
    pub receipt_no: Option<String>,
    /// 实际付款往来主体；`None` 表示不筛选。
    pub counterparty_party_id: Option<PartyId>,
    /// 回款单状态；`None` 表示不筛选。
    pub status: Option<CustomerReceiptStatus>,
    /// 账户关联作用域；为空时不触发关联扫描。
    pub scope: ReceivableListScope,
    /// 页码（1 起）。
    pub page: u64,
    /// 单页条数。
    pub page_size: u32,
    /// 排序字段（白名单内有效，默认 `created_at`）。
    pub sort_by: Option<String>,
    /// 是否升序；`false` 表示降序（默认）。
    pub sort_ascending: bool,
}

/// 发票作用域分页查询（FIN-R08）。
///
/// 作用域解析与分页搜索合并在仓储内完成；禁止向 Service 返回无界中间 ID。
#[derive(Debug, Clone)]
pub struct ScopedInvoiceQuery {
    /// 发票方向；`None` 表示不筛选。
    pub invoice_direction: Option<InvoiceDirection>,
    /// 蓝红类型；`None` 表示不筛选。
    pub invoice_kind: Option<InvoiceKind>,
    /// 客户或供应商；`None` 表示不筛选。
    pub party_id: Option<PartyId>,
    /// 发票号码模糊匹配；`None` 表示不筛选。
    pub invoice_no: Option<String>,
    /// 发票状态；`None` 表示不筛选。
    pub status: Option<InvoiceStatus>,
    /// 账户关联作用域；为空时不触发关联扫描。
    pub scope: ReceivableListScope,
    /// 页码（1 起）。
    pub page: u64,
    /// 单页条数。
    pub page_size: u32,
    /// 排序字段（白名单内有效，默认 `created_at`）。
    pub sort_by: Option<String>,
    /// 是否升序；`false` 表示降序（默认）。
    pub sort_ascending: bool,
}

impl<'a> ReceivableRepository<'a> {
    /// 建立应收往来子账与原始应收分录（跨集合多步骤写入）。
    ///
    /// 依次写入 `receivable_accounts` 与 `receivable_entries`，保证「子账 + 原始
    /// 应收」原子可见（数据模型 §6.8 销售单生效后才形成原始应收）。
    /// **必须收到事务执行器**：本方法不构成原子边界，传入 `NoTransaction` 时
    /// 两笔写入各自自动提交，第二笔失败会留下只有子账没有分录的半成品；
    /// Service 必须通过 `persistence_core::Transactional::with_transaction` 传入事务会话。
    ///
    /// # 参数
    /// * `account` - 待写入的应收往来子账
    /// * `entry` - 待写入的原始应收分录
    /// * `executor` - 数据访问执行器，必须位于事务中
    ///
    /// # 错误
    /// 当唯一索引冲突（透出 [`persistence_core::Error::DuplicateKey`]，由 Service 映射
    /// 为冲突语义）或 MongoDB 写入失败时返回错误。
    pub async fn create_receivable_with_entry(
        &self,
        account: &ReceivableAccount,
        entry: &ReceivableEntry,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        mongo_ops::insert_one(
            &self
                .db
                .collection::<ReceivableAccount>(<mongodb::Database as ReceivableExt>::RECEIVABLE_ACCOUNTS),
            account,
            executor,
        )
        .await?;
        mongo_ops::insert_one(
            &self.db.collection::<ReceivableEntry>(RECEIVABLE_ENTRIES),
            entry,
            executor,
        )
        .await?;
        Ok(())
    }

    /// 追加卡券票款正式复核（复核链尾锁定，跨集合读后写）。
    ///
    /// 复核链按数据模型 §6.8 逐号递增：`review_no = 1` 必须是链头（此前无任何
    /// 复核），`review_no > 1` 必须引用当前链尾且复核号连续。方法先读当前链尾
    /// （同子账最大 `review_no`）再插入新记录；链尾已被其他并发复核占用时
    /// 返回 [`persistence_core::Error::OptimisticLockingError`]（链尾锁定失败），
    /// 并发写同号复核由 `uk_receivable_funds_reviews_account_no` 唯一索引兜底。
    /// **必须收到事务执行器**：读后写构成两步骤，传入 `NoTransaction` 时
    /// 链尾判定与插入各自自动提交，并发场景下可能读出旧链尾后插入失败留下
    /// 半个复核；Service 必须传入事务会话。
    ///
    /// # 参数
    /// * `review` - 待写入的复核记录（含链尾引用）
    /// * `executor` - 数据访问执行器，必须位于事务中
    ///
    /// # 错误
    /// 链尾不匹配时返回 [`persistence_core::Error::OptimisticLockingError`]；
    /// 复核号重复时返回 [`persistence_core::Error::DuplicateKey`]。
    pub async fn append_funds_review(
        &self,
        review: &ReceivableFundsReview,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        let collection = self
            .db
            .collection::<ReceivableFundsReview>(RECEIVABLE_FUNDS_REVIEWS);
        let options = FindOptions::builder()
            .sort(doc! { "review_no": -1 })
            .limit(1)
            .build();
        let mut tail = mongo_ops::find_many(
            &collection,
            doc! { "receivable_account_id": review.receivable_account_id.to_string() },
            options,
            executor,
        )
        .await?;
        let tail = tail.pop();

        let chain_locked = match (&tail, review.review_no) {
            (None, 1) => true,
            (Some(tail), no) if no > 1 => {
                review.supersedes_review_id.as_ref().map(ToString::to_string) == Some(tail.base.id.clone())
                    && tail.review_no + 1 == no
            }
            _ => false,
        };
        if !chain_locked {
            return Err(persistence_core::Error::OptimisticLockingError);
        }
        mongo_ops::insert_one(&collection, review, executor).await
    }

    /// 批量创建销项发票分配（`insert_many`，调用方事务内原子写入，FIN-R10）。
    ///
    /// 唯一键冲突，由 Service 转译并整体回滚。
    /// **必须收到事务执行器**：本方法不构成原子边界，Service 必须通过
    /// `persistence_core::Transactional::with_transaction` 传入事务会话。
    ///
    /// # 参数
    /// * `allocations` - 待持久化的销项发票分配
    /// * `executor` - 数据访问执行器，必须位于事务中
    ///
    /// # 返回
    /// 全部写入成功返回 `Ok(())`。
    ///
    /// # 错误
    /// 当唯一索引冲突（透出 [`persistence_core::Error::DuplicateKey`]，由 Service 映射
    /// 为冲突语义）或 MongoDB 写入失败时返回错误。
    pub async fn create_sales_invoice_allocations_many(
        &self,
        allocations: &[SalesInvoiceAllocation],
        executor: &mut dyn Executor,
    ) -> Result<()> {
        mongo_ops::insert_many(
            &self.db.collection::<SalesInvoiceAllocation>(
                <mongodb::Database as ReceivableExt>::SALES_INVOICE_ALLOCATIONS,
            ),
            allocations.to_vec(),
            executor,
        )
        .await
    }

    /// 作用域合并的客户回款单分页搜索（FIN-R08）。
    ///
    /// 把 account scope（销售单/子账交集）经“账户→分录→核销分配→回款”的
    /// join-equivalent 批量关联直接表达并合并进分页搜索：无 scope 时不触发
    /// 关联扫描；有 scope 时中间 ID 只在仓储内流转，空集短路不再发起后续
    /// `$in` 查询。排序/投影/total 口径与 `search_customer_receipts` 一致，
    /// Service 只做 view 映射。
    ///
    /// # 参数
    /// * `query` - 基础筛选、作用域与分页条件
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 返回当前页投影行与满足筛选条件的总数；作用域交集为空时返回空页。
    ///
    /// # 错误
    /// 当 MongoDB 查询、游标读取或计数失败时返回错误。
    ///
    /// # 约束
    /// 只做事实读取与确定性归组，不开事务、不做跨聚合决定、不返回 services
    /// View；软删除过滤与稳定排序保持不变。外键索引：`uk_receivable_accounts_sales_order`
    ///（销售单前缀）、`idx_receivable_entries_account_due`（账户前缀）、
    /// `idx_receipt_allocations_entry_time`（分录前缀）覆盖三段关联。
    pub async fn search_customer_receipts_in_account_scope(
        &self,
        query: &ScopedCustomerReceiptQuery,
        executor: &mut dyn Executor,
    ) -> Result<PageResult<CustomerReceiptRow>> {
        let receipt_ids = if query.scope.is_empty() {
            None
        } else {
            Some(self.receipt_ids_for_scope(&query.scope, executor).await?)
        };
        let filter = CustomerReceiptFilter {
            receipt_ids,
            receipt_no: query.receipt_no.clone(),
            counterparty_party_id: query.counterparty_party_id.clone(),
            status: query.status,
            page: query.page,
            page_size: query.page_size,
            sort_by: query.sort_by.clone(),
            sort_ascending: query.sort_ascending,
        };
        CustomerReceiptRepository::new(self.db, <mongodb::Database as ReceivableExt>::CUSTOMER_RECEIPTS)
            .search_customer_receipts(&filter, executor)
            .await
    }

    /// 作用域合并的发票分页搜索（FIN-R08）。
    ///
    /// 把 account scope（销售单/子账交集）经“账户→销项分配→发票”的
    /// join-equivalent 批量关联直接表达并合并进分页搜索；空集短路语义、
    /// 排序/投影/total 口径与 `search_invoices` 一致。外键索引：
    /// `uk_receivable_accounts_sales_order`（销售单前缀）、
    /// `idx_sales_invoice_allocations_account`（账户）覆盖两段关联。
    ///
    /// # 参数
    /// * `query` - 基础筛选、作用域与分页条件
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 返回当前页投影行与满足筛选条件的总数；作用域交集为空时返回空页。
    ///
    /// # 错误
    /// 当 MongoDB 查询、游标读取或计数失败时返回错误。
    pub async fn search_invoices_in_account_scope(
        &self,
        query: &ScopedInvoiceQuery,
        executor: &mut dyn Executor,
    ) -> Result<PageResult<InvoiceRow>> {
        let invoice_ids = if query.scope.is_empty() {
            None
        } else {
            Some(self.invoice_ids_for_scope(&query.scope, executor).await?)
        };
        let filter = InvoiceFilter {
            invoice_ids,
            invoice_direction: query.invoice_direction,
            invoice_kind: query.invoice_kind,
            party_id: query.party_id.clone(),
            invoice_no: query.invoice_no.clone(),
            status: query.status,
            page: query.page,
            page_size: query.page_size,
            sort_by: query.sort_by.clone(),
            sort_ascending: query.sort_ascending,
        };
        InvoiceRepository::new(self.db, <mongodb::Database as ReceivableExt>::INVOICES)
            .search_invoices(&filter, executor)
            .await
    }

    /// 解析作用域内出现过核销分配的回款单主键（仓储内中间事实，不外泄）。
    ///
    /// 三段批量关联，任一段为空直接返回空集合，不再发起后续 `$in` 查询；
    /// 结果排序去重，保持确定性。
    async fn receipt_ids_for_scope(
        &self,
        scope: &ReceivableListScope,
        executor: &mut dyn Executor,
    ) -> Result<Vec<String>> {
        let account_ids = self.scoped_account_ids(scope, executor).await?;
        if account_ids.is_empty() {
            return Ok(Vec::new());
        }
        let entries: Vec<ReceivableEntry> =
            ReceivableEntryRepository::new(self.db, <mongodb::Database as ReceivableExt>::RECEIVABLE_ENTRIES)
                .find_many(doc! { "receivable_account_id": { "$in": account_ids } }, executor)
                .await?;
        if entries.is_empty() {
            return Ok(Vec::new());
        }
        let entry_ids = entries
            .iter()
            .map(|entry| entry.base.id.clone())
            .collect::<Vec<_>>();
        let allocations: Vec<ReceiptAllocation> = ReceiptAllocationRepository::new(
            self.db,
            <mongodb::Database as ReceivableExt>::RECEIPT_ALLOCATIONS,
        )
        .find_many(doc! { "receivable_entry_id": { "$in": entry_ids } }, executor)
        .await?;
        let mut receipt_ids = allocations
            .into_iter()
            .map(|allocation| allocation.customer_receipt_id.to_string())
            .collect::<Vec<_>>();
        receipt_ids.sort();
        receipt_ids.dedup();
        Ok(receipt_ids)
    }

    /// 解析作用域内出现过分配的销项发票主键（仓储内中间事实，不外泄）。
    ///
    /// 两段批量关联，任一段为空直接返回空集合；结果排序去重，保持确定性。
    async fn invoice_ids_for_scope(
        &self,
        scope: &ReceivableListScope,
        executor: &mut dyn Executor,
    ) -> Result<Vec<String>> {
        let account_ids = self.scoped_account_ids(scope, executor).await?;
        if account_ids.is_empty() {
            return Ok(Vec::new());
        }
        let allocations: Vec<SalesInvoiceAllocation> = SalesInvoiceAllocationRepository::new(
            self.db,
            <mongodb::Database as ReceivableExt>::SALES_INVOICE_ALLOCATIONS,
        )
        .find_many(doc! { "receivable_account_id": { "$in": account_ids } }, executor)
        .await?;
        let mut invoice_ids = allocations
            .into_iter()
            .map(|allocation| allocation.invoice_id.to_string())
            .collect::<Vec<_>>();
        invoice_ids.sort();
        invoice_ids.dedup();
        Ok(invoice_ids)
    }

    /// 解析列表关联作用域的账户主键（销售单/子账交集，仓储内中间事实）。
    ///
    /// 未指定范围时返回空集合；子账不存在、已软删除或与销售单不一致时返回
    /// 空集合（交集语义），调用方据此返回空页而不触发后续关联扫描。
    async fn scoped_account_ids(
        &self,
        scope: &ReceivableListScope,
        executor: &mut dyn Executor,
    ) -> Result<Vec<String>> {
        let accounts: ReceivableAccountRepository<'_> = ReceivableAccountRepository::new(
            self.db,
            <mongodb::Database as ReceivableExt>::RECEIVABLE_ACCOUNTS,
        );
        if let Some(account_id) = &scope.receivable_account_id {
            let account = accounts.find_by_id(account_id.as_ref(), executor).await?;
            let Some(account) = account else {
                return Ok(Vec::new());
            };
            if let Some(sales_order_id) = &scope.sales_order_id {
                if account.sales_order_id.as_ref() != sales_order_id.as_str() {
                    return Ok(Vec::new());
                }
            }
            return Ok(vec![account.base.id]);
        }
        if let Some(sales_order_id) = &scope.sales_order_id {
            let rows = accounts
                .find_many(doc! { "sales_order_id": sales_order_id }, executor)
                .await?;
            return Ok(rows.into_iter().map(|row| row.base.id).collect());
        }
        Ok(Vec::new())
    }
}

#[cfg(test)]
mod tests {
    use super::ReceivableListScope;
    use erp_core::ids::ReceivableAccountId;

    #[test]
    fn list_scope_empty_only_when_both_dimensions_absent() {
        assert!(ReceivableListScope::default().is_empty());
        assert!(!ReceivableListScope {
            sales_order_id: Some("so-1".to_string()),
            receivable_account_id: None,
        }
        .is_empty());
        assert!(!ReceivableListScope {
            sales_order_id: None,
            receivable_account_id: Some(ReceivableAccountId::new("acct-1")),
        }
        .is_empty());
    }
}
