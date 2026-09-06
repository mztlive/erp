use entities::payable::{PayableAccount, PayableSourceType};
use erp_core::ids::{PayableAccountId, PurchaseOrderId};
use mongodb::bson::{doc, Document};
use mongodb::options::FindOptions;

use super::super::super::{PageResult, Pagination, QueryFilter, Repository};
use super::super::sort_doc;
use super::{PayableAccountFilter, PayableAccountRow};
use persistence_core::Executor;
use persistence_core::{mongo_ops, Result};

impl<'a> Repository<'a, PayableAccount> {
    /// 分页检索应付往来子账列表（投影查询）。
    ///
    /// 只返回 [`PayableAccountRow`] 所需的列表字段，不加载整文档；
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
    pub async fn search_payable_accounts(
        &self,
        filter: &PayableAccountFilter,
        executor: &mut dyn Executor,
    ) -> Result<PageResult<PayableAccountRow>> {
        let options = FindOptions::builder()
            .sort(sort_doc(
                filter.sort_by.as_deref(),
                filter.sort_ascending,
                &[
                    "gross_total",
                    "settled_total",
                    "open_total",
                    "open_invoiceable_total",
                    "created_at",
                ],
            ))
            .skip(filter.skip())
            .limit(filter.limit())
            .projection(payable_account_projection())
            .build();
        let collection = self.collection().clone_with_type::<PayableAccountRow>();
        let items = mongo_ops::find_many(&collection, filter.to_doc(), options, executor).await?;
        let total = mongo_ops::count_documents(&self.collection(), filter.to_doc(), executor).await?;

        Ok(PageResult {
            items,
            total: total as i64,
        })
    }

    /// 按主键集合批量取回应付子账（`$in` 一次取回，禁止 N+1）。
    ///
    /// # 参数
    /// * `account_ids` - 应付往来子账 ID 集合
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 返回全部匹配子账；空集合直接返回空列表。
    ///
    /// # 错误
    /// 当 MongoDB 查询或游标读取失败时返回错误。
    pub async fn find_accounts_by_ids(
        &self,
        account_ids: &[PayableAccountId],
        executor: &mut dyn Executor,
    ) -> Result<Vec<PayableAccount>> {
        if account_ids.is_empty() {
            return Ok(Vec::new());
        }
        let ids: Vec<String> = account_ids.iter().map(ToString::to_string).collect();
        self.find_many(doc! { "id": { "$in": ids } }, executor).await
    }

    /// 按采购单查询其来源的应付往来子账。
    ///
    /// # 参数
    /// * `purchase_order_id` - 采购单稳定身份
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 返回来源单据为该采购单的应付子账；尚未形成时返回 `None`。
    ///
    /// # 错误
    /// 当 MongoDB 查询失败时返回错误。
    ///
    /// # 约束
    /// 未删除过滤由基类 `find_one` 统一追加；来源类型固定为采购单。
    pub async fn find_by_purchase_order(
        &self,
        purchase_order_id: &PurchaseOrderId,
        executor: &mut dyn Executor,
    ) -> Result<Option<PayableAccount>> {
        self.find_one(
            doc! {
                "source_document_id": purchase_order_id.to_string(),
                "source_type": PayableSourceType::PurchaseOrder.as_str(),
            },
            executor,
        )
        .await
    }
}

/// 应付往来子账列表投影字段。
///
/// # 返回
/// 返回投影条件文档。
fn payable_account_projection() -> Document {
    doc! {
        "id": 1,
        "status": 1,
        "current_revision_id": 1,
        "created_by": 1,
        "updated_by": 1,
        "source_document_id": 1,
        "supplier_id": 1,
        "source_type": 1,
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
