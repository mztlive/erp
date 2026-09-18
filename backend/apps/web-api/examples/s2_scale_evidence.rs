//! S2 规模/性能隔离证据：任务代表性查询计划、审批首段匹配计划、
//! 任务管理与库存 20000 超限整体拒绝、生产事务耗时记录。
//! 仅允许显式 ERP_TEST_MONGO_URI（隔离副本集），随机库用后删除；explain 不用 hint；耗时只记录不判定。
//! 审批全管线聚合计划与 20000 审批超限仍待专用业务数据批次（见 S2 文档剩余项登记）。
use std::error::Error;
use std::num::NonZeroU32;
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
use mongodb::bson::{Document, doc};
use mongodb::{Client, Database};
use persistence_core::{NoTransaction, QueryFilter};
use test_support::seed_admin_account;

type Outcome<T = ()> = Result<T, Box<dyn Error>>;

#[tokio::main(flavor = "current_thread")]
async fn main() -> Outcome {
    let uri = std::env::var("ERP_TEST_MONGO_URI")?;
    let client = Client::with_uri_str(uri).await?;
    let suffix = SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos();
    let db = client.database(&format!("erp_s2_scale_{suffix}"));
    let outcome = verify(&db).await;
    db.drop().await?;
    outcome
}

async fn verify(db: &Database) -> Outcome {
    erp_identity::indexes::ensure(db).await?;
    erp_workflow::indexes::ensure(db).await?;
    let manager_id = seed_admin_account(db).await?;
    let outsider_id = seed_admin_account(db).await?;
    let manager = AuditActor::new(manager_id.clone(), "s2-scale".into(), AccountKind::Admin);
    let role = grant(
        db,
        &manager_id,
        &[
            ("org_unit", "list"),
            ("org_unit", "manage"),
            ("work_item", "list"),
            ("work_item", "manage"),
            ("stock_balance", "list"),
        ],
    )
    .await?;
    seed_company(db, &role, "org_unit", &["list", "manage"]).await?;

    let org = organization_service(db.clone(), shared_rbac_service(db.clone()));
    let dept = create_unit(&org, &manager, "s2-scale-dept").await?;
    transfer(&org, &manager, &manager_id, &dept).await?;
    let mut members = vec![manager_id.clone()];
    for index in 0..5 {
        sleep().await?;
        let user = seed_admin_account(db).await?;
        transfer(&org, &manager, &user, &dept).await?;
        members.push(user);
        let _ = index;
    }
    seed_explicit_org(db, &role, "work_item", &["manage"], std::slice::from_ref(&dept)).await?;

    // E1：2000 条任务上的管理队列代表性查询计划（生产 WorkItemFilter 条件）。
    seed_work_items(db, &members, &outsider_id, &dept, 2000).await?;
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
    assert!(!batch.is_empty(), "managed scan must hit seeded tasks");
    let plan = db.run_command(doc! {"explain": {"find": "work_items", "filter": filter.to_doc()}, "verbosity": "executionStats"}).await?;
    let evidence = plan_evidence(&plan)?;
    assert!(evidence.contains_ixscan, "managed queue must use IXSCAN: {evidence:?}");
    println!("PASS work_item_managed_plan");
    println!(
        "EVIDENCE_JSON {{\"case\":\"work_item_managed_plan\",\"collection\":\"work_items\",\"managed_owners\":{},\"batch\":{},\"nReturned\":{},\"totalKeysExamined\":{},\"totalDocsExamined\":{},\"winningPlan_contains\":\"IXSCAN\",\"index_hint_used\":false,\"data_volume\":{{\"work_items\":2000}}}}",
        owners.len(),
        batch.len(),
        evidence.n_returned,
        evidence.keys,
        evidence.docs
    );

    // E2：审批首段匹配计划（$match status＋$sort＋$limit；全管线 $lookup＋$facet 待专用批次）。
    seed_approval_skeletons(db, 1000).await?;
    let pipeline = vec![
        doc! {"$match": {"status": "RUNNING", "deleted_at": 0_i64}},
        doc! {"$sort": {"updated_at": -1, "id": -1}},
        doc! {"$limit": 20},
    ];
    let plan = db.run_command(doc! {"explain": {"aggregate": "approval_process_instances", "pipeline": pipeline, "cursor": {}}, "verbosity": "executionStats"}).await?;
    let evidence = plan_evidence(&plan)?;
    assert!(evidence.contains_ixscan, "approval first-stage match must use IXSCAN: {evidence:?}");
    println!("PASS approval_first_stage_plan");
    println!(
        "EVIDENCE_JSON {{\"case\":\"approval_first_stage_plan\",\"collection\":\"approval_process_instances\",\"winningPlan_contains\":\"IXSCAN\",\"index_hint_used\":false,\"data_volume\":{{\"approval_process_instances\":1000}},\"note\":\"first $match stage only; full lookup+facet pipeline remains open\"}}"
    );

    // E5：库存仓库 20000 超限整体拒绝。单条 scope_targets 上限 128，改用 160 条×128 个仓库的多规则并集触发。
    insert_overflow_warehouse_scopes(db, &role).await?;
    match erp_processes::adapters::authorize_inventory(
        db,
        &shared_rbac_service(db.clone()),
        &manager,
        &mut NoTransaction,
    )
    .await
    {
        Err(erp_inventory::Error::ValidationError(message)) => {
            assert_eq!(message, "库存范围超过 20000 个仓库，请缩小配置范围")
        },
        other => panic!("expected inventory over-limit rejection, got {other:?}"),
    }
    println!("PASS inventory_scope_over_limit_20000_rejected");
    println!(
        "EVIDENCE_JSON {{\"case\":\"inventory_scope_over_limit_20000_rejected\",\"warehouse_ids\":20480,\"rules\":160,\"returned\":\"error\",\"truncated\":false}}"
    );

    // E3：任务管理 20000 超限整体拒绝（20001 条同组织成员，文档形状复刻真实成员行）。
    insert_overflow_memberships(db, &dept).await?;
    match auth.managed_task_owners(&manager, &mut NoTransaction).await {
        Err(erp_workflow::Error::ValidationError(message)) => {
            assert_eq!(message, "任务管理范围超过 20000 人，请缩小配置范围")
        },
        other => panic!("expected task over-limit rejection, got {other:?}"),
    }
    println!("PASS task_scope_over_limit_20000_rejected");
    println!(
        "EVIDENCE_JSON {{\"case\":\"task_scope_over_limit_20000_rejected\",\"memberships\":20001,\"returned\":\"error\",\"truncated\":false}}"
    );

    // E6：生产规模事务耗时记录（组织调岗事务：授权解析＋版本推进＋审计同一事务，仅记录）。
    // 目标选 outsider（隔离账号、无成员关系），避免与已在组织内的成员冲突。
    let work_items = db.collection::<Document>("work_items").count_documents(doc! {}).await?;
    let memberships = db.collection::<Document>("org_memberships").count_documents(doc! {}).await?;
    let target: String = outsider_id.clone();
    sleep().await?;
    let started = std::time::Instant::now();
    let version = org.state(&manager).await?.version;
    org.change(
        &manager,
        OrganizationChangeRequest {
            expected_version: version,
            idempotency_key: format!("s2-scale-timed-{version}"),
            reason: "S2 scale timing".into(),
            change: OrganizationOperation::TransferMember { user_id: target, org_unit_id: dept.clone() },
        },
    )
    .await?;
    let elapsed = started.elapsed();
    let mongod_version: String =
        db.run_command(doc! {"buildInfo": 1}).await?.get_str("version").unwrap_or("unknown").to_string();
    println!("PASS tx_scope_write_timed");
    println!(
        "EVIDENCE_JSON {{\"case\":\"tx_scope_write_timed\",\"tx\":\"OrganizationService::change(TransferMember) resolve+write same transaction\",\"elapsed_ms\":{},\"result\":\"ok\",\"data_volume\":{{\"work_items\":{work_items},\"org_memberships\":{memberships}}},\"env\":{{\"os\":\"{}\",\"arch\":\"{}\",\"mongod_version\":\"{mongod_version}\"}},\"verdict\":\"record-only\"}}",
        elapsed.as_millis(),
        std::env::consts::OS,
        std::env::consts::ARCH
    );

    println!(
        "LIMIT synthetic documents in an isolated random database; approval full-pipeline plan and approval 20000 rejection remain for dedicated business-data batch"
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
                WorkItemId::new(format!("s2-scale-task-{index}")),
                WorkItemData {
                    work_item_type: WorkItemType::CustomerAcceptanceRegistration,
                    business_object_type: "sales_order".into(),
                    business_object_id: format!("s2-scale-order-{index}"),
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
                format!("sales_order:s2-scale-order-{index}:customer_acceptance"),
            )?);
        }
        collection.insert_many(docs).await?;
    }
    Ok(())
}

async fn seed_approval_skeletons(db: &Database, count: usize) -> Outcome {
    let collection = db.collection::<Document>("approval_process_instances");
    for chunk in (0..count).collect::<Vec<_>>().chunks(500) {
        let docs = chunk.iter().map(|index| {
            doc! {"id": format!("s2-scale-approval-{index}"), "status": "RUNNING", "updated_at": 1_700_000_000_i64 + (*index as i64), "deleted_at": 0_i64,
                "subject": {"subject_kind": "SALES_ORDER", "subject_id": format!("s2-scale-approval-{index}")}, "subject_version": *index as i32}
        }).collect::<Vec<_>>();
        collection.insert_many(docs).await?;
    }
    Ok(())
}

async fn insert_overflow_memberships(db: &Database, dept: &str) -> Outcome {
    let template = db
        .collection::<Document>("org_memberships")
        .find_one(doc! {})
        .await?
        .ok_or("membership template missing")?;
    let mut docs = Vec::new();
    for index in 0..20001 {
        let mut row = template.clone();
        row.remove("_id");
        set_nested_str(&mut row, &["id"], &format!("s2-scale-member-{index}"));
        set_nested_str(&mut row, &["base", "id"], &format!("s2-scale-member-{index}"));
        set_nested_str(&mut row, &["user_id"], &format!("s2-scale-user-{index}"));
        set_nested_str(&mut row, &["org_unit_id"], dept);
        docs.push(row);
    }
    for chunk in docs.chunks(2000) {
        db.collection::<Document>("org_memberships").insert_many(chunk.to_vec()).await?;
    }
    Ok(())
}

fn set_nested_str(doc: &mut Document, path: &[&str], value: &str) {
    if path.len() == 1 {
        doc.insert(path[0], value);
        return;
    }
    if let Some(mongodb::bson::Bson::Document(inner)) = doc.get_mut(path[0]) {
        set_nested_str(inner, &path[1..], value);
    }
}

async fn insert_overflow_warehouse_scopes(db: &Database, role: &str) -> Outcome {
    use erp_core::ids::DataScopeId;
    use erp_identity::access_control::{DataScope, DataScopeData};
    let mut scopes = Vec::new();
    for rule in 0..160 {
        let ids = (0..128).map(|index| format!("s2-scale-warehouse-{rule}-{index}")).collect::<Vec<_>>();
        scopes.push(DataScope::new(
            DataScopeId::new(format!("s2-scale-overflow-wh-{rule}")),
            DataScopeData {
                subject_type: DataScopeSubjectType::Role,
                subject_id: role.into(),
                scope_type: DataScopeType::Organization,
                scope_targets: ids,
                binding: ScopeBinding {
                    schema_version: 2,
                    resource: "stock_balance".into(),
                    actions: vec!["list".into()],
                    target_dimension: ScopeDimension::Warehouse,
                    target_mode: Some(ScopeTargetMode::Explicit),
                    include_descendants: None,
                    enabled: true,
                },
            },
        )?);
    }
    db.collection::<DataScope>("data_scopes").insert_many(scopes).await?;
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
                idempotency_key: format!("s2-scale-{name}"),
                reason: "S2 scale isolated evidence".into(),
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

async fn transfer(
    org: &erp_identity::service::organization::OrganizationService,
    actor: &AuditActor,
    user_id: &str,
    org_unit_id: &str,
) -> Outcome {
    let version = org.state(actor).await?.version;
    org.change(
        actor,
        OrganizationChangeRequest {
            expected_version: version,
            idempotency_key: format!("s2-scale-transfer-{user_id}-{org_unit_id}-{version}"),
            reason: "S2 scale isolated evidence".into(),
            change: OrganizationOperation::TransferMember {
                user_id: user_id.into(),
                org_unit_id: org_unit_id.into(),
            },
        },
    )
    .await?;
    Ok(())
}

async fn sleep() -> Outcome {
    tokio::time::sleep(std::time::Duration::from_millis(1200)).await;
    Ok(())
}
