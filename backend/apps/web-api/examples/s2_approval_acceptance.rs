//! S2 W2 审批验收（隔离库）：审批全管线聚合计划、Started 视图 20000 超限整体拒绝。
//! 仅允许显式 ERP_TEST_MONGO_URI（隔离副本集），随机库用后删除；不用 hint，不用内存解释器做通过依据。
use std::error::Error;
use std::str::FromStr;
use std::time::{SystemTime, UNIX_EPOCH};

use application_core::AuditActor;
use erp_core::AccountKind;
use erp_core::ids::{StockAdjustmentId, WarehouseId};
use erp_identity::access_control::{
    DataScopeData, DataScopeSubjectType, DataScopeType, ScopeBinding, ScopeDimension,
};
use erp_inventory::{AdjustmentReasonType, StockAdjustment, StockAdjustmentData};
use erp_processes::adapters::identity::shared_rbac_service;
use erp_processes::adapters::workflow::workflow_auth;
use erp_workflow::service::approval::execution::{
    ApprovalRuntimeService, RuntimeInstanceListQuery, RuntimeInstanceListView,
};
use mongodb::bson::{Bson, Document, doc};
use mongodb::{Client, Database};
use test_support::seed_admin_account;

type Outcome<T = ()> = Result<T, Box<dyn Error>>;

const STARTED_AT_BASE: i64 = 1_700_000_000;
const PLAN_SEED: usize = 1200;
const OVER_LIMIT: usize = 20_001;
const CHUNK: usize = 500;

#[tokio::main(flavor = "current_thread")]
async fn main() -> Outcome {
    let uri = std::env::var("ERP_TEST_MONGO_URI")?;
    let client = Client::with_uri_str(uri).await?;
    let suffix = SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos();
    let db = client.database(&format!("erp_s2_approval_{suffix}"));
    let outcome = verify(&db).await;
    db.drop().await?;
    outcome
}

async fn verify(db: &Database) -> Outcome {
    erp_identity::indexes::ensure(db).await?;
    erp_workflow::indexes::ensure(db).await?;
    erp_inventory::indexes::ensure(db).await?;

    let manager_id = seed_admin_account(db).await?;
    let planner_id = seed_admin_account(db).await?;
    let manager = AuditActor::new(manager_id.clone(), "s2a-manager".into(), AccountKind::Admin);
    let role = grant(
        db,
        &manager_id,
        &[("approval_instance", "read"), ("approval_instance", "decide"), ("stock_adjustment", "detail")],
    )
    .await?;
    seed_company(db, &role, "approval_instance", &["read", "decide"]).await?;
    // stock_adjustment:detail 只支持仓库维度，不在此处做 Company 种子；
    // Started 视图走发起人事实，不经过该权限检查。

    // 计划证据用独立发起人，避免计入超限证据的 Started 扫描。
    evidence_a_full_pipeline_plan(db, &planner_id).await?;
    evidence_b_started_over_limit(db, &manager, &manager_id).await?;

    println!(
        "LIMIT synthetic accounts, approvals, snapshots and adjustments in an isolated random database; browser acceptance remains separate"
    );
    Ok(())
}

// 证据 A：生产 Started 全管线（$match＋$sort＋$lookup＋$set×3＋$facet）的 explain 计划。
async fn evidence_a_full_pipeline_plan(db: &Database, planner_id: &str) -> Outcome {
    seed_instances(db, "s2a", planner_id, PLAN_SEED).await?;
    seed_snapshots(db, "s2a", planner_id, PLAN_SEED).await?;
    let plan = db
        .run_command(doc! {
            "explain": {
                "aggregate": "approval_process_instances",
                "pipeline": full_pipeline(planner_id),
                "cursor": {},
            },
            "verbosity": "queryPlanner",
        })
        .await?;
    let stages = plan.get_array("stages")?;
    let cursor = stages
        .first()
        .ok_or("explain stages missing")?
        .as_document()
        .ok_or("first explain stage must be $cursor")?
        .get_document("$cursor")?;
    let winning = serde_json::to_string(cursor.get_document("queryPlanner")?.get_document("winningPlan")?)?;
    assert!(winning.contains("IXSCAN"), "approval full pipeline must use IXSCAN: {winning}");
    let flat = serde_json::to_string(&plan)?;
    assert!(
        flat.contains("$lookup") && flat.contains("$facet"),
        "approval full pipeline must keep $lookup and $facet"
    );
    println!("PASS approval_full_pipeline_plan");
    println!(
        "EVIDENCE_JSON {{\"case\":\"approval_full_pipeline_plan\",\"collection\":\"approval_process_instances\",\"winningPlan_contains\":\"IXSCAN\",\"stages_contain\":[\"$lookup\",\"$facet\"],\"index_hint_used\":false,\"data_volume\":{{\"approval_process_instances\":{PLAN_SEED},\"approval_subject_snapshots\":{PLAN_SEED}}}}}"
    );
    Ok(())
}

// 证据 B：Started 视图 20001 条授权候选走真实 instance_list，整体拒绝且零截断。
async fn evidence_b_started_over_limit(db: &Database, manager: &AuditActor, manager_id: &str) -> Outcome {
    seed_instances(db, "s2b", manager_id, OVER_LIMIT).await?;
    seed_snapshots(db, "s2b", manager_id, OVER_LIMIT).await?;
    seed_adjustments(db, "s2b", manager_id, OVER_LIMIT).await?;
    let service =
        ApprovalRuntimeService::new(db.clone(), workflow_auth(db.clone(), shared_rbac_service(db.clone())));
    let query =
        RuntimeInstanceListQuery::prepare(RuntimeInstanceListView::Started, None, None, None, None, None)?;
    match service.instance_list(manager, query).await {
        Err(erp_workflow::Error::ValidationError(message)) => {
            assert_eq!(message, "审批查询授权结果超过 20000 条，请增加类型或业务筛选");
        },
        other => panic!("expected approval over-limit rejection, got {other:?}"),
    }
    println!("PASS approval_started_over_limit_20000_rejected");
    println!(
        "EVIDENCE_JSON {{\"case\":\"approval_started_over_limit_20000_rejected\",\"seeded\":{{\"approval_process_instances\":{OVER_LIMIT},\"approval_subject_snapshots\":{OVER_LIMIT},\"stock_adjustments\":{OVER_LIMIT}}},\"returned\":\"error\",\"truncated\":false,\"message\":\"审批查询授权结果超过 20000 条，请增加类型或业务筛选\"}}"
    );
    Ok(())
}

// 与 runtime_read_pipeline 同形的 Started 全管线；$in 为政策全部必须审批类型（按 as_str 排序）。
fn full_pipeline(starter: &str) -> Vec<Document> {
    let instance_match = doc! {
        "deleted_at": 0_i64,
        "process_kind": {"$in": [
            "customer_receipt", "customer_refund", "payment_reversal", "purchase_change_order",
            "purchase_order", "receipt_reversal", "sales_change_order", "sales_invoice_request",
            "sales_order", "stock_adjustment", "supplier_refund", "voucher_sales_order",
        ]},
        "started_by": starter,
        "$expr": {"$eq": ["$process_kind", "$subject.subject_kind"]},
    };
    // Started 视图的生产排序；默认页 20＋1 条前瞻。
    let sort = doc! {"started_at": -1_i32, "id": -1_i32};
    let projection = doc! {
        "id": 1_i32,
        "process_kind": 1_i32,
        "process_definition_id": 1_i32,
        "definition_version": 1_i32,
        "subject": 1_i32,
        "subject_version": 1_i32,
        "status": 1_i32,
        "current_round_no": 1_i32,
        "current_node_execution_id": 1_i32,
        "current_node_key": 1_i32,
        "current_node_name": 1_i32,
        "current_assignee_participant_id": 1_i32,
        "current_assignee_name": 1_i32,
        "latest_rejected_execution_id": 1_i32,
        "latest_rejection_summary": 1_i32,
        "last_status_changed_at": 1_i32,
        "started_by": 1_i32,
        "started_at": 1_i32,
        "blocked_at": 1_i32,
        "version": 1_i32,
        "updated_at": 1_i32,
        "snapshot": {"$cond": ["$_runtime_snapshot_exact", "$_runtime_snapshot", Bson::Null]},
        "_id": 0_i32,
    };
    vec![
        doc! {"$match": instance_match},
        doc! {"$sort": sort},
        doc! {"$lookup": {
            "from": "approval_subject_snapshots",
            "localField": "id",
            "foreignField": "approval_process_instance_id",
            "as": "_runtime_snapshots",
        }},
        doc! {"$set": {"_runtime_live_snapshots": {"$filter": {
            "input": "$_runtime_snapshots",
            "as": "snapshot",
            "cond": {"$eq": ["$$snapshot.deleted_at", 0_i64]},
        }}}},
        doc! {"$set": {"_runtime_snapshot": {"$arrayElemAt": ["$_runtime_live_snapshots", 0_i32]}}},
        doc! {"$set": {"_runtime_snapshot_exact": {"$and": [
            {"$eq": [{"$size": "$_runtime_live_snapshots"}, 1_i32]},
            {"$eq": ["$_runtime_snapshot.approval_process_instance_id", "$id"]},
            {"$eq": ["$_runtime_snapshot.document_type", "$process_kind"]},
            {"$eq": ["$_runtime_snapshot.document_type", "$subject.subject_kind"]},
            {"$eq": ["$_runtime_snapshot.business_object_id", "$subject.subject_id"]},
            {"$eq": ["$_runtime_snapshot.subject_version", "$subject_version"]},
        ]}}},
        // Started 范围无组织分支，生产不追加 scope $match。
        doc! {"$facet": {
            "items": [doc! {"$limit": 21_i64}, doc! {"$project": projection}],
            "total": [doc! {"$count": "count"}],
        }},
    ]
}

fn instance_doc(id: &str, object_id: &str, starter: &str, at: i64) -> Document {
    doc! {
        "id": id,
        "version": 1_i64,
        "created_at": at,
        "updated_at": at,
        "deleted_at": 0_i64,
        "process_definition_id": "s2a-def-1",
        "definition_version": 1_i32,
        "process_kind": "stock_adjustment",
        "subject": {"subject_kind": "stock_adjustment", "subject_id": object_id},
        "subject_version": 1_i32,
        "status": "RUNNING",
        "current_round_no": 1_i32,
        "current_node_execution_id": Bson::Null,
        "current_node_key": Bson::Null,
        "current_node_name": Bson::Null,
        "current_assignee_participant_id": Bson::Null,
        "current_assignee_name": Bson::Null,
        "latest_rejected_execution_id": Bson::Null,
        "latest_rejection_summary": Bson::Null,
        "last_status_changed_at": Bson::Null,
        "blocker_code": Bson::Null,
        "blocked_at": Bson::Null,
        "started_by": starter,
        "started_at": at,
        "ended_at": Bson::Null,
    }
}

fn snapshot_doc(
    id: &str,
    instance_id: &str,
    object_id: &str,
    submitter: &str,
    at: i64,
    quantity: Bson,
) -> Document {
    doc! {
        "id": id,
        "version": 1_i64,
        "created_at": at,
        "updated_at": at,
        "deleted_at": 0_i64,
        "approval_process_instance_id": instance_id,
        "document_type": "stock_adjustment",
        "business_object_id": object_id,
        "subject_version": 1_i32,
        "payload": {
            "document_no": format!("S2A-{object_id}"),
            "responsible_org_id": "s2a-org-1",
            "submitted_by": submitter,
            "submitted_at": at,
            "counterparty": Bson::Null,
            "total_amount": Bson::Null,
            "total_quantity": quantity,
            "line_count": 1_i32,
        },
    }
}

async fn seed_instances(db: &Database, prefix: &str, starter: &str, count: usize) -> Outcome {
    let collection = db.collection::<Document>("approval_process_instances");
    for chunk in (0..count).collect::<Vec<_>>().chunks(CHUNK) {
        let docs = chunk
            .iter()
            .map(|index| {
                let at = STARTED_AT_BASE + (*index as i64);
                instance_doc(&format!("{prefix}-inst-{index}"), &format!("{prefix}-adj-{index}"), starter, at)
            })
            .collect::<Vec<_>>();
        collection.insert_many(docs).await?;
    }
    Ok(())
}

async fn seed_snapshots(db: &Database, prefix: &str, submitter: &str, count: usize) -> Outcome {
    let quantity =
        Bson::Decimal128(mongodb::bson::Decimal128::from_str("3").map_err(|_| "quantity decimal")?);
    let collection = db.collection::<Document>("approval_subject_snapshots");
    for chunk in (0..count).collect::<Vec<_>>().chunks(CHUNK) {
        let docs = chunk
            .iter()
            .map(|index| {
                let at = STARTED_AT_BASE + (*index as i64);
                snapshot_doc(
                    &format!("{prefix}-snap-{index}"),
                    &format!("{prefix}-inst-{index}"),
                    &format!("{prefix}-adj-{index}"),
                    submitter,
                    at,
                    quantity.clone(),
                )
            })
            .collect::<Vec<_>>();
        collection.insert_many(docs).await?;
    }
    Ok(())
}

async fn seed_adjustments(db: &Database, prefix: &str, manager_id: &str, count: usize) -> Outcome {
    let collection = db.collection::<StockAdjustment>("stock_adjustments");
    for chunk in (0..count).collect::<Vec<_>>().chunks(CHUNK) {
        let mut docs = Vec::with_capacity(chunk.len());
        for index in chunk {
            docs.push(StockAdjustment::new(
                StockAdjustmentId::new(format!("{prefix}-adj-{index}")),
                StockAdjustmentData {
                    adjustment_no: format!("S2A-{prefix}-{index:05}"),
                    warehouse_id: WarehouseId::new("s2a-wh-1"),
                    reason_type: AdjustmentReasonType::Damage,
                    prepared_by: manager_id.to_string(),
                    note: None,
                    occurred_at: None,
                },
                manager_id,
            )?);
        }
        collection.insert_many(docs).await?;
    }
    Ok(())
}

async fn grant(db: &Database, user: &str, grants: &[(&str, &str)]) -> Outcome<String> {
    let rule = db
        .collection::<Document>("casbin_rules")
        .find_one(doc! {"ptype": "g", "values.0": format!("user:admin:{user}")})
        .await?
        .ok_or("role missing")?;
    let key = rule.get_array("values")?[1].as_str().ok_or("invalid role")?.to_string();
    for (resource, action) in grants {
        db.collection::<Document>("casbin_rules")
            .insert_one(doc! {"_id": format!("p\u{1f}p\u{1f}{key}\u{1f}{resource}\u{1f}{action}"), "sec": "p", "ptype": "p", "values": [key.clone(), resource, action]})
            .await?;
    }
    Ok(key.strip_prefix("role:").ok_or("invalid role prefix")?.to_string())
}

async fn seed_company(db: &Database, role: &str, resource: &str, actions: &[&str]) -> Outcome {
    shared_rbac_service(db.clone())
        .seed_data_scope_manifest(
            role,
            resource,
            vec![DataScopeData {
                subject_type: DataScopeSubjectType::Role,
                subject_id: role.into(),
                scope_type: DataScopeType::Company,
                scope_targets: vec![],
                binding: ScopeBinding {
                    schema_version: 2,
                    resource: resource.into(),
                    actions: actions.iter().map(|action| (*action).to_string()).collect(),
                    target_dimension: ScopeDimension::InternalOrg,
                    target_mode: None,
                    include_descendants: None,
                    enabled: true,
                },
            }],
        )
        .await?;
    Ok(())
}
