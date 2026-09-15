//! S2 A36 入口级真实范围变化验收：同资源各入口分别按动作解析、版本正确传递、撤权失败关闭。
//! 仅允许显式 ERP_TEST_MONGO_URI（隔离副本集），使用随机库并在结束后删除；不用 hint，不用内存解释器做通过依据。
//! 本入口在 Port/Access 解析层对四类资源做真实库断言；Service 列表/详情的 409 映射由各域单元测试与 s2_http_acceptance 覆盖，此处不断言 HTTP 层。
use std::error::Error;
use std::time::{SystemTime, UNIX_EPOCH};

use application_core::AuditActor;
use erp_contract::service::contract::access::contract_scope;
use erp_contract::{ContractDataScopePort, ContractScopeObject};
use erp_core::AccountKind;
use erp_customer::service::customer::access::customer_scope;
use erp_customer::{CustomerDataScopePort, CustomerScopeObject};
use erp_identity::access_control::{
    DataScopeData, DataScopeSubjectType, DataScopeType, ScopeBinding, ScopeDimension,
};
use erp_identity::entity::organization::OrgUnitKind;
use erp_identity::entity::organization_change::{OrganizationChangeRequest, OrganizationOperation};
use erp_identity::service::access_control::resolve::DataScopeService;
use erp_processes::adapters::identity::shared_rbac_service;
use erp_processes::adapters::{
    MongoContractDataScope, MongoCustomerDataScope, MongoPurchaseDataScope, organization_service,
};
use erp_procurement::service::purchase_order::access::purchase_scope;
use erp_procurement::{PurchaseDataScopePort, PurchaseScopeObject};
use erp_read_models::sales_center::access::sales_scope;
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
    let db = client.database(&format!("erp_s2_a36_{suffix}"));
    let outcome = verify(&db).await;
    db.drop().await?;
    outcome
}

async fn verify(db: &Database) -> Outcome {
    erp_identity::indexes::ensure(db).await?;
    let actor_id = seed_admin_account(db).await?;
    let actor = AuditActor::new(actor_id.clone(), "s2-a36".into(), AccountKind::Admin);
    let role = grant_permissions(db, &actor_id).await?;
    seed_org_unit_scope(db, &role).await?;

    // 组织基线：建两个部门，责任人先归属 dept-a。
    let org = organization_service(db.clone(), shared_rbac_service(db.clone()));
    let dept_a = create_unit(&org, &actor, "s2-a36-dept-a").await?;
    let dept_b = create_unit(&org, &actor, "s2-a36-dept-b").await?;
    transfer(&org, &actor, &actor_id, &dept_a).await?;

    // 各资源仅登记读取动作的 Company 范围；写动作有权限但无范围（空集），证明按动作独立解析。
    seed_company(db, &role, "customer", &["list", "detail"]).await?;
    seed_company(db, &role, "contract", &["list", "detail"]).await?;
    seed_company(db, &role, "purchase_order", &["list", "detail"]).await?;
    seed_company(db, &role, "sales_order", &["list", "detail"]).await?;

    let customer_port = MongoCustomerDataScope::new(db.clone(), shared_rbac_service(db.clone()));
    let contract_port = MongoContractDataScope::new(db.clone(), shared_rbac_service(db.clone()));
    let purchase_port = MongoPurchaseDataScope::new(db.clone(), shared_rbac_service(db.clone()));

    // C1/T1/P1/S1：读有范围，写为空集；读写的 scope_version 不同（动作进入指纹）。
    let customer_list = customer_port.resolve(&actor, "list", &mut NoTransaction).await?;
    let customer_create = customer_port.resolve(&actor, "create", &mut NoTransaction).await?;
    assert!(customer_list.has_scope_rules(), "customer list must have rules");
    assert!(!customer_create.has_scope_rules(), "customer create must stay empty");
    assert_ne!(customer_list.scope_version, customer_create.scope_version);
    println!("PASS a36_customer_list_detail_vs_create_independent");

    let contract_list = contract_port.resolve(&actor, "list", &mut NoTransaction).await?;
    let contract_create = contract_port.resolve(&actor, "create", &mut NoTransaction).await?;
    assert!(contract_list.has_scope_rules());
    assert!(!contract_create.has_scope_rules());
    assert_ne!(contract_list.scope_version, contract_create.scope_version);
    println!("PASS a36_contract_list_detail_vs_create_independent");

    let purchase_list = purchase_port.resolve(&actor, "list", &mut NoTransaction).await?;
    let purchase_submit = purchase_port.resolve(&actor, "submit", &mut NoTransaction).await?;
    assert!(purchase_list.has_scope_rules());
    assert!(!purchase_submit.has_scope_rules());
    assert_ne!(purchase_list.scope_version, purchase_submit.scope_version);
    println!("PASS a36_purchase_list_detail_vs_submit_independent");

    let sales_list = DataScopeService::new(db.clone(), shared_rbac_service(db.clone()))
        .resolve(&actor, "sales_order", "list", &mut NoTransaction)
        .await?;
    let sales_submit = DataScopeService::new(db.clone(), shared_rbac_service(db.clone()))
        .resolve(&actor, "sales_order", "submit", &mut NoTransaction)
        .await?;
    assert!(sales_list.scope.has_role_scope());
    assert!(!sales_submit.scope.has_role_scope());
    assert_ne!(sales_list.scope_version, sales_submit.scope_version);
    println!("PASS a36_sales_list_vs_submit_independent");

    // 历史参与只补读取：detail 允许纯历史对象，create 不允许。
    let history_object = CustomerScopeObject {
        owned: false,
        collaborating: false,
        historical_read_participant: true,
        org_unit_id: None,
    };
    assert!(
        customer_port.resolve(&actor, "detail", &mut NoTransaction).await.map(|access| {
            let scoped = erp_customer::CustomerResolvedScope {
                user_id: access.user_id.clone(),
                resource: "customer".into(),
                action: "detail".into(),
                role_clauses: vec![],
                user_limit: None,
                policy_version: access.policy_version,
                organization_version: access.organization_version,
                scope_version: access.scope_version.clone(),
                as_of: access.as_of,
            };
            customer_port.allows(&scoped, &history_object).unwrap_or(false)
        })?,
        "detail must allow history-only object"
    );
    let create_empty = customer_port.resolve(&actor, "create", &mut NoTransaction).await?;
    assert!(!customer_port.allows(&create_empty, &history_object)?, "create must not borrow history");
    println!("PASS a36_history_supplements_read_only");

    // 版本正确传递：组织调岗后同一动作的 scope_version 与组织版本都变化。
    let before_version = customer_list.scope_version.clone();
    let before_org = customer_list.organization_version;
    // 同秒重复调岗按合同拒绝，等待进入下一秒后重试。
    tokio::time::sleep(std::time::Duration::from_millis(1200)).await;
    transfer(&org, &actor, &actor_id, &dept_b).await?;
    let after = customer_port.resolve(&actor, "list", &mut NoTransaction).await?;
    assert!(after.organization_version > before_org, "org version must advance");
    assert_ne!(after.scope_version, before_version, "scope version must change after transfer");
    println!("PASS a36_scope_version_changes_after_transfer");

    // 撤权失败关闭 + 真实 find 集合变化：customer 与 purchase 各 4 条 scratch 文档。
    verify_revoke_customer(db, &customer_port, &actor).await?;
    verify_revoke_purchase(db, &purchase_port, &actor).await?;
    verify_revoke_contract(db, &contract_port, &actor).await?;
    verify_revoke_sales(db, &actor).await?;

    println!(
        "LIMIT synthetic scratch documents in an isolated random database; service HTTP 409 mapping is covered by domain unit tests and s2_http_acceptance, not re-asserted here"
    );
    Ok(())
}

async fn verify_revoke_customer(db: &Database, port: &MongoCustomerDataScope, actor: &AuditActor) -> Outcome {
    let collection = db.collection::<Document>("s2_a36_customer");
    collection
        .insert_many(vec![doc! {"id": "c-0"}, doc! {"id": "c-1"}, doc! {"id": "c-2"}, doc! {"id": "c-3"}])
        .await?;
    let access = port.resolve(actor, "list", &mut NoTransaction).await?;
    let compiled = customer_scope(&access, actor.id(), &[], &[], vec![], &[]);
    let before = count_ids(&collection, compiled.document()).await?;
    assert_eq!(before, 4, "company scope must see all scratch docs");
    db.collection::<Document>("data_scopes")
        .update_many(
            doc! {"subject_type": "role", "resource": "customer"},
            doc! {"$set": {"deleted_at": 1_i64}},
        )
        .await?;
    let revoked = port.resolve(actor, "list", &mut NoTransaction).await?;
    assert!(!revoked.has_scope_rules(), "revoked customer list must be empty");
    let recompiled = customer_scope(&revoked, actor.id(), &[], &[], vec![], &[]);
    let after = count_ids(&collection, recompiled.document()).await?;
    assert_eq!(after, 0, "revoked scope must match nothing in real find");
    assert!(
        !port.allows(&revoked, &CustomerScopeObject { owned: true, ..Default::default() })?,
        "revoked scope must deny owned object"
    );
    println!("PASS a36_customer_revoke_fails_closed_real_find");
    Ok(())
}

async fn verify_revoke_purchase(db: &Database, port: &MongoPurchaseDataScope, actor: &AuditActor) -> Outcome {
    let collection = db.collection::<Document>("s2_a36_purchase");
    collection
        .insert_many(vec![doc! {"id": "p-0"}, doc! {"id": "p-1"}, doc! {"id": "p-2"}, doc! {"id": "p-3"}])
        .await?;
    let access = port.resolve(actor, "list", &mut NoTransaction).await?;
    let compiled = purchase_scope(&access, actor.id(), vec![]);
    let before = count_ids(&collection, compiled.document()).await?;
    assert_eq!(before, 4);
    db.collection::<Document>("data_scopes")
        .update_many(
            doc! {"subject_type": "role", "resource": "purchase_order"},
            doc! {"$set": {"deleted_at": 1_i64}},
        )
        .await?;
    let revoked = port.resolve(actor, "list", &mut NoTransaction).await?;
    assert!(!revoked.has_scope_rules());
    let recompiled = purchase_scope(&revoked, actor.id(), vec![]);
    let after = count_ids(&collection, recompiled.document()).await?;
    assert_eq!(after, 0);
    assert!(
        !port.allows(&revoked, &PurchaseScopeObject { owned: true, ..Default::default() })?,
        "revoked purchase must deny owned object"
    );
    println!("PASS a36_purchase_revoke_fails_closed_real_find");
    Ok(())
}

async fn verify_revoke_contract(db: &Database, port: &MongoContractDataScope, actor: &AuditActor) -> Outcome {
    let collection = db.collection::<Document>("s2_a36_contract");
    collection
        .insert_many(vec![doc! {"id": "t-0"}, doc! {"id": "t-1"}, doc! {"id": "t-2"}, doc! {"id": "t-3"}])
        .await?;
    let access = port.resolve(actor, "list", &mut NoTransaction).await?;
    let compiled = contract_scope(&access, actor.id(), &[], &[], vec![], &[]);
    let before = count_ids(&collection, compiled.document()).await?;
    assert_eq!(before, 4);
    db.collection::<Document>("data_scopes")
        .update_many(
            doc! {"subject_type": "role", "resource": "contract"},
            doc! {"$set": {"deleted_at": 1_i64}},
        )
        .await?;
    let revoked = port.resolve(actor, "list", &mut NoTransaction).await?;
    assert!(!revoked.has_scope_rules());
    let recompiled = contract_scope(&revoked, actor.id(), &[], &[], vec![], &[]);
    let after = count_ids(&collection, recompiled.document()).await?;
    assert_eq!(after, 0);
    assert!(
        !port.allows(&revoked, &ContractScopeObject { owned: true, ..Default::default() })?,
        "revoked contract must deny owned object"
    );
    println!("PASS a36_contract_revoke_fails_closed_real_find");
    Ok(())
}

async fn verify_revoke_sales(db: &Database, actor: &AuditActor) -> Outcome {
    let collection = db.collection::<Document>("s2_a36_sales");
    collection
        .insert_many(vec![
            doc! {"id": "s-0", "sales_owner_user_id": actor.id(), "business_org_unit_id": "org-a", "customer_id": "customer"},
            doc! {"id": "s-1", "sales_owner_user_id": actor.id(), "business_org_unit_id": "org-a", "customer_id": "customer"},
            doc! {"id": "s-2", "sales_owner_user_id": actor.id(), "business_org_unit_id": "org-a", "customer_id": "customer"},
            doc! {"id": "s-3", "sales_owner_user_id": actor.id(), "business_org_unit_id": "org-a", "customer_id": "customer"},
        ])
        .await?;
    let rbac = shared_rbac_service(db.clone());
    let access = DataScopeService::new(db.clone(), rbac.clone())
        .resolve(actor, "sales_order", "list", &mut NoTransaction)
        .await?;
    let scope = sales_scope(&access, actor.id(), &[], vec![]);
    let before = count_ids(&collection, scope.document()).await?;
    assert_eq!(before, 4);
    db.collection::<Document>("data_scopes")
        .update_many(
            doc! {"subject_type": "role", "resource": "sales_order"},
            doc! {"$set": {"deleted_at": 1_i64}},
        )
        .await?;
    let revoked = DataScopeService::new(db.clone(), rbac)
        .resolve(actor, "sales_order", "list", &mut NoTransaction)
        .await?;
    assert!(!revoked.scope.has_role_scope());
    let rescoped = sales_scope(&revoked, actor.id(), &[], vec![]);
    let after = count_ids(&collection, rescoped.document()).await?;
    assert_eq!(after, 0);
    println!("PASS a36_sales_revoke_fails_closed_real_find");
    Ok(())
}

async fn count_ids(collection: &mongodb::Collection<Document>, filter: Document) -> Outcome<u64> {
    let mut cursor = collection.find(filter).await?;
    let mut count = 0_u64;
    while cursor.advance().await? {
        let _: Document = cursor.deserialize_current()?;
        count += 1;
    }
    Ok(count)
}

async fn grant_permissions(db: &Database, user: &str) -> Outcome<String> {
    let rule = db
        .collection::<Document>("casbin_rules")
        .find_one(doc! {"ptype": "g", "values.0": format!("user:admin:{user}")})
        .await?
        .ok_or("role missing")?;
    let key = rule.get_array("values")?[1].as_str().ok_or("invalid role")?.to_string();
    let grants = [
        ("customer", "list"),
        ("customer", "detail"),
        ("customer", "create"),
        ("customer", "update"),
        ("customer", "delete"),
        ("contract", "list"),
        ("contract", "detail"),
        ("contract", "create"),
        ("contract", "update"),
        ("purchase_order", "list"),
        ("purchase_order", "detail"),
        ("purchase_order", "create"),
        ("purchase_order", "update"),
        ("purchase_order", "delete"),
        ("purchase_order", "submit"),
        ("purchase_order", "cancel_approval"),
        ("sales_order", "list"),
        ("sales_order", "detail"),
        ("sales_order", "submit"),
        ("org_unit", "list"),
        ("org_unit", "manage"),
    ];
    for (resource, action) in grants {
        db.collection::<Document>("casbin_rules")
            .insert_one(doc! {"_id": format!("p\u{1f}p\u{1f}{key}\u{1f}{resource}\u{1f}{action}"), "sec": "p", "ptype": "p", "values": [key.clone(), resource, action]})
            .await?;
    }
    Ok(key.strip_prefix("role:").ok_or("invalid role prefix")?.to_string())
}

async fn seed_org_unit_scope(db: &Database, role: &str) -> Outcome {
    shared_rbac_service(db.clone())
        .seed_data_scope_manifest(
            role,
            "org_unit",
            vec![DataScopeData {
                subject_type: DataScopeSubjectType::Role,
                subject_id: role.into(),
                scope_type: DataScopeType::Company,
                scope_targets: vec![],
                binding: ScopeBinding {
                    schema_version: 2,
                    resource: "org_unit".into(),
                    actions: vec!["list".into(), "manage".into()],
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

async fn current_org_version(
    org: &erp_identity::service::organization::OrganizationService,
    actor: &AuditActor,
) -> Outcome<u64> {
    Ok(org.state(actor).await?.version)
}

async fn create_unit(
    org: &erp_identity::service::organization::OrganizationService,
    actor: &AuditActor,
    name: &str,
) -> Outcome<String> {
    let version = current_org_version(org, actor).await?;
    let receipt = org
        .change(
            actor,
            OrganizationChangeRequest {
                expected_version: version,
                idempotency_key: format!("s2-a36-{name}"),
                reason: "S2 A36 isolated acceptance".into(),
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
    let version = current_org_version(org, actor).await?;
    org.change(
        actor,
        OrganizationChangeRequest {
            expected_version: version,
            idempotency_key: format!("s2-a36-transfer-{user_id}-{org_unit_id}-{version}"),
            reason: "S2 A36 isolated acceptance".into(),
            change: OrganizationOperation::TransferMember {
                user_id: user_id.into(),
                org_unit_id: org_unit_id.into(),
            },
        },
    )
    .await?;
    Ok(())
}
