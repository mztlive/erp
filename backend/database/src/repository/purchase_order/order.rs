//! `purchase_order` 采购主表仓储：列表投影查询与按采购单号身份查询。

use entities::ids::{SalesOrderId, SkuId, SupplierAccountId};
use entities::payable::{PayableAccount, PayableSourceType};
use entities::purchase_order::{
    ProgressStatus, PurchaseOrder, PurchaseOrderStatus, PurchaseReviewStatus, PurchaseType,
};
use entities::sales_order::{CommercialStatus, SalesOrder};
use entities::supplier_offering::{OfferingStatus, SupplierOffering};
use entities::warehouse::{EnableStatus, Warehouse};
use entities::work_item::{WorkItem, WorkItemStatus};
use entity_core::NOT_DELETED_TIMESTAMP_BSON;
use mongodb::bson::{doc, Document};
use mongodb::options::FindOptions;
use serde::{Deserialize, Serialize};

use super::common::{in_filter, sort_doc, PURCHASE_ORDER_SORT_FIELDS};
use super::PurchaseOrderRepository;
use crate::executor::Executor;
use crate::repository::extensions::{
    PayableExt, SalesOrderExt, SupplierOfferingExt, WarehouseExt, WorkItemExt,
};
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
    /// 付款条件代码。
    pub payment_term_code: String,
    /// 创建人账号 ID（只读审计事实，不得作为责任回退来源）。
    pub created_by: String,
    /// 当前采购单责任人；存量数据允许为空，但执行必须失败关闭。
    #[serde(default)]
    pub owner_user_id: Option<String>,
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

impl<'a> PurchaseOrderRepository<'a> {
    /// 批量读取采购单关联的销售单。
    ///
    /// # 参数
    /// * `sales_order_ids` - 销售单稳定身份集合
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 返回已存在的销售单；空输入直接返回空集合。
    ///
    /// # 错误
    /// 当 MongoDB 查询或游标读取失败时返回错误。
    pub async fn find_sales_orders_by_ids(
        &self,
        sales_order_ids: &[SalesOrderId],
        executor: &mut dyn Executor,
    ) -> Result<Vec<SalesOrder>> {
        if sales_order_ids.is_empty() {
            return Ok(Vec::new());
        }
        self.db
            .sales_orders()
            .find_many(
                in_filter("id", sales_order_ids.iter().map(ToString::to_string)),
                executor,
            )
            .await
    }

    /// 批量读取已生效且可作为采购来源的销售单。
    ///
    /// # 参数
    /// * `sales_order_ids` - 销售单稳定身份集合
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 返回已存在且商业状态为生效的销售单；空输入直接返回空集合。
    ///
    /// # 错误
    /// 当 MongoDB 查询或游标读取失败时返回错误。
    pub async fn find_effective_sales_orders_by_ids(
        &self,
        sales_order_ids: &[SalesOrderId],
        executor: &mut dyn Executor,
    ) -> Result<Vec<SalesOrder>> {
        if sales_order_ids.is_empty() {
            return Ok(Vec::new());
        }
        self.db
            .sales_orders()
            .find_many(
                doc! {
                    "id": { "$in": sales_order_ids.iter().map(ToString::to_string).collect::<Vec<_>>() },
                    "commercial_status": CommercialStatus::Effective.as_str(),
                },
                executor,
            )
            .await
    }

    /// 查询采购单对应的应付子账。
    ///
    /// # 参数
    /// * `purchase_order_id` - 采购单稳定身份
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 返回采购单来源的应付子账；尚未形成时返回 `None`。
    ///
    /// # 错误
    /// 当 MongoDB 查询失败时返回错误。
    pub async fn find_payable_account(
        &self,
        purchase_order_id: &entities::ids::PurchaseOrderId,
        executor: &mut dyn Executor,
    ) -> Result<Option<PayableAccount>> {
        self.db
            .payable_accounts()
            .find_one(
                doc! {
                    "deleted_at": NOT_DELETED_TIMESTAMP_BSON,
                    "source_document_id": purchase_order_id.to_string(),
                    "source_type": PayableSourceType::PurchaseOrder.as_str(),
                },
                executor,
            )
            .await
    }

    /// 按 SKU 读取启用供给，并按供应商与供给 ID 稳定排序。
    ///
    /// # 参数
    /// * `sku_id` - 公司 SKU 稳定身份
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 返回该 SKU 的全部启用供给。
    ///
    /// # 错误
    /// 当 MongoDB 查询或游标读取失败时返回错误。
    pub async fn list_active_offerings_by_sku(
        &self,
        sku_id: &SkuId,
        executor: &mut dyn Executor,
    ) -> Result<Vec<SupplierOffering>> {
        self.db
            .supplier_offerings()
            .find_many_sorted(
                doc! {
                    "deleted_at": NOT_DELETED_TIMESTAMP_BSON,
                    "sku_id": sku_id.to_string(),
                    "status": OfferingStatus::Active.as_str(),
                },
                doc! { "supplier_id": 1, "id": 1 },
                executor,
            )
            .await
    }

    /// 查询最早创建的启用仓库。
    ///
    /// # 参数
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 返回最早创建的启用仓库；没有启用仓库时返回 `None`。
    ///
    /// # 错误
    /// 当 MongoDB 查询或游标读取失败时返回错误。
    pub async fn find_first_active_warehouse(
        &self,
        executor: &mut dyn Executor,
    ) -> Result<Option<Warehouse>> {
        Ok(self
            .db
            .warehouses()
            .find_many_sorted(
                doc! {
                    "deleted_at": NOT_DELETED_TIMESTAMP_BSON,
                    "status": EnableStatus::Active.as_str(),
                },
                doc! { "created_at": 1, "id": 1 },
                executor,
            )
            .await?
            .into_iter()
            .next())
    }

    /// 读取审批节点执行上的全部开放任务，并按创建时间升序返回。
    ///
    /// # 参数
    /// * `approval_node_execution_id` - 审批节点执行主键
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 返回匹配的开放任务。
    ///
    /// # 错误
    /// 当 MongoDB 查询或游标读取失败时返回错误。
    pub async fn list_open_work_items_by_execution(
        &self,
        approval_node_execution_id: &str,
        executor: &mut dyn Executor,
    ) -> Result<Vec<WorkItem>> {
        self.db
            .work_items()
            .find_many_sorted(
                doc! {
                    "approval_node_execution_id": approval_node_execution_id,
                    "status": WorkItemStatus::Open.as_str(),
                },
                doc! { "created_at": 1, "id": 1 },
                executor,
            )
            .await
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

    /// 查询销售单当前采购覆盖所需的采购单。
    ///
    /// 只返回未删除且状态为草稿、旧待财务、审批中、生效、部分执行或已完成的
    /// 采购单；作废采购单不占用销售数量。调用方必须继续沿每张采购单的当前提交
    /// 或当前版本指针读取数量，禁止累计历史提交或历史版本。
    ///
    /// # 参数
    /// * `sales_order_id` - 来源销售单稳定身份
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 返回参与当前采购覆盖计算的采购单。
    ///
    /// # 错误
    /// 当 MongoDB 查询或游标读取失败时返回错误。
    pub async fn find_covering_by_sales_order(
        &self,
        sales_order_id: &SalesOrderId,
        executor: &mut dyn Executor,
    ) -> Result<Vec<PurchaseOrder>> {
        self.find_many(active_purchase_order_filter(sales_order_id), executor)
            .await
    }

    /// 统计销售单关联的有效采购单。
    ///
    /// 统计口径与采购创建依据一致：草稿、审批中、生效、部分执行和已完成均视为
    /// 已建采购；作废单不阻断重新建单，也不计入销售单的有效采购关联数。
    ///
    /// # 参数
    /// * `sales_order_id` - 来源销售单稳定身份
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 返回未删除且非作废的关联采购单数量。
    ///
    /// # 错误
    /// 当 MongoDB 计数失败时返回错误。
    pub async fn count_active_by_sales_order(
        &self,
        sales_order_id: &SalesOrderId,
        executor: &mut dyn Executor,
    ) -> Result<u64> {
        mongo_ops::count_documents(
            &self.collection(),
            active_purchase_order_filter(sales_order_id),
            executor,
        )
        .await
    }
}

/// 构造销售单有效采购关联的统一统计条件。
///
/// # 参数
/// * `sales_order_id` - 来源销售单稳定身份
///
/// # 返回
/// 返回排除软删除与作废采购单的 MongoDB 条件。
fn active_purchase_order_filter(sales_order_id: &SalesOrderId) -> Document {
    doc! {
        "deleted_at": NOT_DELETED_TIMESTAMP_BSON,
        "sales_order_id": sales_order_id.to_string(),
        "status": { "$in": [
            PurchaseOrderStatus::Draft.as_str(),
            PurchaseOrderStatus::PendingFinanceReview.as_str(),
            PurchaseOrderStatus::InApproval.as_str(),
            PurchaseOrderStatus::Effective.as_str(),
            PurchaseOrderStatus::PartiallyExecuted.as_str(),
            PurchaseOrderStatus::Completed.as_str(),
        ]},
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
        "payment_term_code": 1,
        "created_by": 1,
        "owner_user_id": 1,
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
    use super::{active_purchase_order_filter, PurchaseOrderFilter, QueryFilter};
    use entities::ids::{SalesOrderId, SupplierAccountId};
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
    fn projection_includes_payment_term_and_current_owner() {
        let document = super::purchase_order_projection();
        assert_eq!(document.get_i32("payment_term_code").unwrap(), 1);
        assert_eq!(document.get_i32("created_by").unwrap(), 1);
        assert_eq!(document.get_i32("owner_user_id").unwrap(), 1);
    }

    #[test]
    fn active_relation_filter_excludes_voided_purchase_orders() {
        let document = active_purchase_order_filter(&SalesOrderId::new("so-1"));
        let statuses = document
            .get_document("status")
            .unwrap()
            .get_array("$in")
            .unwrap()
            .iter()
            .map(|value| value.as_str().unwrap())
            .collect::<Vec<_>>();

        assert_eq!(document.get_str("sales_order_id").unwrap(), "so-1");
        assert_eq!(document.get_i64("deleted_at").unwrap(), 0);
        assert_eq!(
            statuses,
            vec![
                "DRAFT",
                "PENDING_FINANCE_REVIEW",
                "IN_APPROVAL",
                "EFFECTIVE",
                "PARTIALLY_EXECUTED",
                "COMPLETED",
            ]
        );
        assert!(!statuses.contains(&"VOIDED"));
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
