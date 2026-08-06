//! 域 D25 `supplier_api` 仓储集成测试（真实 MongoDB 单节点副本集）。
//!
//! 统一 `#[ignore]` + `ERP_TEST_MONGO_URI` 门控（test-support `require_mongo!`）：
//! 无数据库环境 `cargo test` 跳过；CI 与验收执行
//! `cargo test -p database --test supplier_api_repository -- --include-ignored`。
//! 每个测试使用独立随机数据库名，结束自动 drop（TestDb）。

use database::repository::extensions::SupplierApiExt;
use database::{ensure_indexes, NoTransaction, Transactional};
use entities::ids::SupplierAccountId;
use entities::supplier_api::{
    ConnectionEnvironment, HealthCheckResult, SupplierApiCapability, SupplierApiCapabilityCode,
    SupplierApiCapabilityData, SupplierApiCapabilityId, SupplierApiCapabilityStatus, SupplierApiConnection,
    SupplierApiConnectionData, SupplierApiConnectionId, SupplierApiConnectionStatus,
    SupplierApiConnectionUpdate,
};
use mongodb::Database;
use test_support::{assert_indexes, require_mongo, TestDb};

/// 连接列表筛选条件类型（经 `SupplierApiExt` 关联类型跨 crate 可达）。
type SupplierApiConnectionFilter = <Database as SupplierApiExt>::SupplierApiConnectionFilter;
/// 连接能力列表筛选条件类型。
type SupplierApiCapabilityFilter = <Database as SupplierApiExt>::SupplierApiCapabilityFilter;

/// 构造可复用的连接实体。
fn sample_connection(code: &str, created_by: &str) -> SupplierApiConnection {
    SupplierApiConnection::new(
        SupplierApiConnectionId::new(format!("conn-{code}")),
        SupplierApiConnectionData {
            supplier_id: SupplierAccountId::new("sup-1"),
            connection_code: code.to_string(),
            environment: ConnectionEnvironment::Production,
            endpoint_reference: format!("config://supplier/{code}"),
            credential_reference: Some(format!("kms://prod/{code}")),
            rate_limit_policy: None,
            status: SupplierApiConnectionStatus::Active,
        },
        created_by,
    )
    .unwrap()
}

/// 构造可复用的连接能力声明实体。
fn sample_capability(
    connection_id: &SupplierApiConnectionId,
    capability_code: SupplierApiCapabilityCode,
    suffix: &str,
) -> SupplierApiCapability {
    SupplierApiCapability::new(
        SupplierApiCapabilityId::new(format!("cap-{connection_id}-{suffix}")),
        SupplierApiCapabilityData {
            connection_id: connection_id.clone(),
            capability_code,
            status: SupplierApiCapabilityStatus::Active,
            constraint_snapshot: None,
        },
    )
    .unwrap()
}

/// 执行 `ensure_indexes` 并断言本域全部必需索引就位。
async fn assert_domain_indexes(db: &Database) {
    ensure_indexes(db).await.expect("ensure_indexes 应成功");
    assert_indexes(
        db,
        <Database as SupplierApiExt>::SUPPLIER_API_CONNECTIONS,
        &[
            "uk_supplier_api_connections_connection_code",
            "idx_supplier_api_connections_supplier_status",
        ],
    )
    .await
    .expect("supplier_api_connections 索引缺失");
    assert_indexes(
        db,
        <Database as SupplierApiExt>::SUPPLIER_API_CAPABILITIES,
        &["uk_supplier_api_capabilities_connection_capability"],
    )
    .await
    .expect("supplier_api_capabilities 索引缺失");
}

#[tokio::test]
#[ignore]
async fn create_update_soft_delete_restore_roundtrip() {
    require_mongo!(async {
        let test_db = TestDb::new("sup_api_crud").await.unwrap();
        let db = test_db.db();
        assert_domain_indexes(db).await;

        let mut connection = sample_connection("CN-001", "admin-1");
        db.supplier_api_connections()
            .create(&connection, &mut NoTransaction)
            .await
            .unwrap();
        assert_eq!(connection.base.version, 1);

        let found = db
            .supplier_api_connections()
            .find_by_id(&connection.base.id, &mut NoTransaction)
            .await
            .unwrap()
            .expect("创建后应可读回");
        assert_eq!(found.connection_code, "CN-001");
        assert_eq!(found.stable.created_by, "admin-1");

        connection
            .update(
                SupplierApiConnectionUpdate {
                    environment: Some(ConnectionEnvironment::Testing),
                    status: Some(SupplierApiConnectionStatus::Disabled),
                    ..Default::default()
                },
                "admin-2",
            )
            .unwrap();
        db.supplier_api_connections()
            .update(&mut connection, &mut NoTransaction)
            .await
            .unwrap();
        assert_eq!(connection.base.version, 2, "乐观锁成功后 version 递增");
        assert_eq!(connection.environment, ConnectionEnvironment::Testing);

        db.supplier_api_connections()
            .soft_delete(&mut connection, &mut NoTransaction)
            .await
            .unwrap();
        let after_delete = db
            .supplier_api_connections()
            .find_by_id(&connection.base.id, &mut NoTransaction)
            .await
            .unwrap();
        assert!(after_delete.is_none(), "软删除后按 ID 不可见");

        db.supplier_api_connections()
            .restore(&mut connection, &mut NoTransaction)
            .await
            .unwrap();
        let after_restore = db
            .supplier_api_connections()
            .find_by_id(&connection.base.id, &mut NoTransaction)
            .await
            .unwrap();
        assert!(after_restore.is_some(), "恢复后按 ID 重新可见");
    })
}

#[tokio::test]
#[ignore]
async fn unique_connection_code_conflict_surfaces_duplicate_key() {
    require_mongo!(async {
        let test_db = TestDb::new("sup_api_dup_code").await.unwrap();
        let db = test_db.db();
        assert_domain_indexes(db).await;

        let connection = sample_connection("CN-001", "admin-1");
        db.supplier_api_connections()
            .create(&connection, &mut NoTransaction)
            .await
            .unwrap();

        let duplicate = sample_connection("CN-001", "admin-2");
        let error = db
            .supplier_api_connections()
            .create(&duplicate, &mut NoTransaction)
            .await
            .expect_err("重复 connection_code 必须被唯一索引拒绝");
        assert!(
            matches!(error, database::Error::DuplicateKey(_)),
            "期望 DuplicateKey，实际为 {error:?}"
        );
    })
}

#[tokio::test]
#[ignore]
async fn stale_version_update_returns_optimistic_locking_error() {
    require_mongo!(async {
        let test_db = TestDb::new("sup_api_optlock").await.unwrap();
        let db = test_db.db();

        let mut connection = sample_connection("CN-001", "admin-1");
        db.supplier_api_connections()
            .create(&connection, &mut NoTransaction)
            .await
            .unwrap();

        let mut stale = connection.clone();
        connection
            .update(
                SupplierApiConnectionUpdate {
                    status: Some(SupplierApiConnectionStatus::Disabled),
                    ..Default::default()
                },
                "admin-2",
            )
            .unwrap();
        db.supplier_api_connections()
            .update(&mut connection, &mut NoTransaction)
            .await
            .unwrap();

        stale
            .update(
                SupplierApiConnectionUpdate {
                    status: Some(SupplierApiConnectionStatus::Fault),
                    ..Default::default()
                },
                "admin-3",
            )
            .unwrap();
        let error = db
            .supplier_api_connections()
            .update(&mut stale, &mut NoTransaction)
            .await
            .expect_err("陈旧 version 更新必须被 CAS 拒绝");
        assert!(
            matches!(error, database::Error::OptimisticLockingError),
            "期望 OptimisticLockingError，实际为 {error:?}"
        );
        assert_eq!(stale.base.version, 1, "CAS 失败不得改动内存版本");
    })
}

#[tokio::test]
#[ignore]
async fn capability_duplicate_key_conflict_surfaces_duplicate_key() {
    require_mongo!(async {
        let test_db = TestDb::new("sup_api_dup_cap").await.unwrap();
        let db = test_db.db();
        assert_domain_indexes(db).await;

        let connection = sample_connection("CN-001", "admin-1");
        db.supplier_api_connections()
            .create(&connection, &mut NoTransaction)
            .await
            .unwrap();
        let connection_id = connection.base.id.clone().into();

        let capability = sample_capability(&connection_id, SupplierApiCapabilityCode::Order, "a");
        db.supplier_api_capabilities()
            .create(&capability, &mut NoTransaction)
            .await
            .unwrap();

        let duplicate = sample_capability(&connection_id, SupplierApiCapabilityCode::Order, "b");
        let error = db
            .supplier_api_capabilities()
            .create(&duplicate, &mut NoTransaction)
            .await
            .expect_err("同一 (connection_id, capability_code) 重复写入必须被唯一索引拒绝");
        assert!(
            matches!(error, database::Error::DuplicateKey(_)),
            "期望 DuplicateKey，实际为 {error:?}"
        );
    })
}

#[tokio::test]
#[ignore]
async fn connection_list_search_respects_filters_pagination_and_projection() {
    require_mongo!(async {
        let test_db = TestDb::new("sup_api_list").await.unwrap();
        let db = test_db.db();
        assert_domain_indexes(db).await;

        let mut testing = sample_connection("CN-002", "admin-1");
        testing.environment = ConnectionEnvironment::Testing;
        db.supplier_api_connections()
            .create(&sample_connection("CN-001", "admin-1"), &mut NoTransaction)
            .await
            .unwrap();
        db.supplier_api_connections()
            .create(&testing, &mut NoTransaction)
            .await
            .unwrap();
        db.supplier_api_connections()
            .create(&sample_connection("CN-010", "admin-1"), &mut NoTransaction)
            .await
            .unwrap();

        let filter = SupplierApiConnectionFilter {
            supplier_id: Some("sup-1".to_string()),
            connection_code: Some("cn-0".to_string()),
            environment: None,
            status: Some(SupplierApiConnectionStatus::Active),
            page: 2,
            page_size: 2,
            sort_by: Some("connection_code".to_string()),
            sort_ascending: true,
        };
        let page = db
            .supplier_api_connections()
            .search_supplier_api_connections(&filter, &mut NoTransaction)
            .await
            .unwrap();
        assert_eq!(page.total, 3, "supplier_id + 代码子串 + 状态命中三条");
        assert_eq!(page.items.len(), 1, "第二页只剩一条");
        let row = &page.items[0];
        assert_eq!(row.connection_code, "CN-010", "按 connection_code 升序取末条");
        assert_eq!(row.supplier_id, "sup-1");
        assert_eq!(row.environment, ConnectionEnvironment::Production);
        assert_eq!(row.status, SupplierApiConnectionStatus::Active);
        assert!(row.version >= 1);
        assert!(row.created_at > 0);

        let row_doc = mongodb::bson::to_document(row).unwrap();
        assert!(
            !row_doc.contains_key("credential_reference"),
            "密钥引用不得进入列表投影"
        );
        assert!(!row_doc.contains_key("rate_limit_policy"));
    })
}

#[tokio::test]
#[ignore]
async fn capability_list_search_filters_by_connection_and_status() {
    require_mongo!(async {
        let test_db = TestDb::new("sup_api_cap_list").await.unwrap();
        let db = test_db.db();
        assert_domain_indexes(db).await;

        let connection = sample_connection("CN-001", "admin-1");
        db.supplier_api_connections()
            .create(&connection, &mut NoTransaction)
            .await
            .unwrap();
        let connection_id = connection.base.id.clone().into();
        let mut disabled = sample_capability(&connection_id, SupplierApiCapabilityCode::Stock, "b");
        disabled.status = SupplierApiCapabilityStatus::Disabled;
        db.supplier_api_capabilities()
            .create(
                &sample_capability(&connection_id, SupplierApiCapabilityCode::Order, "a"),
                &mut NoTransaction,
            )
            .await
            .unwrap();
        db.supplier_api_capabilities()
            .create(&disabled, &mut NoTransaction)
            .await
            .unwrap();

        let filter = SupplierApiCapabilityFilter {
            connection_id: Some(connection_id.clone()),
            capability_code: None,
            status: Some(SupplierApiCapabilityStatus::Active),
            page: 1,
            page_size: 20,
            sort_by: Some("created_at".to_string()),
            sort_ascending: false,
        };
        let page = db
            .supplier_api_capabilities()
            .search_supplier_api_capabilities(&filter, &mut NoTransaction)
            .await
            .unwrap();
        assert_eq!(page.total, 1, "该连接启用中的能力只有一条");
        assert_eq!(page.items[0].capability_code, SupplierApiCapabilityCode::Order);
        assert_eq!(page.items[0].status, SupplierApiCapabilityStatus::Active);
        assert!(!mongodb::bson::to_document(&page.items[0])
            .unwrap()
            .contains_key("constraint_snapshot"));
    })
}

#[tokio::test]
#[ignore]
async fn batch_capability_fetch_avoids_n_plus_one() {
    require_mongo!(async {
        let test_db = TestDb::new("sup_api_batch").await.unwrap();
        let db = test_db.db();
        assert_domain_indexes(db).await;

        let connection_a = sample_connection("CN-A", "admin-1");
        let connection_b = sample_connection("CN-B", "admin-1");
        db.supplier_api_connections()
            .create(&connection_a, &mut NoTransaction)
            .await
            .unwrap();
        db.supplier_api_connections()
            .create(&connection_b, &mut NoTransaction)
            .await
            .unwrap();
        let id_a = connection_a.base.id.clone().into();
        let id_b = connection_b.base.id.clone().into();
        db.supplier_api_capabilities()
            .create(
                &sample_capability(&id_a, SupplierApiCapabilityCode::Order, "a1"),
                &mut NoTransaction,
            )
            .await
            .unwrap();
        db.supplier_api_capabilities()
            .create(
                &sample_capability(&id_a, SupplierApiCapabilityCode::Stock, "a2"),
                &mut NoTransaction,
            )
            .await
            .unwrap();
        db.supplier_api_capabilities()
            .create(
                &sample_capability(&id_b, SupplierApiCapabilityCode::Query, "b1"),
                &mut NoTransaction,
            )
            .await
            .unwrap();

        let found = db
            .supplier_api_capabilities()
            .find_capabilities_by_connections(&[id_a.clone(), id_b.clone()], &mut NoTransaction)
            .await
            .unwrap();
        assert_eq!(found.len(), 3, "$in 一次取回两个连接的全部能力");

        let single = db
            .supplier_api_capabilities()
            .find_capabilities_by_connection(&id_a, &mut NoTransaction)
            .await
            .unwrap();
        assert_eq!(single.len(), 2);
        assert_eq!(single[0].capability_code, SupplierApiCapabilityCode::Order);

        let empty = db
            .supplier_api_capabilities()
            .find_capabilities_by_connections(&[], &mut NoTransaction)
            .await
            .unwrap();
        assert!(empty.is_empty(), "空 ID 列表直接返回空集合");
    })
}

#[tokio::test]
#[ignore]
async fn multi_step_connection_with_capabilities_commits_atomically() {
    require_mongo!(async {
        let test_db = TestDb::new("sup_api_tx_commit").await.unwrap();
        let db = test_db.db();
        assert_domain_indexes(db).await;

        let connection = sample_connection("CN-001", "admin-1");
        let connection_id = connection.base.id.clone().into();
        let capabilities = vec![
            sample_capability(&connection_id, SupplierApiCapabilityCode::Order, "a"),
            sample_capability(&connection_id, SupplierApiCapabilityCode::Stock, "b"),
        ];

        let db_clone = db.clone();
        let connection_for_tx = connection.clone();
        let capabilities_for_tx = capabilities.clone();
        test_db
            .client()
            .with_transaction::<_, (), database::Error>(move |session| {
                Box::pin(async move {
                    db_clone
                        .supplier_api()
                        .create_connection_with_capabilities(
                            &connection_for_tx,
                            &capabilities_for_tx,
                            session,
                        )
                        .await?;
                    Ok(())
                })
            })
            .await
            .expect("事务提交应成功");

        let found = db
            .supplier_api_connections()
            .find_by_connection_code("CN-001", &mut NoTransaction)
            .await
            .unwrap()
            .expect("事务提交后连接必须可见");
        assert_eq!(found.base.id, connection.base.id);
        let capabilities_found = db
            .supplier_api_capabilities()
            .find_capabilities_by_connection(&connection_id, &mut NoTransaction)
            .await
            .unwrap();
        assert_eq!(capabilities_found.len(), 2, "事务提交后能力清单必须可见");
    })
}

#[tokio::test]
#[ignore]
async fn transaction_abort_rolls_back_connection_and_capabilities() {
    require_mongo!(async {
        let test_db = TestDb::new("sup_api_tx_abort").await.unwrap();
        let db = test_db.db();
        assert_domain_indexes(db).await;

        let connection = sample_connection("CN-001", "admin-1");
        let connection_id = connection.base.id.clone().into();
        let capabilities = vec![sample_capability(
            &connection_id,
            SupplierApiCapabilityCode::Order,
            "a",
        )];

        let db_clone = db.clone();
        let connection_for_tx = connection.clone();
        let capabilities_for_tx = capabilities.clone();
        let result: std::result::Result<(), database::Error> = test_db
            .client()
            .with_transaction(move |session| {
                Box::pin(async move {
                    db_clone
                        .supplier_api()
                        .create_connection_with_capabilities(
                            &connection_for_tx,
                            &capabilities_for_tx,
                            session,
                        )
                        .await?;
                    Err(database::Error::OptimisticLockingError)
                })
            })
            .await;
        assert!(result.is_err(), "闭包返回错误必须整体回滚");

        let found = db
            .supplier_api_connections()
            .find_by_connection_code("CN-001", &mut NoTransaction)
            .await
            .unwrap();
        assert!(found.is_none(), "回滚后连接不得残留");
        let capabilities_found = db
            .supplier_api_capabilities()
            .find_capabilities_by_connection(&connection_id, &mut NoTransaction)
            .await
            .unwrap();
        assert!(capabilities_found.is_empty(), "回滚后能力不得残留");
    })
}

#[tokio::test]
#[ignore]
async fn no_transaction_partial_write_leaves_half_committed_state() {
    require_mongo!(async {
        let test_db = TestDb::new("sup_api_no_tx").await.unwrap();
        let db = test_db.db();
        assert_domain_indexes(db).await;

        let first = sample_connection("CN-001", "admin-1");
        let first_id = first.base.id.clone().into();
        db.supplier_api()
            .create_connection_with_capabilities(
                &first,
                &[sample_capability(
                    &first_id,
                    SupplierApiCapabilityCode::Order,
                    "a",
                )],
                &mut NoTransaction,
            )
            .await
            .unwrap();

        let second = sample_connection("CN-002", "admin-1");
        let second_id = second.base.id.clone().into();
        let conflicting = vec![
            sample_capability(&second_id, SupplierApiCapabilityCode::Stock, "b"),
            sample_capability(&second_id, SupplierApiCapabilityCode::Stock, "b"),
        ];
        let error = db
            .supplier_api()
            .create_connection_with_capabilities(&second, &conflicting, &mut NoTransaction)
            .await
            .expect_err("同批次内重复能力必须被唯一索引拒绝");
        assert!(matches!(error, database::Error::DuplicateKey(_)));

        let second_found = db
            .supplier_api_connections()
            .find_by_connection_code("CN-002", &mut NoTransaction)
            .await
            .unwrap();
        assert!(
            second_found.is_some(),
            "NoTransaction 下连接写入已自动提交，部分状态可预期"
        );
        let partial = db
            .supplier_api_capabilities()
            .find_capabilities_by_connection(&second_id, &mut NoTransaction)
            .await
            .unwrap();
        assert_eq!(partial.len(), 1, "能力批次中断，只留下已提交的第一条");
    })
}

#[tokio::test]
#[ignore]
async fn replace_connection_capabilities_rolls_back_whole_on_conflict() {
    require_mongo!(async {
        let test_db = TestDb::new("sup_api_replace").await.unwrap();
        let db = test_db.db();
        assert_domain_indexes(db).await;

        let connection = sample_connection("CN-001", "admin-1");
        let connection_id = connection.base.id.clone().into();
        db.supplier_api()
            .create_connection_with_capabilities(
                &connection,
                &[sample_capability(
                    &connection_id,
                    SupplierApiCapabilityCode::Product,
                    "old",
                )],
                &mut NoTransaction,
            )
            .await
            .unwrap();

        let db_clone = db.clone();
        let id_for_tx = connection_id.clone();
        let conflicting = vec![
            sample_capability(&connection_id, SupplierApiCapabilityCode::Order, "n1"),
            sample_capability(&connection_id, SupplierApiCapabilityCode::Order, "n2"),
        ];
        let result: std::result::Result<(), database::Error> = test_db
            .client()
            .with_transaction(move |session| {
                Box::pin(async move {
                    db_clone
                        .supplier_api()
                        .replace_connection_capabilities(&id_for_tx, &conflicting, session)
                        .await?;
                    Ok(())
                })
            })
            .await;
        assert!(result.is_err(), "替换清单冲突必须让事务整体回滚");

        let remaining = db
            .supplier_api_capabilities()
            .find_capabilities_by_connection(&connection_id, &mut NoTransaction)
            .await
            .unwrap();
        assert_eq!(remaining.len(), 1, "回滚后旧能力清单原样保留");
        assert_eq!(
            remaining[0].capability_code,
            SupplierApiCapabilityCode::Product,
            "删除动作一并回滚，没有新旧混杂"
        );
    })
}

#[tokio::test]
#[ignore]
async fn health_check_update_roundtrip() {
    require_mongo!(async {
        let test_db = TestDb::new("sup_api_health").await.unwrap();
        let db = test_db.db();

        let mut connection = sample_connection("CN-001", "admin-1");
        db.supplier_api_connections()
            .create(&connection, &mut NoTransaction)
            .await
            .unwrap();

        connection
            .update(
                SupplierApiConnectionUpdate {
                    last_health_at: Some(entities::common::time::Instant::from_unix_secs(1_700_000_000)),
                    last_health_result: Some(HealthCheckResult::Healthy),
                    ..Default::default()
                },
                "admin-2",
            )
            .unwrap();
        db.supplier_api_connections()
            .update(&mut connection, &mut NoTransaction)
            .await
            .unwrap();

        let found = db
            .supplier_api_connections()
            .find_by_id(&connection.base.id, &mut NoTransaction)
            .await
            .unwrap()
            .expect("健康检查更新后应可读回");
        assert_eq!(found.last_health_result, Some(HealthCheckResult::Healthy));
        assert_eq!(
            found.last_health_at.unwrap().unix_secs(),
            1_700_000_000,
            "时间字段往返一致"
        );
    })
}
