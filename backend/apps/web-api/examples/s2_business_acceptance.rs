//! S2 业务矩阵验收（授权核心，隔离库）：A21 查询不写、A22 管理收窄可追溯、
//! §8.3.2(a)(b) 维度隔离、(c) 审批失读、(d) 上限收窄、(e) 成员到期、(f) 跨页撤权。
//! 仅允许显式 ERP_TEST_MONGO_URI（隔离副本集），随机库用后删除；不用 hint。
//! 任务改派候选双向对照、采购级联原子性、非审批阻塞视图与统计排除等任务层条目，
//! 仍由专用业务环境与后续批次覆盖，此处不断言（见 S2 文档剩余项登记）。
use std::collections::BTreeSet;
use std::error::Error;
use std::time::{SystemTime, UNIX_EPOCH};

use application_core::AuditActor;
use erp_core::AccountKind;
use erp_core::ids::DataScopeId;
use erp_customer::{CustomerDataScopePort, CustomerScopeObject};
use erp_identity::access_control::{
    DataScope, DataScopeData, DataScopeSubjectType, DataScopeType, ScopeBinding, ScopeClause, ScopeDimension,
    ScopeTargetMode, ScopedObject,
};
use erp_identity::entity::organization::OrgUnitKind;
use erp_identity::entity::organization_change::{OrganizationChangeRequest, OrganizationOperation};
use erp_identity::service::access_control::resolve::DataScopeService;
use erp_processes::adapters::identity::shared_rbac_service;
use erp_processes::adapters::organization_service;
use erp_processes::adapters::workflow::workflow_auth;
use erp_workflow::ports::WorkflowAuthorizationPort;
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
    let db = client.database(&format!("erp_s2_business_{suffix}"));
    let outcome = verify(&db).await;
    db.drop().await?;
    outcome
}

async fn verify(db: &Database) -> Outcome {
    erp_identity::indexes::ensure(db).await?;
    let manager_id = seed_admin_account(db).await?;
    let member_id = seed_admin_account(db).await?;
    let outsider_id = seed_admin_account(db).await?;
    let manager = AuditActor::new(manager_id.clone(), "s2b-manager".into(), AccountKind::Admin);
    let member = AuditActor::new(member_id.clone(), "s2b-member".into(), AccountKind::Admin);
    let manager_role = grant(
        db,
        &manager_id,
        &[
            ("org_unit", "list"),
            ("org_unit", "manage"),
            ("work_item", "list"),
            ("work_item", "manage"),
            ("customer", "list"),
            ("customer", "detail"),
            ("sales_order", "list"),
            ("sales_order", "detail"),
        ],
    )
    .await?;
    let member_role = grant(
        db,
        &member_id,
        &[("customer", "list"), ("customer", "detail"), ("sales_order", "list"), ("sales_order", "detail")],
    )
    .await?;
    let _ = grant(db, &outsider_id, &[("customer", "list"), ("customer", "detail")]).await?;
    seed_company(db, &manager_role, "org_unit", &["list", "manage"]).await?;

    let org = organization_service(db.clone(), shared_rbac_service(db.clone()));
    let dept_a = create_unit(&org, &manager, "s2b-dept-a").await?;
    let dept_b = create_unit(&org, &manager, "s2b-dept-b").await?;
    transfer(&org, &manager, &manager_id, &dept_a).await?;
    sleep().await?;
    transfer(&org, &manager, &member_id, &dept_a).await?;

    // B2/A22：管理范围 Company→显式组织收窄可追溯（managed owners 从无界变为有界子集）。
    seed_company(db, &manager_role, "work_item", &["manage"]).await?;
    let auth = workflow_auth(db.clone(), shared_rbac_service(db.clone()));
    let before = auth.managed_task_owners(&manager, &mut NoTransaction).await?;
    assert!(before.is_none(), "company manage must be unbounded None");
    // 同主体同资源已有 Company 历史，manifest 不会追加；改用直接写入显式组织文档模拟收窄后的第二条规则前，
    // 先软删 Company 再登记显式组织，保证单资源单条有效规则。
    db.collection::<Document>("data_scopes")
        .update_many(
            doc! {"subject_type": "role", "subject_id": &manager_role, "resource": "work_item"},
            doc! {"$set": {"deleted_at": 1_i64}},
        )
        .await?;
    seed_explicit_org(db, &manager_role, "work_item", &["manage"], std::slice::from_ref(&dept_a)).await?;
    let after = auth.managed_task_owners(&manager, &mut NoTransaction).await?;
    let owners = after.ok_or("narrowed manage must be bounded")?;
    assert!(
        owners.contains(&manager_id) && owners.contains(&member_id),
        "dept members must be managed, got {owners:?}"
    );
    assert!(!owners.contains(&outsider_id), "outsider must fall out after narrowing");
    let audit_count = db.collection::<Document>("org_changes").count_documents(doc! {}).await?;
    assert!(audit_count >= 4, "each change must leave audit receipt, got {audit_count}");
    println!("PASS b2_manage_narrowing_traceable");

    // B1/A21：只读查询不触发写入（解析＋真实 find 前后业务集合计数不变）。
    let port =
        erp_processes::adapters::MongoCustomerDataScope::new(db.clone(), shared_rbac_service(db.clone()));
    seed_company(db, &member_role, "customer", &["list", "detail"]).await?;
    let counts_before = business_counts(db).await?;
    for action in ["list", "detail"] {
        let access = port.resolve(&member, action, &mut NoTransaction).await?;
        let compiled = erp_customer::service::customer::access::customer_scope(
            &access,
            member.id(),
            &[],
            &[],
            vec![],
            &[],
        );
        let _ = count_ids(&db.collection::<Document>("s2b_noop"), compiled.document()).await.unwrap_or(0);
    }
    let _ = DataScopeService::new(db.clone(), shared_rbac_service(db.clone()))
        .resolve(&member, "customer", "list", &mut NoTransaction)
        .await?;
    assert_eq!(business_counts(db).await?, counts_before, "read-only queries must not write");
    println!("PASS b1_query_does_not_write");

    // B3/§8.3.2(b)：同字符串跨维度不授权（生产 ScopeClause::covers 语义）。
    let org_clause =
        ScopeClause { org_unit_ids: BTreeSet::from(["dept-x".to_string()]), ..ScopeClause::default() };
    let warehouse_clause =
        ScopeClause { warehouse_ids: BTreeSet::from(["dept-x".to_string()]), ..ScopeClause::default() };
    // 仓库维度的同字符串不得充当内部组织授权。
    assert!(
        !warehouse_clause.covers(&ScopedObject {
            owned: false,
            collaborating: false,
            historical_read_participant: false,
            org_unit_id: Some("dept-x"),
            settlement_party_id: None,
            warehouse_id: None,
        }),
        "warehouse id must not authorize org object"
    );
    assert!(org_clause.covers(&ScopedObject {
        owned: false,
        collaborating: false,
        historical_read_participant: false,
        org_unit_id: Some("dept-x"),
        settlement_party_id: None,
        warehouse_id: None,
    }));
    // B4/§8.3.2(a)：多维求交，缺一维即拒绝。
    let multi = ScopeClause {
        org_unit_ids: BTreeSet::from(["d1".to_string()]),
        settlement_party_ids: BTreeSet::from(["p1".to_string()]),
        ..ScopeClause::default()
    };
    assert!(
        !multi.covers(&ScopedObject {
            owned: false,
            collaborating: false,
            historical_read_participant: false,
            org_unit_id: Some("d1"),
            settlement_party_id: Some("p-other"),
            warehouse_id: None,
        }),
        "settlement mismatch must deny even when org matches"
    );
    assert!(multi.covers(&ScopedObject {
        owned: false,
        collaborating: false,
        historical_read_participant: false,
        org_unit_id: Some("d1"),
        settlement_party_id: Some("p1"),
        warehouse_id: None,
    }));
    println!("PASS b3_b4_dimension_isolation_and_intersection");

    // B5/§8.3.2(c)：审批/详情读取资格被撤销后失败关闭（sales_order detail 解析变空）。
    seed_company(db, &member_role, "sales_order", &["list", "detail"]).await?;
    let sales_before = DataScopeService::new(db.clone(), shared_rbac_service(db.clone()))
        .resolve(&member, "sales_order", "detail", &mut NoTransaction)
        .await?;
    assert!(sales_before.scope.has_role_scope());
    db.collection::<Document>("data_scopes")
        .update_many(
            doc! {"subject_type": "role", "subject_id": &member_role, "resource": "sales_order"},
            doc! {"$set": {"deleted_at": 1_i64}},
        )
        .await?;
    let sales_after = DataScopeService::new(db.clone(), shared_rbac_service(db.clone()))
        .resolve(&member, "sales_order", "detail", &mut NoTransaction)
        .await?;
    assert!(!sales_after.scope.has_role_scope(), "revoked detail must be empty");
    assert_ne!(sales_before.scope_version, sales_after.scope_version);
    println!("PASS b5_lost_read_fails_closed");

    // B6/§8.3.2(d)：个人上限收窄角色并集与历史参与（真实解析）。
    seed_company(db, &member_role, "customer", &["list", "detail"]).await?;
    insert_user_limit(db, &member_id, "customer", &["list"], std::slice::from_ref(&dept_b)).await?;
    let narrowed = port.resolve(&member, "list", &mut NoTransaction).await?;
    assert!(narrowed.user_limit.is_some(), "user limit must be present");
    // 角色 Company 全覆盖，但上限仅 dept-b：dept-a 对象被裁剪，dept-b 对象保留。
    assert!(
        !port.allows(
            &narrowed,
            &CustomerScopeObject {
                owned: false,
                collaborating: false,
                historical_read_participant: false,
                org_unit_id: Some(dept_a.clone()),
            }
        )?,
        "object outside user limit must be denied"
    );
    assert!(
        port.allows(
            &narrowed,
            &CustomerScopeObject {
                owned: false,
                collaborating: false,
                historical_read_participant: false,
                org_unit_id: Some(dept_b.clone()),
            }
        )?,
        "object inside user limit must stay visible"
    );
    // 历史参与同样受上限裁剪。
    assert!(
        !port.allows(
            &{
                let mut scoped = narrowed.clone();
                scoped.action = "list".into();
                scoped
            },
            &CustomerScopeObject {
                owned: false,
                collaborating: false,
                historical_read_participant: true,
                org_unit_id: Some(dept_a.clone()),
            }
        )?,
        "history outside user limit must be denied"
    );
    println!("PASS b6_user_limit_narrows_role_and_history");

    // B7/§8.3.2(e)：成员关系结束后动态本人组织范围变空（自然到期共享同一有效期检查，见单元测试）。
    seed_own_org(db, &member_role, "customer", &["list"]).await?;
    let dynamic_before = port.resolve(&member, "list", &mut NoTransaction).await?;
    assert!(dynamic_before.has_scope_rules());
    end_membership(&org, &manager, &member_id).await?;
    let dynamic_after = port.resolve(&member, "list", &mut NoTransaction).await?;
    assert!(!dynamic_after.has_scope_rules(), "ended membership must empty own-org scope");
    assert_ne!(dynamic_before.scope_version, dynamic_after.scope_version);
    println!("PASS b7_membership_end_empties_own_org");

    // B8/§8.3.2(f)：跨页/导出语义——同一动作前后两次解析版本不同即必须刷新（409 映射由各域单元测试覆盖）。
    let first = port.resolve(&manager, "list", &mut NoTransaction).await?;
    sleep().await?;
    transfer(&org, &manager, &manager_id, &dept_b).await?;
    let second = port.resolve(&manager, "list", &mut NoTransaction).await?;
    assert_ne!(first.scope_version, second.scope_version, "export must re-verify with fresh version");
    println!("PASS b8_cross_page_version_must_refresh");

    println!(
        "LIMIT synthetic accounts and scopes in an isolated random database; task candidate/cascade/blocked-view specifics remain for dedicated business environment"
    );
    Ok(())
}

async fn business_counts(db: &Database) -> Outcome<(u64, u64, u64)> {
    Ok((
        db.collection::<Document>("work_items").count_documents(doc! {}).await?,
        db.collection::<Document>("org_changes").count_documents(doc! {}).await?,
        db.collection::<Document>("data_scopes").count_documents(doc! {}).await?,
    ))
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
    // 同主体同资源已有历史（含软删除）时 manifest 跳过；调用方自行保证每资源只播一次或先清理。
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
    // manifest 在有历史时跳过（软删除也算历史），此时改用类型化实体直接写入显式组织规则。
    if db
        .collection::<Document>("data_scopes")
        .count_documents(doc! {"subject_id": role, "resource": resource, "deleted_at": 0_i64})
        .await?
        == 0
    {
        let scope = DataScope::new(
            DataScopeId::new(format!("s2b:{role}:{resource}:explicit")),
            DataScopeData {
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
            },
        )?;
        db.collection::<DataScope>("data_scopes").insert_one(scope).await?;
    }
    Ok(())
}

async fn seed_own_org(db: &Database, role: &str, resource: &str, actions: &[&str]) -> Outcome {
    db.collection::<Document>("data_scopes")
        .update_many(
            doc! {"subject_type": "role", "subject_id": role, "resource": resource},
            doc! {"$set": {"deleted_at": 1_i64}},
        )
        .await?;
    shared_rbac_service(db.clone())
        .seed_data_scope_manifest(
            role,
            resource,
            vec![DataScopeData {
                subject_type: DataScopeSubjectType::Role,
                subject_id: role.into(),
                scope_type: DataScopeType::Organization,
                scope_targets: vec![],
                binding: ScopeBinding {
                    schema_version: 2,
                    resource: resource.into(),
                    actions: actions.iter().map(|action| (*action).to_string()).collect(),
                    target_dimension: ScopeDimension::InternalOrg,
                    target_mode: Some(ScopeTargetMode::OwnOrg),
                    include_descendants: Some(false),
                    enabled: true,
                },
            }],
        )
        .await?;
    // manifest 在有历史时跳过（软删除也算历史），此时改用类型化实体直接写入动态规则。
    if db
        .collection::<Document>("data_scopes")
        .count_documents(doc! {"subject_id": role, "resource": resource, "deleted_at": 0_i64})
        .await?
        == 0
    {
        let scope = DataScope::new(
            DataScopeId::new(format!("s2b:{role}:{resource}:own-org")),
            DataScopeData {
                subject_type: DataScopeSubjectType::Role,
                subject_id: role.into(),
                scope_type: DataScopeType::Organization,
                scope_targets: vec![],
                binding: ScopeBinding {
                    schema_version: 2,
                    resource: resource.into(),
                    actions: actions.iter().map(|action| (*action).to_string()).collect(),
                    target_dimension: ScopeDimension::InternalOrg,
                    target_mode: Some(ScopeTargetMode::OwnOrg),
                    include_descendants: Some(false),
                    enabled: true,
                },
            },
        )?;
        db.collection::<DataScope>("data_scopes").insert_one(scope).await?;
    }
    Ok(())
}

async fn insert_user_limit(
    db: &Database,
    user: &str,
    resource: &str,
    actions: &[&str],
    orgs: &[String],
) -> Outcome {
    let scope = DataScope::new(
        DataScopeId::new(format!("s2b:user-limit:{user}:{resource}")),
        DataScopeData {
            subject_type: DataScopeSubjectType::User,
            subject_id: user.into(),
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
        },
    )?;
    db.collection::<DataScope>("data_scopes").insert_one(scope).await?;
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
                idempotency_key: format!("s2b-{name}"),
                reason: "S2 business isolated acceptance".into(),
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
            idempotency_key: format!("s2b-transfer-{user_id}-{org_unit_id}-{version}"),
            reason: "S2 business isolated acceptance".into(),
            change: OrganizationOperation::TransferMember {
                user_id: user_id.into(),
                org_unit_id: org_unit_id.into(),
            },
        },
    )
    .await?;
    Ok(())
}

async fn end_membership(
    org: &erp_identity::service::organization::OrganizationService,
    actor: &AuditActor,
    user_id: &str,
) -> Outcome {
    sleep().await?;
    let version = current_org_version(org, actor).await?;
    org.change(
        actor,
        OrganizationChangeRequest {
            expected_version: version,
            idempotency_key: format!("s2b-end-{user_id}-{version}"),
            reason: "S2 business isolated acceptance".into(),
            change: OrganizationOperation::EndMembership { user_id: user_id.into() },
        },
    )
    .await?;
    Ok(())
}

async fn sleep() -> Outcome {
    tokio::time::sleep(std::time::Duration::from_millis(1200)).await;
    Ok(())
}
