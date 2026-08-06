//! 域 D06 `access_control` 仓储集成测试（真实 MongoDB 单节点副本集）。
//!
//! 统一 `#[ignore]` + `ERP_TEST_MONGO_URI` 门控（test-support `require_mongo!`）：
//! 无数据库环境 `cargo test` 跳过；CI 与验收执行
//! `cargo test -p database --test access_control_repository -- --include-ignored`。
//! 每个测试使用独立随机数据库名，结束自动 drop（TestDb）。
//!
//! 只覆盖本批次新增集合（permission / user_role / data_scope / audit_event）；
//! accounts / roles / audit_logs 既有能力不在此回归。

use database::repository::extensions::AccessControlExt;
use database::{ensure_indexes, NoTransaction, Transactional};
use entities::access_control::{
    AuditEvent, AuditEventData, AuditEventResult, DataScope, DataScopeData, DataScopeSubjectType,
    DataScopeType, Permission, PermissionData, UserRole, UserRoleData, UserRoleRevokeData,
};
use entities::common::time::Instant;
use entities::ids::{AuditEventId, DataScopeId, PermissionId, UserRoleId};
use entities::rbac::RoleId;
use mongodb::Database;
use test_support::{assert_indexes, require_mongo, TestDb};

/// 权限定义列表筛选条件类型（经 `AccessControlExt` 关联类型跨 crate 可达）。
type PermissionFilter = <Database as AccessControlExt>::PermissionFilter;
/// 数据范围列表筛选条件类型。
type DataScopeFilter = <Database as AccessControlExt>::DataScopeFilter;
/// 审计事件列表筛选条件类型。
type AuditEventFilter = <Database as AccessControlExt>::AuditEventFilter;

/// 构造可复用的权限定义实体。
fn sample_permission(id: &str, resource: &str, action: &str) -> Permission {
    Permission::new(
        PermissionId::new(id),
        PermissionData {
            resource: resource.to_string(),
            action: action.to_string(),
            name: format!("{resource}:{action} 审批"),
            description: Some("权限定义".to_string()),
            system: false,
        },
    )
    .unwrap()
}

/// 构造可复用的用户角色绑定实体。
fn sample_binding(id: &str, user_id: &str, role: &str) -> UserRole {
    UserRole::new(
        UserRoleId::new(id),
        UserRoleData {
            user_id: user_id.to_string(),
            role_id: RoleId::parse(role).unwrap(),
            effective_from: Instant::from_unix_secs(1_700_000_000),
            effective_to: Some(Instant::from_unix_secs(1_700_604_800)),
            assigned_by: "admin-1".to_string(),
        },
    )
    .unwrap()
}

/// 构造可复用的数据范围实体。
fn sample_data_scope(id: &str, subject_id: &str) -> DataScope {
    DataScope::new(
        DataScopeId::new(id),
        DataScopeData {
            subject_type: DataScopeSubjectType::Role,
            subject_id: subject_id.to_string(),
            scope_type: DataScopeType::Team,
            scope_targets: vec!["team-1".to_string(), "team-2".to_string()],
        },
    )
    .unwrap()
}

/// 构造可复用的审计事件实体。
fn sample_audit_event(id: &str, action_type: &str) -> AuditEvent {
    AuditEvent::new(
        AuditEventId::new(id),
        AuditEventData {
            actor_id: "user-1".to_string(),
            actor_label: "张三".to_string(),
            actor_role: "sales".to_string(),
            action_type: action_type.to_string(),
            object_type: "sales_order".to_string(),
            object_id: Some("SO-1".to_string()),
            object_label: Some("销售单 SO-1".to_string()),
            request_id: Some("req-1".to_string()),
            trace_id: None,
            result: AuditEventResult::Success,
            changed_field_names: vec!["status".to_string()],
            safe_digest: Some("digest-1".to_string()),
            source_ip: Some("10.0.0.1".to_string()),
            device_context: None,
        },
    )
    .unwrap()
}

/// 执行 `ensure_indexes` 并断言本域新增集合全部必需索引就位。
async fn assert_domain_indexes(db: &Database) {
    ensure_indexes(db).await.expect("ensure_indexes 应成功");
    assert_indexes(
        db,
        <Database as AccessControlExt>::PERMISSIONS,
        &["uk_permissions_resource_action", "idx_permissions_disabled"],
    )
    .await
    .expect("permissions 索引缺失");
    assert_indexes(
        db,
        <Database as AccessControlExt>::USER_ROLES,
        &["uk_user_roles_active", "idx_user_roles_user_effective"],
    )
    .await
    .expect("user_roles 索引缺失");
    assert_indexes(
        db,
        <Database as AccessControlExt>::DATA_SCOPES,
        &["uk_data_scopes_subject_scope", "idx_data_scopes_scope_type"],
    )
    .await
    .expect("data_scopes 索引缺失");
    assert_indexes(
        db,
        <Database as AccessControlExt>::AUDIT_EVENTS,
        &[
            "uk_audit_events_id",
            "idx_audit_events_actor_created",
            "idx_audit_events_object_created",
            "idx_audit_events_created",
            "idx_audit_events_request_id",
        ],
    )
    .await
    .expect("audit_events 索引缺失");
}

#[tokio::test]
#[ignore]
async fn permission_roundtrip_and_optimistic_lock() {
    require_mongo!(async {
        let test_db = TestDb::new("acctl_perm_crud").await.unwrap();
        let db = test_db.db();
        assert_domain_indexes(db).await;

        let mut permission = sample_permission("perm-1", "sales_order", "approve");
        db.permissions()
            .create(&permission, &mut NoTransaction)
            .await
            .unwrap();
        assert_eq!(permission.base.version, 1);

        let found = db
            .permissions()
            .find_by_id(&permission.base.id, &mut NoTransaction)
            .await
            .unwrap()
            .expect("创建后应可读回");
        assert_eq!(found.resource, "sales_order");
        assert_eq!(found.action, "approve");
        assert_eq!(found.name, "sales_order:approve 审批");
        assert!(!found.system);
        assert!(!found.disabled);

        let by_key = db
            .permissions()
            .find_by_resource_action("sales_order", "approve", &mut NoTransaction)
            .await
            .unwrap()
            .expect("按资源动作应命中");
        assert_eq!(by_key.base.id, "perm-1");

        permission
            .update(entities::access_control::PermissionUpdate {
                name: Some("新名称".to_string()),
                description: None,
                disabled: Some(true),
            })
            .unwrap();
        db.permissions()
            .update(&mut permission, &mut NoTransaction)
            .await
            .unwrap();
        assert_eq!(permission.base.version, 2, "乐观锁成功后 version 递增");
        assert!(permission.disabled);
    })
}

#[tokio::test]
#[ignore]
async fn permission_duplicate_identity_surfaces_duplicate_key() {
    require_mongo!(async {
        let test_db = TestDb::new("acctl_perm_dup").await.unwrap();
        let db = test_db.db();
        assert_domain_indexes(db).await;

        db.permissions()
            .create(
                &sample_permission("perm-1", "sales_order", "approve"),
                &mut NoTransaction,
            )
            .await
            .unwrap();

        let duplicate = sample_permission("perm-2", "SALES_ORDER", "APPROVE");
        let error = db
            .permissions()
            .create(&duplicate, &mut NoTransaction)
            .await
            .expect_err("重复 (resource, action) 必须被唯一索引拒绝");
        assert!(
            matches!(error, database::Error::DuplicateKey(_)),
            "期望 DuplicateKey，实际为 {error:?}"
        );
        assert_eq!(duplicate.resource, "sales_order", "实体层已规范化身份字段");
    })
}

#[tokio::test]
#[ignore]
async fn user_role_partial_unique_blocks_double_active_binding_but_allows_after_revoke() {
    require_mongo!(async {
        let test_db = TestDb::new("acctl_role_uniq").await.unwrap();
        let db = test_db.db();
        assert_domain_indexes(db).await;

        let mut binding = sample_binding("ur-1", "user-1", "role-sales");
        db.user_roles()
            .create(&binding, &mut NoTransaction)
            .await
            .unwrap();

        let duplicate = sample_binding("ur-2", "user-1", "role-sales");
        let error = db
            .user_roles()
            .create(&duplicate, &mut NoTransaction)
            .await
            .expect_err("同一用户同一角色两条未撤权绑定必须被唯一索引拒绝");
        assert!(
            matches!(error, database::Error::DuplicateKey(_)),
            "期望 DuplicateKey，实际为 {error:?}"
        );

        binding
            .revoke(
                UserRoleRevokeData {
                    revoke_reason_code: "EMERGENCY_REVOKE".to_string(),
                    revoke_reason_text: Some("紧急撤权".to_string()),
                },
                "admin-2",
                Instant::from_unix_secs(1_700_100_000),
            )
            .unwrap();
        db.user_roles()
            .update(&mut binding, &mut NoTransaction)
            .await
            .unwrap();
        assert!(binding.revoked_at.is_some());

        let reassigned = sample_binding("ur-3", "user-1", "role-sales");
        db.user_roles()
            .create(&reassigned, &mut NoTransaction)
            .await
            .expect("撤权后允许重新授权");

        let active = db
            .user_roles()
            .list_active_by_user_and_role(
                "user-1",
                &RoleId::parse("role-sales").unwrap(),
                &mut NoTransaction,
            )
            .await
            .unwrap();
        assert_eq!(active.len(), 1, "未撤权绑定只有新的一条");
        assert_eq!(active[0].base.id, "ur-3");
        let all = db
            .user_roles()
            .list_by_user("user-1", &mut NoTransaction)
            .await
            .unwrap();
        assert_eq!(all.len(), 2, "历史撤权记录保留展示");
    })
}

#[tokio::test]
#[ignore]
async fn data_scope_unique_subject_scope_and_soft_delete() {
    require_mongo!(async {
        let test_db = TestDb::new("acctl_scope").await.unwrap();
        let db = test_db.db();
        assert_domain_indexes(db).await;

        let mut scope = sample_data_scope("ds-1", "role-sales");
        db.data_scopes().create(&scope, &mut NoTransaction).await.unwrap();

        let duplicate = sample_data_scope("ds-2", "role-sales");
        let error = db
            .data_scopes()
            .create(&duplicate, &mut NoTransaction)
            .await
            .expect_err("同一主体同一范围类型重复必须被唯一索引拒绝");
        assert!(
            matches!(error, database::Error::DuplicateKey(_)),
            "期望 DuplicateKey，实际为 {error:?}"
        );

        let team = DataScope::new(
            DataScopeId::new("ds-3"),
            DataScopeData {
                subject_type: DataScopeSubjectType::Role,
                subject_id: "role-sales".to_string(),
                scope_type: DataScopeType::Company,
                scope_targets: vec![],
            },
        )
        .unwrap();
        db.data_scopes()
            .create(&team, &mut NoTransaction)
            .await
            .expect("同一主体不同范围类型可共存");

        let by_subject = db
            .data_scopes()
            .list_by_subject(DataScopeSubjectType::Role, "role-sales", &mut NoTransaction)
            .await
            .unwrap();
        assert_eq!(by_subject.len(), 2);

        db.data_scopes()
            .soft_delete(&mut scope, &mut NoTransaction)
            .await
            .unwrap();
        let after_delete = db
            .data_scopes()
            .find_by_id(&scope.base.id, &mut NoTransaction)
            .await
            .unwrap();
        assert!(after_delete.is_none(), "软删除后按 ID 不可见");

        db.data_scopes()
            .restore(&mut scope, &mut NoTransaction)
            .await
            .unwrap();
        let after_restore = db
            .data_scopes()
            .find_by_id(&scope.base.id, &mut NoTransaction)
            .await
            .unwrap();
        assert!(after_restore.is_some(), "恢复后按 ID 重新可见");
    })
}

#[tokio::test]
#[ignore]
async fn audit_event_roundtrip_and_search_with_projection() {
    require_mongo!(async {
        let test_db = TestDb::new("acctl_audit").await.unwrap();
        let db = test_db.db();
        assert_domain_indexes(db).await;

        let mut denied = sample_audit_event("ae-1", "sales_order.approve");
        denied.result = AuditEventResult::Denied;
        db.audit_events()
            .create(&denied, &mut NoTransaction)
            .await
            .unwrap();
        db.audit_events()
            .create(
                &sample_audit_event("ae-2", "sales_order.approve"),
                &mut NoTransaction,
            )
            .await
            .unwrap();

        let found = db
            .audit_events()
            .find_by_id(&denied.base.id, &mut NoTransaction)
            .await
            .unwrap()
            .expect("创建后应可读回");
        assert_eq!(found.actor_id, "user-1");
        assert_eq!(found.actor_label, "张三");
        assert_eq!(found.object_id.as_deref(), Some("SO-1"));
        assert_eq!(found.result, AuditEventResult::Denied);
        assert_eq!(found.changed_field_names, vec!["status".to_string()]);
        assert_eq!(found.source_ip.as_deref(), Some("10.0.0.1"));

        let filter = AuditEventFilter {
            actor_id: Some("user-1".to_string()),
            action_type: Some("sales_order.approve".to_string()),
            object_type: Some("sales_order".to_string()),
            object_id: Some("SO-1".to_string()),
            result: Some(AuditEventResult::Denied),
            page: 1,
            page_size: 1,
            sort_by: Some("created_at".to_string()),
            sort_ascending: true,
        };
        let page = db
            .audit_events()
            .search_audit_events(&filter, &mut NoTransaction)
            .await
            .unwrap();
        assert_eq!(page.total, 1, "结果筛选只命中拒绝事件");
        assert_eq!(page.items.len(), 1);
        let row = &page.items[0];
        assert_eq!(row.id, "ae-1");
        assert_eq!(row.actor_label, "张三");
        assert_eq!(row.action_type, "sales_order.approve");
        assert_eq!(row.result, AuditEventResult::Denied);
        assert_eq!(row.object_id.as_deref(), Some("SO-1"));
        assert!(row.created_at > 0);

        let all = AuditEventFilter {
            actor_id: None,
            action_type: None,
            object_type: None,
            object_id: None,
            result: None,
            page: 1,
            page_size: 1,
            sort_by: Some("created_at".to_string()),
            sort_ascending: true,
        };
        let first = db
            .audit_events()
            .search_audit_events(&all, &mut NoTransaction)
            .await
            .unwrap();
        assert_eq!(first.total, 2, "无筛选命中两条");
        assert_eq!(first.items.len(), 1, "分页边界第一页一条");
        let second_page = AuditEventFilter { page: 2, ..all };
        let second = db
            .audit_events()
            .search_audit_events(&second_page, &mut NoTransaction)
            .await
            .unwrap();
        assert_eq!(second.items.len(), 1, "分页边界第二页一条");
        assert_ne!(second.items[0].id, first.items[0].id);
    })
}

#[tokio::test]
#[ignore]
async fn permission_and_data_scope_search_respect_sort_whitelist() {
    require_mongo!(async {
        let test_db = TestDb::new("acctl_list").await.unwrap();
        let db = test_db.db();
        assert_domain_indexes(db).await;

        db.permissions()
            .create(
                &sample_permission("perm-1", "sales_order", "approve"),
                &mut NoTransaction,
            )
            .await
            .unwrap();
        db.permissions()
            .create(
                &sample_permission("perm-2", "purchase_order", "review"),
                &mut NoTransaction,
            )
            .await
            .unwrap();

        let filter = PermissionFilter {
            resource: Some("order".to_string()),
            disabled: Some(false),
            system: Some(false),
            page: 1,
            page_size: 20,
            sort_by: Some("resource".to_string()),
            sort_ascending: false,
        };
        let page = db
            .permissions()
            .search_permissions(&filter, &mut NoTransaction)
            .await
            .unwrap();
        assert_eq!(page.total, 2, "资源模糊命中两条");
        assert_eq!(page.items.len(), 2);
        let mut resources: Vec<&str> = page.items.iter().map(|row| row.resource.as_str()).collect();
        resources.sort_unstable();
        assert_eq!(resources, vec!["purchase_order", "sales_order"]);
        let row = &page.items[0];
        assert_eq!(row.action, "approve");
        assert!(matches!(row.resource.as_str(), "sales_order" | "purchase_order"));
        assert_eq!(row.name, "sales_order:approve 审批");
        assert!(!row.system);
        assert!(!row.disabled);

        let data_scope = DataScopeFilter {
            subject_type: Some(DataScopeSubjectType::Role),
            scope_type: Some(DataScopeType::Team),
            page: 1,
            page_size: 20,
            sort_by: Some("subject_id".to_string()),
            sort_ascending: false,
        };
        let scopes = db
            .data_scopes()
            .search_data_scopes(&data_scope, &mut NoTransaction)
            .await
            .unwrap();
        assert_eq!(scopes.total, 0, "白名单外排序字段回落默认排序，不报错");
    })
}

#[tokio::test]
#[ignore]
async fn assign_user_role_with_audit_commits_atomically_inside_transaction() {
    require_mongo!(async {
        let test_db = TestDb::new("acctl_tx_commit").await.unwrap();
        let db = test_db.db();
        assert_domain_indexes(db).await;

        let binding = sample_binding("ur-1", "user-1", "role-sales");
        let event = sample_audit_event("ae-1", "rbac.assign_role");

        let db_clone = db.clone();
        let binding_for_tx = binding.clone();
        let event_for_tx = event.clone();
        test_db
            .client()
            .with_transaction::<_, (), database::Error>(move |session| {
                Box::pin(async move {
                    db_clone
                        .access_control()
                        .assign_user_role_with_audit(&binding_for_tx, &event_for_tx, session)
                        .await?;
                    Ok(())
                })
            })
            .await
            .expect("事务提交应成功");

        let binding_found = db
            .user_roles()
            .find_by_id(&binding.base.id, &mut NoTransaction)
            .await
            .unwrap();
        assert!(binding_found.is_some(), "事务提交后绑定可见");
        let event_found = db
            .audit_events()
            .find_by_id(&event.base.id, &mut NoTransaction)
            .await
            .unwrap();
        assert!(event_found.is_some(), "事务提交后审计事件可见");
    })
}

#[tokio::test]
#[ignore]
async fn assign_user_role_with_audit_rolls_back_on_binding_conflict() {
    require_mongo!(async {
        let test_db = TestDb::new("acctl_tx_conflict").await.unwrap();
        let db = test_db.db();
        assert_domain_indexes(db).await;

        db.user_roles()
            .create(
                &sample_binding("ur-0", "user-1", "role-sales"),
                &mut NoTransaction,
            )
            .await
            .unwrap();

        let conflicting = sample_binding("ur-1", "user-1", "role-sales");
        let event = sample_audit_event("ae-1", "rbac.assign_role");

        let db_clone = db.clone();
        let binding_for_tx = conflicting.clone();
        let event_for_tx = event.clone();
        let result: std::result::Result<(), database::Error> = test_db
            .client()
            .with_transaction(move |session| {
                Box::pin(async move {
                    db_clone
                        .access_control()
                        .assign_user_role_with_audit(&binding_for_tx, &event_for_tx, session)
                        .await?;
                    Ok(())
                })
            })
            .await;
        assert!(
            matches!(result, Err(database::Error::DuplicateKey(_))),
            "未撤权绑定冲突必须整体回滚并透出 DuplicateKey，实际为 {result:?}"
        );

        let event_found = db
            .audit_events()
            .find_by_id(&event.base.id, &mut NoTransaction)
            .await
            .unwrap();
        assert!(event_found.is_none(), "冲突回滚后审计事件不得残留");
        let binding_found = db
            .user_roles()
            .find_by_id(&conflicting.base.id, &mut NoTransaction)
            .await
            .unwrap();
        assert!(binding_found.is_none(), "冲突回滚后绑定不得残留");
    })
}

#[tokio::test]
#[ignore]
async fn assign_user_role_with_audit_no_transaction_leaves_partial_write() {
    require_mongo!(async {
        let test_db = TestDb::new("acctl_notx").await.unwrap();
        let db = test_db.db();
        assert_domain_indexes(db).await;

        db.audit_events()
            .create(
                &sample_audit_event("ae-0", "rbac.assign_role"),
                &mut NoTransaction,
            )
            .await
            .unwrap();

        let binding = sample_binding("ur-1", "user-1", "role-sales");
        let duplicated_event = sample_audit_event("ae-0", "rbac.assign_role");
        let error = db
            .access_control()
            .assign_user_role_with_audit(&binding, &duplicated_event, &mut NoTransaction)
            .await
            .expect_err("第二笔写入冲突必须返回错误");
        assert!(
            matches!(error, database::Error::DuplicateKey(_)),
            "期望 DuplicateKey，实际为 {error:?}"
        );

        let binding_found = db
            .user_roles()
            .find_by_id(&binding.base.id, &mut NoTransaction)
            .await
            .unwrap();
        assert!(
            binding_found.is_some(),
            "NoTransaction 下第一笔已自动提交，留下半成品（方法注释已声明该行为）"
        );
    })
}
