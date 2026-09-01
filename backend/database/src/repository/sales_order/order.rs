//! `sales_order` 与 `sales_order_line` 仓储：主单列表查询、稳定明细维护。

use entities::receivable::ReceivableAccount;
use entities::sales_order::{
    BusinessType, CommercialStatus, ReviewStatus, SalesOrder, SalesOrderId, SalesOrderLine,
};
use entities::work_item::{WorkItem, WorkItemStatus, WorkItemType};
use entity_core::NOT_DELETED_TIMESTAMP_BSON;
use mongodb::bson::{doc, Document};
use mongodb::options::FindOptions;
use serde::{Deserialize, Serialize};

use super::super::regex_filter::insert_literal_regex_filter;
use super::super::{PageResult, Pagination, QueryFilter, Repository};
use super::{sort_doc, SalesOrderRepository, SALES_ORDERS};
use crate::executor::Executor;
use crate::{mongo_ops, Result};
use std::collections::HashSet;

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
    /// 负责销售账号；ERP 建单负责人固定为建单人。
    pub created_by: String,
}

/// 销售单列表筛选条件。
#[derive(Debug, Clone)]
pub struct SalesOrderFilter {
    /// 销售单号（字面量正则，忽略大小写）；`None` 表示不筛选。
    pub order_no: Option<String>,
    /// 客户；`None` 表示不筛选。
    pub customer_id: Option<String>,
    /// 合同；`None` 表示不筛选。
    pub contract_id: Option<String>,
    /// 最初创建入口；`None` 表示不筛选。
    pub origin_system: Option<entities::sales_order::OriginSystem>,
    /// 商业主状态；`None` 表示不筛选。
    pub commercial_status: Option<CommercialStatus>,
    /// 审核轨状态；`None` 表示不筛选。
    pub review_status: Option<ReviewStatus>,
    /// 业务性质；`None` 表示不筛选。
    pub business_type: Option<BusinessType>,
    /// 履约进度；`None` 表示不筛选。
    pub fulfillment_progress: Option<entities::sales_order::FulfillmentProgress>,
    /// 回款进度；`None` 表示不筛选。
    pub collection_progress: Option<entities::sales_order::CollectionProgress>,
    /// 开票进度；`None` 表示不筛选。
    pub invoice_progress: Option<entities::sales_order::InvoiceProgress>,
    /// 关闭状态；`None` 表示不筛选。
    pub close_status: Option<entities::sales_order::CloseStatus>,
    /// 创建时间下界（含）；`None` 表示不设下界。
    pub created_from: Option<u64>,
    /// 创建时间上界（含）；`None` 表示不设上界。
    pub created_to: Option<u64>,
    /// 创建人账号；`None` 表示不筛选（"我创建的"/"待我处理"视图用）。
    pub created_by: Option<String>,
    /// "待我处理"视图：仅草稿或被驳回/低毛利待处理回销售的单
    /// （`commercial_status=DRAFT` 或 `review_status IN [REJECTED, PENDING_LOW_MARGIN_SUPERIOR]`）。
    /// 与 `commercial_status`/`review_status` 互斥，调用方不应同时传两者。
    pub my_todo: bool,
    /// "异常"视图：审核轨被驳回（与 `review_status` 互斥）。
    pub exception_only: bool,
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
        if let Some(contract_id) = &self.contract_id {
            filter.insert("contract_id", contract_id);
        }
        if let Some(origin_system) = self.origin_system {
            filter.insert("origin_system", origin_system.as_str());
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
        if let Some(progress) = self.fulfillment_progress {
            filter.insert("fulfillment_progress", progress.as_str());
        }
        if let Some(progress) = self.collection_progress {
            filter.insert("collection_progress", progress.as_str());
        }
        if let Some(progress) = self.invoice_progress {
            filter.insert("invoice_progress", progress.as_str());
        }
        if let Some(status) = self.close_status {
            filter.insert("close_status", status.as_str());
        }
        if self.created_from.is_some() || self.created_to.is_some() {
            let mut created_at = Document::new();
            if let Some(from) = self.created_from {
                created_at.insert("$gte", i64::try_from(from).unwrap_or(i64::MAX));
            }
            if let Some(to) = self.created_to {
                created_at.insert("$lte", i64::try_from(to).unwrap_or(i64::MAX));
            }
            filter.insert("created_at", created_at);
        }
        if let Some(created_by) = &self.created_by {
            filter.insert("created_by", created_by);
        }
        if self.my_todo {
            filter.insert(
                "$or",
                vec![
                    doc! { "commercial_status": CommercialStatus::Draft.as_str() },
                    doc! {
                        "review_status": {
                            "$in": [
                                ReviewStatus::Rejected.as_str(),
                                ReviewStatus::PendingLowMarginSuperior.as_str(),
                            ]
                        }
                    },
                ],
            );
        }
        if self.exception_only {
            filter.insert("review_status", ReviewStatus::Rejected.as_str());
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
    /// 按销售单 ID 集合批量读取活跃销售单。
    ///
    /// # 参数
    /// * `sales_order_ids` - 销售单稳定身份集合；空集合直接返回空结果
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 返回全部匹配且未删除的销售单；返回顺序不承诺与输入一致。
    ///
    /// # 错误
    /// 当 MongoDB 查询或游标读取失败时返回错误。
    pub async fn find_orders_by_ids(
        &self,
        sales_order_ids: &[SalesOrderId],
        executor: &mut dyn Executor,
    ) -> Result<Vec<SalesOrder>> {
        if sales_order_ids.is_empty() {
            return Ok(Vec::new());
        }
        let ids = sales_order_ids
            .iter()
            .map(ToString::to_string)
            .collect::<Vec<_>>();
        self.find_many(doc! { "id": { "$in": ids } }, executor).await
    }

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
    #[tracing::instrument(
        name = "repository.sales_order.search",
        skip_all,
        fields(
            layer = "repository",
            domain = "sales_order",
            db.system.name = "mongodb",
            db.collection.name = "sales_orders",
            db.operation.name = "search"
        )
    )]
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
    #[tracing::instrument(
        name = "repository.sales_order.list_lines",
        skip_all,
        fields(
            layer = "repository",
            domain = "sales_order",
            db.system.name = "mongodb",
            db.collection.name = "sales_order_lines",
            db.operation.name = "find"
        )
    )]
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

impl<'a> Repository<'a, ReceivableAccount> {
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
    pub async fn list_by_sales_order(
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
    pub async fn find_primary_by_sales_order(
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
}

impl<'a> Repository<'a, WorkItem> {
    /// 列出指定销售责任范围的开放供给分配任务。
    ///
    /// # 参数
    /// * `sales_order_id` - 来源销售单
    /// * `responsibility_key` - 冻结责任范围键
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 返回匹配的开放任务；调用方据此处理幂等与异常重复。
    ///
    /// # 错误
    /// 当 MongoDB 查询或游标读取失败时返回错误。
    pub async fn list_open_procurement_by_responsibility(
        &self,
        sales_order_id: &SalesOrderId,
        responsibility_key: &str,
        executor: &mut dyn Executor,
    ) -> Result<Vec<WorkItem>> {
        self.find_many(
            doc! {
                "business_object_type": "sales_order",
                "business_object_id": sales_order_id.to_string(),
                "work_item_type": WorkItemType::ProcurementOrderCreation.as_str(),
                "responsibility_key": responsibility_key,
                "status": WorkItemStatus::Open.as_str(),
            },
            executor,
        )
        .await
    }
}

impl<'a> SalesOrderRepository<'a> {
    /// 按 ID 集合批量返回存在的未删除销售单 ID（最小存在性事实）。
    ///
    /// 一次 `$in` 查询完成全部存在性装载，查询次数与输入数量无关；空输入
    /// 或全部为重复 ID 时直接返回空集合，不发起数据库往返。软删除语义与
    /// [`Repository::find_by_id`] 一致（仅统计 `deleted_at` 为未删除标记的
    /// 销售单）。返回集合只保证是输入的子集，不承诺顺序；跨聚合报错决策
    /// （如“哪些订单缺失”）由调用方 Service 解释。
    ///
    /// # 参数
    /// * `ids` - 待校验的销售单 ID；重复 ID 自动去重，结果中每个 ID 至多出现一次
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 返回输入中真实存在且未删除的销售单 ID 集合。
    ///
    /// # 错误
    /// 当 MongoDB 查询或投影反序列化失败时返回错误。
    pub async fn find_existing_ids(
        &self,
        ids: &[SalesOrderId],
        executor: &mut dyn Executor,
    ) -> Result<Vec<SalesOrderId>> {
        let unique = dedupe_sales_order_ids(ids);
        if unique.is_empty() {
            return Ok(Vec::new());
        }
        let id_strings = unique.iter().map(ToString::to_string).collect::<Vec<_>>();
        let rows = mongo_ops::find_many(
            &self.db.collection::<SalesOrderIdRow>(SALES_ORDERS),
            doc! { "id": { "$in": id_strings }, "deleted_at": NOT_DELETED_TIMESTAMP_BSON },
            FindOptions::builder()
                .projection(doc! { "_id": 0, "id": 1 })
                .build(),
            executor,
        )
        .await?;
        Ok(rows.into_iter().map(|row| SalesOrderId::new(row.id)).collect())
    }
}

/// 销售单存在性投影行（只取 `id` 字段的最小事实）。
#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
struct SalesOrderIdRow {
    /// 实体主键。
    id: String,
}

/// 去重销售单 ID（保持首次出现顺序）。
///
/// # 参数
/// * `ids` - 输入 ID 列表，可能包含重复
///
/// # 返回
/// 返回去重后的 ID 列表，顺序与首次出现一致。
///
/// # 错误
/// 无。
fn dedupe_sales_order_ids(ids: &[SalesOrderId]) -> Vec<SalesOrderId> {
    let mut seen = HashSet::with_capacity(ids.len());
    ids.iter().filter(|&id| seen.insert(id.clone())).cloned().collect()
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
        "created_by": 1,
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
            contract_id: Some("contract-1".to_string()),
            origin_system: Some(entities::sales_order::OriginSystem::Erp),
            commercial_status: Some(CommercialStatus::PendingReview),
            review_status: Some(ReviewStatus::PendingOperations),
            business_type: Some(BusinessType::Voucher),
            fulfillment_progress: Some(entities::sales_order::FulfillmentProgress::NotStarted),
            collection_progress: Some(entities::sales_order::CollectionProgress::NotCollected),
            invoice_progress: Some(entities::sales_order::InvoiceProgress::NotInvoiced),
            close_status: Some(entities::sales_order::CloseStatus::NotSatisfied),
            created_from: Some(1_700_000_000),
            created_to: Some(1_800_000_000),
            created_by: Some("user-1".to_string()),
            my_todo: false,
            exception_only: false,
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
        assert_eq!(document.get_str("contract_id").unwrap(), "contract-1");
        assert_eq!(document.get_str("origin_system").unwrap(), "ERP");
        assert_eq!(document.get_str("commercial_status").unwrap(), "PENDING_REVIEW");
        assert_eq!(document.get_str("review_status").unwrap(), "PENDING_OPERATIONS");
        assert_eq!(document.get_str("business_type").unwrap(), "VOUCHER");
        assert_eq!(document.get_str("fulfillment_progress").unwrap(), "NOT_STARTED");
        assert_eq!(document.get_str("collection_progress").unwrap(), "NOT_COLLECTED");
        assert_eq!(document.get_str("invoice_progress").unwrap(), "NOT_INVOICED");
        assert_eq!(document.get_str("close_status").unwrap(), "NOT_SATISFIED");
        assert_eq!(
            document
                .get_document("created_at")
                .unwrap()
                .get_i64("$gte")
                .unwrap(),
            1_700_000_000
        );
        assert_eq!(document.get_str("created_by").unwrap(), "user-1");
    }

    #[test]
    fn sales_order_projection_includes_responsible_sales_account() {
        assert_eq!(sales_order_projection().get_i32("created_by").unwrap(), 1);
    }

    #[test]
    fn sales_order_filter_escapes_regex_metacharacters() {
        let filter = SalesOrderFilter {
            order_no: Some("SO-2026.[x]".to_string()),
            customer_id: None,
            contract_id: None,
            origin_system: None,
            commercial_status: None,
            review_status: None,
            business_type: None,
            fulfillment_progress: None,
            collection_progress: None,
            invoice_progress: None,
            close_status: None,
            created_from: None,
            created_to: None,
            created_by: None,
            my_todo: false,
            exception_only: false,
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

    #[test]
    fn dedupe_sales_order_ids_keeps_first_occurrence_order() {
        let ids = vec![
            SalesOrderId::new("so-1"),
            SalesOrderId::new("so-2"),
            SalesOrderId::new("so-1"),
            SalesOrderId::new("so-3"),
            SalesOrderId::new("so-2"),
        ];
        let unique = dedupe_sales_order_ids(&ids);
        let strings = unique.iter().map(ToString::to_string).collect::<Vec<_>>();
        assert_eq!(strings, vec!["so-1", "so-2", "so-3"]);
    }

    #[test]
    fn dedupe_sales_order_ids_empty_input_returns_empty() {
        assert!(dedupe_sales_order_ids(&[]).is_empty());
    }
}
