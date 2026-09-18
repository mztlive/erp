//! 审批运行实例与不可变业务快照的联合只读仓储。

use bpm::ProcessKind;
use entity_core::NOT_DELETED_TIMESTAMP_BSON;
use futures_util::TryStreamExt;
use mongodb::bson::{Bson, Document, doc};
use mongodb::{Collection, Database};
use persistence_core::{Executor, Result};
use serde::Deserialize;

use super::super::bpm::{
    ApprovalInstanceListFilter, ApprovalInstanceListView, ApprovalInstanceSummary, instance_cursor_or,
    instance_list_filter_doc, instance_list_limit, instance_list_scope_empty, instance_list_sort,
    instance_summary_projection,
};
use super::super::extensions::{ApprovalIntegrationExt, BpmExt};
use super::facet_total_or_empty;
use crate::entity::approval_integration::ApprovalSubjectSnapshot;

const INSTANCES: &str = <Database as BpmExt>::APPROVAL_PROCESS_INSTANCES;
const SNAPSHOTS: &str = <Database as ApprovalIntegrationExt>::APPROVAL_SUBJECT_SNAPSHOTS;

/// Service 已证明的单一流程种类读取范围。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ApprovalRuntimeReadTypeScope {
    /// 已证明可进入当前视图的流程种类。
    pub process_kind: ProcessKind,
    /// 管理视图中由对象读取权限对应角色得出的组织范围；`None` 表示不需要
    /// 具体组织证明（本人发起或公司级范围）。
    pub organization_ids: Option<Vec<String>>,
}

/// Service 已证明的运行实例读取范围。
///
/// Repository 只接收发起人事实，或逐流程种类绑定的组织范围，不读取或解释
/// RBAC、角色和 DataScopeFact。枚举形态禁止把 Started 错接到管理 DataScopeFact，也
/// 禁止把管理视图伪装成发起人旁路。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ApprovalRuntimeReadScope {
    /// 本人发起的普通读取；无需从展示快照证明组织。
    Started {
        /// 允许进入审批运行时的流程种类。
        process_kinds: Vec<ProcessKind>,
        /// 必须与实例 `started_by` 过滤完全一致的当前账号。
        submitted_by: String,
    },
    /// Managed/Blocked 管理读取；每类流程绑定自己的对象权限 DataScopeFact。
    Managed {
        /// 已证明类型级运行管理、对象读取权限及其组织范围的流程种类。
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
}

#[derive(Debug, Deserialize)]
struct ApprovalRuntimeReadCount {
    count: i64,
}

impl<'a> ApprovalRuntimeReadRepository<'a> {
    /// 创建审批运行联合只读仓储。
    ///
    /// # 参数
    /// * `db` - 当前 MongoDB 数据库
    ///
    /// # 返回
    /// 返回不自行开启事务的只读仓储。
    pub fn new(db: &'a Database) -> Self {
        Self { db }
    }

    /// 在 MongoDB 内完成快照授权范围、精确三元组、检索、计数与分页。
    ///
    /// # 参数
    /// * `filter` - 视图、状态、字面量检索、游标与页大小
    /// * `scope` - Service 已证明的流程种类、责任组织和可选提交人
    /// * `executor` - 数据访问执行器
    ///
    /// # 返回
    /// 返回当前稳定页与不含游标的完整授权总数。
    ///
    /// # 错误
    /// MongoDB 聚合、反序列化或计数越界时返回错误。
    ///
    /// # 关键业务约束
    /// 空类型、显式空组织和无发起人的 Started 范围必须在访问数据库前返回空页；
    /// Repository 不得解释 RBAC，且不得在分页后过滤授权事实。
    pub async fn search(
        &self,
        filter: &ApprovalInstanceListFilter,
        scope: &ApprovalRuntimeReadScope,
        executor: &mut dyn Executor,
    ) -> Result<ApprovalRuntimeReadPage> {
        if runtime_read_scope_empty(filter, scope) {
            return Ok(ApprovalRuntimeReadPage { items: Vec::new(), total: 0 });
        }
        let rows = aggregate_runtime_read(
            &self.db.collection::<Document>(INSTANCES),
            runtime_read_pipeline(filter, scope),
            executor,
        )
        .await?;
        runtime_read_page(rows.into_iter().next())
    }
}

async fn aggregate_runtime_read(
    collection: &Collection<Document>,
    pipeline: Vec<Document>,
    executor: &mut dyn Executor,
) -> Result<Vec<ApprovalRuntimeReadFacet>> {
    match executor.session() {
        Some(session) => Ok(collection
            .aggregate(pipeline)
            .with_type::<ApprovalRuntimeReadFacet>()
            .session(&mut *session)
            .await?
            .stream(session)
            .try_collect::<Vec<_>>()
            .await?),
        None => Ok(collection
            .aggregate(pipeline)
            .with_type::<ApprovalRuntimeReadFacet>()
            .await?
            .try_collect::<Vec<_>>()
            .await?),
    }
}

fn runtime_read_page(facet: Option<ApprovalRuntimeReadFacet>) -> Result<ApprovalRuntimeReadPage> {
    let Some(facet) = facet else {
        return Ok(ApprovalRuntimeReadPage { items: Vec::new(), total: 0 });
    };
    let total = facet_total_or_empty(facet.total.first().map(|row| row.count), "approval_runtime_total")?;
    Ok(ApprovalRuntimeReadPage { items: facet.items, total })
}

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
    let mut pipeline = vec![
        doc! { "$match": instance_match },
        doc! { "$sort": instance_list_sort(filter) },
        doc! {
            "$lookup": {
                "from": SNAPSHOTS,
                "localField": "id",
                "foreignField": "approval_process_instance_id",
                "as": "_runtime_snapshots",
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
    ];
    if let Some(scope_match) = runtime_snapshot_scope_match(scope) {
        pipeline.push(doc! { "$match": scope_match });
    }
    if let Some(text_query) = &filter.text_query {
        pipeline.push(doc! { "$match": runtime_text_match(&text_query.query) });
    }
    pipeline.push(doc! { "$facet": runtime_read_facets(filter) });
    pipeline
}

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

/// 组织范围不是公司级时，必须由精确快照证明责任组织。
///
/// 缺失或漂移快照不得被用于组织授权；本人发起或公司级范围无需从快照推断
/// 组织，因而可保留实例并把快照投影为空。
fn runtime_snapshot_scope_match(scope: &ApprovalRuntimeReadScope) -> Option<Document> {
    let ApprovalRuntimeReadScope::Managed { type_scopes } = scope else {
        return None;
    };
    let branches = type_scopes
        .iter()
        .filter(|type_scope| !type_scope.organization_ids.as_ref().is_some_and(Vec::is_empty))
        .map(|type_scope| match &type_scope.organization_ids {
            None => doc! { "process_kind": type_scope.process_kind.as_str() },
            Some(organization_ids) => doc! {
                "process_kind": type_scope.process_kind.as_str(),
                "$expr": {
                    "$and": [
                        "$_runtime_snapshot_exact",
                        {
                            "$in": [
                                "$_runtime_snapshot.payload.responsible_org_id",
                                organization_ids.clone(),
                            ]
                        },
                    ]
                },
            },
        })
        .collect::<Vec<_>>();
    (!branches.is_empty()).then(|| doc! { "$or": branches })
}

fn runtime_read_process_kinds(scope: &ApprovalRuntimeReadScope) -> Vec<ProcessKind> {
    let mut process_kinds = match scope {
        ApprovalRuntimeReadScope::Started { process_kinds, .. } => process_kinds.clone(),
        ApprovalRuntimeReadScope::Managed { type_scopes } => type_scopes
            .iter()
            .filter(|type_scope| !type_scope.organization_ids.as_ref().is_some_and(Vec::is_empty))
            .map(|type_scope| type_scope.process_kind)
            .collect::<Vec<_>>(),
    };
    process_kinds.sort_by_key(|process_kind| process_kind.as_str());
    process_kinds.dedup();
    process_kinds
}

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

fn runtime_read_facets(filter: &ApprovalInstanceListFilter) -> Document {
    let mut items = Vec::new();
    if let Some(cursor) = &filter.cursor {
        items.push(doc! { "$match": { "$or": instance_cursor_or(filter.view, cursor) } });
    }
    items.push(doc! { "$limit": instance_list_limit(filter.limit) });
    let mut projection = instance_summary_projection();
    projection.insert(
        "snapshot",
        doc! {
            "$cond": ["$_runtime_snapshot_exact", "$_runtime_snapshot", Bson::Null]
        },
    );
    projection.insert("_id", 0);
    items.push(doc! { "$project": projection });
    doc! {
        "items": items,
        "total": [{ "$count": "count" }],
    }
}

#[cfg(test)]
mod tests {
    use bpm::ProcessKind;
    use bpm::model::types::ApprovalProcessInstanceStatus;
    use mongodb::bson::{Bson, doc};

    use super::{
        ApprovalRuntimeReadScope, ApprovalRuntimeReadTypeScope, runtime_read_pipeline,
        runtime_read_scope_empty, runtime_snapshot_scope_match,
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
            ApprovalRuntimeReadTypeScope {
                process_kind: ProcessKind::StockAdjustment,
                organization_ids: Some(vec!["org-1".to_string()]),
            },
            ApprovalRuntimeReadTypeScope {
                process_kind: ProcessKind::SalesOrder,
                organization_ids: Some(vec!["org-1".to_string()]),
            },
        ]
    }

    fn runtime_scope() -> ApprovalRuntimeReadScope {
        ApprovalRuntimeReadScope::Managed { type_scopes: runtime_type_scopes() }
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
        let scope_match = pipeline[6].get_document("$match").unwrap().to_string();
        assert!(scope_match.contains("_runtime_snapshot_exact"));
        assert!(scope_match.contains("responsible_org_id"));
        assert!(scope_match.contains("org-1"));

        let text = pipeline[7].get_document("$match").unwrap().to_string();
        assert!(text.contains("_runtime_snapshot.payload.document_no"));
        assert!(text.contains("_runtime_snapshot_exact"));
        assert!(text.contains(r"ADJ\.\[1\]"));
        let facet = pipeline[8].get_document("$facet").unwrap();
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
    fn runtime_company_scope_keeps_missing_snapshot_before_facet() {
        let scope = ApprovalRuntimeReadScope::Managed {
            type_scopes: runtime_type_scopes()
                .into_iter()
                .map(|type_scope| ApprovalRuntimeReadTypeScope { organization_ids: None, ..type_scope })
                .collect(),
        };
        let pipeline = runtime_read_pipeline(&runtime_filter(), &scope);
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

    #[test]
    fn runtime_scope_rejects_empty_and_requested_type_outside_proven_types() {
        let filter = runtime_filter();
        let empty = ApprovalRuntimeReadScope::Managed { type_scopes: Vec::new() };
        assert!(runtime_read_scope_empty(&filter, &empty));
        let empty_org = ApprovalRuntimeReadScope::Managed {
            type_scopes: runtime_type_scopes()
                .into_iter()
                .map(|type_scope| ApprovalRuntimeReadTypeScope {
                    organization_ids: Some(Vec::new()),
                    ..type_scope
                })
                .collect(),
        };
        assert!(runtime_read_scope_empty(&filter, &empty_org));

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
        assert!(runtime_snapshot_scope_match(&matching).is_none());
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
