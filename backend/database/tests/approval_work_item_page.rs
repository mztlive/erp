//! APP-R02：本人审批 WorkItem 的稳定游标、真实总数、字面量检索与索引执行计划。
//!
//! 真实 MongoDB 用例统一 `#[ignore]` + `require_mongo!()`；测试使用独立随机库，
//! 仅验证 Repository 持久化合同，不替代 Service 的 execution/instance 批量 hydration。

use bpm::ApprovalNodeExecutionId;
use database::{ensure_indexes, NoTransaction, WorkItemExt};
use entities::common::time::Instant;
use entities::ids::WorkItemId;
use entities::work_item::{DocumentApprovalWorkItemData, WorkItem, WorkItemPriority};
use entity_core::HasBaseModel;
use mongodb::bson::{doc, Bson, Document};
use test_support::{assert_indexes, require_mongo, TestDb};

const OWNER_PAGE_INDEX: &str = "idx_work_items_document_approval_owner_page";
const OWNER_TYPE_PAGE_INDEX: &str = "idx_work_items_document_approval_owner_type_page";

/// 构造具有独立分派与创建时间的开放审批任务。
fn approval_item(
    id: &str,
    owner_user_id: &str,
    business_object_type: &str,
    business_object_id: &str,
    assigned_at: i64,
    created_at: u64,
) -> WorkItem {
    let mut item = WorkItem::new_document_approval(
        WorkItemId::new(id),
        DocumentApprovalWorkItemData {
            approval_node_execution_id: ApprovalNodeExecutionId::new(format!("exec-{id}")),
            business_object_type: business_object_type.to_string(),
            business_object_id: business_object_id.to_string(),
            subject_version: "1".to_string(),
            owner_role: "document_approver".to_string(),
            owner_organization_id: "org-1".to_string(),
            owner_user_id: owner_user_id.to_string(),
            priority: WorkItemPriority::Normal,
            due_at: None,
        },
        Instant::from_unix_secs(assigned_at),
    )
    .expect("审批任务 fixture");
    item.base_mut().created_at = created_at;
    item.base_mut().updated_at = created_at;
    item
}

/// 插入能被检索聚合读取的最小 execution、instance 与 snapshot 链。
async fn insert_search_chain(
    db: &mongodb::Database,
    work_item_id: &str,
    object_id: &str,
    snapshot_object_id: &str,
    document_no: &str,
    node_name: &str,
) {
    let execution_id = format!("exec-{work_item_id}");
    let instance_id = format!("inst-{work_item_id}");
    db.collection::<Document>("approval_node_executions")
        .insert_one(doc! {
            "id": &execution_id,
            "process_instance_id": &instance_id,
            "deleted_at": 0_i64,
        })
        .await
        .expect("插入检索执行");
    db.collection::<Document>("approval_process_instances")
        .insert_one(doc! {
            "id": &instance_id,
            "process_kind": "purchase_order",
            "subject": {
                "subject_kind": "purchase_order",
                "subject_id": object_id,
            },
            "subject_version": 1_i64,
            "current_node_execution_id": &execution_id,
            "current_node_name": node_name,
            "current_assignee_name": "张三",
            "deleted_at": 0_i64,
        })
        .await
        .expect("插入检索实例");
    db.collection::<Document>("approval_subject_snapshots")
        .insert_one(doc! {
            "id": format!("snapshot-{work_item_id}"),
            "approval_process_instance_id": &instance_id,
            "document_type": "purchase_order",
            "business_object_id": snapshot_object_id,
            "subject_version": 1_i64,
            "payload": { "document_no": document_no },
            "deleted_at": 0_i64,
        })
        .await
        .expect("插入检索快照");
}

/// 插入完整性检测所需的最小 execution 引用事实。
async fn insert_execution_fact(
    db: &mongodb::Database,
    execution_id: &str,
    instance_id: &str,
    execution_no: i32,
) {
    db.collection::<Document>("approval_node_executions")
        .insert_one(doc! {
            "id": execution_id,
            "process_instance_id": instance_id,
            "execution_no": execution_no,
            "deleted_at": 0_i64,
        })
        .await
        .expect("插入完整性 execution");
}

/// 在完成分页行为断言后插入索引竞争数据，使无 hint 规划具有可判定性。
async fn insert_plan_noise(db: &mongodb::Database) {
    let mut documents = Vec::new();
    for sequence in 0..32_i64 {
        documents.push(doc! {
            "id": format!("wi-generic-noise-{sequence:02}"),
            "owner_user_id": "alice",
            "work_item_type": "BUSINESS_EXCEPTION",
            "status": "OPEN",
            "deleted_at": 0_i64,
            "assigned_at": 1_000_i64 - sequence,
            "business_object_type": "generic_noise",
            "business_object_id": format!("generic-noise-{sequence:02}"),
        });
        documents.push(doc! {
            "id": format!("wi-approval-noise-{sequence:02}"),
            "owner_user_id": "alice",
            "work_item_type": "DOCUMENT_APPROVAL",
            "status": "OPEN",
            "deleted_at": 0_i64,
            "assigned_at": 2_000_i64 - sequence,
            "approval_node_execution_id": format!("exec-approval-noise-{sequence:02}"),
            "business_object_type": "sales_order",
            "business_object_id": format!("approval-noise-{sequence:02}"),
        });
    }
    db.collection::<Document>("work_items")
        .insert_many(documents)
        .await
        .expect("插入 explain 竞争数据");
}

/// 构造与生产同形的重复 execution 检测分支。
fn duplicate_execution_stages() -> Vec<Document> {
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

/// 构造与生产无检索分支同形的重复 instance 检测分支。
fn duplicate_instance_stages() -> Vec<Document> {
    vec![
        doc! { "$lookup": {
            "from": "approval_node_executions",
            "let": { "execution_id": "$approval_node_execution_id" },
            "pipeline": [
                { "$match": {
                    "deleted_at": 0_i64,
                    "$expr": { "$eq": ["$id", "$$execution_id"] },
                }},
                { "$project": { "_id": 0, "id": 1, "process_instance_id": 1 } },
            ],
            "as": "_mine_executions",
        }},
        doc! { "$set": {
            "_mine_execution": { "$arrayElemAt": ["$_mine_executions", 0] }
        }},
        doc! { "$set": {
            "_mine_integrity_group_key": { "$ifNull": [
                "$_mine_execution.process_instance_id",
                { "$concat": [
                    "execution:",
                    { "$ifNull": ["$approval_node_execution_id", "<missing>"] },
                ]},
            ]},
        }},
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
            "approval_process_instance_id": {
                "$first": "$approval_process_instance_id"
            },
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
    ]
}

/// 构造与生产无检索分支同形的索引页管道。
fn page_pipeline(business_object_type: Option<&str>, cursor: Option<(i64, &str)>) -> Vec<Document> {
    let mut filter = doc! {
        "owner_user_id": "alice",
        "work_item_type": "DOCUMENT_APPROVAL",
        "status": "OPEN",
        "deleted_at": 0_i64,
    };
    if let Some(business_object_type) = business_object_type {
        filter.insert("business_object_type", business_object_type);
    }
    let mut items = Vec::new();
    if let Some((assigned_at, id)) = cursor {
        items.push(doc! { "$match": { "$or": [
            { "assigned_at": { "$lt": assigned_at } },
            { "assigned_at": assigned_at, "id": { "$lt": id } },
        ]}});
    }
    items.push(doc! { "$limit": 3_i64 });
    items.push(doc! { "$project": {
        "_id": 0,
        "_mine_executions": 0,
        "_mine_execution": 0,
        "_mine_instances": 0,
        "_mine_instance": 0,
        "_mine_snapshots": 0,
    }});
    vec![
        doc! { "$match": filter },
        doc! { "$sort": { "assigned_at": -1, "id": -1 } },
        doc! { "$facet": {
            "items": items,
            "total": [{ "$count": "count" }],
            "duplicate_executions": duplicate_execution_stages(),
            "duplicate_instances": duplicate_instance_stages(),
        }},
    ]
}

/// 无 hint 执行与生产同形管道的聚合执行计划。
async fn page_explain(
    db: &mongodb::Database,
    business_object_type: Option<&str>,
    cursor: Option<(i64, &str)>,
) -> Document {
    db.run_command(doc! {
        "explain": {
            "aggregate": "work_items",
            "pipeline": page_pipeline(business_object_type, cursor),
            "cursor": {},
        },
        "verbosity": "executionStats",
    })
    .await
    .expect("审批任务页 explain")
}

#[derive(Debug, Default)]
struct ExplainEvidence {
    winning_plan_count: usize,
    winning_index_names: Vec<String>,
    winning_stage_names: Vec<String>,
    total_keys_examined: Vec<i64>,
    total_docs_examined: Vec<i64>,
}

/// 递归收集 winning plan 中的索引与物理 stage。
fn collect_winning_plan(value: &Bson, evidence: &mut ExplainEvidence) {
    match value {
        Bson::Document(document) => {
            if let Ok(index_name) = document.get_str("indexName") {
                evidence.winning_index_names.push(index_name.to_string());
            }
            if let Ok(stage_name) = document.get_str("stage") {
                evidence.winning_stage_names.push(stage_name.to_string());
            }
            for nested in document.values() {
                collect_winning_plan(nested, evidence);
            }
        }
        Bson::Array(values) => {
            for nested in values {
                collect_winning_plan(nested, evidence);
            }
        }
        _ => {}
    }
}

/// 递归收集执行计划及 keys/docs examined 证据。
fn collect_explain_evidence(value: &Bson, evidence: &mut ExplainEvidence) {
    match value {
        Bson::Document(document) => {
            if let Some(winning_plan) = document.get("winningPlan") {
                evidence.winning_plan_count += 1;
                collect_winning_plan(winning_plan, evidence);
            }
            collect_metric(document, "totalKeysExamined", &mut evidence.total_keys_examined);
            collect_metric(document, "totalDocsExamined", &mut evidence.total_docs_examined);
            for nested in document.values() {
                collect_explain_evidence(nested, evidence);
            }
        }
        Bson::Array(values) => {
            for nested in values {
                collect_explain_evidence(nested, evidence);
            }
        }
        _ => {}
    }
}

/// 收集 explain 中的非负整数执行统计。
fn collect_metric(document: &Document, key: &str, values: &mut Vec<i64>) {
    let Some(value) = document.get(key) else {
        return;
    };
    match value {
        Bson::Int32(value) => values.push(i64::from(*value)),
        Bson::Int64(value) => values.push(*value),
        _ => {}
    }
}

/// 断言生产同形管道的 winning plan 以指定索引承担排序。
fn assert_indexed_page_explain(explain: Document, expected_index: &str) {
    let mut evidence = ExplainEvidence::default();
    collect_explain_evidence(&Bson::Document(explain), &mut evidence);
    assert!(evidence.winning_plan_count > 0, "explain 必须包含 winningPlan");
    assert!(
        evidence
            .winning_index_names
            .iter()
            .any(|name| name == expected_index),
        "winning plan 未使用 {expected_index}: {evidence:?}"
    );
    assert!(
        evidence
            .winning_stage_names
            .iter()
            .all(|stage| stage != "COLLSCAN" && stage != "SORT"),
        "winning plan 不得含 COLLSCAN 或阻塞 SORT: {evidence:?}"
    );
    assert!(
        evidence.total_keys_examined.iter().any(|value| *value > 0),
        "explain 必须报告正数 totalKeysExamined: {evidence:?}"
    );
    assert!(
        evidence.total_docs_examined.iter().any(|value| *value > 0),
        "explain 必须报告正数 totalDocsExamined: {evidence:?}"
    );
}

#[tokio::test]
#[ignore = "需要 ERP_TEST_MONGO_URI 指向 MongoDB 副本集"]
async fn owner_page_is_stable_filtered_and_uses_both_partial_indexes() {
    require_mongo!(async {
        let fixture = TestDb::new("approval_wi_page").await.expect("测试数据库创建失败");
        ensure_indexes(fixture.db()).await.expect("索引创建失败");
        assert_indexes(
            fixture.db(),
            "work_items",
            &[OWNER_PAGE_INDEX, OWNER_TYPE_PAGE_INDEX],
        )
        .await
        .expect("审批任务页索引");

        let fixtures = [
            approval_item("wi-e", "alice", "purchase_order", "order-e", 40, 10),
            approval_item("wi-d", "alice", "purchase_order", "order-d", 30, 20),
            approval_item("wi-c", "alice", "purchase_order", "order.[literal]", 30, 30),
            approval_item("wi-b", "alice", "sales_order", "order-b", 20, 40),
            approval_item("wi-a", "alice", "purchase_order", "order-a", 10, 50),
            approval_item("wi-other", "bob", "purchase_order", "order-other", 50, 60),
        ];
        for item in fixtures {
            fixture
                .db()
                .work_items()
                .create(&item, &mut NoTransaction)
                .await
                .expect("插入审批任务");
        }

        let first_page = fixture
            .db()
            .work_items()
            .page_open_document_approval_owned_by("alice", None, None, None, 2, &mut NoTransaction)
            .await
            .expect("首页");
        assert_eq!(first_page.total, 5);
        assert!(first_page.has_more);
        assert!(first_page.integrity_conflicts.is_empty());
        assert_eq!(
            first_page
                .items
                .iter()
                .map(|item| item.base.id.as_str())
                .collect::<Vec<_>>(),
            vec!["wi-e", "wi-d"]
        );
        assert_eq!(first_page.next_cursor, Some((30, "wi-d".to_string())));

        let second_page = fixture
            .db()
            .work_items()
            .page_open_document_approval_owned_by(
                "alice",
                None,
                None,
                first_page
                    .next_cursor
                    .as_ref()
                    .map(|(assigned_at, id)| (*assigned_at, id.as_str())),
                2,
                &mut NoTransaction,
            )
            .await
            .expect("第二页");
        assert_eq!(second_page.total, 5, "总数不得随 cursor 缩小");
        assert!(second_page.integrity_conflicts.is_empty());
        assert_eq!(
            second_page
                .items
                .iter()
                .map(|item| item.base.id.as_str())
                .collect::<Vec<_>>(),
            vec!["wi-c", "wi-b"]
        );
        let third_page = fixture
            .db()
            .work_items()
            .page_open_document_approval_owned_by(
                "alice",
                None,
                None,
                second_page
                    .next_cursor
                    .as_ref()
                    .map(|(assigned_at, id)| (*assigned_at, id.as_str())),
                2,
                &mut NoTransaction,
            )
            .await
            .expect("第三页");
        assert_eq!(
            third_page
                .items
                .iter()
                .map(|item| item.base.id.as_str())
                .collect::<Vec<_>>(),
            vec!["wi-a"]
        );
        assert!(!third_page.has_more);
        assert!(third_page.next_cursor.is_none());
        assert!(third_page.integrity_conflicts.is_empty());

        let typed_page = fixture
            .db()
            .work_items()
            .page_open_document_approval_owned_by(
                "alice",
                Some("purchase_order"),
                None,
                None,
                10,
                &mut NoTransaction,
            )
            .await
            .expect("业务类型筛选页");
        assert_eq!(typed_page.total, 4);
        assert!(typed_page
            .items
            .iter()
            .all(|item| item.business_object_type == "purchase_order"));
        assert!(typed_page.integrity_conflicts.is_empty());

        insert_plan_noise(fixture.db()).await;
        for cursor in [None, Some((30, "wi-d"))] {
            let owner_explain = page_explain(fixture.db(), None, cursor).await;
            assert_indexed_page_explain(owner_explain, OWNER_PAGE_INDEX);
            let owner_type_explain = page_explain(fixture.db(), Some("purchase_order"), cursor).await;
            assert_indexed_page_explain(owner_type_explain, OWNER_TYPE_PAGE_INDEX);
        }
    });
}

#[tokio::test]
#[ignore = "需要 ERP_TEST_MONGO_URI 指向 MongoDB 副本集"]
async fn literal_query_runs_before_page_and_rejects_mismatched_snapshot_number() {
    require_mongo!(async {
        let fixture = TestDb::new("approval_wi_query")
            .await
            .expect("测试数据库创建失败");
        ensure_indexes(fixture.db()).await.expect("索引创建失败");
        for item in [
            approval_item("wi-good", "alice", "purchase_order", "order-good", 10, 30),
            approval_item("wi-bad", "alice", "purchase_order", "order-bad", 30, 10),
            approval_item("wi-literal", "alice", "purchase_order", "order.[literal]", 20, 20),
        ] {
            fixture
                .db()
                .work_items()
                .create(&item, &mut NoTransaction)
                .await
                .expect("插入检索任务");
        }
        insert_search_chain(
            fixture.db(),
            "wi-good",
            "order-good",
            "order-good",
            "PO-LOOKUP-001",
            "采购复核",
        )
        .await;
        insert_search_chain(
            fixture.db(),
            "wi-bad",
            "order-bad",
            "different-order",
            "PO-LOOKUP-001",
            "异常节点",
        )
        .await;

        let number_page = fixture
            .db()
            .work_items()
            .page_open_document_approval_owned_by(
                "alice",
                None,
                Some("PO-LOOKUP"),
                None,
                1,
                &mut NoTransaction,
            )
            .await
            .expect("快照单号检索");
        assert_eq!(number_page.total, 1, "坏快照不得靠 document_no 命中");
        assert_eq!(number_page.items[0].base.id, "wi-good");
        assert!(!number_page.has_more);
        assert!(number_page.integrity_conflicts.is_empty());

        let node_page = fixture
            .db()
            .work_items()
            .page_open_document_approval_owned_by(
                "alice",
                None,
                Some("采购复核"),
                None,
                1,
                &mut NoTransaction,
            )
            .await
            .expect("节点名称检索");
        assert_eq!(node_page.total, 1);
        assert_eq!(node_page.items[0].base.id, "wi-good");
        assert!(node_page.integrity_conflicts.is_empty());

        let literal_page = fixture
            .db()
            .work_items()
            .page_open_document_approval_owned_by(
                "alice",
                None,
                Some("order.[literal]"),
                None,
                1,
                &mut NoTransaction,
            )
            .await
            .expect("字面量对象 ID 检索");
        assert_eq!(literal_page.total, 1);
        assert_eq!(literal_page.items[0].base.id, "wi-literal");
        assert!(literal_page.integrity_conflicts.is_empty());
    });
}

#[tokio::test]
#[ignore = "需要 ERP_TEST_MONGO_URI 指向 MongoDB 副本集"]
async fn duplicate_execution_conflict_is_detected_across_cursor_pages() {
    require_mongo!(async {
        let fixture = TestDb::new("approval_wi_duplicate_execution")
            .await
            .expect("测试数据库创建失败");
        ensure_indexes(fixture.db()).await.expect("索引创建失败");
        fixture
            .db()
            .collection::<Document>("work_items")
            .drop_index("uk_work_items_approval_execution")
            .await
            .expect("移除唯一索引以构造历史损坏数据");
        insert_execution_fact(fixture.db(), "exec-shared", "inst-shared", 1).await;

        let mut newer = approval_item(
            "wi-duplicate-execution-newer",
            "alice",
            "purchase_order",
            "order-duplicate-execution-newer",
            20,
            10,
        );
        newer.approval_node_execution_id = Some(ApprovalNodeExecutionId::new("exec-shared"));
        let mut older = approval_item(
            "wi-duplicate-execution-older",
            "alice",
            "purchase_order",
            "order-duplicate-execution-older",
            10,
            20,
        );
        older.approval_node_execution_id = Some(ApprovalNodeExecutionId::new("exec-shared"));
        for item in [newer, older] {
            fixture
                .db()
                .work_items()
                .create(&item, &mut NoTransaction)
                .await
                .expect("插入重复 execution 任务");
        }

        let first_page = fixture
            .db()
            .work_items()
            .page_open_document_approval_owned_by("alice", None, None, None, 1, &mut NoTransaction)
            .await
            .expect("重复 execution 首页");
        assert_eq!(first_page.total, 2);
        assert_eq!(first_page.items.len(), 1, "冲突必须横跨分页");
        assert!(first_page.has_more);
        let conflict = format!("{:?}", first_page.integrity_conflicts);
        assert!(conflict.contains("MultipleOpenTasksForExecution"));
        assert!(conflict.contains("exec-shared"));

        let second_page = fixture
            .db()
            .work_items()
            .page_open_document_approval_owned_by(
                "alice",
                None,
                None,
                first_page
                    .next_cursor
                    .as_ref()
                    .map(|(assigned_at, id)| (*assigned_at, id.as_str())),
                1,
                &mut NoTransaction,
            )
            .await
            .expect("重复 execution 第二页");
        assert_eq!(second_page.total, 2, "cursor 不得缩小 total");
        assert_eq!(second_page.items.len(), 1);
        assert!(!second_page.has_more);
        assert_eq!(second_page.integrity_conflicts, first_page.integrity_conflicts);
    });
}

#[tokio::test]
#[ignore = "需要 ERP_TEST_MONGO_URI 指向 MongoDB 副本集"]
async fn duplicate_instance_conflict_is_detected_across_cursor_pages() {
    require_mongo!(async {
        let fixture = TestDb::new("approval_wi_duplicate_instance")
            .await
            .expect("测试数据库创建失败");
        ensure_indexes(fixture.db()).await.expect("索引创建失败");
        insert_execution_fact(fixture.db(), "exec-wi-instance-newer", "inst-shared", 1).await;
        insert_execution_fact(fixture.db(), "exec-wi-instance-older", "inst-shared", 2).await;

        for item in [
            approval_item(
                "wi-instance-newer",
                "alice",
                "purchase_order",
                "order-instance-newer",
                20,
                10,
            ),
            approval_item(
                "wi-instance-older",
                "alice",
                "purchase_order",
                "order-instance-older",
                10,
                20,
            ),
        ] {
            fixture
                .db()
                .work_items()
                .create(&item, &mut NoTransaction)
                .await
                .expect("插入重复 instance 任务");
        }

        let first_page = fixture
            .db()
            .work_items()
            .page_open_document_approval_owned_by("alice", None, None, None, 1, &mut NoTransaction)
            .await
            .expect("重复 instance 首页");
        assert_eq!(first_page.total, 2);
        assert_eq!(first_page.items.len(), 1, "冲突必须横跨分页");
        assert!(first_page.has_more);
        let conflict = format!("{:?}", first_page.integrity_conflicts);
        assert!(conflict.contains("MultipleOpenExecutionsForInstance"));
        assert!(conflict.contains("inst-shared"));

        let second_page = fixture
            .db()
            .work_items()
            .page_open_document_approval_owned_by(
                "alice",
                None,
                None,
                first_page
                    .next_cursor
                    .as_ref()
                    .map(|(assigned_at, id)| (*assigned_at, id.as_str())),
                1,
                &mut NoTransaction,
            )
            .await
            .expect("重复 instance 第二页");
        assert_eq!(second_page.total, 2, "cursor 不得缩小 total");
        assert_eq!(second_page.items.len(), 1);
        assert!(!second_page.has_more);
        assert_eq!(second_page.integrity_conflicts, first_page.integrity_conflicts);
    });
}
