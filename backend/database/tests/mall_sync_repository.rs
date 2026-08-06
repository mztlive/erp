//! 域 D23 `mall_sync` 仓储集成测试（真实 MongoDB 单节点副本集）。
//!
//! 统一 `#[ignore]` + `ERP_TEST_MONGO_URI` 门控（test-support `require_mongo!`）：
//! 无数据库环境 `cargo test` 跳过；CI 与验收执行
//! `cargo test -p database --test mall_sync_repository -- --include-ignored`。
//! 每个测试使用独立随机数据库名，结束自动 drop（TestDb）。

use database::repository::extensions::MallSyncExt;
use database::{ensure_indexes, NoTransaction, Transactional};
use entities::common::time::Instant;
use entities::ids::{
    MallSalesOrderSnapshotId, MallSalesReconciliationItemId, MallSalesReconciliationJobId,
    MallSalesSyncCursorId, MallSalesSyncJobId, MasterMappingTaskId, SalesOrderId, SalesOrderRevisionId,
    SourceSystemId,
};
use entities::mall_sync::{
    MallSalesOrderSnapshot, MallSalesOrderSnapshotData, MallSalesReconciliationItem,
    MallSalesReconciliationItemData, MallSalesReconciliationJob, MallSalesReconciliationJobData,
    MallSalesSyncCursor, MallSalesSyncJob, MallSalesSyncJobData, MallSalesSyncJobStatus,
    MallSalesSyncJobType, MappingTaskStatus, MappingTaskType, MasterMappingTask, MasterMappingTaskData,
    ReconciliationDifferenceType, ReconciliationItemStatus, ReconciliationJobStatus, SnapshotMappingStatus,
};
use mongodb::Database;
use test_support::{assert_indexes, require_mongo, TestDb};

/// 同步作业列表筛选条件类型（经 `MallSyncExt` 关联类型跨 crate 可达）。
type MallSalesSyncJobFilter = <Database as MallSyncExt>::MallSalesSyncJobFilter;
/// 快照列表筛选条件类型。
type MallSalesOrderSnapshotFilter = <Database as MallSyncExt>::MallSalesOrderSnapshotFilter;
/// 核对作业列表筛选条件类型。
type MallSalesReconciliationJobFilter = <Database as MallSyncExt>::MallSalesReconciliationJobFilter;
/// 核对差异明细列表筛选条件类型。
type MallSalesReconciliationItemFilter = <Database as MallSyncExt>::MallSalesReconciliationItemFilter;
/// 映射任务列表筛选条件类型。
type MasterMappingTaskFilter = <Database as MallSyncExt>::MasterMappingTaskFilter;

/// 构造可复用的同步作业实体。
fn sample_sync_job(source_system_id: &SourceSystemId) -> MallSalesSyncJob {
    MallSalesSyncJob::new(
        MallSalesSyncJobId::new(format!("job-{}", source_system_id)),
        MallSalesSyncJobData {
            source_system_id: source_system_id.clone(),
            job_type: MallSalesSyncJobType::Incremental,
            range_start: Some(Instant::from_unix_secs(1_700_000_000)),
            range_end: Some(Instant::from_unix_secs(1_700_030_000)),
            started_at: Instant::from_unix_secs(1_700_000_100),
        },
    )
    .unwrap()
}

/// 构造可复用的快照实体。
fn sample_snapshot(source_system_id: &SourceSystemId, order_no: &str) -> MallSalesOrderSnapshot {
    MallSalesOrderSnapshot::new(
        MallSalesOrderSnapshotId::new(format!("snap-{order_no}")),
        MallSalesOrderSnapshotData {
            source_system_id: source_system_id.clone(),
            external_order_no: format!(" {order_no} "),
            source_updated_at: Instant::from_unix_secs(1_700_000_000),
            content_hash: Some("sha256:abc".to_string()),
            source_status_code: "EFFECTIVE".to_string(),
            normalized_snapshot: format!("{{\"sell_order\":\"{order_no}\"}}"),
            raw_payload_reference: Some("enc://raw-1".to_string()),
            observed_at: Instant::from_unix_secs(1_700_000_100),
            sync_job_id: MallSalesSyncJobId::new("job-1"),
        },
    )
    .unwrap()
}

/// 构造可复用的核对作业实体。
fn sample_reconciliation_job(source_system_id: &SourceSystemId) -> MallSalesReconciliationJob {
    MallSalesReconciliationJob::new(
        MallSalesReconciliationJobId::new("recon-1"),
        MallSalesReconciliationJobData {
            source_system_id: source_system_id.clone(),
            job_no: "REC-2026-06".to_string(),
            source_list_as_of: Instant::from_unix_secs(1_700_000_000),
            started_at: Instant::from_unix_secs(1_700_000_100),
        },
    )
    .unwrap()
}

/// 构造属于指定核对作业的差异明细实体。
fn sample_reconciliation_item(
    job_id: &MallSalesReconciliationJobId,
    order_no: &str,
) -> MallSalesReconciliationItem {
    MallSalesReconciliationItem::new(
        MallSalesReconciliationItemId::new(format!("item-{order_no}")),
        MallSalesReconciliationItemData {
            reconciliation_job_id: job_id.clone(),
            external_order_no: order_no.to_string(),
            source_status_code: "EFFECTIVE".to_string(),
            source_updated_at: Instant::from_unix_secs(1_700_000_000),
            source_content_hash: Some("h1".to_string()),
            sales_order_id: Some(SalesOrderId::new("so-1")),
            erp_revision_id: Some(SalesOrderRevisionId::new("rev-1")),
            erp_content_hash: Some("h2".to_string()),
            difference_type: ReconciliationDifferenceType::StatusDifference,
        },
    )
    .unwrap()
}

/// 构造可复用的映射任务实体。
fn sample_mapping_task(source_snapshot_id: &MallSalesOrderSnapshotId) -> MasterMappingTask {
    MasterMappingTask::new(
        MasterMappingTaskId::new(format!("task-{}", source_snapshot_id)),
        MasterMappingTaskData {
            source_snapshot_id: source_snapshot_id.clone(),
            mapping_type: MappingTaskType::Customer,
            owner_role: "销售".to_string(),
            owner_user_id: Some("user-1".to_string()),
        },
    )
    .unwrap()
}

/// 执行 `ensure_indexes` 并断言本域全部必需索引就位。
async fn assert_domain_indexes(db: &Database) {
    ensure_indexes(db).await.expect("ensure_indexes 应成功");
    assert_indexes(
        db,
        <Database as MallSyncExt>::MALL_SALES_SYNC_JOBS,
        &["idx_mall_sales_sync_jobs_source_started"],
    )
    .await
    .expect("mall_sales_sync_jobs 索引缺失");
    assert_indexes(
        db,
        <Database as MallSyncExt>::MALL_SALES_SYNC_CURSORS,
        &["uk_mall_sales_sync_cursors_source"],
    )
    .await
    .expect("mall_sales_sync_cursors 索引缺失");
    assert_indexes(
        db,
        <Database as MallSyncExt>::MALL_SALES_ORDER_SNAPSHOTS,
        &[
            "uk_mall_sales_order_snapshots_fact_key",
            "idx_mall_sales_order_snapshots_incremental",
            "idx_mall_sales_order_snapshots_difference",
        ],
    )
    .await
    .expect("mall_sales_order_snapshots 索引缺失");
    assert_indexes(
        db,
        <Database as MallSyncExt>::MALL_SALES_RECONCILIATION_JOBS,
        &[
            "uk_mall_sales_reconciliation_jobs_job_no",
            "idx_mall_sales_reconciliation_jobs_source_asof",
        ],
    )
    .await
    .expect("mall_sales_reconciliation_jobs 索引缺失");
    assert_indexes(
        db,
        <Database as MallSyncExt>::MALL_SALES_RECONCILIATION_ITEMS,
        &[
            "uk_mall_sales_reconciliation_items_job_key",
            "idx_mall_sales_reconciliation_items_job_status",
        ],
    )
    .await
    .expect("mall_sales_reconciliation_items 索引缺失");
    assert_indexes(
        db,
        <Database as MallSyncExt>::MASTER_MAPPING_TASKS,
        &[
            "uk_master_mapping_tasks_snapshot_type_pending",
            "idx_master_mapping_tasks_todo",
            "idx_master_mapping_tasks_snapshot",
        ],
    )
    .await
    .expect("master_mapping_tasks 索引缺失");
}

#[tokio::test]
#[ignore]
async fn cursor_single_row_semantics_and_version_cas_advance() {
    require_mongo!(async {
        let test_db = TestDb::new("mall_sync_cursor").await.unwrap();
        let db = test_db.db();
        assert_domain_indexes(db).await;

        let source = SourceSystemId::new("sys-mall");
        let mut cursor = MallSalesSyncCursor::new(
            MallSalesSyncCursorId::new("cur-1"),
            source.clone(),
            Instant::from_unix_secs(1_700_000_000),
        );
        db.mall_sales_sync_cursors()
            .create(&cursor, &mut NoTransaction)
            .await
            .unwrap();
        assert_eq!(cursor.base.version, 1);

        let found = db
            .mall_sales_sync_cursors()
            .find_by_source(&source, &mut NoTransaction)
            .await
            .unwrap()
            .expect("每个来源商城一个水位，创建后应可读回");
        assert_eq!(found.high_water_updated_at.unix_secs(), 1_700_000_000);

        let mut stale = cursor.clone();
        db.mall_sales_sync_cursors()
            .advance(
                &mut cursor,
                Instant::from_unix_secs(1_700_000_300),
                MallSalesSyncJobId::new("j-1"),
                &mut NoTransaction,
            )
            .await
            .unwrap();
        assert_eq!(cursor.base.version, 2, "水位前移成功后 version 递增");
        assert_eq!(cursor.high_water_updated_at.unix_secs(), 1_700_000_300);

        let error = db
            .mall_sales_sync_cursors()
            .advance(
                &mut stale,
                Instant::from_unix_secs(1_700_000_400),
                MallSalesSyncJobId::new("j-2"),
                &mut NoTransaction,
            )
            .await
            .expect_err("陈旧 version 推进水位必须被 CAS 拒绝");
        assert!(
            matches!(error, database::Error::OptimisticLockingError),
            "期望 OptimisticLockingError，实际为 {error:?}"
        );

        let duplicate = MallSalesSyncCursor::new(
            MallSalesSyncCursorId::new("cur-2"),
            source,
            Instant::from_unix_secs(1_700_000_000),
        );
        let dup_error = db
            .mall_sales_sync_cursors()
            .create(&duplicate, &mut NoTransaction)
            .await
            .expect_err("同一来源商城第二个水位必须被唯一索引拒绝");
        assert!(
            matches!(dup_error, database::Error::DuplicateKey(_)),
            "期望 DuplicateKey，实际为 {dup_error:?}"
        );
    })
}

#[tokio::test]
#[ignore]
async fn snapshot_fact_key_dedup_and_latest_by_order() {
    require_mongo!(async {
        let test_db = TestDb::new("mall_sync_snap").await.unwrap();
        let db = test_db.db();
        assert_domain_indexes(db).await;

        let source = SourceSystemId::new("sys-mall");
        let snapshot = sample_snapshot(&source, "SO-2026-001");
        db.mall_sales_order_snapshots()
            .create(&snapshot, &mut NoTransaction)
            .await
            .unwrap();

        let by_key = db
            .mall_sales_order_snapshots()
            .find_by_fact_key(
                &source,
                &snapshot.external_order_key,
                snapshot.source_updated_at,
                &mut NoTransaction,
            )
            .await
            .unwrap()
            .expect("按事实键应读回快照");
        assert_eq!(by_key.external_order_no, "SO-2026-001");
        assert_eq!(by_key.source_status_code, "EFFECTIVE");

        let duplicate = sample_snapshot(&source, "SO-2026-001");
        let error = db
            .mall_sales_order_snapshots()
            .create(&duplicate, &mut NoTransaction)
            .await
            .expect_err("business_fact_key 重复必须被唯一索引拒绝（P2 §5）");
        assert!(
            matches!(error, database::Error::DuplicateKey(_)),
            "期望 DuplicateKey，实际为 {error:?}"
        );

        let mut newer = sample_snapshot(&source, "SO-2026-001");
        newer.source_updated_at = Instant::from_unix_secs(1_700_000_200);
        db.mall_sales_order_snapshots()
            .create(&newer, &mut NoTransaction)
            .await
            .unwrap();

        let latest = db
            .mall_sales_order_snapshots()
            .find_latest_by_order(&source, &snapshot.external_order_key, &mut NoTransaction)
            .await
            .unwrap()
            .expect("同来源单应取回最新快照");
        assert_eq!(latest.source_updated_at.unix_secs(), 1_700_000_200);
    })
}

#[tokio::test]
#[ignore]
async fn snapshot_mapping_status_advances_with_optimistic_lock() {
    require_mongo!(async {
        let test_db = TestDb::new("mall_sync_snap_lock").await.unwrap();
        let db = test_db.db();
        assert_domain_indexes(db).await;

        let source = SourceSystemId::new("sys-mall");
        let mut snapshot = sample_snapshot(&source, "SO-2026-002");
        db.mall_sales_order_snapshots()
            .create(&snapshot, &mut NoTransaction)
            .await
            .unwrap();

        let mut stale = snapshot.clone();
        snapshot.mark_applied(SalesOrderRevisionId::new("rev-1")).unwrap();
        db.mall_sales_order_snapshots()
            .update(&mut snapshot, &mut NoTransaction)
            .await
            .unwrap();
        assert_eq!(snapshot.mapping_status, SnapshotMappingStatus::Applied);
        assert_eq!(snapshot.base.version, 2);

        stale.mark_no_change().unwrap();
        let error = db
            .mall_sales_order_snapshots()
            .update(&mut stale, &mut NoTransaction)
            .await
            .expect_err("陈旧 version 更新必须被 CAS 拒绝");
        assert!(
            matches!(error, database::Error::OptimisticLockingError),
            "期望 OptimisticLockingError，实际为 {error:?}"
        );
    })
}

#[tokio::test]
#[ignore]
async fn sync_job_list_search_and_running_incremental_lookup() {
    require_mongo!(async {
        let test_db = TestDb::new("mall_sync_job_list").await.unwrap();
        let db = test_db.db();
        assert_domain_indexes(db).await;

        let source = SourceSystemId::new("sys-mall");
        let running = sample_sync_job(&source);
        db.mall_sales_sync_jobs()
            .create(&running, &mut NoTransaction)
            .await
            .unwrap();
        let mut done = sample_sync_job(&SourceSystemId::new("sys-other"));
        done.record_progress(3, 100, 0).unwrap();
        done.finish(
            MallSalesSyncJobStatus::Success,
            Instant::from_unix_secs(1_700_000_200),
        )
        .unwrap();
        db.mall_sales_sync_jobs()
            .create(&done, &mut NoTransaction)
            .await
            .unwrap();

        let active = db
            .mall_sales_sync_jobs()
            .find_running_incremental_by_source(&source, &mut NoTransaction)
            .await
            .unwrap()
            .expect("运行中的增量任务应命中");
        assert_eq!(active.status, MallSalesSyncJobStatus::Running);
        assert!(
            db.mall_sales_sync_jobs()
                .find_running_incremental_by_source(&SourceSystemId::new("sys-other"), &mut NoTransaction)
                .await
                .unwrap()
                .is_none(),
            "已结束任务不再命中"
        );

        let filter = MallSalesSyncJobFilter {
            source_system_id: None,
            job_type: Some(MallSalesSyncJobType::Incremental),
            status: Some(MallSalesSyncJobStatus::Running),
            started_at_from: Some(Instant::from_unix_secs(1_700_000_000)),
            started_at_to: Some(Instant::from_unix_secs(1_700_000_100)),
            page: 1,
            page_size: 1,
            sort_by: Some("started_at".to_string()),
            sort_ascending: true,
        };
        let page = db
            .mall_sales_sync_jobs()
            .search_mall_sales_sync_jobs(&filter, &mut NoTransaction)
            .await
            .unwrap();
        assert_eq!(page.total, 1, "运行中且区间命中只有 sys-mall 一条");
        let row = &page.items[0];
        assert_eq!(row.source_system_id, source);
        assert_eq!(row.job_type, MallSalesSyncJobType::Incremental);
        assert_eq!(row.status, MallSalesSyncJobStatus::Running);
        assert_eq!(row.page_count, 0);
        assert!(row.started_at.unix_secs() == 1_700_000_100);
        assert!(row.version >= 1);
    })
}

#[tokio::test]
#[ignore]
async fn sync_job_soft_delete_and_restore() {
    require_mongo!(async {
        let test_db = TestDb::new("mall_sync_job_delete").await.unwrap();
        let db = test_db.db();
        assert_domain_indexes(db).await;

        let mut job = sample_sync_job(&SourceSystemId::new("sys-mall"));
        db.mall_sales_sync_jobs()
            .create(&job, &mut NoTransaction)
            .await
            .unwrap();

        db.mall_sales_sync_jobs()
            .soft_delete(&mut job, &mut NoTransaction)
            .await
            .unwrap();
        assert!(db
            .mall_sales_sync_jobs()
            .find_by_id(&job.base.id, &mut NoTransaction)
            .await
            .unwrap()
            .is_none());

        db.mall_sales_sync_jobs()
            .restore(&mut job, &mut NoTransaction)
            .await
            .unwrap();
        assert!(db
            .mall_sales_sync_jobs()
            .find_by_id(&job.base.id, &mut NoTransaction)
            .await
            .unwrap()
            .is_some());
    })
}

#[tokio::test]
#[ignore]
async fn snapshot_projection_list_respects_filters_and_pagination() {
    require_mongo!(async {
        let test_db = TestDb::new("mall_sync_snap_list").await.unwrap();
        let db = test_db.db();
        assert_domain_indexes(db).await;

        let source = SourceSystemId::new("sys-mall");
        let mut applied = sample_snapshot(&source, "SO-100");
        applied.mark_applied(SalesOrderRevisionId::new("rev-1")).unwrap();
        let mut diff = sample_snapshot(&source, "SO-101");
        diff.mark_difference().unwrap();
        let mut later = sample_snapshot(&SourceSystemId::new("sys-other"), "SO-102");
        later.observed_at = Instant::from_unix_secs(1_700_000_500);
        db.mall_sales_order_snapshots()
            .create(&applied, &mut NoTransaction)
            .await
            .unwrap();
        db.mall_sales_order_snapshots()
            .create(&diff, &mut NoTransaction)
            .await
            .unwrap();
        db.mall_sales_order_snapshots()
            .create(&later, &mut NoTransaction)
            .await
            .unwrap();

        let filter = MallSalesOrderSnapshotFilter {
            source_system_id: Some(source),
            mapping_status: None,
            observed_at_from: None,
            observed_at_to: None,
            page: 1,
            page_size: 1,
            sort_by: Some("source_updated_at".to_string()),
            sort_ascending: true,
        };
        let page = db
            .mall_sales_order_snapshots()
            .search_mall_sales_order_snapshots(&filter, &mut NoTransaction)
            .await
            .unwrap();
        assert_eq!(page.total, 2);
        assert_eq!(page.items.len(), 1);
        let row = &page.items[0];
        assert_eq!(row.external_order_no, "SO-100");
        assert_eq!(row.mapping_status, SnapshotMappingStatus::Applied);
        assert_eq!(
            row.applied_sales_order_revision_id,
            Some(SalesOrderRevisionId::new("rev-1"))
        );
        assert_eq!(row.content_hash.as_deref(), Some("sha256:abc"));
        assert!(row.external_order_key.as_bytes() == b"SO-100");

        let candidates = db
            .mall_sales_order_snapshots()
            .find_by_mapping_status_before(
                SnapshotMappingStatus::Difference,
                Instant::from_unix_secs(1_700_000_600),
                &mut NoTransaction,
            )
            .await
            .unwrap();
        assert_eq!(candidates.len(), 1, "差异处理索引命中 SO-101");
        assert_eq!(candidates[0].external_order_no, "SO-101");
    })
}

#[tokio::test]
#[ignore]
async fn reconciliation_job_no_and_item_key_unique() {
    require_mongo!(async {
        let test_db = TestDb::new("mall_sync_recon_dup").await.unwrap();
        let db = test_db.db();
        assert_domain_indexes(db).await;

        let job = sample_reconciliation_job(&SourceSystemId::new("sys-mall"));
        db.mall_sales_reconciliation_jobs()
            .create(&job, &mut NoTransaction)
            .await
            .unwrap();

        let duplicate_job = sample_reconciliation_job(&SourceSystemId::new("sys-other"));
        let error = db
            .mall_sales_reconciliation_jobs()
            .create(&duplicate_job, &mut NoTransaction)
            .await
            .expect_err("重复 job_no 必须被唯一索引拒绝");
        assert!(
            matches!(error, database::Error::DuplicateKey(_)),
            "期望 DuplicateKey，实际为 {error:?}"
        );

        let job_id: MallSalesReconciliationJobId = job.base.id.clone().into();
        let item = sample_reconciliation_item(&job_id, "SO-1");
        db.mall_sales_reconciliation_items()
            .create(&item, &mut NoTransaction)
            .await
            .unwrap();

        let duplicate_item = sample_reconciliation_item(&job_id, "SO-1");
        let error2 = db
            .mall_sales_reconciliation_items()
            .create(&duplicate_item, &mut NoTransaction)
            .await
            .expect_err("同作业同来源单比较键重复必须被唯一索引拒绝");
        assert!(
            matches!(error2, database::Error::DuplicateKey(_)),
            "期望 DuplicateKey，实际为 {error2:?}"
        );

        let found = db
            .mall_sales_reconciliation_jobs()
            .find_by_job_no("REC-2026-06", &mut NoTransaction)
            .await
            .unwrap()
            .expect("按批次号应读回核对作业");
        assert_eq!(found.job_no, "REC-2026-06");
        let item_found = db
            .mall_sales_reconciliation_items()
            .find_by_job_and_key(&job_id, &item.external_order_key, &mut NoTransaction)
            .await
            .unwrap()
            .expect("按作业+比较键应读回明细");
        assert_eq!(item_found.external_order_no, "SO-1");
    })
}

#[tokio::test]
#[ignore]
async fn reconciliation_item_list_search_and_batch_read() {
    require_mongo!(async {
        let test_db = TestDb::new("mall_sync_recon_list").await.unwrap();
        let db = test_db.db();
        assert_domain_indexes(db).await;

        let job = sample_reconciliation_job(&SourceSystemId::new("sys-mall"));
        db.mall_sales_reconciliation_jobs()
            .create(&job, &mut NoTransaction)
            .await
            .unwrap();
        let job_id: MallSalesReconciliationJobId = job.base.id.clone().into();
        let mut item = sample_reconciliation_item(&job_id, "SO-1");
        item.start_backfill(MallSalesSyncJobId::new("bf-1")).unwrap();
        db.mall_sales_reconciliation_items()
            .create(&item, &mut NoTransaction)
            .await
            .unwrap();
        db.mall_sales_reconciliation_items()
            .create(&sample_reconciliation_item(&job_id, "SO-2"), &mut NoTransaction)
            .await
            .unwrap();

        let filter = MallSalesReconciliationItemFilter {
            reconciliation_job_id: Some(job_id.clone()),
            status: None,
            difference_type: Some(ReconciliationDifferenceType::StatusDifference),
            page: 1,
            page_size: 1,
            sort_by: Some("source_updated_at".to_string()),
            sort_ascending: true,
        };
        let page = db
            .mall_sales_reconciliation_items()
            .search_mall_sales_reconciliation_items(&filter, &mut NoTransaction)
            .await
            .unwrap();
        assert_eq!(page.total, 2);
        assert_eq!(page.items.len(), 1);
        assert_eq!(page.items[0].status, ReconciliationItemStatus::Backfilling);
        assert_eq!(
            page.items[0].single_order_sync_job_id,
            Some(MallSalesSyncJobId::new("bf-1"))
        );

        let all = db
            .mall_sales_reconciliation_items()
            .find_items_by_job(&job_id, &mut NoTransaction)
            .await
            .unwrap();
        assert_eq!(all.len(), 2, "一次取回作业全部差异明细");
    })
}

#[tokio::test]
#[ignore]
async fn multi_step_reconciliation_commits_atomically_inside_transaction() {
    require_mongo!(async {
        let test_db = TestDb::new("mall_sync_tx_commit").await.unwrap();
        let db = test_db.db();
        assert_domain_indexes(db).await;

        let job = sample_reconciliation_job(&SourceSystemId::new("sys-mall"));
        let job_id: MallSalesReconciliationJobId = job.base.id.clone().into();
        let items = vec![
            sample_reconciliation_item(&job_id, "SO-1"),
            sample_reconciliation_item(&job_id, "SO-2"),
        ];

        let db_clone = db.clone();
        let job_for_tx = job.clone();
        let items_for_tx = items.clone();
        test_db
            .client()
            .with_transaction::<_, (), database::Error>(move |session| {
                Box::pin(async move {
                    db_clone
                        .mall_sync()
                        .create_reconciliation_job_with_items(&job_for_tx, &items_for_tx, session)
                        .await?;
                    Ok(())
                })
            })
            .await
            .expect("事务提交应成功");

        let job_found = db
            .mall_sales_reconciliation_jobs()
            .find_by_id(&job.base.id, &mut NoTransaction)
            .await
            .unwrap();
        assert!(job_found.is_some(), "事务提交后核对作业必须可见");
        let items_found = db
            .mall_sales_reconciliation_items()
            .find_items_by_job(&job_id, &mut NoTransaction)
            .await
            .unwrap();
        assert_eq!(items_found.len(), 2, "事务提交后全部明细必须可见");
    })
}

#[tokio::test]
#[ignore]
async fn multi_step_abort_rolls_back_both_collections() {
    require_mongo!(async {
        let test_db = TestDb::new("mall_sync_tx_abort").await.unwrap();
        let db = test_db.db();
        assert_domain_indexes(db).await;

        let job = sample_reconciliation_job(&SourceSystemId::new("sys-mall"));
        let job_id: MallSalesReconciliationJobId = job.base.id.clone().into();
        let items = vec![
            sample_reconciliation_item(&job_id, "SO-1"),
            sample_reconciliation_item(&job_id, "SO-2"),
        ];

        let db_clone = db.clone();
        let job_for_tx = job.clone();
        let items_for_tx = items.clone();
        let result: std::result::Result<(), database::Error> = test_db
            .client()
            .with_transaction(move |session| {
                Box::pin(async move {
                    db_clone
                        .mall_sync()
                        .create_reconciliation_job_with_items(&job_for_tx, &items_for_tx, session)
                        .await?;
                    Err(database::Error::OptimisticLockingError)
                })
            })
            .await;
        assert!(result.is_err(), "闭包返回错误必须整体回滚");

        let job_found = db
            .mall_sales_reconciliation_jobs()
            .find_by_id(&job.base.id, &mut NoTransaction)
            .await
            .unwrap();
        assert!(job_found.is_none(), "回滚后核对作业不得残留");
        let items_found = db
            .mall_sales_reconciliation_items()
            .find_items_by_job(&job_id, &mut NoTransaction)
            .await
            .unwrap();
        assert!(items_found.is_empty(), "回滚后明细不得残留");
    })
}

#[tokio::test]
#[ignore]
async fn multi_step_no_transaction_writes_both_independently() {
    require_mongo!(async {
        let test_db = TestDb::new("mall_sync_notx").await.unwrap();
        let db = test_db.db();
        assert_domain_indexes(db).await;

        let job = sample_reconciliation_job(&SourceSystemId::new("sys-mall"));
        let job_id: MallSalesReconciliationJobId = job.base.id.clone().into();
        let items = vec![sample_reconciliation_item(&job_id, "SO-1")];

        db.mall_sync()
            .create_reconciliation_job_with_items(&job, &items, &mut NoTransaction)
            .await
            .expect("NoTransaction 下两笔写入各自自动提交，应全部成功");

        assert!(db
            .mall_sales_reconciliation_jobs()
            .find_by_id(&job.base.id, &mut NoTransaction)
            .await
            .unwrap()
            .is_some());
        assert_eq!(
            db.mall_sales_reconciliation_items()
                .find_items_by_job(&job_id, &mut NoTransaction)
                .await
                .unwrap()
                .len(),
            1,
            "非事务执行器写入行为可预期：全部落盘"
        );
    })
}

#[tokio::test]
#[ignore]
async fn mapping_task_pending_partial_unique_and_todo_search() {
    require_mongo!(async {
        let test_db = TestDb::new("mall_sync_mapping").await.unwrap();
        let db = test_db.db();
        assert_domain_indexes(db).await;

        let snapshot_id = MallSalesOrderSnapshotId::new("snap-1");
        let task = sample_mapping_task(&snapshot_id);
        db.master_mapping_tasks()
            .create(&task, &mut NoTransaction)
            .await
            .unwrap();

        let duplicate = sample_mapping_task(&snapshot_id);
        let error = db
            .master_mapping_tasks()
            .create(&duplicate, &mut NoTransaction)
            .await
            .expect_err("同一快照+映射类型只允许一个进行中任务");
        assert!(
            matches!(error, database::Error::DuplicateKey(_)),
            "期望 DuplicateKey，实际为 {error:?}"
        );

        let pending = db
            .master_mapping_tasks()
            .find_pending_by_snapshot_and_type(&snapshot_id, MappingTaskType::Customer, &mut NoTransaction)
            .await
            .unwrap()
            .expect("进行中任务应命中");
        assert_eq!(pending.status, MappingTaskStatus::Pending);

        let mut resolved = task.clone();
        resolved
            .resolve(
                "映射到客户 C-1".to_string(),
                Instant::from_unix_secs(1_700_000_100),
            )
            .unwrap();
        db.master_mapping_tasks()
            .update(&mut resolved, &mut NoTransaction)
            .await
            .unwrap();

        let new_round = sample_mapping_task(&snapshot_id);
        db.master_mapping_tasks()
            .create(&new_round, &mut NoTransaction)
            .await
            .expect("已解决任务终结后允许新任务（部分唯一索引）");

        let filter = MasterMappingTaskFilter {
            source_snapshot_id: None,
            mapping_type: None,
            status: Some(MappingTaskStatus::Pending),
            owner_role: Some("销售".to_string()),
            owner_user_id: Some("user-1".to_string()),
            page: 1,
            page_size: 20,
            sort_by: None,
            sort_ascending: false,
        };
        let page = db
            .master_mapping_tasks()
            .search_master_mapping_tasks(&filter, &mut NoTransaction)
            .await
            .unwrap();
        assert_eq!(page.total, 1, "待办索引只命中新的待处理任务");
        assert_eq!(page.items[0].owner_role, "销售");
        assert_eq!(page.items[0].mapping_type, MappingTaskType::Customer);

        let by_snapshot = db
            .master_mapping_tasks()
            .find_by_snapshot(&snapshot_id, &mut NoTransaction)
            .await
            .unwrap();
        assert_eq!(by_snapshot.len(), 2, "历史任务永久可查");
    })
}

#[tokio::test]
#[ignore]
async fn reconciliation_job_search_and_status_filter() {
    require_mongo!(async {
        let test_db = TestDb::new("mall_sync_recon_search").await.unwrap();
        let db = test_db.db();
        assert_domain_indexes(db).await;

        let job = sample_reconciliation_job(&SourceSystemId::new("sys-mall"));
        db.mall_sales_reconciliation_jobs()
            .create(&job, &mut NoTransaction)
            .await
            .unwrap();
        let mut failed = sample_reconciliation_job(&SourceSystemId::new("sys-other"));
        failed.job_no = "REC-2026-07".to_string();
        db.mall_sales_reconciliation_jobs()
            .create(&failed, &mut NoTransaction)
            .await
            .unwrap();

        let filter = MallSalesReconciliationJobFilter {
            source_system_id: Some(SourceSystemId::new("sys-mall")),
            status: Some(ReconciliationJobStatus::Running),
            page: 1,
            page_size: 20,
            sort_by: Some("source_list_as_of".to_string()),
            sort_ascending: true,
        };
        let page = db
            .mall_sales_reconciliation_jobs()
            .search_mall_sales_reconciliation_jobs(&filter, &mut NoTransaction)
            .await
            .unwrap();
        assert_eq!(page.total, 1);
        assert_eq!(page.items[0].job_no, "REC-2026-06");
        assert_eq!(page.items[0].status, ReconciliationJobStatus::Running);
        assert_eq!(page.items[0].difference_count, 0);
    })
}
