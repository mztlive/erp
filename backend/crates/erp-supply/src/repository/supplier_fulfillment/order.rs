//! 供应商履约订单查询：列表行、筛选条件与订单集合特有查询。

#![allow(async_fn_in_trait)]

use super::*;

/// 供应商履约订单列表投影行。
///
/// 列表接口只取必要字段，禁止返回整文档；履约地址快照（加密值与查询指纹）是敏感值，
/// 一律不进列表投影（§4.5.5）。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SupplierFulfillmentOrderRow {
    /// 实体主键。
    pub id: String,
    /// ERP 供应商子订单号。
    pub fulfillment_order_no: String,
    /// 固定供应商。
    pub supplier_id: SupplierAccountId,
    /// 供应商 API 连接。
    pub connection_id: SupplierApiConnectionId,
    /// 确定性拆单序号。
    pub split_no: u32,
    /// 履约主线状态。
    pub fulfillment_status: FulfillmentStatus,
    /// 取消进度状态。
    pub cancel_status: CancelStatus,
    /// 退款进度状态。
    pub refund_status: RefundStatus,
    /// 供应商订单号。
    pub external_order_no: Option<String>,
    /// 提交给供应商的时间。
    pub submitted_at: Option<Instant>,
    /// 供应商接单时间。
    pub accepted_at: Option<Instant>,
    /// 履约完成时间。
    pub completed_at: Option<Instant>,
    /// 内部跟进人。
    #[serde(default)]
    pub follow_up_user_id: String,
    /// 当前业务组织。
    #[serde(default)]
    pub business_org_unit_id: String,
    /// 乐观锁版本（`BaseModel.version` ≡ 数据模型 `lock_version`）。
    pub version: u64,
    /// 创建时间（秒级时间戳）。
    pub created_at: u64,
}

/// 供应商履约订单列表筛选条件。
#[derive(Debug, Clone)]
pub struct SupplierFulfillmentOrderFilter {
    /// 关键词与既有精确条件取交集。
    pub q: Option<String>,
    /// 固定供应商；`None` 表示不筛选。
    pub supplier_id: Option<SupplierAccountId>,
    /// 履约主线状态；`None` 表示不筛选。
    pub fulfillment_status: Option<FulfillmentStatus>,
    /// 取消进度状态；`None` 表示不筛选。
    pub cancel_status: Option<CancelStatus>,
    /// 退款进度状态；`None` 表示不筛选。
    pub refund_status: Option<RefundStatus>,
    /// 列表视图；`None` 或 `all` 不追加视图条件。
    pub view: Option<String>,
    /// 售后待处理快捷筛选。
    pub aftersale_pending: bool,
    /// 已证明的跟进人授权条件；`None` 表示调用方尚未注入范围。
    pub scope: Option<crate::repository::supplier_fulfillment_scope::FulfillmentOrderReadScope>,
    /// 跟进人筛选，只收窄授权结果。
    pub follow_up_user_ids: Option<Vec<String>>,
    /// 业务组织筛选，只收窄授权结果。
    pub business_org_unit_ids: Option<Vec<String>>,
    /// 当前开放 W26 处理人命中的订单 ID；`None` 表示不按处理人收窄。
    pub handler_order_ids: Option<Vec<String>>,
    /// 供应商订单号（按字面量部分匹配，忽略大小写）；`None` 表示不筛选。
    pub external_order_no: Option<String>,
    /// 页码（1 起）。
    pub page: u64,
    /// 单页条数。
    pub page_size: u32,
    /// 排序字段（白名单内生效，白名单外回退 `created_at`）。
    pub sort_by: Option<String>,
    /// 是否升序；`false` 表示降序（默认）。
    pub sort_ascending: bool,
}

impl Default for SupplierFulfillmentOrderFilter {
    fn default() -> Self {
        Self {
            q: None,
            supplier_id: None,
            fulfillment_status: None,
            cancel_status: None,
            refund_status: None,
            view: None,
            aftersale_pending: false,
            scope: None,
            follow_up_user_ids: None,
            business_org_unit_ids: None,
            handler_order_ids: None,
            external_order_no: None,
            page: 1,
            page_size: 20,
            sort_by: None,
            sort_ascending: false,
        }
    }
}

impl QueryFilter for SupplierFulfillmentOrderFilter {
    /// 转换为 MongoDB 查询条件（自动追加未删除过滤）。
    ///
    /// # 返回
    /// 返回查询条件文档。
    fn to_doc(&self) -> Document {
        let mut filter = doc! { "deleted_at": NOT_DELETED_TIMESTAMP_BSON };
        if let Some(supplier_id) = &self.supplier_id {
            filter.insert("supplier_id", supplier_id.to_string());
        }
        if let Some(fulfillment_status) = self.fulfillment_status {
            filter.insert("fulfillment_status", fulfillment_status.as_str());
        }
        if let Some(cancel_status) = self.cancel_status {
            filter.insert("cancel_status", cancel_status.as_str());
        }
        if let Some(refund_status) = self.refund_status {
            filter.insert("refund_status", refund_status.as_str());
        }
        insert_literal_regex_filter(&mut filter, "external_order_no", self.external_order_no.as_deref());
        insert_id_in(&mut filter, "follow_up_user_id", self.follow_up_user_ids.as_deref());
        insert_id_in(&mut filter, "business_org_unit_id", self.business_org_unit_ids.as_deref());
        insert_id_in(&mut filter, "id", self.handler_order_ids.as_deref());
        if let Some(q) = &self.q {
            let clauses = ["fulfillment_order_no", "external_order_no"]
                .into_iter()
                .map(|field| {
                    let mut clause = Document::new();
                    insert_literal_regex_filter(&mut clause, field, Some(q));
                    clause
                })
                .collect::<Vec<_>>();
            filter.insert("$or", clauses);
        }
        and_optional(&mut filter, view_clause(self.view.as_deref()));
        if self.aftersale_pending {
            and_optional(&mut filter, Some(aftersale_pending_clause()));
        }
        if let Some(scope) = &self.scope {
            filter = doc! { "$and": [filter, scope.document()] };
        }
        filter
    }
}

impl Pagination for SupplierFulfillmentOrderFilter {
    /// 返回页码与单页条数。
    ///
    /// # 返回
    /// 返回 `(page, page_size)` 元组。
    fn page_and_size(&self) -> (u64, u64) {
        (self.page, u64::from(self.page_size))
    }
}

/// 供应商履约订单仓储的域查询。
#[allow(async_fn_in_trait)]
pub trait SupplierFulfillmentOrderRepositoryExt {
    /// 分页检索供应商履约订单列表（投影查询）。
    ///
    /// 只返回 [`SupplierFulfillmentOrderRow`] 所需的列表字段，不加载整文档；
    /// 排序字段走白名单映射（`ORDER_SORT_FIELDS`），白名单外一律回退 `created_at`。
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
    async fn search_supplier_fulfillment_orders(
        &self,
        filter: &SupplierFulfillmentOrderFilter,
        executor: &mut dyn Executor,
    ) -> Result<PageResult<SupplierFulfillmentOrderRow>>;

    /// 按供应商批量读取全部未删除履约订单。
    ///
    /// # 参数
    /// * `supplier_id` - 供应商主键
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 返回该供应商的履约订单集合。
    ///
    /// # 错误
    /// MongoDB 查询或游标读取失败时返回错误。
    async fn list_by_supplier_id(
        &self,
        supplier_id: &SupplierAccountId,
        executor: &mut dyn Executor,
    ) -> Result<Vec<SupplierFulfillmentOrder>>;

    /// 按 ERP 供应商子订单号查找唯一履约订单。
    ///
    /// 唯一性由 `uk_supplier_fulfillment_orders_order_no` 唯一索引保证；该方法用于
    /// 下单幂等判定，服务层不得做「先查后插」的重复性判断（§6.19）。
    ///
    /// # 参数
    /// * `fulfillment_order_no` - ERP 供应商子订单号
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 返回匹配的未删除履约订单；无匹配时返回 `None`。
    ///
    /// # 错误
    /// 当 MongoDB 查询失败时返回错误。
    async fn find_by_fulfillment_order_no(
        &self,
        fulfillment_order_no: &str,
        executor: &mut dyn Executor,
    ) -> Result<Option<SupplierFulfillmentOrder>>;
}

impl SupplierFulfillmentOrderRepositoryExt for SupplierFulfillmentOrderRepository<'_> {
    async fn search_supplier_fulfillment_orders(
        &self,
        filter: &SupplierFulfillmentOrderFilter,
        executor: &mut dyn Executor,
    ) -> Result<PageResult<SupplierFulfillmentOrderRow>> {
        let options = FindOptions::builder()
            .sort(order_sort_doc(filter.sort_by.as_deref(), filter.sort_ascending))
            .skip(filter.skip())
            .limit(filter.limit())
            .projection(supplier_fulfillment_order_projection())
            .build();
        let collection = self.collection().clone_with_type::<SupplierFulfillmentOrderRow>();
        let items = mongo_ops::find_many(&collection, filter.to_doc(), options, executor).await?;
        let total = mongo_ops::count_documents(&self.collection(), filter.to_doc(), executor).await?;

        Ok(PageResult { items, total: total as i64 })
    }

    async fn list_by_supplier_id(
        &self,
        supplier_id: &SupplierAccountId,
        executor: &mut dyn Executor,
    ) -> Result<Vec<SupplierFulfillmentOrder>> {
        self.find_many(doc! { "supplier_id": supplier_id.to_string() }, executor).await
    }

    async fn find_by_fulfillment_order_no(
        &self,
        fulfillment_order_no: &str,
        executor: &mut dyn Executor,
    ) -> Result<Option<SupplierFulfillmentOrder>> {
        self.find_one(doc! { "fulfillment_order_no": fulfillment_order_no }, executor).await
    }
}
/// 构建履约订单排序文档（白名单映射，禁止透传任意字段名）。
///
/// # 参数
/// * `sort_by` - 排序字段；`None` 或不在白名单内时默认 `created_at`
/// * `sort_ascending` - 升序为 `true`，降序为 `false`
///
/// # 返回
/// 返回排序条件文档。
pub(crate) fn order_sort_doc(sort_by: Option<&str>, sort_ascending: bool) -> Document {
    let direction = if sort_ascending { 1 } else { -1 };
    let field = sort_by.filter(|field| ORDER_SORT_FIELDS.contains(field)).unwrap_or("created_at");
    doc! { field: direction, "id": direction }
}
pub(crate) fn supplier_fulfillment_order_projection() -> Document {
    doc! {
        "id": 1,
        "fulfillment_order_no": 1,
        "supplier_id": 1,
        "connection_id": 1,
        "split_no": 1,
        "fulfillment_status": 1,
        "cancel_status": 1,
        "refund_status": 1,
        "external_order_no": 1,
        "submitted_at": 1,
        "accepted_at": 1,
        "completed_at": 1,
        "follow_up_user_id": 1,
        "business_org_unit_id": 1,
        "version": 1,
        "created_at": 1,
    }
}

pub(crate) fn insert_id_in(filter: &mut Document, field: &str, ids: Option<&[String]>) {
    let Some(ids) = ids.filter(|values| !values.is_empty()) else {
        if ids.is_some() {
            filter.insert("$expr", false);
        }
        return;
    };
    filter.insert(field, doc! { "$in": ids });
}

pub(crate) fn and_optional(filter: &mut Document, extra: Option<Document>) {
    let Some(extra) = extra else {
        return;
    };
    let current = std::mem::take(filter);
    *filter = doc! { "$and": [current, extra] };
}

pub(crate) fn view_clause(view: Option<&str>) -> Option<Document> {
    match view {
        Some("actionable") => Some(doc! {
            "$or": [
                { "fulfillment_status": { "$in": ["RESULT_UNKNOWN", "EXCEPTION", "REJECTED", "SUBMITTING", "RECEIVED"] } },
                { "cancel_status": { "$in": ["FAILED", "MANUAL", "CANCEL_PENDING"] } },
                { "refund_status": { "$in": ["REFUND_FAILED", "MANUAL", "REFUND_PENDING"] } },
            ]
        }),
        Some("recent_completed") => Some(doc! { "fulfillment_status": "COMPLETED" }),
        _ => None,
    }
}

pub(crate) fn aftersale_pending_clause() -> Document {
    doc! {
        "$or": [
            { "cancel_status": { "$in": ["FAILED", "MANUAL", "CANCEL_PENDING"] } },
            { "refund_status": { "$in": ["REFUND_FAILED", "MANUAL", "REFUND_PENDING"] } },
        ]
    }
}
