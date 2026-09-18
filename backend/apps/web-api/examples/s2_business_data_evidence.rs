//! S2 代表性业务数据验收（隔离库）：在业务种子形态的数据上复核管理队列查询计划、
//! 审批 Started 全管线计划、任务 20000 超限整体拒绝、组织调岗并发场景与耗时。
//! 需要显式 `ERP_TEST_MONGO_URI`（隔离副本集）与 `S2_BUSINESS_SOURCE_DB`（只读业务种子库，
//! 如浏览器验收库；本入口只读该库、不写入）；业务集合复制到随机库后验证，用后删除；
//! explain 不用 hint；耗时只记录不判定。数据量仍低于生产规模，见 LIMIT 行。
use std::error::Error;
use std::num::NonZeroU32;
use std::str::FromStr;
use std::time::{SystemTime, UNIX_EPOCH};

use application_core::AuditActor;
use erp_core::AccountKind;
use erp_core::ids::WorkItemId;
use erp_identity::access_control::{
    DataScopeData, DataScopeSubjectType, DataScopeType, ScopeBinding, ScopeDimension, ScopeTargetMode,
};
use erp_identity::entity::organization::OrgUnitKind;
use erp_identity::entity::organization_change::{OrganizationChangeRequest, OrganizationOperation};
use erp_processes::adapters::identity::shared_rbac_service;
use erp_processes::adapters::organization_service;
use erp_processes::adapters::workflow::workflow_auth;
use erp_workflow::WorkItemRepository;
use erp_workflow::entity::work_item::{
    AssignmentSource, WorkItem, WorkItemData, WorkItemPriority, WorkItemStatus, WorkItemType,
};
use erp_workflow::ports::WorkflowAuthorizationPort;
use erp_workflow::repository::WorkItemRepositoryExt;
use erp_workflow::repository::work_item::WorkItemFilter;
use mongodb::bson::{Bson, Document, doc};
use mongodb::{Client, Database};
use persistence_core::{NoTransaction, QueryFilter};
use test_support::seed_admin_account;

type Outcome<T = ()> = Result<T, Box<dyn Error>>;

// 仍低于生产规模的代表性体量；并发移动用真实种子账号。
const TASK_SEED: usize = 500;
const APPROVAL_SEED: usize = 300;
const OVER_LIMIT: usize = 20_001;
const CONCURRENCY: usize = 6;
const SEED_ACCOUNTS: [&str; 6] = ["cangchu", "caiwu", "guanli", "xiaoshou", "caigou", "yunying"];

#[tokio::main(flavor = "current_thread")]
async fn main() -> Outcome {
    let uri = std::env::var("ERP_TEST_MONGO_URI")?;
    let source_name = std::env::var("S2_BUSINESS_SOURCE_DB")?;
    assert!(
        !source_name.trim().is_empty() && !source_name.starts_with("erp_s2_bizdata_"),
        "必须显式指定只读业务种子库 S2_BUSINESS_SOURCE_DB"
    );
    let client = Client::with_uri_str(uri).await?;
    let suffix = SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos();
    let target_name = format!("erp_s2_bizdata_{suffix}");
    assert_ne!(source_name, target_name, "随机库不得与来源库同名");
    copy_business_database(&client.database(&source_name), &client.database(&target_name)).await?;
    let db = client.database(&target_name);
    let outcome = verify(&db).await;
    db.drop().await?;
    outcome
}

/// 把业务种子库的全部用户集合复制到随机库；来源库只读，不写回。
async fn copy_business_database(source: &Database, target: &Database) -> Outcome {
    let mut names = source.list_collection_names().await?;
    names.retain(|name| !name.starts_with("system."));
    names.sort();
    assert!(!names.is_empty(), "业务种子库必须包含用户集合");
    let mut copied = 0_usize;
    for name in &names {
        let origin = source.collection::<Document>(name);
        let destination = target.collection::<Document>(name);
        let mut cursor = origin.find(doc! {}).await?;
        let mut chunk = Vec::with_capacity(1000);
        while cursor.advance().await? {
            chunk.push(cursor.deserialize_current()?);
            if chunk.len() >= 1000 {
                let rows = std::mem::replace(&mut chunk, Vec::with_capacity(1000));
                if !rows.is_empty() {
                    destination.insert_many(rows).await?;
                    copied += 1000;
                }
            }
        }
        if !chunk.is_empty() {
            copied += chunk.len();
            destination.insert_many(chunk).await?;
        }
    }
    assert!(copied > 0, "业务种子库必须包含文档");
    println!("seeded business-shaped copy: {copied} documents in {} collections", names.len());
    Ok(())
}

async fn verify(db: &Database) -> Outcome {
    erp_identity::indexes::ensure(db).await?;
    erp_workflow::indexes::ensure(db).await?;
    erp_inventory::indexes::ensure(db).await?;

    let manager_id = seed_admin_account(db).await?;
    let outsider_id = seed_admin_account(db).await?;
    let manager = AuditActor::new(manager_id.clone(), "s2-bizdata".into(), AccountKind::Admin);
    let role = grant(
        db,
        &manager_id,
        &[
            ("org_unit", "list"),
            ("org_unit", "manage"),
            ("work_item", "list"),
            ("work_item", "manage"),
            ("approval_instance", "read"),
        ],
    )
    .await?;
    seed_company(db, &role, "org_unit", &["list", "manage"]).await?;

    // 真实种子账号：任务属主、审批发起人与并发移动对象。
    let mut seed_users = Vec::new();
    for account in SEED_ACCOUNTS {
        let row = db
            .collection::<Document>("accounts")
            .find_one(doc! {"account": account})
            .await?
            .unwrap_or_else(|| panic!("业务种子账号缺失 {account}"));
        seed_users.push(row.get_str("id")?.to_string());
    }
    let starter = seed_users[0].clone();

    let org = organization_service(db.clone(), shared_rbac_service(db.clone()));
    let dept = create_unit(&org, &manager, "s2-bizdata-dept").await?;
    for user in &seed_users {
        assert!(
            transfer_with_retry(&org, &manager, user, &dept, "seed").await?,
            "seeded user must transfer into the representative department"
        );
    }
    seed_explicit_org(db, &role, "work_item", &["manage"], std::slice::from_ref(&dept)).await?;

    // R1：业务种子身份下的管理队列生产条件查询计划（500 条真实属主任务）。
    seed_work_items(db, &seed_users, &outsider_id, &dept, TASK_SEED).await?;
    let auth = workflow_auth(db.clone(), shared_rbac_service(db.clone()));
    let owners = auth
        .managed_task_owners(&manager, &mut NoTransaction)
        .await?
        .ok_or("manage scope must be bounded")?;
    let filter = WorkItemFilter {
        statuses: vec![WorkItemStatus::Open],
        managed_owner_ids: Some(owners.clone()),
        page: 1,
        page_size: 20,
        ..Default::default()
    };
    let repo = WorkItemRepository::new(db, "work_items");
    let batch =
        repo.scan_work_item_batch(&filter, 0, NonZeroU32::new(20).unwrap(), &mut NoTransaction).await?;
    assert!(!batch.is_empty(), "managed scan must hit business-shaped tasks");
    let plan = db
        .run_command(
            doc! {"explain": {"find": "work_items", "filter": filter.to_doc()}, "verbosity": "executionStats"},
        )
        .await?;
    let evidence = plan_evidence(&plan)?;
    assert!(evidence.contains_ixscan, "managed queue must use IXSCAN: {evidence:?}");
    println!("PASS representative_work_item_managed_plan");
    println!(
        "EVIDENCE_JSON {{\"case\":\"representative_work_item_managed_plan\",\"collection\":\"work_items\",\"managed_owners\":{},\"batch\":{},\"nReturned\":{},\"totalKeysExamined\":{},\"totalDocsExamined\":{},\"winningPlan_contains\":\"IXSCAN\",\"index_hint_used\":false,\"data_volume\":{{\"work_items\":{TASK_SEED},\"owners\":\"seeded business accounts\"}}}}",
        owners.len(),
        batch.len(),
        evidence.n_returned,
        evidence.keys,
        evidence.docs
    );

    // R2：审批 Started 全管线在业务种子库形态上的计划（发起人为真实种子账号）。
    seed_instances(db, &starter).await?;
    let pipeline = full_pipeline(&starter);
    let plan = db
        .run_command(doc! {"explain": {"aggregate": "approval_process_instances", "pipeline": pipeline, "cursor": {}}, "verbosity": "queryPlanner"})
        .await?;
    let stages = plan.get_array("stages")?;
    let cursor = stages
        .first()
        .ok_or("explain stages missing")?
        .as_document()
        .ok_or("first explain stage must be $cursor")?
        .get_document("$cursor")?;
    let winning = serde_json::to_string(cursor.get_document("queryPlanner")?.get_document("winningPlan")?)?;
    assert!(winning.contains("IXSCAN"), "approval pipeline must use IXSCAN: {winning}");
    let flat = serde_json::to_string(&plan)?;
    assert!(flat.contains("$lookup") && flat.contains("$facet"), "pipeline must keep $lookup and $facet");
    println!("PASS representative_approval_pipeline_plan");
    println!(
        "EVIDENCE_JSON {{\"case\":\"representative_approval_pipeline_plan\",\"collection\":\"approval_process_instances\",\"winningPlan_contains\":\"IXSCAN\",\"stages_contain\":[\"$lookup\",\"$facet\"],\"index_hint_used\":false,\"data_volume\":{{\"approval_process_instances\":{APPROVAL_SEED},\"starter\":\"seeded business account\"}}}}"
    );

    // R3：任务管理 20000 超限在业务成员行形态上整体拒绝（模板取自真实种子成员行）。
    insert_overflow_memberships(db, &seed_users[0], &dept).await?;
    match auth.managed_task_owners(&manager, &mut NoTransaction).await {
        Err(erp_workflow::Error::ValidationError(message)) => {
            assert_eq!(message, "任务管理范围超过 20000 人，请缩小配置范围")
        },
        other => panic!("expected task over-limit rejection, got {other:?}"),
    }
    println!("PASS representative_task_over_limit_20000_rejected");
    println!(
        "EVIDENCE_JSON {{\"case\":\"representative_task_over_limit_20000_rejected\",\"memberships\":{OVER_LIMIT},\"template\":\"business seed row\",\"returned\":\"error\",\"truncated\":false}}"
    );

    // R4：真实种子账号的并发组织移动（同版本冲突重试），耗时只记录。
    let dept2 = create_unit(&org, &manager, "s2-bizdata-dept2").await?;
    let started = std::time::Instant::now();
    let mut set = tokio::task::JoinSet::new();
    for (index, user) in seed_users.iter().take(CONCURRENCY).cloned().enumerate() {
        let worker_org = org.clone();
        let worker_manager = manager.clone();
        let target = dept2.clone();
        set.spawn(async move {
            match transfer_with_retry(&worker_org, &worker_manager, &user, &target, &format!("race-{index}"))
                .await
            {
                Ok(true) => true,
                Ok(false) => false,
                Err(error) => {
                    eprintln!("move task error: {error}");
                    false
                },
            }
        });
    }
    let mut failures = 0_usize;
    while let Some(joined) = set.join_next().await {
        match joined {
            Ok(true) => {},
            _ => failures += 1,
        }
    }
    let elapsed = started.elapsed();
    assert_eq!(failures, 0, "concurrent seeded moves must all succeed");
    let memberships = db.collection::<Document>("org_memberships").count_documents(doc! {}).await?;
    let work_items = db.collection::<Document>("work_items").count_documents(doc! {}).await?;
    println!("PASS representative_concurrent_org_moves_timed");
    println!(
        "EVIDENCE_JSON {{\"case\":\"representative_concurrent_org_moves_timed\",\"moves\":{CONCURRENCY},\"actors\":\"seeded business accounts\",\"failures\":{failures},\"elapsed_ms\":{},\"data_volume\":{{\"org_memberships\":{memberships},\"work_items\":{work_items}}},\"verdict\":\"record-only\"}}",
        elapsed.as_millis()
    );

    println!(
        "LIMIT business-seeded copy in an isolated random database; volumes remain below production and export-download is covered by the browser spec, not re-asserted here"
    );
    Ok(())
}

#[derive(Debug)]
struct PlanEvidence {
    n_returned: i32,
    keys: i32,
    docs: i32,
    contains_ixscan: bool,
}

fn plan_evidence(plan: &Document) -> Outcome<PlanEvidence> {
    let stats = plan.get_document("executionStats")?;
    let winning = serde_json::to_string(plan.get_document("queryPlanner")?.get_document("winningPlan")?)?;
    Ok(PlanEvidence {
        n_returned: stats.get_i32("nReturned").unwrap_or_default(),
        keys: stats.get_i32("totalKeysExamined").unwrap_or_default(),
        docs: stats.get_i32("totalDocsExamined").unwrap_or_default(),
        contains_ixscan: winning.contains("IXSCAN"),
    })
}

async fn seed_work_items(
    db: &Database,
    members: &[String],
    outsider: &str,
    dept: &str,
    count: usize,
) -> Outcome {
    let collection = db.collection::<WorkItem>("work_items");
    for chunk in (0..count).collect::<Vec<_>>().chunks(500) {
        let mut docs = Vec::new();
        for index in chunk {
            let owner =
                if index % 2 == 0 { members[index % members.len()].clone() } else { outsider.to_string() };
            docs.push(WorkItem::new_with_responsibility_key(
                WorkItemId::new(format!("s2r-task-{index}")),
                WorkItemData {
                    work_item_type: WorkItemType::CustomerAcceptanceRegistration,
                    business_object_type: "sales_order".into(),
                    business_object_id: format!("s2r-order-{index}"),
                    subject_version: "1".into(),
                    owner_role: "sales_order_owner".into(),
                    owner_organization_id: if index % 2 == 0 { dept.into() } else { "other-dept".into() },
                    owner_user_id: owner,
                    assignment_source: AssignmentSource::SystemRule,
                    priority: WorkItemPriority::Normal,
                    due_at: None,
                    reason_code: Some("CUSTOMER_ACCEPTANCE_REQUIRED".into()),
                    impact_summary: None,
                },
                format!("sales_order:s2r-order-{index}:customer_acceptance"),
            )?);
        }
        collection.insert_many(docs).await?;
    }
    Ok(())
}

fn instance_doc(id: &str, object_id: &str, starter: &str, at: i64) -> Document {
    doc! {
        "id": id,
        "version": 1_i64,
        "created_at": at,
        "updated_at": at,
        "deleted_at": 0_i64,
        "process_definition_id": "s2r-def-1",
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

async fn seed_instances(db: &Database, starter: &str) -> Outcome {
    let instances = db.collection::<Document>("approval_process_instances");
    let snapshots = db.collection::<Document>("approval_subject_snapshots");
    let quantity =
        Bson::Decimal128(mongodb::bson::Decimal128::from_str("3").map_err(|_| "quantity decimal")?);
    for chunk in (0..APPROVAL_SEED).collect::<Vec<_>>().chunks(500) {
        let mut inst = Vec::with_capacity(chunk.len());
        let mut snaps = Vec::with_capacity(chunk.len());
        for index in chunk {
            let at = 1_700_000_000_i64 + (*index as i64);
            inst.push(instance_doc(&format!("s2r-inst-{index}"), &format!("s2r-adj-{index}"), starter, at));
            snaps.push(doc! {
                "id": format!("s2r-snap-{index}"),
                "version": 1_i64,
                "created_at": at,
                "updated_at": at,
                "deleted_at": 0_i64,
                "approval_process_instance_id": format!("s2r-inst-{index}"),
                "document_type": "stock_adjustment",
                "business_object_id": format!("s2r-adj-{index}"),
                "subject_version": 1_i32,
                "payload": {
                    "document_no": format!("S2R-s2r-adj-{index}"),
                    "responsible_org_id": "s2r-org-1",
                    "submitted_by": starter,
                    "submitted_at": at,
                    "counterparty": Bson::Null,
                    "total_amount": Bson::Null,
                    "total_quantity": quantity.clone(),
                    "line_count": 1_i32,
                },
            });
        }
        instances.insert_many(inst).await?;
        snapshots.insert_many(snaps).await?;
    }
    Ok(())
}

// 与生产 Started 视图同形的全管线；$in 为政策全部必须审批类型（按 as_str 排序）。
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
        doc! {"$facet": {
            "items": [doc! {"$limit": 21_i64}, doc! {"$project": projection}],
            "total": [doc! {"$count": "count"}],
        }},
    ]
}

/// 业务种子成员行做模板，扩写 20001 条同组织成员；整体拒绝且零截断。
/// 模板取本次转入的真实种子成员行，其余字段原样保留，只换主键与用户。
async fn insert_overflow_memberships(db: &Database, seed_user: &str, dept: &str) -> Outcome {
    let mut template = db
        .collection::<Document>("org_memberships")
        .find_one(doc! {"user_id": seed_user, "org_unit_id": dept, "deleted_at": 0_i64})
        .await?
        .ok_or("business membership template missing")?;
    template.remove("_id");
    assert!(
        template.contains_key("user_id") && template.contains_key("org_unit_id"),
        "template must carry member keys"
    );
    let dept = template.get_str("org_unit_id")?.to_string();
    assert!(!dept.is_empty(), "template must carry a real org unit");
    let mut docs = Vec::with_capacity(OVER_LIMIT);
    for index in 0..OVER_LIMIT {
        let mut row = template.clone();
        row.insert("id", format!("s2r-member-{index}"));
        row.insert("user_id", format!("s2r-user-{index}"));
        row.insert("org_unit_id", dept.clone());
        docs.push(row);
    }
    for chunk in docs.chunks(2000) {
        db.collection::<Document>("org_memberships").insert_many(chunk.to_vec()).await?;
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

async fn seed_explicit_org(
    db: &Database,
    role: &str,
    resource: &str,
    actions: &[&str],
    orgs: &[String],
) -> Outcome {
    shared_rbac_service(db.clone())
        .seed_data_scope_manifest(
            role,
            resource,
            vec![DataScopeData {
                subject_type: DataScopeSubjectType::Role,
                subject_id: role.into(),
                scope_type: DataScopeType::Organization,
                scope_targets: orgs.to_vec(),
                binding: ScopeBinding {
                    schema_version: 2,
                    resource: resource.into(),
                    actions: actions.iter().map(|action| (*action).to_string()).collect(),
                    target_dimension: ScopeDimension::InternalOrg,
                    target_mode: Some(ScopeTargetMode::Explicit),
                    include_descendants: Some(false),
                    enabled: true,
                },
            }],
        )
        .await?;
    Ok(())
}

async fn create_unit(
    org: &erp_identity::service::organization::OrganizationService,
    actor: &AuditActor,
    name: &str,
) -> Outcome<String> {
    let version = org.state(actor).await?.version;
    let receipt = org
        .change(
            actor,
            OrganizationChangeRequest {
                expected_version: version,
                idempotency_key: format!("s2r-{name}"),
                reason: "S2 business-data isolated evidence".into(),
                change: OrganizationOperation::CreateUnit {
                    name: name.into(),
                    parent_id: None,
                    kind: OrgUnitKind::Department,
                },
            },
        )
        .await?;
    receipt
        .after
        .units
        .into_iter()
        .find(|unit| unit.name == name)
        .map(|unit| unit.base.id.to_string())
        .ok_or_else(|| "created unit missing".into())
}

async fn transfer_with_retry(
    org: &erp_identity::service::organization::OrganizationService,
    actor: &AuditActor,
    user_id: &str,
    org_unit_id: &str,
    tag: &str,
) -> Outcome<bool> {
    for attempt in 0..12 {
        let version = org.state(actor).await?.version;
        let outcome = org
            .change(
                actor,
                OrganizationChangeRequest {
                    expected_version: version,
                    idempotency_key: format!("s2r-transfer-{tag}-{user_id}-{version}-{attempt}"),
                    reason: "S2 business-data isolated evidence".into(),
                    change: OrganizationOperation::TransferMember {
                        user_id: user_id.into(),
                        org_unit_id: org_unit_id.into(),
                    },
                },
            )
            .await;
        match outcome {
            Ok(_) => return Ok(true),
            Err(error) => {
                let message = error.to_string();
                // 并发写冲突与同秒关系刚生效都按串行重试消化；尝试次数耗尽才算失败。
                if attempt < 11
                    && (message.contains("组织范围已变化")
                        || message.contains("关系刚生效")
                        || message.contains("并发事务冲突"))
                {
                    sleep().await?;
                    continue;
                }
                eprintln!("transfer failed for {user_id}: {message}");
                return Ok(false);
            },
        }
    }
    eprintln!("transfer retries exhausted for {user_id}");
    Ok(false)
}

async fn sleep() -> Outcome {
    tokio::time::sleep(std::time::Duration::from_millis(1200)).await;
    Ok(())
}
