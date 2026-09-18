//! 合同列表先关联当前修订再筛选、排序、计数，禁止分页后匹配。

use std::collections::HashMap;

use erp_core::common::time::BusinessDate;
use mongodb::bson::{Document, doc};
use persistence_core::{Executor, Pagination, QueryFilter, Result, insert_literal_regex_filter};
use serde::Deserialize;

use super::ContractExt;
use super::contract::{ContractDomainRepository, ContractFilter, ContractRow};
use crate::dto::contract::{
    ContractFilterOption, ContractMetric, ContractMetrics, ContractRevisionView, ContractView,
};

/// 可见合同引用的客户搜索事实，外域数据由端口提供。
#[derive(Debug, Clone)]
pub struct ContractCustomer {
    /// 当前负责人 ID；未分配时为空。
    pub owner_id: Option<String>,
    pub id: String,
    pub number: String,
    pub owner: String,
}

impl ContractCustomer {
    /// 解析负责人展示标签（纯规则，无 I/O）。
    ///
    /// 空显示名回退稳定用户 ID；未分配显示破折号。
    ///
    /// # 参数
    /// * `owner` - 当前主负责人 ID
    /// * `names` - 账号显示名
    ///
    /// # 返回
    /// 返回展示标签。
    ///
    /// # 关键业务约束
    /// 不得用签约经办姓名填充未分配主责。
    pub(crate) fn resolve_owner_label(owner: Option<&String>, names: &HashMap<String, String>) -> String {
        owner
            .map(|id| {
                names
                    .get(id)
                    .map(|name| name.trim())
                    .filter(|name| !name.is_empty())
                    .unwrap_or(id)
                    .to_string()
            })
            .unwrap_or_else(|| "—".to_string())
    }
}

impl ContractRow {
    /// 由行投影与已装配事实确定性组装列表视图（纯规则，无 I/O）。
    ///
    /// 同一客户事实供接口显示与搜索使用，防止搜索命中后展示另一套名称。
    ///
    /// # 参数
    /// * `self` - 合同列表投影行
    /// * `current_revision` - 当前修订摘要
    /// * `customer` - 客户编号与当前主负责人
    ///
    /// # 返回
    /// 返回列表视图。
    ///
    /// # 关键业务约束
    /// `owner_user_id` 只来自客户当前主负责人。
    pub(crate) fn into_view(
        self,
        current_revision: Option<ContractRevisionView>,
        customer: Option<&ContractCustomer>,
    ) -> ContractView {
        ContractView {
            id: self.id,
            contract_no: self.contract_no,
            customer_id: self.customer_id,
            settlement_party_id: self.settlement_party_id,
            status: self.status,
            current_revision_id: self.current_revision_id,
            current_revision,
            customer_no: customer.map(|c| c.number.clone()).filter(|number| !number.is_empty()),
            owner_user_id: customer.and_then(|c| c.owner_id.clone()),
            owner_user_name: customer.filter(|c| c.owner_id.is_some()).map(|c| c.owner.clone()),
            created_at: self.created_at,
            version: self.version,
        }
    }
}
/// 列表新增筛选，精确字段与关键词按交集执行。
#[derive(Debug, Clone, Default)]
pub struct ContractSearch {
    pub q: Option<String>,
    pub metric: Option<ContractMetric>,
    pub settlement_party_id: Option<String>,
    pub owner_user_ids: Option<application_core::QueryIds>,
    pub customers: Vec<ContractCustomer>,
}

impl ContractSearch {
    /// 字面量关键词 OR 匹配多个业务字段，结构化条件继续收窄范围。
    fn filter(&self) -> Document {
        let mut filter = Document::new();
        if let Some(id) = &self.settlement_party_id {
            filter.insert("settlement_party_id", id);
        }
        if let Some(ids) = &self.owner_user_ids {
            let customer_ids = self
                .customers
                .iter()
                .filter(|c| c.owner_id.as_ref().is_some_and(|id| ids.as_slice().contains(id)))
                .map(|c| c.id.clone())
                .collect::<Vec<_>>();
            filter.insert("customer_id", doc! { "$in": customer_ids });
        }
        apply_metric(&mut filter, self.metric);
        if let Some(q) = &self.q {
            let mut clauses = ["contract_no", "search_customer", "search_settlement", "search_owner"]
                .into_iter()
                .map(|field| {
                    let mut clause = Document::new();
                    insert_literal_regex_filter(&mut clause, field, Some(q));
                    clause
                })
                .collect::<Vec<_>>();
            let needle = q.to_lowercase();
            let ids = self
                .customers
                .iter()
                .filter(|c| c.number.to_lowercase().contains(&needle))
                .map(|c| c.id.clone())
                .collect::<Vec<_>>();
            clauses.push(doc! { "customer_id": { "$in": ids } });
            filter.insert("$or", clauses);
        }
        filter
    }

    /// MongoDB 排序与关键词使用相同当前负责人显示值。
    fn owner_expression(&self) -> Document {
        let branches = self.customers.iter().map(|c| doc! { "case": { "$eq": ["$customer_id", { "$literal": &c.id }] }, "then": { "$literal": &c.owner } }).collect::<Vec<_>>();
        if branches.is_empty() {
            return doc! { "$literal": "—" };
        }
        doc! { "$switch": { "branches": branches, "default": "—" } }
    }
}

#[derive(Debug, Deserialize)]
pub struct ContractCount {
    pub total: i64,
}
/// 单个聚合同时返回结果页、筛选总数、范围指标与结算主体候选项。
#[derive(Debug, Deserialize, Default)]
pub struct ContractSearchResult {
    pub items: Vec<ContractRow>,
    pub totals: Vec<ContractCount>,
    pub metrics: Vec<ContractMetrics>,
    pub settlement_options: Vec<ContractFilterOption>,
}
impl ContractSearchResult {
    /// 空命中聚合计数视为零。
    pub fn total(&self) -> i64 {
        self.totals.first().map(|c| c.total).unwrap_or(0)
    }
}

impl ContractDomainRepository<'_> {
    /// 只读取可见合同引用的去重客户 ID，用于批量解析当前负责人。
    ///
    /// # 错误
    /// MongoDB 查询失败。
    pub async fn list_customer_ids(
        &self,
        filter: &ContractFilter,
        executor: &mut dyn Executor,
    ) -> Result<Vec<String>> {
        let collection = self.db.collection::<Document>(<mongodb::Database as ContractExt>::CONTRACTS);
        let mut query = collection.distinct("customer_id", filter.to_doc());
        if let Some(session) = executor.session() {
            query = query.session(session);
        }
        Ok(query.await?.into_iter().filter_map(|id| id.as_str().map(str::to_owned)).collect())
    }

    /// 在拥有的合同与修订集合内完成分页；元数据基于同一可见范围。
    ///
    /// 会话/非会话两分支的游标推进与反序列化经 `advance_search_cursor` 收敛；
    /// 两分支只保留执行器接入差异，空命中返回默认值的行为不变。
    ///
    /// # 错误
    /// MongoDB 聚合或反序列化失败。
    pub async fn search_list(
        &self,
        filter: &ContractFilter,
        search: &ContractSearch,
        executor: &mut dyn Executor,
    ) -> Result<ContractSearchResult> {
        let collection = self.db.collection::<Document>(<mongodb::Database as ContractExt>::CONTRACTS);
        let pipeline = list_pipeline(filter, search, BusinessDate::today());
        run_search_aggregate(&collection, pipeline, executor).await
    }
}

/// 执行合同搜索聚合：会话与非会话只差执行器接入，游标推进与反序列化共用。
///
/// # 参数
/// * `collection` - 合同集合文档句柄
/// * `pipeline` - 已组装的搜索流水线
/// * `executor` - 调用方执行器
///
/// # 返回
/// 返回聚合首文档；空命中返回默认值。
///
/// # 错误
/// MongoDB 聚合或反序列化失败。
async fn run_search_aggregate(
    collection: &mongodb::Collection<Document>,
    pipeline: Vec<Document>,
    executor: &mut dyn Executor,
) -> Result<ContractSearchResult> {
    if let Some(session) = executor.session() {
        let mut cursor =
            collection.aggregate(pipeline).with_type::<ContractSearchResult>().session(&mut *session).await?;
        let advanced = cursor.advance(session).await.map_err(persistence_core::Error::from)?;
        return advance_search_result(advanced, || cursor.deserialize_current());
    }
    let mut cursor = collection.aggregate(pipeline).with_type::<ContractSearchResult>().await?;
    let advanced = cursor.advance().await.map_err(persistence_core::Error::from)?;
    advance_search_result(advanced, || cursor.deserialize_current())
}

/// 由游标推进结果反序列化首文档；空命中返回默认值（两分支共用）。
///
/// 会话/非会话游标类型不同，推进后的反序列化经本函数收敛：调用方只传
/// `deserialize_current()` 闭包结果的映射入口，两分支的推进差异保留在调用点。
///
/// # 参数
/// * `advanced` - 游标是否定位到首文档
/// * `deserialize` - 首文档反序列化闭包（仅 `advanced` 为真时调用）
///
/// # 返回
/// 有首文档时返回反序列化结果，否则返回默认值。
///
/// # 错误
/// 反序列化失败时返回仓储错误。
fn advance_search_result(
    advanced: bool,
    deserialize: impl FnOnce() -> mongodb::error::Result<ContractSearchResult>,
) -> Result<ContractSearchResult> {
    if advanced {
        return deserialize().map_err(persistence_core::Error::from);
    }
    Ok(ContractSearchResult::default())
}

/// 查询流水线：范围 → 当前修订 → 统一派生字段 → 分面分页与统计。
fn list_pipeline(filter: &ContractFilter, search: &ContractSearch, today: BusinessDate) -> Vec<Document> {
    vec![
        doc! { "$match": filter.to_doc() },
        doc! { "$lookup": { "from": <mongodb::Database as ContractExt>::CONTRACT_REVISIONS, "localField": "current_revision_id", "foreignField": "id", "as": "current" } },
        doc! { "$set": { "current": { "$arrayElemAt": ["$current", 0] } } },
        doc! { "$set": display_fields(search, today) },
        doc! { "$facet": {
            "items": [doc! { "$match": search.filter() }, doc! { "$sort": list_sort(filter) }, doc! { "$skip": filter.skip() as i64 }, doc! { "$limit": filter.limit() }],
            "totals": [doc! { "$match": search.filter() }, doc! { "$count": "total" }],
            "metrics": [doc! { "$match": search.filter() }, doc! { "$group": metric_group() }],
            "settlement_options": [doc! { "$sort": { "id": 1 } }, doc! { "$group": { "_id": "$settlement_party_id", "label": { "$first": "$search_settlement" } } }, doc! { "$project": { "_id": 0, "value": "$_id", "label": 1 } }, doc! { "$sort": { "label": 1, "value": 1 } }]
        } },
    ]
}

/// 到期口径为业务自然日当天至第 30 天（两端包含）。
fn display_fields(search: &ContractSearch, today: BusinessDate) -> Document {
    let mut end = today.as_naive_date();
    for _ in 0..30 {
        end = end.succ_opt().unwrap_or(end);
    }
    doc! {
        "search_customer": { "$ifNull": ["$current.customer_snapshot.customer_name", "$customer_id"] },
        "search_settlement": { "$ifNull": ["$current.settlement_party_snapshot.settlement_party_name", "$settlement_party_id"] },
        "search_owner": search.owner_expression(),
        "search_valid_to": { "$ifNull": ["$current.valid_to", "9999-12-31"] },
        "search_expiring": { "$and": [ { "$eq": ["$status", "EFFECTIVE"] }, { "$gte": ["$current.valid_to", today.to_string()] }, { "$lte": ["$current.valid_to", end.to_string()] } ] }
    }
}

/// 后端排序白名单，全部追加唯一 ID；默认到期优先与页面一致。
fn list_sort(filter: &ContractFilter) -> Document {
    let direction = if filter.sort_ascending { 1 } else { -1 };
    let field = match filter.sort_by.as_deref() {
        Some("contract_no") => "contract_no",
        Some("customer") => "search_customer",
        Some("settlement") => "search_settlement",
        Some("validity") => "search_valid_to",
        Some("revision") => "current.revision_no",
        Some("owner") => "search_owner",
        Some("expiry_priority") => return doc! { "search_expiring": -1, "search_valid_to": 1, "id": 1 },
        Some("sales") => "id",
        _ => "created_at",
    };
    let mut sort = Document::new();
    sort.insert(field, direction);
    sort.insert("id", direction);
    sort
}

/// 状态过滤与指标使用同一持久化字段。
fn apply_metric(filter: &mut Document, metric: Option<ContractMetric>) {
    match metric {
        Some(ContractMetric::Effective) => {
            filter.insert("status", "EFFECTIVE");
        },
        Some(ContractMetric::Expired) => {
            filter.insert("status", "EXPIRED");
        },
        Some(ContractMetric::Terminated) => {
            filter.insert("status", "TERMINATED");
        },
        Some(ContractMetric::Expiring30d) => {
            filter.insert("search_expiring", true);
        },
        _ => {},
    }
}

/// 范围统计不受关键词或快捷状态筛选影响。
fn metric_group() -> Document {
    doc! { "_id": null, "all": { "$sum": 1 },
        "effective": { "$sum": { "$cond": [{ "$eq": ["$status", "EFFECTIVE"] }, 1, 0] } },
        "expired": { "$sum": { "$cond": [{ "$eq": ["$status", "EXPIRED"] }, 1, 0] } },
        "terminated": { "$sum": { "$cond": [{ "$eq": ["$status", "TERMINATED"] }, 1, 0] } },
        "expiring_30d": { "$sum": { "$cond": ["$search_expiring", 1, 0] } }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 所有搜索分支仍位于权限范围之后，结果页与 total 使用同一条件。
    #[test]
    fn search_precedes_pagination_and_preserves_empty_scope() {
        let filter = ContractFilter {
            contract_no: None,
            customer_id: None,
            customer_ids: Some(vec![]),
            historical_contract_ids: vec![],
            status: None,
            page: 3,
            page_size: 20,
            sort_by: Some("expiry_priority".into()),
            sort_ascending: false,
        };
        let search = ContractSearch {
            q: Some("客户.[x]".into()),
            metric: Some(ContractMetric::Expiring30d),
            settlement_party_id: Some("party-1".into()),
            owner_user_ids: None,
            customers: vec![],
        };
        let pipeline = list_pipeline(&filter, &search, BusinessDate::from_ymd(2026, 9, 8).unwrap());
        assert_eq!(
            pipeline[0].get_document("$match").unwrap(),
            &doc! { "$and": [{ "deleted_at": 0_i64 }, { "$expr": false }] }
        );
        let facets = pipeline.last().unwrap().get_document("$facet").unwrap();
        let items = facets.get_array("items").unwrap();
        let totals = facets.get_array("totals").unwrap();
        assert_eq!(items[0], totals[0]);
        assert_eq!(items[2].as_document().unwrap(), &doc! { "$skip": 40_i64 });
        let matching = items[0].as_document().unwrap().get_document("$match").unwrap();
        assert_eq!(matching.get_str("settlement_party_id").unwrap(), "party-1");
        assert!(matching.get_bool("search_expiring").unwrap());
        let first =
            matching.get_array("$or").unwrap()[0].as_document().unwrap().get_document("contract_no").unwrap();
        assert_eq!(first.get_str("$regex").unwrap(), r"客户\.\[x\]");
        assert_eq!(first.get_str("$options").unwrap(), "i");
    }

    /// 候选项来自完整范围，重复负责人合并，编号命中不会扩大客户范围。
    #[test]
    fn foreign_search_and_owner_options_are_not_page_derived() {
        let search = ContractSearch {
            q: Some("c-101".into()),
            metric: None,
            settlement_party_id: None,
            owner_user_ids: Some(serde_json::from_str("\"user-1\"").unwrap()),
            customers: vec![
                ContractCustomer {
                    owner_id: Some("user-1".into()),
                    id: "customer-101".into(),
                    number: "C-101".into(),
                    owner: "张三".into(),
                },
                ContractCustomer {
                    owner_id: Some("user-2".into()),
                    id: "customer-102".into(),
                    number: "C-102".into(),
                    owner: "张三".into(),
                },
            ],
        };

        assert_eq!(
            search.filter().get_array("$or").unwrap().last().unwrap().as_document().unwrap(),
            &doc! { "customer_id": { "$in": ["customer-101"] } }
        );
        assert_eq!(search.filter().get_document("customer_id").unwrap(), &doc! { "$in": ["customer-101"] });
    }

    /// 到期边界使用业务日并保留空页计数。
    #[test]
    fn expiry_uses_inclusive_business_dates_and_empty_page_count() {
        let search = ContractSearch {
            q: None,
            metric: None,
            settlement_party_id: None,
            owner_user_ids: None,
            customers: vec![],
        };
        let fields = display_fields(&search, BusinessDate::from_ymd(2026, 9, 8).unwrap());
        let expression = fields.get_document("search_expiring").unwrap().get_array("$and").unwrap();
        assert_eq!(
            expression[1].as_document().unwrap(),
            &doc! { "$gte": ["$current.valid_to", "2026-09-08"] }
        );
        assert_eq!(
            expression[2].as_document().unwrap(),
            &doc! { "$lte": ["$current.valid_to", "2026-10-08"] }
        );
        assert_eq!(ContractSearchResult::default().total(), 0);
    }

    #[test]
    fn owner_label_falls_back_to_id_and_dash() {
        use std::collections::HashMap;

        let names: HashMap<String, String> =
            [("user-1".to_string(), " 张三 ".to_string()), ("user-2".to_string(), "   ".to_string())]
                .into_iter()
                .collect();
        assert_eq!(ContractCustomer::resolve_owner_label(Some(&"user-1".to_string()), &names), "张三");
        assert_eq!(ContractCustomer::resolve_owner_label(Some(&"user-2".to_string()), &names), "user-2");
        assert_eq!(ContractCustomer::resolve_owner_label(None, &names), "—");
    }

    #[test]
    fn row_into_view_keeps_owner_only_with_assignment() {
        let row = ContractRow {
            id: "c-1".into(),
            contract_no: "HT-1".into(),
            customer_id: "cust-1".into(),
            settlement_party_id: "party-1".into(),
            status: crate::entity::contract::ContractStatus::Effective,
            current_revision_id: Some("rev-1".into()),
            version: 2,
            created_at: 1_700_000_000,
            updated_at: 1_700_000_100,
        };
        let assigned = ContractCustomer {
            owner_id: Some("user-1".into()),
            id: "cust-1".into(),
            number: "C-1".into(),
            owner: "张三".into(),
        };
        let view = row.clone().into_view(None, Some(&assigned));
        assert_eq!(view.customer_no.as_deref(), Some("C-1"));
        assert_eq!(view.owner_user_id.as_deref(), Some("user-1"));
        assert_eq!(view.owner_user_name.as_deref(), Some("张三"));

        let unassigned = ContractCustomer {
            owner_id: None,
            id: "cust-1".into(),
            number: String::new(),
            owner: "—".into(),
        };
        let bare = row.into_view(None, Some(&unassigned));
        assert_eq!(bare.customer_no, None);
        assert_eq!(bare.owner_user_id, None);
        assert_eq!(bare.owner_user_name, None);
    }
}
