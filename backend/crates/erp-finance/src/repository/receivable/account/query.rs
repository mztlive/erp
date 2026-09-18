use erp_core::ids::{SalesOrderId, SalesOrderRevisionId};
use mongodb::bson::{Document, doc};
use mongodb::options::FindOptions;
use persistence_core::{Executor, PageResult, Pagination, QueryFilter, Result, mongo_ops};

use super::super::sort_doc;
use super::{ReceivableAccountFilter, ReceivableAccountRow};
use crate::entity::receivable::ReceivableAccount;

#[allow(async_fn_in_trait)]
pub trait ReceivableAccountRepositoryExt {
    /// 分页检索应收往来子账列表（投影查询）。
    ///
    /// 只返回 [`ReceivableAccountRow`] 所需的列表字段，不加载整文档；
    /// 排序字段经白名单映射，未命中回退 `created_at` 降序。
    ///
    /// # 参数
    /// * `filter` - 筛选与分页条件
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 返回当前页投影行与满足筛选条件的总数。
    ///
    /// # 错误
    /// 当 MongoDB 查询、游标读取或计数失败时返回错误。
    async fn search_receivable_accounts(
        &self,
        filter: &ReceivableAccountFilter,
        executor: &mut dyn Executor,
    ) -> Result<PageResult<ReceivableAccountRow>>;

    /// 按销售单读取全部活跃应收子账，供服务端关联列表投影使用。
    async fn find_accounts_by_sales_order_id(
        &self,
        sales_order_id: &str,
        executor: &mut dyn Executor,
    ) -> Result<Vec<ReceivableAccount>>;

    /// 批量按应收子账 ID 读取活跃账户。
    ///
    /// # 参数
    /// * `account_ids` - 应收子账 ID 字符串集合；空集合直接返回空结果
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 返回全部匹配且未删除的应收子账；返回顺序不承诺与输入一致。
    ///
    /// # 错误
    /// 当 MongoDB 查询或游标读取失败时返回错误。
    async fn find_accounts_by_ids(
        &self,
        account_ids: &[String],
        executor: &mut dyn Executor,
    ) -> Result<Vec<ReceivableAccount>>;

    /// 列出销售单的全部应收子账。
    ///
    /// # 参数
    /// * `sales_order_id` - 来源销售单
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 返回按子账序号升序排列的应收子账。
    ///
    /// # 错误
    /// 当 MongoDB 查询或游标读取失败时返回错误。
    ///
    /// # 约束
    /// 仅查询本仓储拥有的 `receivable_accounts` 集合，不访问销售单集合。
    async fn list_by_sales_order(
        &self,
        sales_order_id: &SalesOrderId,
        executor: &mut dyn Executor,
    ) -> Result<Vec<ReceivableAccount>>;

    /// 查找销售单的首个应收子账。
    ///
    /// # 参数
    /// * `sales_order_id` - 来源销售单
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 返回 `account_seq = 1` 的应收子账；尚未形成时返回 `None`。
    ///
    /// # 错误
    /// 当 MongoDB 查询失败时返回错误。
    ///
    /// # 约束
    /// 仅查询本仓储拥有的 `receivable_accounts` 集合，不访问销售单集合。
    async fn find_primary_by_sales_order(
        &self,
        sales_order_id: &SalesOrderId,
        executor: &mut dyn Executor,
    ) -> Result<Option<ReceivableAccount>>;

    /// 按稳定 ID 读取销项开票任务使用的应收子账。
    ///
    /// 工作项入口的历史名称；纯主键读取，直接委托基类单条查询。
    ///
    /// # 参数
    /// * `id` - 应收子账 ID
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 返回未删除应收子账；不存在时返回 `None`。
    ///
    /// # 错误
    /// 当 MongoDB 查询或反序列化失败时返回错误。
    ///
    /// # 约束
    /// 仅查询本仓储拥有的 `receivable_accounts` 集合，不访问销售单集合。
    async fn find_work_item_receivable_account(
        &self,
        id: &str,
        executor: &mut dyn Executor,
    ) -> Result<Option<ReceivableAccount>>;

    /// 按来源销售版本查找应收结果。
    ///
    /// # 参数
    /// * `revision_id` - 来源销售单修订 ID
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 返回引用该销售版本的应收账户；无匹配时返回 `None`。
    ///
    /// # 错误
    /// 当 MongoDB 查询失败时返回错误。
    ///
    /// # 约束
    /// 仅查询本仓储拥有的 `receivable_accounts` 集合，按来源版本引用过滤，不访问销售单集合。
    async fn find_by_source_sales_order_revision(
        &self,
        revision_id: &SalesOrderRevisionId,
        executor: &mut dyn Executor,
    ) -> Result<Option<ReceivableAccount>>;
}

impl ReceivableAccountRepositoryExt for persistence_core::Repository<'_, ReceivableAccount> {
    async fn search_receivable_accounts(
        &self,
        filter: &ReceivableAccountFilter,
        executor: &mut dyn Executor,
    ) -> Result<PageResult<ReceivableAccountRow>> {
        let options = FindOptions::builder()
            .sort(sort_doc(
                filter.sort_by.as_deref(),
                filter.sort_ascending,
                &[
                    "account_seq",
                    "gross_total",
                    "settled_total",
                    "open_total",
                    "open_invoiceable_total",
                    "created_at",
                ],
            ))
            .skip(filter.skip())
            .limit(filter.limit())
            .projection(receivable_account_projection())
            .build();
        let collection = self.collection().clone_with_type::<ReceivableAccountRow>();
        let items = mongo_ops::find_many(&collection, filter.to_doc(), options, executor).await?;
        let total = mongo_ops::count_documents(&self.collection(), filter.to_doc(), executor).await?;

        Ok(PageResult { items, total: total as i64 })
    }

    async fn find_accounts_by_sales_order_id(
        &self,
        sales_order_id: &str,
        executor: &mut dyn Executor,
    ) -> Result<Vec<ReceivableAccount>> {
        self.find_many(doc! { "sales_order_id": sales_order_id }, executor).await
    }

    async fn find_accounts_by_ids(
        &self,
        account_ids: &[String],
        executor: &mut dyn Executor,
    ) -> Result<Vec<ReceivableAccount>> {
        if account_ids.is_empty() {
            return Ok(Vec::new());
        }

        self.find_many(doc! { "id": { "$in": account_ids } }, executor).await
    }

    async fn list_by_sales_order(
        &self,
        sales_order_id: &SalesOrderId,
        executor: &mut dyn Executor,
    ) -> Result<Vec<ReceivableAccount>> {
        self.find_many_sorted(
            doc! { "sales_order_id": sales_order_id.to_string() },
            doc! { "account_seq": 1 },
            executor,
        )
        .await
    }

    async fn find_primary_by_sales_order(
        &self,
        sales_order_id: &SalesOrderId,
        executor: &mut dyn Executor,
    ) -> Result<Option<ReceivableAccount>> {
        self.find_one(
            doc! {
                "sales_order_id": sales_order_id.to_string(),
                "account_seq": 1,
            },
            executor,
        )
        .await
    }

    async fn find_work_item_receivable_account(
        &self,
        id: &str,
        executor: &mut dyn Executor,
    ) -> Result<Option<ReceivableAccount>> {
        self.find_by_id(id, executor).await
    }

    async fn find_by_source_sales_order_revision(
        &self,
        revision_id: &SalesOrderRevisionId,
        executor: &mut dyn Executor,
    ) -> Result<Option<ReceivableAccount>> {
        self.find_one(doc! { "source_sales_order_revision_id": revision_id.to_string() }, executor).await
    }
}

/// 应收往来子账列表投影字段。
///
/// # 返回
/// 返回投影条件文档。
fn receivable_account_projection() -> Document {
    doc! {
        "id": 1,
        "status": 1,
        "current_revision_id": 1,
        "created_by": 1,
        "updated_by": 1,
        "sales_order_id": 1,
        "account_seq": 1,
        "customer_id": 1,
        "counterparty_party_id": 1,
        "gross_total": 1,
        "settled_total": 1,
        "open_total": 1,
        "invoiceable_total": 1,
        "invoiced_total": 1,
        "open_invoiceable_total": 1,
        "version": 1,
        "created_at": 1,
    }
}
