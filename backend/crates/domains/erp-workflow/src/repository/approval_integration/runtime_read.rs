//! 审批运行实例与不可变业务快照的联合只读仓储。

use std::collections::BTreeMap;

use bpm::ProcessKind;
use entity_core::NOT_DELETED_TIMESTAMP_BSON;
use erp_identity::CompiledAccess;
use erp_identity::entity::authorization::QueryOutput;
use mongodb::bson::{Bson, Document, doc};
use mongodb::{Collection, Database};
use persistence_core::{Executor, Result, aggregate};
use serde::Deserialize;
use serde::de::DeserializeOwned;

use super::super::bpm::{
    ApprovalInstanceListFilter, ApprovalInstanceListView, ApprovalInstanceSummary, instance_cursor_or,
    instance_list_filter_doc, instance_list_limit, instance_list_scope_empty, instance_list_sort,
    instance_summary_projection,
};
use super::super::extensions::{ApprovalIntegrationExt, BpmExt};
use super::facet_total_or_empty;
use crate::entity::approval_integration::{ApprovalSubjectSnapshot, document_type_of};

const INSTANCES: &str = <Database as BpmExt>::APPROVAL_PROCESS_INSTANCES;
const SNAPSHOTS: &str = <Database as ApprovalIntegrationExt>::APPROVAL_SUBJECT_SNAPSHOTS;

/// 本次业务检索支持的单一流程种类。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ApprovalRuntimeReadTypeScope {
    /// 已接入审批运行时且符合本次类型筛选的流程种类。
    pub process_kind: ProcessKind,
}

/// 业务检索的运行实例范围；完整授权由中央查询凭证约束。
///
/// Repository 接收业务流程种类和中央查询凭证；完整业务来源授权在数据库分页前执行。
/// 管理视图不得使用发起人读取旁路。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ApprovalRuntimeReadScope {
    /// 本人发起的普通读取；无需从展示快照证明组织。
    Started {
        /// 允许进入审批运行时的流程种类。
        process_kinds: Vec<ProcessKind>,
        /// 必须与实例 `started_by` 过滤完全一致的当前账号。
        submitted_by: String,
    },
    /// Managed/Blocked 管理读取；真实来源条件由中央查询凭证提供。
    Managed {
        /// 本次检索支持的流程种类；具体管理资格由中央查询条件证明。
        type_scopes: Vec<ApprovalRuntimeReadTypeScope>,
    },
}

/// 经授权范围过滤后的审批实例列表行。
#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
pub struct ApprovalRuntimeReadRow {
    /// BPM 有界列表投影。
    #[serde(flatten)]
    pub instance: ApprovalInstanceSummary,
    /// 与实例精确三元组匹配的唯一冻结快照；缺失或漂移时为空。
    pub snapshot: Option<ApprovalSubjectSnapshot>,
}

/// 经不可变快照范围过滤后的审批实例页。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ApprovalRuntimeReadPage {
    /// 当前稳定游标页。
    pub items: Vec<ApprovalRuntimeReadRow>,
    /// 不含游标的完整授权范围总数。
    pub total: u64,
    /// 同一授权与检索条件下的完整单据类型数量，不受游标影响。
    pub document_type_counts: BTreeMap<String, u64>,
}

/// 审批运行实例与不可变业务快照的联合只读仓储。
pub struct ApprovalRuntimeReadRepository<'a> {
    db: &'a Database,
}

#[derive(Debug, Deserialize)]
struct ApprovalRuntimeReadFacet {
    #[serde(default)]
    items: Vec<ApprovalRuntimeReadRow>,
    #[serde(default)]
    total: Vec<ApprovalRuntimeReadCount>,
    #[serde(default)]
    document_type_counts: Vec<ApprovalRuntimeTypeCount>,
}

#[derive(Debug, Deserialize)]
struct ApprovalRuntimeTypeCount {
    #[serde(rename = "_id")]
    process_kind: ProcessKind,
    count: i64,
}

#[derive(Debug, Deserialize)]
struct ApprovalRuntimeReadCount {
    count: i64,
}

#[derive(Debug, Deserialize)]
struct ApprovalRuntimeReadScanFacet {
    #[serde(default)]
    items: Vec<ApprovalRuntimeReadRow>,
    #[serde(default)]
    last: Vec<ApprovalInstanceSummary>,
}

impl<'a> ApprovalRuntimeReadRepository<'a> {
    /// 在中央授权过滤后执行原实例查询的检索、计数和分页。
    /// # 参数
    /// filter、scope为原业务筛选，access为当前实例查询凭证，executor为原快照。
    /// # 返回
    /// 完整来源授权后的页面与完整计数。
    /// # 错误
    /// 模型错配、凭证过期或数据库失败时拒绝。
    pub async fn search_authorized(
        &self,
        filter: &ApprovalInstanceListFilter,
        scope: &ApprovalRuntimeReadScope,
        access: &CompiledAccess,
        executor: &mut dyn Executor,
    ) -> crate::Result<ApprovalRuntimeReadPage> {
        let pipeline = authorized_pipeline(access, runtime_read_pipeline(filter, scope), executor)?;
        if runtime_read_scope_empty(filter, scope) {
            return Ok(ApprovalRuntimeReadPage::default());
        }
        let rows = aggregate_runtime_read::<ApprovalRuntimeReadFacet>(
            &self.db.collection::<Document>(INSTANCES),
            pipeline,
            executor,
        )
        .await?;
        Ok(runtime_read_page(rows.into_iter().next())?)
    }

    /// 在中央授权条件后读取本人发起实例的有界候选批次。
    /// # 参数
    /// filter、scope为原业务筛选，access为当前实例凭证，executor为原读取事务。
    /// # 返回
    /// 原稳定游标语义的候选与末行。
    /// # 错误
    /// 凭证错配、跨阶段或数据库错误拒绝。
    pub async fn scan_authorized(
        &self,
        filter: &ApprovalInstanceListFilter,
        scope: &ApprovalRuntimeReadScope,
        access: &CompiledAccess,
        executor: &mut dyn Executor,
    ) -> crate::Result<(Vec<ApprovalRuntimeReadRow>, Option<ApprovalInstanceSummary>)> {
        let pipeline = authorized_pipeline(access, runtime_scan_pipeline(filter, scope), executor)?;
        if runtime_read_scope_empty(filter, scope) {
            return Ok((Vec::new(), None));
        }
        let rows = aggregate_runtime_read::<ApprovalRuntimeReadScanFacet>(
            &self.db.collection::<Document>(INSTANCES),
            pipeline,
            executor,
        )
        .await?;
        let Some(facet) = rows.into_iter().next() else { return Ok((Vec::new(), None)) };
        Ok((facet.items, facet.last.into_iter().next()))
    }
    /// 创建审批运行联合只读仓储。
    ///
    /// # 参数
    /// * `db` - 当前 MongoDB 数据库
    ///
    /// # 返回
    /// 返回不自行开启事务的只读仓储。
    ///
    /// # 错误
    /// 不返回错误。
    pub fn new(db: &'a Database) -> Self {
        Self { db }
    }
}

/// 仓储只消费同一根实例的完整对象凭证，授权条件位于所有业务分页前。
fn authorized_pipeline(
    access: &CompiledAccess,
    business: Vec<Document>,
    executor: &mut dyn Executor,
) -> crate::Result<Vec<Document>> {
    let model = access.model();
    if model.resource != "approval_instance"
        || model.collection != INSTANCES
        || model.id_field != "id"
        || model.output != QueryOutput::Object
    {
        return Err(crate::Error::ValidationError("审批实例授权模型不匹配".into()));
    }
    access.ensure_current(executor)?;
    Ok(access.stages().iter().cloned().chain(business).collect())
}

/// 用调用方执行器执行联合读取，返回指定的有界聚合投影。
async fn aggregate_runtime_read<T: DeserializeOwned + Send + Sync>(
    collection: &Collection<Document>,
    pipeline: Vec<Document>,
    executor: &mut dyn Executor,
) -> Result<Vec<T>> {
    aggregate::rows(&collection.clone_with_type::<T>(), pipeline, executor).await
}

/// 从联合分页分支恢复完整总数与当前页。
fn runtime_read_page(facet: Option<ApprovalRuntimeReadFacet>) -> Result<ApprovalRuntimeReadPage> {
    let Some(facet) = facet else {
        return Ok(ApprovalRuntimeReadPage::default());
    };
    let total = facet_total_or_empty(facet.total.first().map(|row| row.count), "approval_runtime_total")?;
    let document_type_counts = facet
        .document_type_counts
        .into_iter()
        .map(|row| {
            let count = facet_total_or_empty(Some(row.count), "approval_runtime_type_total")?;
            Ok((document_type_of(row.process_kind).as_str().to_string(), count))
        })
        .collect::<Result<BTreeMap<_, _>>>()?;
    Ok(ApprovalRuntimeReadPage { items: facet.items, total, document_type_counts })
}

/// 拒绝空范围、错接视图和不在已证明范围内的请求类型。
fn runtime_read_scope_empty(filter: &ApprovalInstanceListFilter, scope: &ApprovalRuntimeReadScope) -> bool {
    let process_kinds = runtime_read_process_kinds(scope);
    instance_list_scope_empty(filter)
        || process_kinds.is_empty()
        || filter.process_kind.is_some_and(|requested| !process_kinds.contains(&requested))
        || match scope {
            ApprovalRuntimeReadScope::Started { submitted_by, .. } => {
                filter.view != ApprovalInstanceListView::Started
                    || submitted_by.is_empty()
                    || filter.started_by.as_deref() != Some(submitted_by)
            },
            ApprovalRuntimeReadScope::Managed { .. } => filter.view == ApprovalInstanceListView::Started,
        }
}

/// 构造实例列表与唯一快照的授权聚合。
///
/// 游标只进入 `items` facet；`total` 在同一授权、检索与状态条件上独立计数。
fn runtime_read_pipeline(
    filter: &ApprovalInstanceListFilter,
    scope: &ApprovalRuntimeReadScope,
) -> Vec<Document> {
    let mut base_filter = filter.clone();
    base_filter.cursor = None;
    let mut pipeline = vec![
        doc! { "$match": runtime_instance_match(&base_filter, scope) },
        doc! { "$sort": instance_list_sort(filter) },
    ];
    pipeline.extend(runtime_snapshot_stages(filter));
    pipeline.push(doc! { "$facet": runtime_read_facets(filter) });
    pipeline
}

/// 游标和批大小先收窄实例，冻结快照只关联本批候选。
fn runtime_scan_pipeline(
    filter: &ApprovalInstanceListFilter,
    scope: &ApprovalRuntimeReadScope,
) -> Vec<Document> {
    let mut items = runtime_snapshot_stages(filter);
    items.push(doc! { "$project": runtime_read_projection() });
    let mut cursor_projection = instance_summary_projection();
    cursor_projection.insert("_id", 0);
    vec![
        doc! { "$match": runtime_instance_match(filter, scope) },
        doc! { "$sort": instance_list_sort(filter) },
        doc! { "$limit": instance_list_limit(filter.limit) },
        doc! { "$facet": {
            "items": items,
            "last": [
                { "$group": { "_id": Bson::Null, "last": { "$last": "$$ROOT" } } },
                { "$replaceWith": "$last" },
                { "$project": cursor_projection },
            ],
        } },
    ]
}

/// 编译类型、状态及实例游标；快照字段检索由关联分支处理。
fn runtime_instance_match(filter: &ApprovalInstanceListFilter, scope: &ApprovalRuntimeReadScope) -> Document {
    let mut base_filter = filter.clone();
    base_filter.text_query = None;
    let mut instance_match = instance_list_filter_doc(&base_filter);
    if base_filter.process_kind.is_none() {
        instance_match.insert(
            "process_kind",
            doc! {
                "$in": runtime_read_process_kinds(scope)
                    .into_iter()
                    .map(ProcessKind::as_str)
                    .collect::<Vec<_>>()
            },
        );
    }
    instance_match.insert("$expr", doc! { "$eq": ["$process_kind", "$subject.subject_kind"] });
    instance_match
}

/// 仅保留列表和检索所需冻结事实，排除提交材料及展示明细。
fn runtime_snapshot_stages(filter: &ApprovalInstanceListFilter) -> Vec<Document> {
    let mut pipeline = vec![
        doc! {
            "$lookup": {
                "from": SNAPSHOTS,
                "localField": "id",
                "foreignField": "approval_process_instance_id",
                "as": "_runtime_snapshots",
                "pipeline": [{ "$project": {
                    "id": 1, "version": 1, "created_at": 1, "updated_at": 1, "deleted_at": 1,
                    "approval_process_instance_id": 1, "document_type": 1,
                    "business_object_id": 1, "subject_version": 1, "payload": 1,
                    "display.counterparty_label": 1, "display.source.customer": 1,
                } }],
            }
        },
        doc! {
            "$set": {
                "_runtime_live_snapshots": {
                    "$filter": {
                        "input": "$_runtime_snapshots",
                        "as": "snapshot",
                        "cond": { "$eq": ["$$snapshot.deleted_at", NOT_DELETED_TIMESTAMP_BSON] },
                    }
                }
            }
        },
        doc! {
            "$set": {
                "_runtime_snapshot": { "$arrayElemAt": ["$_runtime_live_snapshots", 0] }
            }
        },
        doc! { "$set": { "_runtime_snapshot_exact": runtime_snapshot_exact_expr() } },
        doc! { "$match": { "_runtime_snapshot_exact": true } },
    ];
    if let Some(text_query) = &filter.text_query {
        pipeline.push(doc! { "$match": runtime_text_match(&text_query.query) });
    }
    pipeline.push(doc! { "$unset": "_runtime_snapshot.display" });
    pipeline
}

/// 证明仅一个未删除快照与实例、类型、对象及提交版本精确一致。
fn runtime_snapshot_exact_expr() -> Document {
    doc! {
        "$and": [
            { "$eq": [{ "$size": "$_runtime_live_snapshots" }, 1] },
            { "$eq": ["$_runtime_snapshot.approval_process_instance_id", "$id"] },
            { "$eq": ["$_runtime_snapshot.document_type", "$process_kind"] },
            { "$eq": ["$_runtime_snapshot.document_type", "$subject.subject_kind"] },
            { "$eq": ["$_runtime_snapshot.business_object_id", "$subject.subject_id"] },
            { "$eq": ["$_runtime_snapshot.subject_version", "$subject_version"] },
        ]
    }
}

/// 返回去重并稳定排序的已证明流程种类。
fn runtime_read_process_kinds(scope: &ApprovalRuntimeReadScope) -> Vec<ProcessKind> {
    let mut process_kinds = match scope {
        ApprovalRuntimeReadScope::Started { process_kinds, .. } => process_kinds.clone(),
        ApprovalRuntimeReadScope::Managed { type_scopes } => {
            type_scopes.iter().map(|type_scope| type_scope.process_kind).collect::<Vec<_>>()
        },
    };
    process_kinds.sort_by_key(|process_kind| process_kind.as_str());
    process_kinds.dedup();
    process_kinds
}

/// 在实例和精确快照中执行同一字面量检索。
fn runtime_text_match(query: &str) -> Document {
    let literal = regex::escape(query.trim());
    let regex = doc! { "$regex": literal, "$options": "i" };
    doc! {
        "$or": [
            { "subject.subject_id": regex.clone() },
            { "current_assignee_name": regex.clone() },
            { "current_node_name": regex.clone() },
            {
                "$and": [
                    { "_runtime_snapshot_exact": true },
                    { "$or": [
                        { "_runtime_snapshot.payload.document_no": regex.clone() },
                        { "_runtime_snapshot.display.counterparty_label": regex.clone() },
                        { "_runtime_snapshot.display.source.customer": regex },
                    ] },
                ]
            },
        ]
    }
}

/// 当前页应用游标；总数保持完整的精确快照匹配范围。
fn runtime_read_facets(filter: &ApprovalInstanceListFilter) -> Document {
    let mut items = Vec::new();
    if let Some(cursor) = &filter.cursor {
        items.push(doc! { "$match": { "$or": instance_cursor_or(filter.view, cursor) } });
    }
    items.push(doc! { "$limit": instance_list_limit(filter.limit) });
    items.push(doc! { "$project": runtime_read_projection() });
    doc! {
        "items": items,
        "total": [{ "$count": "count" }],
        "document_type_counts": [{ "$group": { "_id": "$process_kind", "count": { "$sum": 1 } } }],
    }
}

/// 返回列表摘要与已证明的精确冻结载荷，不带提交材料和展示明细。
fn runtime_read_projection() -> Document {
    let mut projection = instance_summary_projection();
    projection.insert(
        "snapshot",
        doc! {
            "$cond": ["$_runtime_snapshot_exact", "$_runtime_snapshot", Bson::Null]
        },
    );
    projection.insert("_id", 0);
    projection
}

#[cfg(test)]
mod tests {
    use bpm::ProcessKind;
    use bpm::model::types::ApprovalProcessInstanceStatus;
    use mongodb::bson::{Bson, deserialize_from_document, doc};

    use super::{
        ApprovalRuntimeReadScope, ApprovalRuntimeReadTypeScope, runtime_read_page, runtime_read_pipeline,
        runtime_read_scope_empty, runtime_scan_pipeline,
    };
    use crate::repository::bpm::{
        ApprovalInstanceListCursor, ApprovalInstanceListFilter, ApprovalInstanceListView,
        ApprovalInstanceTextQuery,
    };

    fn runtime_filter() -> ApprovalInstanceListFilter {
        ApprovalInstanceListFilter {
            view: ApprovalInstanceListView::Blocked,
            process_kind: None,
            status: Some(ApprovalProcessInstanceStatus::Blocked),
            started_by: None,
            subject_kind: None,
            authorized_instance_ids: None,
            subject_ids: None,
            text_query: Some(ApprovalInstanceTextQuery { query: "ADJ.[1]".to_string() }),
            cursor: Some(ApprovalInstanceListCursor { sort_time: 20, id: "inst-2".to_string() }),
            limit: 21,
        }
    }

    fn runtime_type_scopes() -> Vec<ApprovalRuntimeReadTypeScope> {
        vec![
            ApprovalRuntimeReadTypeScope { process_kind: ProcessKind::StockAdjustment },
            ApprovalRuntimeReadTypeScope { process_kind: ProcessKind::SalesOrder },
        ]
    }

    fn runtime_scope() -> ApprovalRuntimeReadScope {
        ApprovalRuntimeReadScope::Managed { type_scopes: runtime_type_scopes() }
    }

    /// 类型数量覆盖完整结果，即使游标页为空仍保留各类型总数。
    #[test]
    fn runtime_type_counts_survive_empty_page() {
        let facet = deserialize_from_document(doc! {
            "items": [],
            "total": [{ "count": 53_i64 }],
            "document_type_counts": [
                { "_id": ProcessKind::PurchaseOrder.as_str(), "count": 31_i64 },
                { "_id": ProcessKind::PurchasePaymentRequest.as_str(), "count": 22_i64 },
            ],
        })
        .unwrap();
        let page = runtime_read_page(Some(facet)).unwrap();
        assert!(page.items.is_empty());
        assert_eq!(page.total, 53);
        assert_eq!(page.document_type_counts.get("purchase_order"), Some(&31));
        assert_eq!(page.document_type_counts.get("purchase_payment_request"), Some(&22));
    }

    /// 空授权结果不伪造类型；损坏计数必须失败。
    #[test]
    fn runtime_type_counts_reject_negative_and_allow_empty() {
        assert!(runtime_read_page(None).unwrap().document_type_counts.is_empty());
        let facet = deserialize_from_document(doc! {
            "document_type_counts": [
                { "_id": ProcessKind::PurchaseOrder.as_str(), "count": -1_i64 },
            ],
        })
        .unwrap();
        assert!(runtime_read_page(Some(facet)).is_err());
    }

    #[test]
    fn runtime_pipeline_filters_scope_and_exact_snapshot_before_facet() {
        let pipeline = runtime_read_pipeline(&runtime_filter(), &runtime_scope());
        let instance_match = pipeline[0].get_document("$match").unwrap();
        assert_eq!(
            instance_match.get_document("process_kind").unwrap(),
            &doc! { "$in": ["sales_order", "stock_adjustment"] }
        );
        assert_eq!(
            instance_match.get_document("$expr").unwrap(),
            &doc! { "$eq": ["$process_kind", "$subject.subject_kind"] }
        );
        assert!(!instance_match.contains_key("$or"));

        assert_eq!(pipeline[1].get_document("$sort").unwrap(), &doc! { "blocked_at": -1, "id": -1 });
        let lookup = pipeline[2].get_document("$lookup").unwrap();
        assert_eq!(lookup.get_str("localField").unwrap(), "id");
        assert_eq!(lookup.get_str("foreignField").unwrap(), "approval_process_instance_id");
        let exact = pipeline[5]
            .get_document("$set")
            .unwrap()
            .get_document("_runtime_snapshot_exact")
            .unwrap()
            .to_string();
        for field in
            ["approval_process_instance_id", "document_type", "business_object_id", "subject_version"]
        {
            assert!(exact.contains(field));
        }
        assert_eq!(pipeline[6], doc! { "$match": { "_runtime_snapshot_exact": true } });
        let text = pipeline[7].get_document("$match").unwrap().to_string();
        assert!(text.contains("_runtime_snapshot.payload.document_no"));
        assert!(text.contains("_runtime_snapshot_exact"));
        assert!(text.contains(r"ADJ\.\[1\]"));
        let facet = pipeline[9].get_document("$facet").unwrap();
        let items = facet.get_array("items").unwrap();
        assert!(items[0].as_document().unwrap().contains_key("$match"));
        assert!(
            items
                .iter()
                .any(|stage| { stage.as_document().is_some_and(|document| document.contains_key("$limit")) })
        );
        assert_eq!(facet.get_array("total").unwrap(), &vec![Bson::Document(doc! { "$count": "count" })]);
        assert!(!facet.get_array("total").unwrap()[0].as_document().unwrap().contains_key("$match"));
    }

    #[test]
    fn runtime_managed_scope_requires_exact_snapshot() {
        let scope = runtime_scope();
        let pipeline = runtime_read_pipeline(&runtime_filter(), &scope);
        assert_eq!(pipeline[6], doc! { "$match": { "_runtime_snapshot_exact": true } });
        assert!(pipeline.iter().all(|stage| {
            stage
                .get_document("$match")
                .map_or(true, |filter| !filter.to_string().contains("responsible_org_id"))
        }));
        let projection = pipeline
            .last()
            .unwrap()
            .get_document("$facet")
            .unwrap()
            .get_array("items")
            .unwrap()
            .last()
            .unwrap()
            .as_document()
            .unwrap()
            .get_document("$project")
            .unwrap();
        assert!(projection.get_document("snapshot").unwrap().contains_key("$cond"));
    }

    /// 批次的游标和上限先进入候选集合；零匹配批次仍返回原始末行用于继续扫描。
    #[test]
    fn runtime_scan_bounds_snapshot_reads_and_keeps_raw_last_candidate() {
        let filter = runtime_filter();
        let pipeline = runtime_scan_pipeline(&filter, &runtime_scope());
        let instance_match = pipeline[0].get_document("$match").unwrap();
        assert_eq!(
            instance_match.get_array("$or").unwrap(),
            &vec![
                Bson::Document(doc! { "blocked_at": { "$lt": 20_i64 } }),
                Bson::Document(doc! { "blocked_at": 20_i64, "id": { "$lt": "inst-2" } }),
            ]
        );
        assert_eq!(pipeline[2], doc! { "$limit": 21_i64 });
        let facet = pipeline[3].get_document("$facet").unwrap();
        assert_eq!(facet.len(), 2);
        assert!(!facet.contains_key("total"));
        assert!(facet.get_array("items").unwrap()[0].as_document().unwrap().contains_key("$lookup"));
        assert_eq!(
            facet.get_array("last").unwrap()[0],
            Bson::Document(doc! {
                "$group": { "_id": Bson::Null, "last": { "$last": "$$ROOT" } },
            })
        );
        assert_eq!(facet.get_array("last").unwrap().len(), 3);
    }

    #[test]
    fn runtime_scope_rejects_empty_and_requested_type_outside_proven_types() {
        let filter = runtime_filter();
        let empty = ApprovalRuntimeReadScope::Managed { type_scopes: Vec::new() };
        assert!(runtime_read_scope_empty(&filter, &empty));

        let requested =
            ApprovalInstanceListFilter { process_kind: Some(ProcessKind::PurchaseOrder), ..filter };
        assert!(runtime_read_scope_empty(&requested, &runtime_scope()));
    }

    #[test]
    fn started_scope_requires_matching_view_and_actor_without_snapshot_scope() {
        let mut filter = runtime_filter();
        filter.view = ApprovalInstanceListView::Started;
        filter.started_by = Some("starter".to_string());
        let matching = ApprovalRuntimeReadScope::Started {
            process_kinds: vec![ProcessKind::StockAdjustment],
            submitted_by: "starter".to_string(),
        };
        assert!(!runtime_read_scope_empty(&filter, &matching));
        let pipeline = runtime_read_pipeline(&filter, &matching);
        assert_eq!(pipeline[6], doc! { "$match": { "_runtime_snapshot_exact": true } });
        assert!(runtime_read_scope_empty(
            &filter,
            &ApprovalRuntimeReadScope::Started {
                process_kinds: vec![ProcessKind::StockAdjustment],
                submitted_by: "other".to_string(),
            }
        ));
        assert!(runtime_read_scope_empty(&runtime_filter(), &matching));
    }
}
