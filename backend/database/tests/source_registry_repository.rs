//! 域 D01 `source_registry` 仓储集成测试（真实 MongoDB 单节点副本集）。
//!
//! 统一 `#[ignore]` + `ERP_TEST_MONGO_URI` 门控（test-support `require_mongo!`）：
//! 无数据库环境 `cargo test` 跳过；CI 与验收执行
//! `cargo test -p database --test source_registry_repository -- --include-ignored`。
//! 每个测试使用独立随机数据库名，结束自动 drop（TestDb）。

use database::repository::extensions::SourceRegistryExt;
use database::{ensure_indexes, NoTransaction, Transactional};
use entities::source_registry::{
    ExternalIdentityMap, ExternalIdentityMapData, ExternalIdentityTarget, ExternalIdentityTargetData,
    ExternalObjectType, MappingStatus, RelationRole, SourceSystem, SourceSystemData, SourceSystemId,
    SourceSystemStatus, SourceSystemType, TargetStatus,
};
use mongodb::Database;
use test_support::{assert_indexes, require_mongo, TestDb};

/// 来源系统列表筛选条件类型（经 `SourceRegistryExt` 关联类型跨 crate 可达）。
type SourceSystemFilter = <Database as SourceRegistryExt>::SourceSystemFilter;
/// 外部身份映射列表筛选条件类型。
type ExternalIdentityMapFilter = <Database as SourceRegistryExt>::ExternalIdentityMapFilter;

/// 构造可复用的来源系统实体。
fn sample_source_system(code: &str, created_by: &str) -> SourceSystem {
    SourceSystem::new(
        SourceSystemId::new(format!("sys-{code}")),
        SourceSystemData {
            code: code.to_string(),
            system_type: SourceSystemType::Mall,
            name: format!("{code} 商城"),
            status: SourceSystemStatus::Active,
        },
        created_by,
    )
    .unwrap()
}

/// 构造可复用的映射与目标实体。
fn sample_link(
    source_system_id: &SourceSystemId,
    external_id: &str,
) -> (ExternalIdentityMap, ExternalIdentityTarget) {
    let map = ExternalIdentityMap::new(
        entities::source_registry::ExternalIdentityMapId::new(format!("map-{external_id}")),
        ExternalIdentityMapData {
            source_system_id: source_system_id.clone(),
            object_type: ExternalObjectType::SalesOrder,
            external_id: external_id.to_string(),
            mapping_status: MappingStatus::Pending,
            mapped_at: None,
            mapped_by: None,
        },
    )
    .unwrap();
    let target = ExternalIdentityTarget::new(
        entities::source_registry::ExternalIdentityTargetId::new(format!("target-{external_id}")),
        ExternalIdentityTargetData {
            external_identity_map_id: map.base.id.clone().into(),
            internal_object_type: ExternalObjectType::SalesOrder,
            internal_object_id: format!("so-{external_id}"),
            relation_role: RelationRole::Primary,
            valid_from: 1_700_000_000,
            valid_to: Some(1_700_086_400),
            status: TargetStatus::Pending,
            approved_at: None,
            approved_by: None,
        },
    )
    .unwrap();
    (map, target)
}

/// 执行 `ensure_indexes` 并断言本域全部必需索引就位。
async fn assert_domain_indexes(db: &Database) {
    ensure_indexes(db).await.expect("ensure_indexes 应成功");
    assert_indexes(
        db,
        <Database as SourceRegistryExt>::SOURCE_SYSTEMS,
        &["uk_source_systems_code", "idx_source_systems_type_status"],
    )
    .await
    .expect("source_systems 索引缺失");
    assert_indexes(
        db,
        <Database as SourceRegistryExt>::EXTERNAL_IDENTITY_MAPS,
        &[
            "uk_external_identity_maps_identity",
            "idx_external_identity_maps_status",
        ],
    )
    .await
    .expect("external_identity_maps 索引缺失");
    assert_indexes(
        db,
        <Database as SourceRegistryExt>::EXTERNAL_IDENTITY_TARGETS,
        &[
            "uk_external_identity_targets_link",
            "idx_external_identity_targets_lineage",
            "idx_external_identity_targets_pending_conflict",
        ],
    )
    .await
    .expect("external_identity_targets 索引缺失");
}

#[tokio::test]
#[ignore]
async fn create_update_soft_delete_restore_roundtrip() {
    require_mongo!(async {
        let test_db = TestDb::new("src_reg_crud").await.unwrap();
        let db = test_db.db();
        assert_domain_indexes(db).await;

        let mut system = sample_source_system("ERP", "admin-1");
        db.source_systems()
            .create(&system, &mut NoTransaction)
            .await
            .unwrap();
        assert_eq!(system.base.version, 1);

        let found = db
            .source_systems()
            .find_by_id(&system.base.id, &mut NoTransaction)
            .await
            .unwrap()
            .expect("创建后应可读回");
        assert_eq!(found.code, "ERP");
        assert_eq!(found.stable.created_by, "admin-1");

        system
            .update(
                entities::source_registry::SourceSystemUpdate {
                    name: Some("新名称".to_string()),
                    status: Some(SourceSystemStatus::Disabled),
                },
                "admin-2",
            )
            .unwrap();
        db.source_systems()
            .update(&mut system, &mut NoTransaction)
            .await
            .unwrap();
        assert_eq!(system.base.version, 2, "乐观锁成功后 version 递增");

        db.source_systems()
            .soft_delete(&mut system, &mut NoTransaction)
            .await
            .unwrap();
        let after_delete = db
            .source_systems()
            .find_by_id(&system.base.id, &mut NoTransaction)
            .await
            .unwrap();
        assert!(after_delete.is_none(), "软删除后按 ID 不可见");

        db.source_systems()
            .restore(&mut system, &mut NoTransaction)
            .await
            .unwrap();
        let after_restore = db
            .source_systems()
            .find_by_id(&system.base.id, &mut NoTransaction)
            .await
            .unwrap();
        assert!(after_restore.is_some(), "恢复后按 ID 重新可见");
    })
}

#[tokio::test]
#[ignore]
async fn unique_code_conflict_surfaces_duplicate_key() {
    require_mongo!(async {
        let test_db = TestDb::new("src_reg_dup").await.unwrap();
        let db = test_db.db();
        assert_domain_indexes(db).await;

        let system = sample_source_system("ERP", "admin-1");
        db.source_systems()
            .create(&system, &mut NoTransaction)
            .await
            .unwrap();

        let duplicate = sample_source_system("ERP", "admin-2");
        let error = db
            .source_systems()
            .create(&duplicate, &mut NoTransaction)
            .await
            .expect_err("重复 code 必须被唯一索引拒绝");
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
        let test_db = TestDb::new("src_reg_optlock").await.unwrap();
        let db = test_db.db();

        let mut system = sample_source_system("ERP", "admin-1");
        db.source_systems()
            .create(&system, &mut NoTransaction)
            .await
            .unwrap();

        let mut stale = system.clone();
        system
            .update(
                entities::source_registry::SourceSystemUpdate {
                    name: Some("第一笔更新".to_string()),
                    status: None,
                },
                "admin-2",
            )
            .unwrap();
        db.source_systems()
            .update(&mut system, &mut NoTransaction)
            .await
            .unwrap();

        stale
            .update(
                entities::source_registry::SourceSystemUpdate {
                    name: Some("陈旧版本更新".to_string()),
                    status: None,
                },
                "admin-3",
            )
            .unwrap();
        let error = db
            .source_systems()
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
async fn external_id_key_distinguishes_case_and_trims_outer_whitespace() {
    require_mongo!(async {
        let test_db = TestDb::new("src_reg_idkey").await.unwrap();
        let db = test_db.db();
        assert_domain_indexes(db).await;

        let source = sample_source_system("MALL", "admin-1");
        db.source_systems()
            .create(&source, &mut NoTransaction)
            .await
            .unwrap();

        let (map_upper, target_upper) = sample_link(&source.base.id.clone().into(), "SO-ABC");
        db.source_registry()
            .create_external_identity_link(&map_upper, &target_upper, &mut NoTransaction)
            .await
            .unwrap();

        let (map_lower, target_lower) = sample_link(&source.base.id.clone().into(), "so-abc");
        db.source_registry()
            .create_external_identity_link(&map_lower, &target_lower, &mut NoTransaction)
            .await
            .unwrap();

        let key_lower = ExternalIdentityMap::external_id_key("so-abc");
        let found = db
            .external_identity_maps()
            .find_by_identity(
                &source.base.id.clone().into(),
                ExternalObjectType::SalesOrder,
                &key_lower,
                &mut NoTransaction,
            )
            .await
            .unwrap()
            .expect("小写 key 应精确命中小写映射");
        assert_eq!(found.external_id, "so-abc");

        let padded_key = ExternalIdentityMap::external_id_key("  so-abc  ");
        assert_eq!(padded_key, key_lower, "首尾空白不参与比较键，必须命中同一映射");
        let padded_hit = db
            .external_identity_maps()
            .find_by_identity(
                &source.base.id.clone().into(),
                ExternalObjectType::SalesOrder,
                &padded_key,
                &mut NoTransaction,
            )
            .await
            .unwrap();
        assert!(padded_hit.is_some(), "去除首尾空白后的 key 应命中");

        let duplicate = sample_link(&source.base.id.clone().into(), " so-abc ");
        let error = db
            .source_registry()
            .create_external_identity_link(&duplicate.0, &duplicate.1, &mut NoTransaction)
            .await
            .expect_err("同一 (来源, 类型, 比较键) 重复写入必须被唯一索引拒绝");
        assert!(
            matches!(error, database::Error::DuplicateKey(_)),
            "期望 DuplicateKey，实际为 {error:?}"
        );
    })
}

#[tokio::test]
#[ignore]
async fn projection_list_search_respects_filters_and_pagination() {
    require_mongo!(async {
        let test_db = TestDb::new("src_reg_list").await.unwrap();
        let db = test_db.db();
        assert_domain_indexes(db).await;

        let mut mall_disabled = sample_source_system("MALL-B", "admin-1");
        mall_disabled.stable.status = SourceSystemStatus::Disabled;
        let mut supplier = sample_source_system("SUP-1", "admin-1");
        supplier.system_type = SourceSystemType::Supplier;
        db.source_systems()
            .create(&sample_source_system("MALL-A", "admin-1"), &mut NoTransaction)
            .await
            .unwrap();
        db.source_systems()
            .create(&mall_disabled, &mut NoTransaction)
            .await
            .unwrap();
        db.source_systems()
            .create(&supplier, &mut NoTransaction)
            .await
            .unwrap();

        let filter = SourceSystemFilter {
            code: None,
            system_type: Some(SourceSystemType::Mall),
            status: Some(SourceSystemStatus::Active),
            page: 1,
            page_size: 1,
            sort_by: Some("created_at".to_string()),
            sort_ascending: true,
        };
        let page = db
            .source_systems()
            .search_source_systems(&filter, &mut NoTransaction)
            .await
            .unwrap();
        assert_eq!(page.total, 1, "MALL 且启用只有一条");
        assert_eq!(page.items.len(), 1);
        let row = &page.items[0];
        assert_eq!(row.code, "MALL-A");
        assert_eq!(row.name, "MALL-A 商城");
        assert_eq!(row.system_type, SourceSystemType::Mall);
        assert_eq!(row.status, SourceSystemStatus::Active);
        assert!(row.version >= 1);
        assert!(row.created_at > 0);
    })
}

#[tokio::test]
#[ignore]
async fn map_projection_list_filters_by_source_system_and_status() {
    require_mongo!(async {
        let test_db = TestDb::new("src_reg_map_list").await.unwrap();
        let db = test_db.db();
        assert_domain_indexes(db).await;

        let source = sample_source_system("MALL", "admin-1");
        db.source_systems()
            .create(&source, &mut NoTransaction)
            .await
            .unwrap();
        let (map, target) = sample_link(&source.base.id.clone().into(), "SO-1");
        db.source_registry()
            .create_external_identity_link(&map, &target, &mut NoTransaction)
            .await
            .unwrap();

        let filter = ExternalIdentityMapFilter {
            source_system_id: Some(source.base.id.clone().into()),
            mapping_status: Some(MappingStatus::Pending),
            page: 1,
            page_size: 20,
            sort_by: Some("created_at".to_string()),
            sort_ascending: false,
        };
        let page = db
            .external_identity_maps()
            .search_external_identity_maps(&filter, &mut NoTransaction)
            .await
            .unwrap();
        assert_eq!(page.total, 1);
        assert_eq!(page.items.len(), 1);
        let row = &page.items[0];
        assert_eq!(row.external_id, "SO-1");
        assert_eq!(row.object_type, ExternalObjectType::SalesOrder);
        assert_eq!(row.mapping_status, MappingStatus::Pending);
        assert_eq!(row.source_system_id, source.base.id);

        let no_match = ExternalIdentityMapFilter {
            source_system_id: Some(SourceSystemId::new("sys-不存在")),
            mapping_status: None,
            page: 1,
            page_size: 20,
            sort_by: None,
            sort_ascending: false,
        };
        let empty = db
            .external_identity_maps()
            .search_external_identity_maps(&no_match, &mut NoTransaction)
            .await
            .unwrap();
        assert_eq!(empty.total, 0, "不存在的来源系统筛选应返回空列表");
    })
}

#[tokio::test]
#[ignore]
async fn multi_step_link_commits_atomically_inside_transaction() {
    require_mongo!(async {
        let test_db = TestDb::new("src_reg_tx_commit").await.unwrap();
        let db = test_db.db();
        assert_domain_indexes(db).await;

        let source = sample_source_system("MALL", "admin-1");
        db.source_systems()
            .create(&source, &mut NoTransaction)
            .await
            .unwrap();
        let (map, target) = sample_link(&source.base.id.clone().into(), "SO-1");

        let db_clone = db.clone();
        let source_id: SourceSystemId = source.base.id.clone().into();
        let map_for_tx = map.clone();
        let target_for_tx = target.clone();
        test_db
            .client()
            .with_transaction::<_, (), database::Error>(move |session| {
                Box::pin(async move {
                    db_clone
                        .source_registry()
                        .create_external_identity_link(&map_for_tx, &target_for_tx, session)
                        .await?;
                    Ok(())
                })
            })
            .await
            .expect("事务提交应成功");

        let map_found = db
            .external_identity_maps()
            .find_by_identity(
                &source_id,
                ExternalObjectType::SalesOrder,
                &ExternalIdentityMap::external_id_key("SO-1"),
                &mut NoTransaction,
            )
            .await
            .unwrap();
        assert!(map_found.is_some(), "事务提交后映射必须可见");
        let target_found = db
            .external_identity_targets()
            .find_by_id(&target.base.id, &mut NoTransaction)
            .await
            .unwrap();
        assert!(target_found.is_some(), "事务提交后目标必须可见");
    })
}

#[tokio::test]
#[ignore]
async fn transaction_abort_rolls_back_both_collections() {
    require_mongo!(async {
        let test_db = TestDb::new("src_reg_tx_abort").await.unwrap();
        let db = test_db.db();
        assert_domain_indexes(db).await;

        let source = sample_source_system("MALL", "admin-1");
        db.source_systems()
            .create(&source, &mut NoTransaction)
            .await
            .unwrap();
        let (map, target) = sample_link(&source.base.id.clone().into(), "SO-2");

        let db_clone = db.clone();
        let map_for_tx = map.clone();
        let target_for_tx = target.clone();
        let result: std::result::Result<(), database::Error> = test_db
            .client()
            .with_transaction(move |session| {
                Box::pin(async move {
                    db_clone
                        .source_registry()
                        .create_external_identity_link(&map_for_tx, &target_for_tx, session)
                        .await?;
                    Err(database::Error::OptimisticLockingError)
                })
            })
            .await;
        assert!(result.is_err(), "闭包返回错误必须整体回滚");

        let map_found = db
            .external_identity_maps()
            .find_by_id(&map.base.id, &mut NoTransaction)
            .await
            .unwrap();
        assert!(map_found.is_none(), "回滚后映射不得残留");
        let target_found = db
            .external_identity_targets()
            .find_by_id(&target.base.id, &mut NoTransaction)
            .await
            .unwrap();
        assert!(target_found.is_none(), "回滚后目标不得残留");
    })
}
