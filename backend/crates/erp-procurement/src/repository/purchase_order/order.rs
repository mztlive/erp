//! `purchase_order` 采购主表仓储：列表投影查询与按采购单号身份查询。

use entity_core::NOT_DELETED_TIMESTAMP_BSON;
use erp_core::ids::{SalesOrderId, SupplierAccountId};
use mongodb::bson::{Document, doc};
use mongodb::options::FindOptions;
use persistence_core::{
    Executor, PageResult, Pagination, QueryFilter, Result, insert_literal_regex_filter, mongo_ops,
};
use serde::{Deserialize, Serialize};

use super::PurchaseOrderDomainRepository;
use super::common::{PURCHASE_ORDER_SORT_FIELDS, in_filter, sort_doc};
use crate::entity::purchase_order::{
    FulfillmentResponsibility, ProgressStatus, PurchaseOrder, PurchaseOrderStatus, PurchaseReviewStatus,
    PurchaseType,
};
use crate::repository::extensions::PurchaseOrderExt;

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
    /// 冻结的履约责任。
    pub fulfillment_responsibility: FulfillmentResponsibility,
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

/// 采购单号最小事实行（FIN-R03 来源单号批量映射，只投影单号）。
#[derive(Debug, Clone, serde::Deserialize)]
struct PurchaseOrderNoRow {
    /// 实体主键。
    id: String,
    /// 采购业务单号。
    purchase_no: String,
}

/// 采购单列表筛选条件。
#[derive(Debug, Clone)]
pub struct PurchaseOrderFilter {
    /// 当前采购负责人条件，不改变责任或任务。
    pub owner_user_ids: Option<application_core::QueryIds>,
    /// 采购单号模糊匹配（字面量、忽略大小写）；`None` 表示不筛选。
    pub purchase_no: Option<String>,
    /// 与采购单号进行 OR 匹配的来源销售单身份，由消费方查询拥有领域后传入。
    pub keyword_sales_order_ids: Vec<SalesOrderId>,
    /// 与采购单号进行 OR 匹配的供应商身份，由消费方查询拥有领域后传入。
    pub keyword_supplier_ids: Vec<SupplierAccountId>,
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

impl Default for PurchaseOrderFilter {
    /// 返回首页空筛选（`page: 1`，`page_size: 20`）。
    ///
    /// # 参数
    /// 无。
    ///
    /// # 返回
    /// 返回筛选为空、降序的首页过滤条件。
    ///
    /// # 错误
    /// 无。
    fn default() -> Self {
        Self {
            owner_user_ids: None,
            purchase_no: None,
            keyword_sales_order_ids: Vec::new(),
            keyword_supplier_ids: Vec::new(),
            sales_order_id: None,
            supplier_id: None,
            status: None,
            page: 1,
            page_size: 20,
            sort_by: None,
            sort_ascending: false,
        }
    }
}

impl QueryFilter for PurchaseOrderFilter {
    /// 转换为 MongoDB 查询条件（自动追加未删除过滤）。
    ///
    /// # 返回
    /// 返回查询条件文档。
    fn to_doc(&self) -> Document {
        let mut filter = doc! { "deleted_at": NOT_DELETED_TIMESTAMP_BSON };
        if let Some(ids) = &self.owner_user_ids {
            filter.insert("owner_user_id", doc! { "$in": ids.as_slice() });
        }
        if !self.keyword_sales_order_ids.is_empty() || !self.keyword_supplier_ids.is_empty() {
            let mut number = Document::new();
            insert_literal_regex_filter(&mut number, "purchase_no", self.purchase_no.as_deref());
            let mut clauses = vec![
                doc! { "sales_order_id": { "$in": self.keyword_sales_order_ids.iter().map(ToString::to_string).collect::<Vec<_>>() } },
                doc! { "supplier_id": { "$in": self.keyword_supplier_ids.iter().map(ToString::to_string).collect::<Vec<_>>() } },
            ];
            if !number.is_empty() {
                clauses.push(number);
            }
            filter.insert("$or", clauses);
        } else {
            insert_literal_regex_filter(&mut filter, "purchase_no", self.purchase_no.as_deref());
        }
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

impl<'a> PurchaseOrderDomainRepository<'a> {
    /// 按采购单 ID 集合批量读取采购单。
    ///
    /// # 参数
    /// * `purchase_order_ids` - 采购单 ID 字符串集合；空集合直接返回空结果
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 返回全部匹配且未删除的采购单；返回顺序不承诺与输入一致。
    ///
    /// # 错误
    /// 当 MongoDB 查询或游标读取失败时返回错误。
    pub async fn find_orders_by_ids(
        &self,
        purchase_order_ids: &[String],
        executor: &mut dyn Executor,
    ) -> Result<Vec<PurchaseOrder>> {
        if purchase_order_ids.is_empty() {
            return Ok(Vec::new());
        }
        self.db
            .purchase_orders()
            .find_many(in_filter("id", purchase_order_ids.iter().cloned()), executor)
            .await
    }

    /// 按采购单 ID 集合一次批量返回来源 ID 到采购单号的事实映射（FIN-R03）。
    ///
    /// 只投影 `id` 与 `purchase_no`；空输入不访问数据库；仓储内去重后单次
    /// `$in` 查询。空单号按缺失处理，不进入映射，Service 保持 `None`
    /// 且不得回退内部 ID。
    ///
    /// # 参数
    /// * `purchase_order_ids` - 采购单 ID 字符串集合
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 返回来源 ID 到非空采购单号的映射；缺失来源不上表。
    ///
    /// # 错误
    /// 当 MongoDB 查询或游标读取失败时返回错误。
    pub async fn purchase_nos_by_ids(
        &self,
        purchase_order_ids: &[String],
        executor: &mut dyn Executor,
    ) -> Result<std::collections::HashMap<String, String>> {
        use std::collections::{HashMap, HashSet};
        let mut seen = HashSet::new();
        let mut deduped = Vec::new();
        for id in purchase_order_ids {
            if !id.trim().is_empty() && seen.insert(id.clone()) {
                deduped.push(id.clone());
            }
        }
        if deduped.is_empty() {
            return Ok(HashMap::new());
        }
        let rows = mongo_ops::find_many(
            &self
                .db
                .collection::<PurchaseOrderNoRow>(<mongodb::Database as PurchaseOrderExt>::PURCHASE_ORDERS),
            in_filter("id", deduped),
            FindOptions::builder().projection(mongodb::bson::doc! { "id": 1, "purchase_no": 1 }).build(),
            executor,
        )
        .await?;
        let mut map = HashMap::new();
        for row in rows {
            if !row.purchase_no.trim().is_empty() {
                map.insert(row.id, row.purchase_no);
            }
        }
        Ok(map)
    }
}

/// 采购主表集合的域查询。
#[allow(async_fn_in_trait)]
pub trait PurchaseOrderRepositoryExt {
    /// 返回未删除且落在已证明范围内的负责人集合。
    ///
    /// # 参数
    /// * `scope` - 已由应用层完成资源动作授权的对象范围
    /// * `executor` - 事务或无事务执行器
    ///
    /// # 返回
    /// 返回去重的负责人 ID；候选不按登录资格过滤，不能用于改派。
    ///
    /// # 错误
    /// 数据库查询失败向上传播。
    ///
    /// # 关键业务约束
    /// 候选必须与列表同一授权边界，不得按登录用户自行推断。
    async fn current_owner_ids(
        &self,
        scope: &super::scope::PurchaseReadScope,
        executor: &mut dyn Executor,
    ) -> Result<Vec<String>>;

    /// 分页检索采购单列表（投影查询）。
    ///
    /// 只返回 [`PurchaseOrderRow`] 所需的列表字段，不加载整文档；排序字段
    /// 经白名单校验（`created_at`/`purchase_no`/`status`），未知字段回退默认值。
    ///
    /// # 参数
    /// * `filter` - 筛选与分页条件
    /// * `scope` - 已证明的对象范围，必须与筛选求交
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 返回当前页投影行与满足筛选条件的总数。
    ///
    /// # 错误
    /// 当 MongoDB 查询、游标读取或计数失败时返回错误。
    ///
    /// # 关键业务约束
    /// 授权条件由调用方传入，仓储不得按登录用户推断权限。
    async fn search_purchase_orders(
        &self,
        filter: &PurchaseOrderFilter,
        scope: &super::scope::PurchaseReadScope,
        executor: &mut dyn Executor,
    ) -> Result<PageResult<PurchaseOrderRow>>;

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
    async fn find_covering_by_sales_order(
        &self,
        sales_order_id: &SalesOrderId,
        executor: &mut dyn Executor,
    ) -> Result<Vec<PurchaseOrder>>;

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
    async fn count_active_by_sales_order(
        &self,
        sales_order_id: &SalesOrderId,
        executor: &mut dyn Executor,
    ) -> Result<u64>;

    /// 按业务编号返回全部匹配身份，供跨域列表在分页前筛选。
    ///
    /// 只投影 ID、排除软删除；数据库错误向上返回。
    async fn matching_ids_by_number(&self, keyword: &str, executor: &mut dyn Executor)
    -> Result<Vec<String>>;

    /// 判断组织是否仍有需要交接的未结单据。
    ///
    /// # 错误
    /// 查询失败返回仓储错误；失败不得解释为没有业务。
    async fn has_unsettled_business_org(&self, org: &str, executor: &mut dyn Executor) -> Result<bool>;
}

impl PurchaseOrderRepositoryExt for persistence_core::Repository<'_, PurchaseOrder> {
    async fn current_owner_ids(
        &self,
        scope: &super::scope::PurchaseReadScope,
        executor: &mut dyn Executor,
    ) -> Result<Vec<String>> {
        let collection = self.collection();
        let mut query = collection.distinct(
            "owner_user_id",
            doc! { "deleted_at": entity_core::NOT_DELETED_TIMESTAMP_BSON, "$and": [scope.document()] },
        );
        if let Some(session) = executor.session() {
            query = query.session(session);
        }
        Ok(query.await?.into_iter().filter_map(|v| v.as_str().map(str::to_owned)).collect())
    }

    async fn search_purchase_orders(
        &self,
        filter: &PurchaseOrderFilter,
        scope: &super::scope::PurchaseReadScope,
        executor: &mut dyn Executor,
    ) -> Result<PageResult<PurchaseOrderRow>> {
        let options = FindOptions::builder()
            .sort(sort_doc(filter.sort_by.as_deref(), PURCHASE_ORDER_SORT_FIELDS, filter.sort_ascending))
            .skip(filter.skip())
            .limit(filter.limit())
            .projection(purchase_order_projection())
            .build();
        let collection = self.collection().clone_with_type::<PurchaseOrderRow>();
        let items = mongo_ops::find_many(
            &collection,
            doc! { "$and": [filter.to_doc(), scope.document()] },
            options,
            executor,
        )
        .await?;
        let total = mongo_ops::count_documents(
            &self.collection(),
            doc! { "$and": [filter.to_doc(), scope.document()] },
            executor,
        )
        .await?;

        Ok(PageResult { items, total: total as i64 })
    }

    async fn find_covering_by_sales_order(
        &self,
        sales_order_id: &SalesOrderId,
        executor: &mut dyn Executor,
    ) -> Result<Vec<PurchaseOrder>> {
        self.find_many(active_purchase_order_filter(sales_order_id), executor).await
    }

    async fn count_active_by_sales_order(
        &self,
        sales_order_id: &SalesOrderId,
        executor: &mut dyn Executor,
    ) -> Result<u64> {
        mongo_ops::count_documents(&self.collection(), active_purchase_order_filter(sales_order_id), executor)
            .await
    }

    async fn matching_ids_by_number(
        &self,
        keyword: &str,
        executor: &mut dyn Executor,
    ) -> Result<Vec<String>> {
        let mut filter = doc! { "deleted_at": NOT_DELETED_TIMESTAMP_BSON };
        insert_literal_regex_filter(&mut filter, "purchase_no", Some(keyword));
        let collection = self.collection();
        let mut query = collection.distinct("id", filter);
        if let Some(session) = executor.session() {
            query = query.session(session);
        }
        Ok(query.await?.into_iter().filter_map(|id| id.as_str().map(str::to_owned)).collect())
    }

    async fn has_unsettled_business_org(&self, org: &str, executor: &mut dyn Executor) -> Result<bool> {
        self.exists(
            mongodb::bson::doc! { "business_org_unit_id": org, "status": { "$nin": ["COMPLETED", "VOIDED"] } },
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
        "fulfillment_responsibility": 1,
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
    use erp_core::ids::{SalesOrderId, SupplierAccountId};
    use mongodb::bson::doc;
    use persistence_core::QueryFilter;

    use super::{PurchaseOrderFilter, active_purchase_order_filter};
    use crate::entity::purchase_order::PurchaseOrderStatus;

    #[test]
    fn filter_applies_optional_fields_and_deleted_filter() {
        let filter = PurchaseOrderFilter {
            purchase_no: Some("PO-2026".to_string()),
            supplier_id: Some(SupplierAccountId::new("sup-1")),
            status: Some(PurchaseOrderStatus::PendingFinanceReview),
            ..Default::default()
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
        assert_eq!(regex.get_str("$regex").unwrap(), "PO\\-2026", "正则必须转义字面量");
        assert_eq!(regex.get_str("$options").unwrap(), "i");
    }

    /// 关键词三路 OR 匹配不得冲掉状态、供应商和软删除的 AND 约束。
    #[test]
    fn keyword_matches_references_before_paging_and_preserves_filters() {
        let mut filter = PurchaseOrderFilter {
            owner_user_ids: Some(serde_json::from_str("\"buyer-2,buyer-3\"").unwrap()),
            purchase_no: Some("XS.1".into()),
            keyword_sales_order_ids: vec![SalesOrderId::new("sales-1")],
            keyword_supplier_ids: vec![SupplierAccountId::new("supplier-1")],
            sales_order_id: None,
            supplier_id: Some(SupplierAccountId::new("supplier-2")),
            status: Some(PurchaseOrderStatus::InApproval),
            page: 2,
            page_size: 1,
            sort_by: None,
            sort_ascending: false,
        };
        let document = filter.to_doc();
        assert_eq!(document.get_document("owner_user_id").unwrap(), &doc! { "$in": ["buyer-2", "buyer-3"] });
        assert_eq!(document.get_i64("deleted_at").unwrap(), 0);
        assert_eq!(document.get_str("supplier_id").unwrap(), "supplier-2");
        assert_eq!(document.get_str("status").unwrap(), "IN_APPROVAL");
        assert!(!document.contains_key("purchase_no"));
        assert_eq!(
            document.get_array("$or").unwrap(),
            &vec![
                doc! { "sales_order_id": { "$in": ["sales-1"] } }.into(),
                doc! { "supplier_id": { "$in": ["supplier-1"] } }.into(),
                doc! { "purchase_no": { "$regex": r"XS\.1", "$options": "i" } }.into(),
            ]
        );
        filter.keyword_sales_order_ids.clear();
        filter.keyword_supplier_ids.clear();
        let unmatched_references = filter.to_doc();
        assert!(!unmatched_references.contains_key("$or"));
        assert!(unmatched_references.contains_key("purchase_no"));
        filter.purchase_no = None;
        assert!(!filter.to_doc().contains_key("purchase_no"));
    }

    #[test]
    fn projection_includes_payment_term_and_current_owner() {
        let document = super::purchase_order_projection();
        assert_eq!(document.get_i32("fulfillment_responsibility").unwrap(), 1);
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
    fn filter_default_is_first_page_size_20() {
        let filter = PurchaseOrderFilter::default();
        assert_eq!((filter.page, filter.page_size), (1, 20));
        assert!(filter.purchase_no.is_none());
        assert!(!filter.sort_ascending);
    }

    #[test]
    fn filter_omits_absent_fields() {
        let filter = PurchaseOrderFilter::default();

        assert_eq!(filter.to_doc(), doc! { "deleted_at": 0i64 });
    }
}
