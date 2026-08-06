//! `purchase_order` 采购主表仓储：列表投影查询与按采购单号身份查询。

use entities::ids::{PurchaseOrderId, SalesOrderId, SupplierAccountId};
use entities::purchase_order::{
    ProgressStatus, PurchaseOrder, PurchaseOrderStatus, PurchaseReviewStatus, PurchaseType,
};
use entity_core::NOT_DELETED_TIMESTAMP_BSON;
use mongodb::bson::{doc, Document};
use mongodb::options::FindOptions;
use serde::{Deserialize, Serialize};

use super::common::{sort_doc, PURCHASE_ORDER_SORT_FIELDS};
use crate::executor::Executor;
use crate::repository::regex_filter::insert_literal_regex_filter;
use crate::repository::{PageResult, Pagination, QueryFilter};
use crate::{mongo_ops, Repository, Result};

/// 采购单列表投影行（列表接口只取必要字段，禁止返回整文档）。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PurchaseOrderRow {
    /// 实体主键。
    pub id: String,
    /// 采购单号。
    pub purchase_no: String,
    /// 来源实物及服务销售单。
    pub sales_order_id: SalesOrderId,
    /// 唯一供应商。
    pub supplier_id: SupplierAccountId,
    /// 采购类型。
    pub purchase_type: PurchaseType,
    /// 主状态。
    pub status: PurchaseOrderStatus,
    /// 财务审核状态。
    pub review_status: PurchaseReviewStatus,
    /// 付款进度。
    pub payment_progress: ProgressStatus,
    /// 收票进度。
    pub invoice_progress: ProgressStatus,
    /// 履约进度。
    pub fulfillment_progress: ProgressStatus,
    /// 当前待财务审核的不可变提交。
    pub current_submission_id: Option<String>,
    /// 当前生效版本。
    pub current_revision_id: Option<String>,
    /// 乐观锁版本（`BaseModel.version` ≡ 数据模型 `lock_version`）。
    pub version: u64,
    /// 创建时间（秒级时间戳）。
    pub created_at: u64,
}

/// 采购单列表筛选条件。
#[derive(Debug, Clone)]
pub struct PurchaseOrderFilter {
    /// 采购单号模糊匹配（字面量、忽略大小写）；`None` 表示不筛选。
    pub purchase_no: Option<String>,
    /// 来源销售单；`None` 表示不筛选。
    pub sales_order_id: Option<SalesOrderId>,
    /// 供应商；`None` 表示不筛选。
    pub supplier_id: Option<SupplierAccountId>,
    /// 主状态；`None` 表示不筛选。
    pub status: Option<PurchaseOrderStatus>,
    /// 页码（1 起）。
    pub page: u64,
    /// 单页条数。
    pub page_size: u32,
    /// 排序字段（白名单内取值，默认 `created_at`）。
    pub sort_by: Option<String>,
    /// 是否升序；`false` 表示降序（默认）。
    pub sort_ascending: bool,
}

impl QueryFilter for PurchaseOrderFilter {
    /// 转换为 MongoDB 查询条件（自动追加未删除过滤）。
    ///
    /// # 返回
    /// 返回查询条件文档。
    fn to_doc(&self) -> Document {
        let mut filter = doc! { "deleted_at": NOT_DELETED_TIMESTAMP_BSON };
        insert_literal_regex_filter(&mut filter, "purchase_no", self.purchase_no.as_deref());
        if let Some(sales_order_id) = &self.sales_order_id {
            filter.insert("sales_order_id", sales_order_id.to_string());
        }
        if let Some(supplier_id) = &self.supplier_id {
            filter.insert("supplier_id", supplier_id.to_string());
        }
        if let Some(status) = self.status {
            filter.insert("status", status.as_str());
        }
        filter
    }
}

impl Pagination for PurchaseOrderFilter {
    /// 返回页码与单页条数。
    ///
    /// # 返回
    /// 返回 `(page, page_size)` 元组。
    fn page_and_size(&self) -> (u64, u64) {
        (self.page, u64::from(self.page_size))
    }
}

impl<'a> Repository<'a, PurchaseOrder> {
    /// 分页检索采购单列表（投影查询）。
    ///
    /// 只返回 [`PurchaseOrderRow`] 所需的列表字段，不加载整文档；排序字段
    /// 经白名单校验（`created_at`/`purchase_no`/`status`），未知字段回退默认值。
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
    pub async fn search_purchase_orders(
        &self,
        filter: &PurchaseOrderFilter,
        executor: &mut dyn Executor,
    ) -> Result<PageResult<PurchaseOrderRow>> {
        let options = FindOptions::builder()
            .sort(sort_doc(
                filter.sort_by.as_deref(),
                PURCHASE_ORDER_SORT_FIELDS,
                filter.sort_ascending,
            ))
            .skip(filter.skip())
            .limit(filter.limit())
            .projection(purchase_order_projection())
            .build();
        let collection = self.collection().clone_with_type::<PurchaseOrderRow>();
        let items = mongo_ops::find_many(&collection, filter.to_doc(), options, executor).await?;
        let total = mongo_ops::count_documents(&self.collection(), filter.to_doc(), executor).await?;

        Ok(PageResult {
            items,
            total: total as i64,
        })
    }

    /// 按采购单号查找唯一采购单（身份查询）。
    ///
    /// 唯一性由 `uk_purchase_orders_purchase_no` 唯一索引保证；本方法用于
    /// 单号查重与详情取回，服务层不得做「先查后插」的重复性判断。
    ///
    /// # 参数
    /// * `purchase_no` - 采购单号
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 返回匹配的未删除采购单；无匹配时返回 `None`。
    ///
    /// # 错误
    /// 当 MongoDB 查询失败时返回错误。
    pub async fn find_by_purchase_no(
        &self,
        purchase_no: &str,
        executor: &mut dyn Executor,
    ) -> Result<Option<PurchaseOrder>> {
        self.find_one(doc! { "purchase_no": purchase_no }, executor).await
    }

    /// 批量取回指定采购单（`$in`，禁止 N+1）。
    ///
    /// # 参数
    /// * `ids` - 采购单 ID 集合（空集合直接返回空结果）
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 返回匹配的未删除采购单集合。
    ///
    /// # 错误
    /// 当 MongoDB 查询或游标读取失败时返回错误。
    pub async fn find_by_ids(
        &self,
        ids: &[PurchaseOrderId],
        executor: &mut dyn Executor,
    ) -> Result<Vec<PurchaseOrder>> {
        if ids.is_empty() {
            return Ok(Vec::new());
        }
        let filter = super::common::in_filter("id", ids.iter().map(|id| id.to_string()));
        self.find_many(filter, executor).await
    }
}

/// 采购单列表投影字段。
///
/// # 返回
/// 返回投影条件文档。
fn purchase_order_projection() -> Document {
    doc! {
        "id": 1,
        "purchase_no": 1,
        "sales_order_id": 1,
        "supplier_id": 1,
        "purchase_type": 1,
        "status": 1,
        "review_status": 1,
        "payment_progress": 1,
        "invoice_progress": 1,
        "fulfillment_progress": 1,
        "current_submission_id": 1,
        "current_revision_id": 1,
        "version": 1,
        "created_at": 1,
    }
}

#[cfg(test)]
mod tests {
    use super::{PurchaseOrderFilter, QueryFilter};
    use entities::ids::SupplierAccountId;
    use entities::purchase_order::PurchaseOrderStatus;
    use mongodb::bson::doc;

    #[test]
    fn filter_applies_optional_fields_and_deleted_filter() {
        let filter = PurchaseOrderFilter {
            purchase_no: Some("PO-2026".to_string()),
            sales_order_id: None,
            supplier_id: Some(SupplierAccountId::new("sup-1")),
            status: Some(PurchaseOrderStatus::PendingFinanceReview),
            page: 1,
            page_size: 20,
            sort_by: None,
            sort_ascending: false,
        };

        let document = filter.to_doc();
        assert_eq!(document.get_i64("deleted_at").unwrap(), 0);
        assert_eq!(
            document.get_str("supplier_id").unwrap(),
            "sup-1",
            "类型化 ID 必须以字符串形态写入过滤条件"
        );
        assert_eq!(document.get_str("status").unwrap(), "PENDING_FINANCE_REVIEW");
        let regex = document.get_document("purchase_no").unwrap();
        assert_eq!(
            regex.get_str("$regex").unwrap(),
            "PO\\-2026",
            "正则必须转义字面量"
        );
        assert_eq!(regex.get_str("$options").unwrap(), "i");
    }

    #[test]
    fn filter_omits_absent_fields() {
        let filter = PurchaseOrderFilter {
            purchase_no: None,
            sales_order_id: None,
            supplier_id: None,
            status: None,
            page: 1,
            page_size: 20,
            sort_by: None,
            sort_ascending: false,
        };

        assert_eq!(filter.to_doc(), doc! { "deleted_at": 0i64 });
    }
}
