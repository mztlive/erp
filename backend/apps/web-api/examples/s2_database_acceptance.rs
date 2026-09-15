//! S2 隔离数据库验收入口。仅允许显式 ERP_TEST_MONGO_URI，使用随机库并在结束后删除。
use std::error::Error;
use std::time::{SystemTime, UNIX_EPOCH};

use application_core::AuditActor;
use erp_core::AccountKind;
use erp_identity::SharedRbacService;
use erp_identity::access_control::{
    DataScopeData, DataScopeSubjectType, DataScopeType, ScopeBinding, ScopeDimension,
};
use erp_identity::entity::organization::OrgUnitKind;
use erp_identity::entity::organization_change::{OrganizationChangeRequest, OrganizationOperation};
use erp_identity::service::access_control::resolve::DataScopeService;
use erp_processes::adapters::identity::shared_rbac_service;
use erp_processes::adapters::organization_service;
use mongodb::bson::{Document, doc};
use mongodb::{Client, Database};
use persistence_core::NoTransaction;
use test_support::seed_admin_account;

type Outcome<T = ()> = Result<T, Box<dyn Error>>;

#[tokio::main(flavor = "current_thread")]
async fn main() -> Outcome {
    let uri = std::env::var("ERP_TEST_MONGO_URI")?;
    let client = Client::with_uri_str(uri).await?;
    let suffix = SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos();
    let db = client.database(&format!("erp_s2_acceptance_{suffix}"));
    let outcome = verify(&db).await;
    db.drop().await?;
    outcome
}

async fn verify(db: &Database) -> Outcome {
    erp_identity::indexes::ensure(db).await?;
    let user = seed_admin_account(db).await?;
    let actor = AuditActor::new(user.clone(), "s2-acceptance".into(), AccountKind::Admin);
    let role = fixture_permissions(db, &user).await?;
    let rbac = shared_rbac_service(db.clone());
    let manifest = company(&role, "org_unit", &["list", "manage"]);
    let (a, b) = tokio::join!(
        rbac.seed_data_scope_manifest(&role, "org_unit", vec![manifest.clone()]),
        rbac.seed_data_scope_manifest(&role, "org_unit", vec![manifest.clone()])
    );
    assert!(a.is_ok() || b.is_ok(), "至少一个并发初始化必须成功");
    rbac.seed_data_scope_manifest(&role, "org_unit", vec![manifest]).await?;
    assert_eq!(scope_count(db, &role, "org_unit").await?, 1);
    println!("PASS initialization_concurrency_and_rerun");

    let service = organization_service(db.clone(), rbac.clone());
    let left = create_unit("department_a", 0);
    let right = create_unit("department_b", 0);
    let (a, b) = tokio::join!(service.change(&actor, left.clone()), service.change(&actor, right.clone()));
    assert_eq!(usize::from(a.is_ok()) + usize::from(b.is_ok()), 1, "相同期望版本只允许一个事务成功");
    assert_eq!(db.collection::<Document>("org_units").count_documents(doc! {}).await?, 1);
    assert_eq!(db.collection::<Document>("org_changes").count_documents(doc! {}).await?, 1);
    let successful = if a.is_ok() { left } else { right };
    let replay = service.change(&actor, successful).await?;
    assert_eq!(replay.after.version, 1);
    assert_eq!(db.collection::<Document>("org_changes").count_documents(doc! {}).await?, 1);
    println!("PASS organization_concurrency_atomic_audit_and_idempotent_replay");

    let scopes = DataScopeService::new(db.clone(), rbac.clone());
    // 旧角色规则不参与 v2 授权，也不得破坏正常读取；旧个人上限必须阻断。
    let legacy = doc! { "id": "legacy-role-scope", "subject_type": "role", "subject_id": &role, "scope_type": "company", "deleted_at": 0_i64 };
    db.collection::<Document>("data_scopes").insert_one(legacy).await?;
    let access = scopes.resolve(&actor, "org_unit", "list", &mut NoTransaction).await?;
    assert!(access.scope.has_role_scope());
    assert_eq!(access.organizations.version, 1);
    db.collection::<Document>("data_scopes").insert_one(doc! { "id": "legacy-user-limit", "subject_type": "user", "subject_id": &user, "scope_type": "team", "deleted_at": 0_i64 }).await?;
    assert!(scopes.resolve(&actor, "org_unit", "list", &mut NoTransaction).await.is_err());
    db.collection::<Document>("data_scopes").delete_one(doc! { "id": "legacy-user-limit" }).await?;
    println!("PASS legacy_role_ignored_and_legacy_user_limit_fails_closed");
    verify_revoked_seed(db, &rbac, &role).await?;
    let revoked = scopes.resolve(&actor, "org_unit", "list", &mut NoTransaction).await?;
    assert!(!revoked.scope.has_role_scope());
    assert_ne!(access.scope_version, revoked.scope_version);
    println!("PASS real_resolution_and_revoked_seed_not_restored");

    let original = db
        .collection::<Document>("data_scopes")
        .find_one(doc! { "subject_id": &role, "schema_version": 2 })
        .await?
        .ok_or("scope fixture missing")?;
    let fixtures = (0..5000)
        .map(|index| {
            let mut row = original.clone();
            row.remove("_id");
            row.insert("id", format!("plan-scope-{index}"));
            row.insert("subject_id", format!("plan-role-{index}"));
            row.insert("deleted_at", 0_i64);
            row
        })
        .collect::<Vec<_>>();
    db.collection::<Document>("data_scopes").insert_many(fixtures).await?;
    // 与公共解析器 list_by_subjects 的实际条件一致；不使用 hint 强迫索引。
    let filter =
        doc! { "subject_type": "role", "subject_id": { "$in": ["plan-role-100"] }, "deleted_at": 0_i64 };
    let plan = db
        .run_command(
            doc! { "explain": { "find": "data_scopes", "filter": filter }, "verbosity": "executionStats" },
        )
        .await?;
    let stats = plan.get_document("executionStats")?;
    assert_eq!(stats.get_i32("nReturned")?, 1);
    assert!(stats.get_i32("totalDocsExamined")? <= 5);
    let winning = serde_json::to_string(plan.get_document("queryPlanner")?.get_document("winningPlan")?)?;
    assert!(winning.contains("IXSCAN"), "subject lookup must use an index: {winning}");
    println!(
        "INDEX_PLAN {}",
        serde_json::to_string(&doc! {
            "nReturned": stats.get_i32("nReturned").unwrap_or_default(),
            "totalKeysExamined": stats.get_i32("totalKeysExamined").unwrap_or_default(),
            "totalDocsExamined": stats.get_i32("totalDocsExamined").unwrap_or_default(),
            "winningPlan": plan.get_document("queryPlanner")?.get_document("winningPlan")?.clone(),
        })?
    );
    println!(
        "LIMIT 5000 synthetic scope rows; production-volume and existing business-account acceptance remain separate"
    );
    Ok(())
}

fn create_unit(key: &str, version: u64) -> OrganizationChangeRequest {
    OrganizationChangeRequest {
        expected_version: version,
        idempotency_key: key.into(),
        reason: "S2 isolated acceptance".into(),
        change: OrganizationOperation::CreateUnit {
            name: key.into(),
            parent_id: None,
            kind: OrgUnitKind::Department,
        },
    }
}

fn company(role: &str, resource: &str, actions: &[&str]) -> DataScopeData {
    DataScopeData {
        subject_type: DataScopeSubjectType::Role,
        subject_id: role.into(),
        scope_type: DataScopeType::Company,
        scope_targets: vec![],
        binding: ScopeBinding {
            schema_version: 2,
            resource: resource.into(),
            actions: actions.iter().map(|s| (*s).into()).collect(),
            target_dimension: ScopeDimension::InternalOrg,
            target_mode: None,
            include_descendants: None,
            enabled: true,
        },
    }
}

async fn scope_count(db: &Database, role: &str, resource: &str) -> Outcome<u64> {
    Ok(db
        .collection::<Document>("data_scopes")
        .count_documents(doc! {"subject_id": role, "resource": resource})
        .await?)
}

async fn fixture_permissions(db: &Database, user: &str) -> Outcome<String> {
    let rule = db
        .collection::<Document>("casbin_rules")
        .find_one(doc! {"ptype": "g", "values.0": format!("user:admin:{user}")})
        .await?
        .ok_or("missing role")?;
    let key = rule.get_array("values")?[1].as_str().ok_or("invalid role")?;
    for action in ["list", "manage"] {
        let id = format!("p\u{1f}p\u{1f}{key}\u{1f}org_unit\u{1f}{action}");
        db.collection::<Document>("casbin_rules")
            .insert_one(doc! {"_id": id, "sec": "p", "ptype": "p", "values": [key, "org_unit", action]})
            .await?;
    }
    Ok(key.strip_prefix("role:").ok_or("invalid role prefix")?.into())
}

async fn verify_revoked_seed(db: &Database, rbac: &SharedRbacService, role: &str) -> Outcome {
    db.collection::<Document>("data_scopes")
        .update_many(doc! {"subject_id": role}, doc! {"$set": {"deleted_at": 1_i64}})
        .await?;
    rbac.seed_data_scope_manifest(role, "org_unit", vec![company(role, "org_unit", &["list", "manage"])])
        .await?;
    assert_eq!(scope_count(db, role, "org_unit").await?, 1);
    assert_eq!(
        db.collection::<Document>("data_scopes")
            .count_documents(doc! {"subject_id": role, "deleted_at": 0_i64})
            .await?,
        0
    );
    Ok(())
}
