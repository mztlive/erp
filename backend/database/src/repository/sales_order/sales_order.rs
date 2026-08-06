//! `sales_order` 与 `sales_order_line` 仓储：主单列表查询、稳定明细维护。

use entities::sales_order::{
    BusinessType, CommercialStatus, ReviewStatus, SalesOrder, SalesOrderId, SalesOrderLine,
};
use entity_core::NOT_DELETED_TIMESTAMP_BSON;
use mongodb::bson::{doc, Document};
use mongodb::options::FindOptions;
use serde::{Deserialize, Serialize};

use super::super::regex_filter::insert_literal_regex_filter;
use super::super::{PageResult, Pagination, QueryFilter, Repository};
use super::{sort_doc, SalesOrderRepository, SALES_ORDER_LINES};
use crate::executor::Executor;
use crate::{mongo_ops, Result};

/// 销售单列表投影行（列表接口只取必要字段，禁止返回整文档）。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SalesOrderRow {
    /// 实体主键。
    pub id: String,
    /// 销售单号。
    pub order_no: String,
    /// 业务性质。
    pub business_type: BusinessType,
    /// 最初创建入口。
    pub origin_system: entities::sales_order::OriginSystem,
    /// 客户稳定身份。
    pub customer_id: String,
    /// 合同稳定身份。
    pub contract_id: Option<String>,
    /// 商业主状态。
    pub commercial_status: CommercialStatus,
    /// 审核轨状态。
    pub review_status: ReviewStatus,
    /// 履约进度。
    pub fulfillment_progress: entities::sales_order::FulfillmentProgress,
    /// 回款进度。
    pub collection_progress: entities::sales_order::CollectionProgress,
    /// 开票进度。
    pub invoice_progress: entities::sales_order::InvoiceProgress,
    /// 关闭状态。
    pub close_status: entities::sales_order::CloseStatus,
    /// 生效时间。
    pub effective_at: Option<u64>,
    /// ERP 关闭时间。
    pub closed_at: Option<u64>,
    /// 乐观锁版本。
    pub version: u64,
    /// 创建时间（秒级时间戳）。
    pub created_at: u64,
    /// 更新时间（秒级时间戳）。
    pub updated_at: u64,
}

/// 销售单列表筛选条件。
#[derive(Debug, Clone)]
pub struct SalesOrderFilter {
    /// 销售单号（字面量正则，忽略大小写）；`None` 表示不筛选。
    pub order_no: Option<String>,
    /// 客户；`None` 表示不筛选。
    pub customer_id: Option<String>,
    /// 商业主状态；`None` 表示不筛选。
    pub commercial_status: Option<CommercialStatus>,
    /// 审核轨状态；`None` 表示不筛选。
    pub review_status: Option<ReviewStatus>,
    /// 业务性质；`None` 表示不筛选。
    pub business_type: Option<BusinessType>,
    /// 页码（1 起）。
    pub page: u64,
    /// 单页条数。
    pub page_size: u32,
    /// 排序字段（Service 层白名单校验后传入，默认 `created_at`）。
    pub sort_by: Option<String>,
    /// 是否升序；`false` 表示降序（默认）。
    pub sort_ascending: bool,
}

impl QueryFilter for SalesOrderFilter {
    /// 转换为 MongoDB 查询条件（自动追加未删除过滤）。
    ///
    /// # 返回
    /// 返回查询条件文档。
    fn to_doc(&self) -> Document {
        let mut filter = doc! { "deleted_at": NOT_DELETED_TIMESTAMP_BSON };
        insert_literal_regex_filter(&mut filter, "order_no", self.order_no.as_deref());
        if let Some(customer_id) = &self.customer_id {
            filter.insert("customer_id", customer_id);
        }
        if let Some(status) = self.commercial_status {
            filter.insert("commercial_status", status.as_str());
        }
        if let Some(status) = self.review_status {
            filter.insert("review_status", status.as_str());
        }
        if let Some(business_type) = self.business_type {
            filter.insert("business_type", business_type.as_str());
        }
        filter
    }
}

impl Pagination for SalesOrderFilter {
    /// 返回页码与单页条数。
    ///
    /// # 返回
    /// 返回 `(page, page_size)` 元组。
    fn page_and_size(&self) -> (u64, u64) {
        (self.page, u64::from(self.page_size))
    }
}

impl<'a> Repository<'a, SalesOrder> {
    /// 分页检索销售单列表（投影查询）。
    ///
    /// 只返回 [`SalesOrderRow`] 所需的列表字段，不加载整文档；排序字段由
    /// Service 层白名单校验后传入（api-contract §4）。
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
    pub async fn search_sales_orders(
        &self,
        filter: &SalesOrderFilter,
        executor: &mut dyn Executor,
    ) -> Result<PageResult<SalesOrderRow>> {
        let options = FindOptions::builder()
            .sort(sort_doc(filter.sort_by.as_deref(), filter.sort_ascending))
            .skip(filter.skip())
            .limit(filter.limit())
            .projection(sales_order_projection())
            .build();
        let collection = self.collection().clone_with_type::<SalesOrderRow>();
        let items = mongo_ops::find_many(&collection, filter.to_doc(), options, executor).await?;
        let total = mongo_ops::count_documents(&self.collection(), filter.to_doc(), executor).await?;

        Ok(PageResult {
            items,
            total: total as i64,
        })
    }

    /// 按销售单号查找销售单。
    ///
    /// 唯一性由 `uk_sales_orders_order_no` 唯一索引保证（软删除后单号不复用）。
    ///
    /// # 参数
    /// * `order_no` - 销售单号
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 返回匹配的未删除销售单；无匹配时返回 `None`。
    ///
    /// # 错误
    /// 当 MongoDB 查询失败时返回错误。
    pub async fn find_by_order_no(
        &self,
        order_no: &str,
        executor: &mut dyn Executor,
    ) -> Result<Option<SalesOrder>> {
        self.find_one_by_field("order_no", order_no, executor).await
    }
}

impl<'a> Repository<'a, SalesOrderLine> {
    /// 列出销售单的全部稳定明细行（按行号升序）。
    ///
    /// # 参数
    /// * `sales_order_id` - 所属销售单
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 返回按行号升序的明细行列表。
    ///
    /// # 错误
    /// 当 MongoDB 查询或游标读取失败时返回错误。
    pub async fn list_lines_by_order(
        &self,
        sales_order_id: &SalesOrderId,
        executor: &mut dyn Executor,
    ) -> Result<Vec<SalesOrderLine>> {
        self.find_many_sorted(
            doc! { "sales_order_id": sales_order_id.to_string() },
            doc! { "line_no": 1 },
            executor,
        )
        .await
    }
}

impl<'a> SalesOrderRepository<'a> {
    /// 批量替换销售单稳定明细行（先删后写）。
    ///
    /// **必须收到事务执行器**：本方法先按销售单删除全部明细再写入新明细，
    /// 不构成原子边界，传入 `NoTransaction` 时中途失败会留下部分写入的明细；
    /// Service 必须通过 `database::Transactional::with_transaction` 传入事务会话。
    ///
    /// # 参数
    /// * `sales_order_id` - 所属销售单
    /// * `lines` - 目标稳定明细行（含被移除行，历史行号不复用）
    /// * `executor` - 数据访问执行器，必须位于事务中
    ///
    /// # 错误
    /// 当唯一索引冲突（透出 [`crate::Error::DuplicateKey`]）或 MongoDB 写入失败
    /// 时返回错误。
    pub async fn replace_sales_order_lines(
        &self,
        sales_order_id: &SalesOrderId,
        lines: &[SalesOrderLine],
        executor: &mut dyn Executor,
    ) -> Result<()> {
        let collection = self.db.collection::<SalesOrderLine>(SALES_ORDER_LINES);
        mongo_ops::delete_many(
            &collection,
            doc! { "sales_order_id": sales_order_id.to_string() },
            executor,
        )
        .await?;
        mongo_ops::insert_many(&collection, lines.to_vec(), executor).await
    }
}

/// 销售单列表投影字段。
///
/// # 返回
/// 返回投影条件文档。
fn sales_order_projection() -> Document {
    doc! {
        "id": 1,
        "order_no": 1,
        "business_type": 1,
        "origin_system": 1,
        "customer_id": 1,
        "contract_id": 1,
        "commercial_status": 1,
        "review_status": 1,
        "fulfillment_progress": 1,
        "collection_progress": 1,
        "invoice_progress": 1,
        "close_status": 1,
        "effective_at": 1,
        "closed_at": 1,
        "version": 1,
        "created_at": 1,
        "updated_at": 1,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sales_order_filter_applies_optional_fields_and_deleted_filter() {
        let filter = SalesOrderFilter {
            order_no: Some("SO-2026".to_string()),
            customer_id: Some("cust-1".to_string()),
            commercial_status: Some(CommercialStatus::PendingReview),
            review_status: Some(ReviewStatus::PendingOperations),
            business_type: Some(BusinessType::Voucher),
            page: 1,
            page_size: 20,
            sort_by: None,
            sort_ascending: false,
        };

        let document = filter.to_doc();
        assert_eq!(document.get_i64("deleted_at").unwrap(), 0);
        assert_eq!(
            document
                .get_document("order_no")
                .unwrap()
                .get_str("$regex")
                .unwrap(),
            r"SO\-2026"
        );
        assert_eq!(document.get_str("customer_id").unwrap(), "cust-1");
        assert_eq!(document.get_str("commercial_status").unwrap(), "PENDING_REVIEW");
        assert_eq!(document.get_str("review_status").unwrap(), "PENDING_OPERATIONS");
        assert_eq!(document.get_str("business_type").unwrap(), "VOUCHER");
    }

    #[test]
    fn sales_order_filter_escapes_regex_metacharacters() {
        let filter = SalesOrderFilter {
            order_no: Some("SO-2026.[x]".to_string()),
            customer_id: None,
            commercial_status: None,
            review_status: None,
            business_type: None,
            page: 1,
            page_size: 20,
            sort_by: None,
            sort_ascending: false,
        };

        let document = filter.to_doc();
        assert_eq!(
            document
                .get_document("order_no")
                .unwrap()
                .get_str("$regex")
                .unwrap(),
            r"SO\-2026\.\[x\]"
        );
    }
}
