//! 域 D28 `card_instance` 仓储集成测试（真实 MongoDB 单节点副本集）。
//!
//! 统一 `#[ignore]` + `ERP_TEST_MONGO_URI` 门控（test-support `require_mongo!`）：
//! 无数据库环境 `cargo test` 跳过；CI 与验收执行
//! `cargo test -p database --test card_instance_repository -- --include-ignored`。
//! 每个测试使用独立随机数据库名，结束自动 drop（TestDb）。

use database::repository::extensions::CardInstanceExt;
use database::{ensure_indexes, NoTransaction, Transactional};
use entities::card_instance::{
    CardSourceType, CorrectionType, CutoverStatus, MallBalanceSnapshot, MallBalanceSnapshotData,
    MallCardInstance, MallCardInstanceCorrection, MallCardInstanceCorrectionData, MallCardInstanceData,
    MallConsumptionCutover, MallConsumptionCutoverData,
};
use entities::common::time::Instant;
use entities::ids::{
    ExternalIdentityMapId, MallBalanceSnapshotId, MallCardInstanceCorrectionId, MallCardInstanceId,
    MallConsumptionCutoverId, SalesOrderId, SalesOrderRevisionId, WorkItemId,
};
use entities::money::Amount;
use mongodb::Database;
use std::str::FromStr;
use test_support::{assert_indexes, require_mongo, TestDb};

/// 切换记录列表筛选条件类型（经 `CardInstanceExt` 关联类型跨 crate 可达）。
type MallConsumptionCutoverFilter = <Database as CardInstanceExt>::MallConsumptionCutoverFilter;
/// 卡实例列表筛选条件类型。
type MallCardInstanceFilter = <Database as CardInstanceExt>::MallCardInstanceFilter;

/// 构造可复用的切换记录实体。
fn sample_cutover(mall_id: &str, id: &str) -> MallConsumptionCutover {
    MallConsumptionCutover::new(
        MallConsumptionCutoverId::new(id),
        MallConsumptionCutoverData {
            mall_id: format!(" {mall_id} "),
            checklist_reference: Some(" attachment-1 ".to_string()),
        },
    )
    .unwrap()
}

/// 构造可复用的卡实例基线实体。
fn sample_card_instance(mall_id: &str, card_id: &str) -> MallCardInstance {
    MallCardInstance::new(
        MallCardInstanceId::new(card_id),
        MallCardInstanceData {
            mall_id: format!(" {mall_id} "),
            opaque_instance_ref: format!(" ref-{card_id} "),
            origin_sales_order_source_identity_id: ExternalIdentityMapId::new("eim-1"),
            origin_sales_order_id: SalesOrderId::new("so-1"),
            origin_sales_order_revision_id: SalesOrderRevisionId::new("sor-1"),
            source_baseline_version: Some(" v1 ".to_string()),
            initial_balance: Amount::from_str("100.00").unwrap(),
            baseline_at: Instant::from_unix_secs(1_700_000_000),
            source_type: CardSourceType::Realtime,
        },
    )
    .unwrap()
}

/// 构造可复用的余额快照实体。
fn sample_snapshot(card_id: &str, snapshot_id: &str, at_secs: i64) -> MallBalanceSnapshot {
    MallBalanceSnapshot::new(
        MallBalanceSnapshotId::new(snapshot_id),
        MallBalanceSnapshotData {
            mall_card_instance_id: MallCardInstanceId::new(card_id),
            snapshot_at: Instant::from_unix_secs(at_secs),
            balance: Amount::from_str("88.00").unwrap(),
            source_snapshot_version: Some(format!(" v{snapshot_id} ")),
            source_event_id: format!(" evt-{snapshot_id} "),
        },
    )
    .unwrap()
}

/// 构造可复用的卡实例纠错实体。
fn sample_correction(card_id: &str, correction_id: &str, correction_no: u32) -> MallCardInstanceCorrection {
    MallCardInstanceCorrection::new(
        MallCardInstanceCorrectionId::new(correction_id),
        MallCardInstanceCorrectionData {
            mall_card_instance_id: MallCardInstanceId::new(card_id),
            correction_no,
            correction_type: CorrectionType::InitialBalance,
            before_value: " 100.00 ".to_string(),
            after_value: " 98.50 ".to_string(),
            work_item_id: WorkItemId::new("wi-1"),
            supersedes_correction_id: if correction_no == 1 {
                None
            } else {
                Some(MallCardInstanceCorrectionId::new("corr-1"))
            },
            reason: " 余额核对差异 ".to_string(),
            approved_by: " fin-1 ".to_string(),
            approved_at: Instant::from_unix_secs(1_700_000_100),
        },
    )
    .unwrap()
}

/// 执行 `ensure_indexes` 并断言本域全部必需索引就位。
async fn assert_domain_indexes(db: &Database) {
    ensure_indexes(db).await.expect("ensure_indexes 应成功");
    assert_indexes(
        db,
        <Database as CardInstanceExt>::MALL_CONSUMPTION_CUTOVERS,
        &["uk_mall_consumption_cutovers_mall"],
    )
    .await
    .expect("mall_consumption_cutovers 索引缺失");
    assert_indexes(
        db,
        <Database as CardInstanceExt>::MALL_CARD_INSTANCES,
        &[
            "uk_mall_card_instances_identity",
            "idx_mall_card_instances_baseline_version",
        ],
    )
    .await
    .expect("mall_card_instances 索引缺失");
    assert_indexes(
        db,
        <Database as CardInstanceExt>::MALL_CARD_INSTANCE_CORRECTIONS,
        &[
            "uk_mall_card_instance_corrections_no",
            "uk_mall_card_instance_corrections_supersedes",
        ],
    )
    .await
    .expect("mall_card_instance_corrections 索引缺失");
    assert_indexes(
        db,
        <Database as CardInstanceExt>::MALL_BALANCE_SNAPSHOTS,
        &[
            "uk_mall_balance_snapshots_business",
            "uk_mall_balance_snapshots_source_version",
        ],
    )
    .await
    .expect("mall_balance_snapshots 索引缺失");
}

#[tokio::test]
#[ignore]
async fn cutover_create_update_soft_delete_restore_roundtrip() {
    require_mongo!(async {
        let test_db = TestDb::new("card_cutover_crud").await.unwrap();
        let db = test_db.db();
        assert_domain_indexes(db).await;

        let mut cutover = sample_cutover("mall-a", "cutover-1");
        db.mall_consumption_cutovers()
            .create(&cutover, &mut NoTransaction)
            .await
            .unwrap();
        assert_eq!(cutover.base.version, 1);

        let found = db
            .mall_consumption_cutovers()
            .find_by_id(&cutover.base.id, &mut NoTransaction)
            .await
            .unwrap()
            .expect("创建后应可读回");
        assert_eq!(found.mall_id, "mall-a");
        assert_eq!(found.status, CutoverStatus::Preparing);

        cutover
            .enable(Instant::from_unix_secs(1_700_000_500), "owner-1")
            .unwrap();
        db.mall_consumption_cutovers()
            .update(&mut cutover, &mut NoTransaction)
            .await
            .unwrap();
        assert_eq!(cutover.base.version, 2, "乐观锁成功后 version 递增");

        let enabled = db
            .mall_consumption_cutovers()
            .find_enabled_cutover_by_mall_id("mall-a", &mut NoTransaction)
            .await
            .unwrap()
            .expect("启用后按商城可查到唯一 T");
        assert!(enabled.status.is_enabled());
        assert_eq!(enabled.enabled_at, Some(Instant::from_unix_secs(1_700_000_500)));

        db.mall_consumption_cutovers()
            .soft_delete(&mut cutover, &mut NoTransaction)
            .await
            .unwrap();
        let after_delete = db
            .mall_consumption_cutovers()
            .find_by_id(&cutover.base.id, &mut NoTransaction)
            .await
            .unwrap();
        assert!(after_delete.is_none(), "软删除后按 ID 不可见");

        db.mall_consumption_cutovers()
            .restore(&mut cutover, &mut NoTransaction)
            .await
            .unwrap();
        let after_restore = db
            .mall_consumption_cutovers()
            .find_by_id(&cutover.base.id, &mut NoTransaction)
            .await
            .unwrap();
        assert!(after_restore.is_some(), "恢复后按 ID 重新可见");
    })
}

#[tokio::test]
#[ignore]
async fn cutover_unique_mall_conflict_surfaces_duplicate_key() {
    require_mongo!(async {
        let test_db = TestDb::new("card_cutover_dup").await.unwrap();
        let db = test_db.db();
        assert_domain_indexes(db).await;

        let cutover = sample_cutover("mall-a", "cutover-1");
        db.mall_consumption_cutovers()
            .create(&cutover, &mut NoTransaction)
            .await
            .unwrap();

        let duplicate = sample_cutover("mall-a", "cutover-2");
        let error = db
            .mall_consumption_cutovers()
            .create(&duplicate, &mut NoTransaction)
            .await
            .expect_err("同一商城的第二条切换记录必须被唯一索引拒绝");
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
        let test_db = TestDb::new("card_cutover_optlock").await.unwrap();
        let db = test_db.db();

        let mut cutover = sample_cutover("mall-a", "cutover-1");
        db.mall_consumption_cutovers()
            .create(&cutover, &mut NoTransaction)
            .await
            .unwrap();

        let mut stale = cutover.clone();
        cutover
            .set_checklist_reference(Some("doc-b".to_string()))
            .unwrap();
        db.mall_consumption_cutovers()
            .update(&mut cutover, &mut NoTransaction)
            .await
            .unwrap();

        stale.set_checklist_reference(Some("doc-c".to_string())).unwrap();
        let error = db
            .mall_consumption_cutovers()
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
async fn card_instance_identity_conflict_surfaces_duplicate_key() {
    require_mongo!(async {
        let test_db = TestDb::new("card_instance_dup").await.unwrap();
        let db = test_db.db();
        assert_domain_indexes(db).await;

        let instance = sample_card_instance("mall-a", "card-1");
        db.mall_card_instances()
            .create(&instance, &mut NoTransaction)
            .await
            .unwrap();

        let mut duplicate = sample_card_instance("mall-a", "card-2");
        duplicate.opaque_instance_ref = "ref-card-1".to_string();
        let error = db
            .mall_card_instances()
            .create(&duplicate, &mut NoTransaction)
            .await
            .expect_err("同一 (商城, 稳定引用) 的重复基线必须被唯一索引拒绝");
        assert!(
            matches!(error, database::Error::DuplicateKey(_)),
            "期望 DuplicateKey，实际为 {error:?}"
        );

        let same_ref_other_mall = sample_card_instance("mall-b", "card-3");
        db.mall_card_instances()
            .create(&same_ref_other_mall, &mut NoTransaction)
            .await
            .unwrap();
        let found = db
            .mall_card_instances()
            .find_by_identity("mall-b", "ref-card-3", &mut NoTransaction)
            .await
            .unwrap()
            .expect("不同商城同引用应可共存");
        assert_eq!(found.base.id, "card-3");
    })
}

#[tokio::test]
#[ignore]
async fn balance_snapshot_unique_business_key_conflict_surfaces_duplicate_key() {
    require_mongo!(async {
        let test_db = TestDb::new("card_snapshot_dup").await.unwrap();
        let db = test_db.db();
        assert_domain_indexes(db).await;

        let snapshot = sample_snapshot("card-1", "snap-1", 1_700_000_000);
        db.balance_snapshots()
            .create(&snapshot, &mut NoTransaction)
            .await
            .unwrap();

        let duplicate = sample_snapshot("card-1", "snap-2", 1_700_000_000);
        let error = db
            .balance_snapshots()
            .create(&duplicate, &mut NoTransaction)
            .await
            .expect_err("同一 (卡实例, 快照时间) 重复快照必须被唯一索引拒绝");
        assert!(
            matches!(error, database::Error::DuplicateKey(_)),
            "期望 DuplicateKey，实际为 {error:?}"
        );

        let other_card = sample_snapshot("card-2", "snap-3", 1_700_000_000);
        db.balance_snapshots()
            .create(&other_card, &mut NoTransaction)
            .await
            .unwrap();
        assert!(
            db.balance_snapshots()
                .find_at(
                    &MallCardInstanceId::new("card-2"),
                    Instant::from_unix_secs(1_700_000_000),
                    &mut NoTransaction,
                )
                .await
                .unwrap()
                .is_some(),
            "不同卡实例同时间可共存"
        );
    })
}

#[tokio::test]
#[ignore]
async fn correction_chain_roundtrip_and_unique_no_conflict() {
    require_mongo!(async {
        let test_db = TestDb::new("card_correction").await.unwrap();
        let db = test_db.db();
        assert_domain_indexes(db).await;

        let correction_1 = sample_correction("card-1", "corr-1", 1);
        db.card_instance_corrections()
            .create(&correction_1, &mut NoTransaction)
            .await
            .unwrap();

        let correction_2 = sample_correction("card-1", "corr-2", 2);
        db.card_instance_corrections()
            .create(&correction_2, &mut NoTransaction)
            .await
            .unwrap();

        let chain = db
            .card_instance_corrections()
            .list_by_card(&MallCardInstanceId::new("card-1"), &mut NoTransaction)
            .await
            .unwrap();
        assert_eq!(chain.len(), 2, "按卡实例取回完整纠错链");
        assert_eq!(chain[0].correction_no, 1);
        assert_eq!(chain[1].correction_no, 2);
        assert_eq!(
            chain[1].supersedes_correction_id,
            Some(MallCardInstanceCorrectionId::new("corr-1"))
        );

        let duplicate_no = sample_correction("card-1", "corr-3", 2);
        let error = db
            .card_instance_corrections()
            .create(&duplicate_no, &mut NoTransaction)
            .await
            .expect_err("同卡实例重复纠错号必须被唯一索引拒绝");
        assert!(
            matches!(error, database::Error::DuplicateKey(_)),
            "期望 DuplicateKey，实际为 {error:?}"
        );
    })
}

#[tokio::test]
#[ignore]
async fn snapshot_list_respects_card_and_time_range() {
    require_mongo!(async {
        let test_db = TestDb::new("card_snapshot_list").await.unwrap();
        let db = test_db.db();
        assert_domain_indexes(db).await;

        for (id, secs) in [
            ("snap-a", 1_700_000_000),
            ("snap-b", 1_700_000_300),
            ("snap-c", 1_700_000_600),
        ] {
            db.balance_snapshots()
                .create(&sample_snapshot("card-1", id, secs), &mut NoTransaction)
                .await
                .unwrap();
        }
        db.balance_snapshots()
            .create(
                &sample_snapshot("card-2", "snap-d", 1_700_000_000),
                &mut NoTransaction,
            )
            .await
            .unwrap();

        let series = db
            .balance_snapshots()
            .list_by_card_and_range(
                &MallCardInstanceId::new("card-1"),
                Some(Instant::from_unix_secs(1_700_000_300)),
                Some(Instant::from_unix_secs(1_700_000_600)),
                &mut NoTransaction,
            )
            .await
            .unwrap();
        assert_eq!(series.len(), 2, "时间范围 [t+300, t+600] 只命中两条");
        assert_eq!(series[0].base.id, "snap-b");
        assert_eq!(series[1].base.id, "snap-c");
        assert_eq!(series[0].balance, Amount::from_str("88.00").unwrap());
        assert_eq!(series[0].source_event_id, "evt-snap-b");
    })
}

#[tokio::test]
#[ignore]
async fn cutover_projection_list_respects_filters_and_pagination() {
    require_mongo!(async {
        let test_db = TestDb::new("card_cutover_list").await.unwrap();
        let db = test_db.db();
        assert_domain_indexes(db).await;

        let mut enabled = sample_cutover("mall-a", "cutover-1");
        enabled
            .enable(Instant::from_unix_secs(1_700_000_500), "owner-1")
            .unwrap();
        db.mall_consumption_cutovers()
            .create(&enabled, &mut NoTransaction)
            .await
            .unwrap();
        db.mall_consumption_cutovers()
            .create(&sample_cutover("mall-b", "cutover-2"), &mut NoTransaction)
            .await
            .unwrap();

        let filter = MallConsumptionCutoverFilter {
            mall_id: Some("mall".to_string()),
            status: Some(CutoverStatus::Enabled),
            page: 1,
            page_size: 10,
            sort_by: Some("enabled_at".to_string()),
            sort_ascending: true,
        };
        let page = db
            .mall_consumption_cutovers()
            .search_cutovers(&filter, &mut NoTransaction)
            .await
            .unwrap();
        assert_eq!(page.total, 1, "已启用且商城含 mall 前缀只有一条");
        assert_eq!(page.items.len(), 1);
        let row = &page.items[0];
        assert_eq!(row.mall_id, "mall-a");
        assert_eq!(row.status, CutoverStatus::Enabled);
        assert_eq!(row.enabled_at, Some(Instant::from_unix_secs(1_700_000_500)));
        assert_eq!(row.enabled_by.as_deref(), Some("owner-1"));
        assert!(row.version >= 1);
        assert!(row.created_at > 0);

        let second_page = MallConsumptionCutoverFilter {
            page: 2,
            page_size: 1,
            ..filter
        };
        let empty = db
            .mall_consumption_cutovers()
            .search_cutovers(&second_page, &mut NoTransaction)
            .await
            .unwrap();
        assert_eq!(empty.items.len(), 0, "分页边界：第二页为空");
    })
}

#[tokio::test]
#[ignore]
async fn card_instance_projection_list_respects_filters_and_pagination() {
    require_mongo!(async {
        let test_db = TestDb::new("card_instance_list").await.unwrap();
        let db = test_db.db();
        assert_domain_indexes(db).await;

        db.mall_card_instances()
            .create(&sample_card_instance("mall-a", "card-1"), &mut NoTransaction)
            .await
            .unwrap();
        let mut historical = sample_card_instance("mall-a", "card-2");
        historical.source_type = CardSourceType::HistoricalBaseline;
        db.mall_card_instances()
            .create(&historical, &mut NoTransaction)
            .await
            .unwrap();

        let filter = MallCardInstanceFilter {
            mall_id: Some("mall-a".to_string()),
            opaque_instance_ref: Some("ref-card".to_string()),
            source_type: Some(CardSourceType::Realtime),
            page: 1,
            page_size: 1,
            sort_by: Some("baseline_at".to_string()),
            sort_ascending: true,
        };
        let page = db
            .mall_card_instances()
            .search_card_instances(&filter, &mut NoTransaction)
            .await
            .unwrap();
        assert_eq!(page.total, 1);
        assert_eq!(page.items.len(), 1);
        let row = &page.items[0];
        assert_eq!(row.mall_id, "mall-a");
        assert_eq!(row.opaque_instance_ref, "ref-card-1");
        assert_eq!(row.source_type, CardSourceType::Realtime);
        assert_eq!(row.initial_balance, Amount::from_str("100.00").unwrap());
        assert_eq!(row.origin_sales_order_id, SalesOrderId::new("so-1"));
        assert_eq!(row.baseline_at, Instant::from_unix_secs(1_700_000_000));
    })
}

#[tokio::test]
#[ignore]
async fn transaction_abort_rolls_back_both_collections() {
    require_mongo!(async {
        let test_db = TestDb::new("card_tx_abort").await.unwrap();
        let db = test_db.db();
        assert_domain_indexes(db).await;

        let instance = sample_card_instance("mall-a", "card-1");
        let snapshot = sample_snapshot("card-1", "snap-1", 1_700_000_000);

        let db_clone = db.clone();
        let instance_for_tx = instance.clone();
        let snapshot_for_tx = snapshot.clone();
        let result: std::result::Result<(), database::Error> = test_db
            .client()
            .with_transaction(move |session| {
                Box::pin(async move {
                    db_clone
                        .card_instance()
                        .create_card_instance_with_initial_snapshot(
                            &instance_for_tx,
                            &snapshot_for_tx,
                            session,
                        )
                        .await?;
                    Err(database::Error::OptimisticLockingError)
                })
            })
            .await;
        assert!(result.is_err(), "闭包返回错误必须整体回滚");

        let instance_found = db
            .mall_card_instances()
            .find_by_id(&instance.base.id, &mut NoTransaction)
            .await
            .unwrap();
        assert!(instance_found.is_none(), "回滚后基线不得残留");
        let snapshot_found = db
            .balance_snapshots()
            .find_by_id(&snapshot.base.id, &mut NoTransaction)
            .await
            .unwrap();
        assert!(snapshot_found.is_none(), "回滚后快照不得残留");
    })
}

#[tokio::test]
#[ignore]
async fn multi_step_writes_commits_atomically_inside_transaction() {
    require_mongo!(async {
        let test_db = TestDb::new("card_tx_commit").await.unwrap();
        let db = test_db.db();
        assert_domain_indexes(db).await;

        let instance = sample_card_instance("mall-a", "card-1");
        let snapshot = sample_snapshot("card-1", "snap-1", 1_700_000_000);

        let db_clone = db.clone();
        let instance_for_tx = instance.clone();
        let snapshot_for_tx = snapshot.clone();
        test_db
            .client()
            .with_transaction::<_, (), database::Error>(move |session| {
                Box::pin(async move {
                    db_clone
                        .card_instance()
                        .create_card_instance_with_initial_snapshot(
                            &instance_for_tx,
                            &snapshot_for_tx,
                            session,
                        )
                        .await?;
                    Ok(())
                })
            })
            .await
            .expect("事务提交应成功");

        let found = db
            .mall_card_instances()
            .find_by_identity("mall-a", "ref-card-1", &mut NoTransaction)
            .await
            .unwrap()
            .expect("事务提交后基线必须可见");
        assert_eq!(found.initial_balance, Amount::from_str("100.00").unwrap());
        assert!(
            db.balance_snapshots()
                .find_by_id(&snapshot.base.id, &mut NoTransaction)
                .await
                .unwrap()
                .is_some(),
            "事务提交后快照必须可见"
        );
    })
}

#[tokio::test]
#[ignore]
async fn multi_step_conflict_rolls_back_whole_write() {
    require_mongo!(async {
        let test_db = TestDb::new("card_tx_conflict").await.unwrap();
        let db = test_db.db();
        assert_domain_indexes(db).await;

        let existing = sample_card_instance("mall-a", "card-1");
        db.mall_card_instances()
            .create(&existing, &mut NoTransaction)
            .await
            .unwrap();

        let mut duplicate = sample_card_instance("mall-a", "card-9");
        duplicate.opaque_instance_ref = "ref-card-1".to_string();
        let snapshot = sample_snapshot("card-9", "snap-9", 1_700_000_000);

        let db_clone = db.clone();
        let duplicate_for_tx = duplicate.clone();
        let snapshot_for_tx = snapshot.clone();
        let result: std::result::Result<(), database::Error> = test_db
            .client()
            .with_transaction(move |session| {
                Box::pin(async move {
                    db_clone
                        .card_instance()
                        .create_card_instance_with_initial_snapshot(
                            &duplicate_for_tx,
                            &snapshot_for_tx,
                            session,
                        )
                        .await?;
                    Ok(())
                })
            })
            .await;
        assert!(
            matches!(result, Err(database::Error::DuplicateKey(_))),
            "唯一冲突必须透出 DuplicateKey，实际为 {result:?}"
        );

        let snapshot_found = db
            .balance_snapshots()
            .find_by_id(&snapshot.base.id, &mut NoTransaction)
            .await
            .unwrap();
        assert!(snapshot_found.is_none(), "冲突回滚后快照不得残留");
    })
}
