//! 单据审批任务聚合查询：本人开放任务分页与执行过滤。

use bpm::ApprovalNodeExecutionId;
use entity_core::NOT_DELETED_TIMESTAMP_BSON;
use futures_util::TryStreamExt;
use mongodb::bson::{Document, doc};
use mongodb::{Collection, Database};
use persistence_core::{Error, Executor, Result};
use serde::Deserialize;

use super::super::super::extensions::{ApprovalIntegrationExt, BpmExt};
use super::{DocumentApprovalWorkItemIntegrityConflict, DocumentApprovalWorkItemPage};
use crate::entity::work_item::{WorkItem, WorkItemStatus, WorkItemType};

const APPROVAL_NODE_EXECUTIONS: &str = <Database as BpmExt>::APPROVAL_NODE_EXECUTIONS;
const APPROVAL_PROCESS_INSTANCES: &str = <Database as BpmExt>::APPROVAL_PROCESS_INSTANCES;
const APPROVAL_SUBJECT_SNAPSHOTS: &str = <Database as ApprovalIntegrationExt>::APPROVAL_SUBJECT_SNAPSHOTS;

/// 本人开放审批任务聚合内部使用的稳定倒序游标。
#[derive(Debug, Clone, PartialEq, Eq)]
struct DocumentApprovalWorkItemCursor {
    /// 当前页最后一条任务的首次分派时间。
    pub assigned_at: i64,
    /// 分派时间并列时的稳定任务主键。
    pub id: String,
}
#[derive(Debug, Default, Deserialize)]
struct DocumentApprovalWorkItemFacet {
    #[serde(default)]
    items: Vec<WorkItem>,
    #[serde(default)]
    total: Vec<DocumentApprovalWorkItemCount>,
    #[serde(default)]
    duplicate_executions: Vec<DocumentApprovalDuplicateExecution>,
    #[serde(default)]
    duplicate_instances: Vec<DocumentApprovalDuplicateInstance>,
}

#[derive(Debug, Deserialize)]
struct DocumentApprovalWorkItemCount {
    count: i64,
}

#[derive(Debug, Deserialize)]
struct DocumentApprovalDuplicateExecution {
    approval_node_execution_id: String,
    open_work_item_count: i64,
}

#[derive(Debug, Deserialize)]
struct DocumentApprovalDuplicateInstance {
    approval_process_instance_id: String,
    open_execution_count: i64,
}
/// 分页查询指定账号当前开放的单据审批任务。
pub(super) async fn page_open_document_approval_owned_by(
    collection: &Collection<WorkItem>,
    owner_user_id: &str,
    business_object_type: Option<&str>,
    query: Option<&str>,
    cursor: Option<(i64, &str)>,
    limit: u32,
    executor: &mut dyn Executor,
) -> Result<DocumentApprovalWorkItemPage> {
    let cursor =
        cursor.map(|(assigned_at, id)| DocumentApprovalWorkItemCursor { assigned_at, id: id.to_string() });
    let pipeline =
        document_approval_page_pipeline(owner_user_id, business_object_type, query, cursor.as_ref(), limit)?;
    let rows = aggregate_document_approval_page(collection, pipeline, executor).await?;
    document_approval_page(rows.into_iter().next(), limit)
}

/// 执行本人审批任务聚合并保持调用方会话语义。
async fn aggregate_document_approval_page(
    collection: &Collection<WorkItem>,
    pipeline: Vec<Document>,
    executor: &mut dyn Executor,
) -> Result<Vec<DocumentApprovalWorkItemFacet>> {
    match executor.session() {
        Some(session) => Ok(collection
            .aggregate(pipeline)
            .with_type::<DocumentApprovalWorkItemFacet>()
            .session(&mut *session)
            .await?
            .stream(session)
            .try_collect::<Vec<_>>()
            .await?),
        None => Ok(collection
            .aggregate(pipeline)
            .with_type::<DocumentApprovalWorkItemFacet>()
            .await?
            .try_collect::<Vec<_>>()
            .await?),
    }
}

/// 构造本人审批任务的单次计数与稳定游标聚合。
fn document_approval_page_pipeline(
    owner_user_id: &str,
    business_object_type: Option<&str>,
    query: Option<&str>,
    cursor: Option<&DocumentApprovalWorkItemCursor>,
    limit: u32,
) -> Result<Vec<Document>> {
    let overfetch = document_approval_overfetch_limit(limit)?;
    let mut pipeline = vec![doc! {
        "$match": document_approval_owner_filter(owner_user_id, business_object_type)
    }];
    let query = query.map(str::trim).filter(|value| !value.is_empty());
    if let Some(query) = query {
        pipeline.extend(document_approval_query_stages(query));
    }
    pipeline.push(doc! { "$sort": { "assigned_at": -1, "id": -1 } });
    pipeline.push(doc! {
        "$facet": document_approval_page_facets(cursor, overfetch, query.is_some())
    });
    Ok(pipeline)
}

/// 构造能命中部分页索引的固定责任范围。
fn document_approval_owner_filter(owner_user_id: &str, business_object_type: Option<&str>) -> Document {
    let mut filter = doc! {
        "owner_user_id": owner_user_id,
        "work_item_type": WorkItemType::DocumentApproval.as_str(),
        "status": WorkItemStatus::Open.as_str(),
        "deleted_at": NOT_DELETED_TIMESTAMP_BSON,
    };
    if let Some(business_object_type) = business_object_type {
        filter.insert("business_object_type", business_object_type);
    }
    filter
}

/// 构造字面量检索所需的有界 execution、instance 与精确快照关联。
fn document_approval_query_stages(query: &str) -> Vec<Document> {
    let literal = regex::escape(query);
    let regex = doc! { "$regex": literal, "$options": "i" };
    vec![
        document_approval_execution_lookup(),
        doc! { "$set": { "_mine_execution": { "$arrayElemAt": ["$_mine_executions", 0] } } },
        document_approval_instance_lookup(),
        doc! { "$set": { "_mine_instance": { "$arrayElemAt": ["$_mine_instances", 0] } } },
        document_approval_snapshot_lookup(),
        doc! {
            "$match": {
                "$or": [
                    { "business_object_id": regex.clone() },
                    { "_mine_instance.current_node_name": regex.clone() },
                    { "_mine_instance.current_assignee_name": regex.clone() },
                    { "_mine_snapshots.payload.document_no": regex },
                ]
            }
        },
    ]
}

/// 按审批任务执行引用读取唯一节点执行。
fn document_approval_execution_lookup() -> Document {
    doc! {
        "$lookup": {
            "from": APPROVAL_NODE_EXECUTIONS,
            "let": { "execution_id": "$approval_node_execution_id" },
            "pipeline": [
                { "$match": {
                    "deleted_at": NOT_DELETED_TIMESTAMP_BSON,
                    "$expr": { "$eq": ["$id", "$$execution_id"] },
                }},
                { "$project": { "_id": 0, "id": 1, "process_instance_id": 1 } },
            ],
            "as": "_mine_executions",
        }
    }
}

/// 按节点执行所属实例读取检索投影与不可变 subject 引用。
fn document_approval_instance_lookup() -> Document {
    doc! {
        "$lookup": {
            "from": APPROVAL_PROCESS_INSTANCES,
            "let": { "instance_id": "$_mine_execution.process_instance_id" },
            "pipeline": [
                { "$match": {
                    "deleted_at": NOT_DELETED_TIMESTAMP_BSON,
                    "$expr": { "$eq": ["$id", "$$instance_id"] },
                }},
                { "$project": {
                    "_id": 0,
                    "id": 1,
                    "process_kind": 1,
                    "subject": 1,
                    "subject_version": 1,
                    "current_node_execution_id": 1,
                    "current_node_name": 1,
                    "current_assignee_name": 1,
                }},
            ],
            "as": "_mine_instances",
        }
    }
}

/// 只关联同时匹配任务、执行、实例和快照三元组的不可变快照。
fn document_approval_snapshot_lookup() -> Document {
    doc! {
        "$lookup": {
            "from": APPROVAL_SUBJECT_SNAPSHOTS,
            "let": {
                "instance_id": "$_mine_instance.id",
                "instance_kind": "$_mine_instance.process_kind",
                "instance_subject_kind": "$_mine_instance.subject.subject_kind",
                "instance_subject_id": "$_mine_instance.subject.subject_id",
                "instance_subject_version": "$_mine_instance.subject_version",
                "instance_execution_id": "$_mine_instance.current_node_execution_id",
                "task_execution_id": "$approval_node_execution_id",
                "task_object_type": "$business_object_type",
                "task_object_id": "$business_object_id",
                "task_subject_version": "$subject_version",
            },
            "pipeline": [
                { "$match": {
                    "deleted_at": NOT_DELETED_TIMESTAMP_BSON,
                    "$expr": { "$and": [
                        { "$eq": ["$approval_process_instance_id", "$$instance_id"] },
                        { "$eq": ["$document_type", "$$instance_kind"] },
                        { "$eq": ["$document_type", "$$instance_subject_kind"] },
                        { "$eq": ["$document_type", "$$task_object_type"] },
                        { "$eq": ["$business_object_id", "$$instance_subject_id"] },
                        { "$eq": ["$business_object_id", "$$task_object_id"] },
                        { "$eq": ["$subject_version", "$$instance_subject_version"] },
                        { "$eq": [{ "$toString": "$subject_version" }, "$$task_subject_version"] },
                        { "$eq": ["$$instance_execution_id", "$$task_execution_id"] },
                    ]},
                }},
                { "$project": { "_id": 0, "payload.document_no": 1 } },
            ],
            "as": "_mine_snapshots",
        }
    }
}

/// 构造游标页与不含游标总数的 facet。
fn document_approval_page_facets(
    cursor: Option<&DocumentApprovalWorkItemCursor>,
    overfetch: i64,
    execution_already_loaded: bool,
) -> Document {
    let mut items = Vec::new();
    if let Some(cursor) = cursor {
        items.push(doc! { "$match": { "$or": document_approval_cursor_or(cursor) } });
    }
    items.push(doc! { "$limit": overfetch });
    items.push(doc! { "$project": {
        "_id": 0,
        "_mine_executions": 0,
        "_mine_execution": 0,
        "_mine_instances": 0,
        "_mine_instance": 0,
        "_mine_snapshots": 0,
    }});
    doc! {
        "items": items,
        "total": [{ "$count": "count" }],
        "duplicate_executions": document_approval_duplicate_execution_stages(),
        "duplicate_instances": document_approval_duplicate_instance_stages(
            execution_already_loaded,
        ),
    }
}

/// 检测同一 execution 挂接多条开放任务的持久化事实。
fn document_approval_duplicate_execution_stages() -> Vec<Document> {
    vec![
        doc! { "$match": {
            "approval_node_execution_id": { "$type": "string" }
        }},
        doc! { "$group": {
            "_id": "$approval_node_execution_id",
            "open_work_item_count": { "$sum": 1 },
        }},
        doc! { "$match": { "open_work_item_count": { "$gt": 1 } } },
        doc! { "$sort": { "_id": 1 } },
        doc! { "$limit": 1 },
        doc! { "$project": {
            "_id": 0,
            "approval_node_execution_id": "$_id",
            "open_work_item_count": 1,
        }},
    ]
}

/// 检测同一 instance 挂接多个不同 execution 的持久化事实。
fn document_approval_duplicate_instance_stages(execution_already_loaded: bool) -> Vec<Document> {
    let mut stages = Vec::new();
    if !execution_already_loaded {
        stages.push(document_approval_execution_lookup());
        stages.push(doc! { "$set": {
            "_mine_execution": { "$arrayElemAt": ["$_mine_executions", 0] }
        }});
    }
    stages.push(doc! { "$set": {
        "_mine_integrity_group_key": { "$ifNull": [
            "$_mine_execution.process_instance_id",
            { "$concat": [
                "execution:",
                { "$ifNull": ["$approval_node_execution_id", "<missing>"] },
            ]},
        ]},
    }});
    stages.extend([
        doc! { "$match": {
            "approval_node_execution_id": { "$type": "string" }
        }},
        doc! { "$group": {
            "_id": {
                "group_key": "$_mine_integrity_group_key",
                "execution_id": "$approval_node_execution_id",
            },
            "approval_process_instance_id": {
                "$first": "$_mine_execution.process_instance_id"
            },
        }},
        doc! { "$group": {
            "_id": "$_id.group_key",
            "approval_process_instance_id": { "$first": "$approval_process_instance_id" },
            "open_execution_count": { "$sum": 1 },
        }},
        doc! { "$match": {
            "approval_process_instance_id": { "$type": "string" },
            "open_execution_count": { "$gt": 1 },
        }},
        doc! { "$sort": { "_id": 1 } },
        doc! { "$limit": 1 },
        doc! { "$project": {
            "_id": 0,
            "approval_process_instance_id": 1,
            "open_execution_count": 1,
        }},
    ]);
    stages
}

/// 构造倒序稳定游标的“小于”条件。
fn document_approval_cursor_or(cursor: &DocumentApprovalWorkItemCursor) -> Vec<Document> {
    vec![
        doc! { "assigned_at": { "$lt": cursor.assigned_at } },
        doc! { "assigned_at": cursor.assigned_at, "id": { "$lt": cursor.id.as_str() } },
    ]
}

/// 校验页大小并计算 `limit + 1`。
fn document_approval_overfetch_limit(limit: u32) -> Result<i64> {
    if limit == 0 {
        return Err(Error::EntityMetadataOutOfRange("document_approval_page_limit"));
    }
    limit.checked_add(1).map(i64::from).ok_or(Error::EntityMetadataOutOfRange("document_approval_page_limit"))
}

/// 将 facet 行切为当前页并形成下一页游标。
fn document_approval_page(
    facet: Option<DocumentApprovalWorkItemFacet>,
    limit: u32,
) -> Result<DocumentApprovalWorkItemPage> {
    let facet = facet.unwrap_or(DocumentApprovalWorkItemFacet {
        items: Vec::new(),
        total: Vec::new(),
        duplicate_executions: Vec::new(),
        duplicate_instances: Vec::new(),
    });
    let total = crate::repository::approval_integration::facet_total_or_empty(
        facet.total.first().map(|row| row.count),
        "document_approval_work_item_total",
    )?;
    let integrity_conflicts =
        document_approval_integrity_conflicts(facet.duplicate_executions, facet.duplicate_instances)?;
    document_approval_page_from_items(facet.items, total, limit, integrity_conflicts)
}

/// 将聚合完整性行转为不包含 MongoDB 细节的仓储事实。
fn document_approval_integrity_conflicts(
    duplicate_executions: Vec<DocumentApprovalDuplicateExecution>,
    duplicate_instances: Vec<DocumentApprovalDuplicateInstance>,
) -> Result<Vec<DocumentApprovalWorkItemIntegrityConflict>> {
    let mut conflicts = Vec::new();
    for duplicate in duplicate_executions {
        conflicts.push(DocumentApprovalWorkItemIntegrityConflict::MultipleOpenTasksForExecution {
            approval_node_execution_id: duplicate.approval_node_execution_id,
            open_work_item_count: document_approval_integrity_count(duplicate.open_work_item_count)?,
        });
    }
    for duplicate in duplicate_instances {
        conflicts.push(DocumentApprovalWorkItemIntegrityConflict::MultipleOpenExecutionsForInstance {
            approval_process_instance_id: duplicate.approval_process_instance_id,
            open_execution_count: document_approval_integrity_count(duplicate.open_execution_count)?,
        });
    }
    Ok(conflicts)
}

/// 将 MongoDB group count 校验为公开仓储事实使用的非负数。
fn document_approval_integrity_count(count: i64) -> Result<u64> {
    u64::try_from(count)
        .map_err(|_| Error::EntityMetadataOutOfRange("document_approval_work_item_integrity_count"))
}

/// 以多取一条的结果形成 `has_more` 与稳定下一页游标。
fn document_approval_page_from_items(
    mut items: Vec<WorkItem>,
    total: u64,
    limit: u32,
    integrity_conflicts: Vec<DocumentApprovalWorkItemIntegrityConflict>,
) -> Result<DocumentApprovalWorkItemPage> {
    let has_more = items.len() > limit as usize;
    if has_more {
        items.truncate(limit as usize);
    }
    let next_cursor = if has_more {
        let item = items.last().ok_or(Error::EntityMetadataOutOfRange("document_approval_work_item_page"))?;
        let assigned_at = item
            .assigned_at
            .ok_or(Error::EntityMetadataOutOfRange("document_approval_work_item_assigned_at"))?;
        Some((assigned_at.unix_secs(), item.base.id.clone()))
    } else {
        None
    };
    Ok(DocumentApprovalWorkItemPage { items, total, has_more, next_cursor, integrity_conflicts })
}

/// 构造按节点执行读取开放审批任务的索引友好过滤条件。
///
/// # 参数
/// * `execution_id` - 当前审批节点执行
///
/// # 返回
/// 返回固定约束任务类型、开放状态和节点执行的查询文档。
///
/// # 错误
/// 无。
pub(super) fn open_approval_execution_filter(execution_id: &ApprovalNodeExecutionId) -> Document {
    doc! {
        "work_item_type": WorkItemType::DocumentApproval.as_str(),
        "approval_node_execution_id": execution_id.as_ref(),
        "status": WorkItemStatus::Open.as_str(),
    }
}

#[cfg(test)]
mod tests {
    use bpm::ApprovalNodeExecutionId;
    use erp_core::common::time::Instant;
    use erp_core::ids::WorkItemId;
    use mongodb::bson::{Bson, doc};

    use super::{
        DocumentApprovalWorkItemCursor, document_approval_page_from_items, document_approval_page_pipeline,
        open_approval_execution_filter,
    };
    use crate::entity::work_item::{DocumentApprovalWorkItemData, WorkItem, WorkItemPriority};

    fn approval_item(id: &str, assigned_at: i64) -> WorkItem {
        WorkItem::new_document_approval(
            WorkItemId::new(id),
            DocumentApprovalWorkItemData {
                approval_node_execution_id: ApprovalNodeExecutionId::new(format!("exec-{id}")),
                business_object_type: "purchase_order".to_string(),
                business_object_id: format!("object-{id}"),
                subject_version: "1".to_string(),
                owner_role: "purchase_order_approver".to_string(),
                owner_organization_id: "org-1".to_string(),
                owner_user_id: "alice".to_string(),
                priority: WorkItemPriority::Normal,
                due_at: None,
            },
            Instant::from_unix_secs(assigned_at),
        )
        .expect("审批任务 fixture")
    }

    #[test]
    fn document_approval_page_uses_desc_cursor_and_cursor_free_total() {
        let pipeline = document_approval_page_pipeline(
            "alice",
            Some("purchase_order"),
            None,
            Some(&DocumentApprovalWorkItemCursor { assigned_at: 30, id: "wi-b".to_string() }),
            2,
        )
        .expect("审批任务页 pipeline");

        assert_eq!(pipeline.len(), 3, "无检索时不得执行关联查询");
        assert_eq!(
            pipeline[0].get_document("$match").unwrap(),
            &doc! {
                "owner_user_id": "alice",
                "work_item_type": "DOCUMENT_APPROVAL",
                "status": "OPEN",
                "deleted_at": 0_i64,
                "business_object_type": "purchase_order",
            }
        );
        assert_eq!(
            pipeline[1].get_document("$sort").unwrap(),
            &doc! { "assigned_at": -1, "id": -1 },
            "稳定排序必须位于 facet 前，由页索引承担"
        );
        let facets = pipeline[2].get_document("$facet").unwrap();
        let items = facets.get_array("items").unwrap();
        assert_eq!(
            items[0].as_document().unwrap(),
            &doc! { "$match": { "$or": [
                { "assigned_at": { "$lt": 30_i64 } },
                { "assigned_at": 30_i64, "id": { "$lt": "wi-b" } },
            ]}}
        );
        assert_eq!(items[1].as_document().unwrap(), &doc! { "$limit": 3_i64 });
        assert_eq!(
            facets.get_array("total").unwrap(),
            &vec![Bson::Document(doc! { "$count": "count" })],
            "total facet 不得带 cursor"
        );
        let duplicate_instances = facets.get_array("duplicate_instances").unwrap();
        assert!(duplicate_instances[0].as_document().unwrap().contains_key("$lookup"));
        for branch in ["duplicate_executions", "duplicate_instances"] {
            let contract = Bson::Array(facets.get_array(branch).unwrap().clone()).to_string();
            assert!(!contract.contains("assigned_at"), "{branch} 不得受 cursor 影响");
            assert!(!contract.contains("wi-b"), "{branch} 不得带 cursor ID");
        }
    }

    #[test]
    fn document_approval_query_is_literal_and_snapshot_lookup_is_exact() {
        let pipeline = document_approval_page_pipeline("alice", None, Some("PO.[1]"), None, 20)
            .expect("带检索审批任务页 pipeline");
        assert_eq!(pipeline.len(), 9);
        assert_eq!(
            pipeline[1].get_document("$lookup").unwrap().get_str("from").unwrap(),
            "approval_node_executions"
        );
        assert_eq!(
            pipeline[3].get_document("$lookup").unwrap().get_str("from").unwrap(),
            "approval_process_instances"
        );
        let snapshot_lookup = pipeline[5].get_document("$lookup").unwrap();
        assert_eq!(snapshot_lookup.get_str("from").unwrap(), "approval_subject_snapshots");
        let snapshot_contract = snapshot_lookup.to_string();
        for required in [
            "$$instance_id",
            "$$instance_kind",
            "$$instance_subject_kind",
            "$$instance_subject_id",
            "$$instance_subject_version",
            "$$instance_execution_id",
            "$$task_execution_id",
            "$$task_object_type",
            "$$task_object_id",
            "$$task_subject_version",
        ] {
            assert!(snapshot_contract.contains(required), "缺少精确快照约束 {required}");
        }
        let query_match = pipeline[6].get_document("$match").unwrap().to_string();
        assert!(query_match.contains(r"PO\.\[1\]"));
        assert!(query_match.contains("_mine_instance.current_node_name"));
        assert!(query_match.contains("_mine_instance.current_assignee_name"));
        assert!(query_match.contains("_mine_snapshots.payload.document_no"));
        let duplicate_instances =
            pipeline[8].get_document("$facet").unwrap().get_array("duplicate_instances").unwrap();
        assert!(
            duplicate_instances[0].as_document().unwrap().contains_key("$set"),
            "q 分支必须复用已加载 execution"
        );
        assert!(
            duplicate_instances.iter().all(|stage| !stage.as_document().unwrap().contains_key("$lookup"))
        );
    }

    #[test]
    fn document_approval_page_overfetch_sets_next_cursor() {
        let page = document_approval_page_from_items(
            vec![approval_item("wi-c", 30), approval_item("wi-b", 30), approval_item("wi-a", 30)],
            9,
            2,
            Vec::new(),
        )
        .expect("分页结果");
        assert_eq!(
            page.items.iter().map(|item| item.base.id.as_str()).collect::<Vec<_>>(),
            vec!["wi-c", "wi-b"]
        );
        assert_eq!(page.total, 9);
        assert!(page.has_more);
        assert_eq!(page.next_cursor, Some((30, "wi-b".to_string())));
        assert!(page.integrity_conflicts.is_empty());
    }

    /// 节点执行开放任务查询固定约束审批类型、开放状态和执行引用。
    ///
    /// 该形态命中执行唯一索引且不会混入独立任务。
    #[test]
    fn open_approval_execution_filter_is_semantic_and_bounded() {
        let filter = open_approval_execution_filter(&ApprovalNodeExecutionId::new("exec-1"));
        assert_eq!(filter.len(), 3);
        assert_eq!(filter.get_str("work_item_type").unwrap(), "DOCUMENT_APPROVAL");
        assert_eq!(filter.get_str("status").unwrap(), "OPEN");
        assert_eq!(filter.get_str("approval_node_execution_id").unwrap(), "exec-1");
    }
}
