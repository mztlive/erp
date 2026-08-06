//! 域 D04 `bulk_job` 仓储集成测试（真实 MongoDB 单节点副本集）。
//!
//! 统一 `#[ignore]` + `ERP_TEST_MONGO_URI` 门控（test-support `require_mongo!`）：
//! 无数据库环境 `cargo test` 跳过；CI 与验收执行
//! `cargo test -p database --test bulk_job_repository -- --include-ignored`。
//! 每个测试使用独立随机数据库名，结束自动 drop（TestDb）。

use database::repository::extensions::BulkJobExt;
use database::{ensure_indexes, NoTransaction, Transactional};
use entities::bulk_job::{
    BackgroundJob, BackgroundJobData, BackgroundJobItem, BackgroundJobItemData, BulkSelectionItem,
    BulkSelectionItemData, BulkSelectionSnapshot, BulkSelectionSnapshotData, ItemStatus, JobStatus, JobType,
    SelectionStatus, SelectionType,
};
use entities::common::time::Instant;
use entities::ids::{BackgroundJobId, BackgroundJobItemId, BulkSelectionItemId, BulkSelectionSnapshotId};
use mongodb::Database;
use test_support::{assert_indexes, require_mongo, TestDb};

/// 选择快照列表筛选条件类型（经 `BulkJobExt` 关联类型跨 crate 可达）。
type BulkSelectionSnapshotFilter = <Database as BulkJobExt>::BulkSelectionSnapshotFilter;
/// 后台任务列表筛选条件类型。
type BackgroundJobFilter = <Database as BulkJobExt>::BackgroundJobFilter;

/// 构造可复用的选择快照实体。
fn sample_snapshot(id: &str, item_count: u32) -> BulkSelectionSnapshot {
    BulkSelectionSnapshot::new(
        BulkSelectionSnapshotId::new(id),
        BulkSelectionSnapshotData {
            selection_type: SelectionType::Export,
            data_cutoff_at: Instant::from_unix_secs(1_700_000_000),
            item_count,
            created_by: "admin-1".to_string(),
            expires_at: Instant::from_unix_secs(1_700_604_800),
        },
    )
    .unwrap()
}

/// 构造可复用的冻结目标实体。
fn sample_selection_item(id: &str, snapshot_id: &str, object_id: &str) -> BulkSelectionItem {
    BulkSelectionItem::new(
        BulkSelectionItemId::new(id),
        BulkSelectionItemData {
            selection_snapshot_id: BulkSelectionSnapshotId::new(snapshot_id),
            object_type: "sales_order".to_string(),
            object_id: object_id.to_string(),
            expected_version: Some("v3".to_string()),
            expected_hash: Some("ab12cd34".to_string()),
        },
    )
    .unwrap()
}

/// 构造可复用的后台任务实体。
fn sample_job(id: &str, job_no: &str, request_id: &str) -> BackgroundJob {
    BackgroundJob::new(
        BackgroundJobId::new(id),
        BackgroundJobData {
            job_no: job_no.to_string(),
            job_type: JobType::Import,
            domain_job_type: Some("legacy_import_batch".to_string()),
            domain_job_id: Some("batch-1".to_string()),
            selection_snapshot_id: None,
            requested_by: "admin-1".to_string(),
            request_id: request_id.to_string(),
            input_file_asset_id: None,
            result_file_asset_id: None,
            total_count: 2,
        },
    )
    .unwrap()
}

/// 构造可复用的任务逐项行。
fn sample_job_item(id: &str, job_id: &str, item_no: u32) -> BackgroundJobItem {
    BackgroundJobItem::new(
        BackgroundJobItemId::new(id),
        BackgroundJobItemData {
            background_job_id: BackgroundJobId::new(job_id),
            item_no,
            object_type: Some("legacy_import_row".to_string()),
            object_id: Some(format!("row-{item_no}")),
            expected_version: None,
            expected_hash: None,
            worksheet_name: Some("Sheet1".to_string()),
            source_row_no: Some(item_no + 1),
            source_column_name: None,
        },
    )
    .unwrap()
}

/// 执行 `ensure_indexes` 并断言本域全部必需索引就位。
async fn assert_domain_indexes(db: &Database) {
    ensure_indexes(db).await.expect("ensure_indexes 应成功");
    assert_indexes(
        db,
        <Database as BulkJobExt>::BULK_SELECTION_SNAPSHOTS,
        &[
            "uk_bulk_selection_snapshots_id",
            "idx_bulk_selection_snapshots_created",
            "idx_bulk_selection_snapshots_status_expires",
        ],
    )
    .await
    .expect("bulk_selection_snapshots 索引缺失");
    assert_indexes(
        db,
        <Database as BulkJobExt>::BULK_SELECTION_ITEMS,
        &[
            "uk_bulk_selection_items_target",
            "idx_bulk_selection_items_result",
        ],
    )
    .await
    .expect("bulk_selection_items 索引缺失");
    assert_indexes(
        db,
        <Database as BulkJobExt>::BACKGROUND_JOBS,
        &[
            "uk_background_jobs_no",
            "uk_background_jobs_request_id",
            "idx_background_jobs_status_created",
            "idx_background_jobs_domain",
            "idx_background_jobs_requested_created",
        ],
    )
    .await
    .expect("background_jobs 索引缺失");
    assert_indexes(
        db,
        <Database as BulkJobExt>::BACKGROUND_JOB_ITEMS,
        &["uk_background_job_items_no", "idx_background_job_items_status"],
    )
    .await
    .expect("background_job_items 索引缺失");
}

#[tokio::test]
#[ignore]
async fn snapshot_item_and_job_roundtrip_with_optimistic_lock() {
    require_mongo!(async {
        let test_db = TestDb::new("bulkjob_crud").await.unwrap();
        let db = test_db.db();
        assert_domain_indexes(db).await;

        let mut snapshot = sample_snapshot("snap-1", 1);
        db.bulk_selection_snapshots()
            .create(&snapshot, &mut NoTransaction)
            .await
            .unwrap();
        let item = sample_selection_item("si-1", "snap-1", "SO-1");
        db.bulk_selection_items()
            .create(&item, &mut NoTransaction)
            .await
            .unwrap();

        let found = db
            .bulk_selection_snapshots()
            .find_by_id(&snapshot.base.id, &mut NoTransaction)
            .await
            .unwrap()
            .expect("快照创建后应可读回");
        assert_eq!(found.selection_type, SelectionType::Export);
        assert_eq!(found.data_cutoff_at, Instant::from_unix_secs(1_700_000_000));
        assert_eq!(found.status, SelectionStatus::Pending);

        let items = db
            .bulk_selection_items()
            .list_by_snapshot(&BulkSelectionSnapshotId::new("snap-1"), &mut NoTransaction)
            .await
            .unwrap();
        assert_eq!(items.len(), 1, "快照冻结目标批量取回");
        assert_eq!(items[0].object_id, "SO-1");

        snapshot.confirm().unwrap();
        db.bulk_selection_snapshots()
            .update(&mut snapshot, &mut NoTransaction)
            .await
            .unwrap();
        assert_eq!(snapshot.base.version, 2, "乐观锁成功后 version 递增");
        assert_eq!(snapshot.status, SelectionStatus::Confirmed);

        let mut job = sample_job("job-1", "JOB-2025-001", "req-001");
        db.background_jobs()
            .create(&job, &mut NoTransaction)
            .await
            .unwrap();
        let by_no = db
            .background_jobs()
            .find_by_job_no("JOB-2025-001", &mut NoTransaction)
            .await
            .unwrap()
            .expect("按任务编号应命中");
        assert_eq!(by_no.base.id, "job-1");
        let by_request = db
            .background_jobs()
            .find_by_request_id("req-001", &mut NoTransaction)
            .await
            .unwrap()
            .expect("按幂等身份应命中");
        assert_eq!(by_request.base.id, "job-1");

        job.start(Instant::from_unix_secs(1_700_100_000)).unwrap();
        db.background_jobs()
            .update(&mut job, &mut NoTransaction)
            .await
            .unwrap();
        assert_eq!(job.status, JobStatus::Running);
    })
}

#[tokio::test]
#[ignore]
async fn unique_job_identity_conflicts_surface_duplicate_key() {
    require_mongo!(async {
        let test_db = TestDb::new("bulkjob_dup").await.unwrap();
        let db = test_db.db();
        assert_domain_indexes(db).await;

        db.background_jobs()
            .create(
                &sample_job("job-1", "JOB-2025-001", "req-001"),
                &mut NoTransaction,
            )
            .await
            .unwrap();

        let duplicate_no = sample_job("job-2", "JOB-2025-001", "req-002");
        let error = db
            .background_jobs()
            .create(&duplicate_no, &mut NoTransaction)
            .await
            .expect_err("重复 job_no 必须被唯一索引拒绝");
        assert!(matches!(error, database::Error::DuplicateKey(_)));

        let duplicate_request = sample_job("job-3", "JOB-2025-003", "req-001");
        let error = db
            .background_jobs()
            .create(&duplicate_request, &mut NoTransaction)
            .await
            .expect_err("重复 request_id 必须被唯一索引拒绝");
        assert!(matches!(error, database::Error::DuplicateKey(_)));

        db.background_jobs()
            .create(
                &sample_job("job-4", "JOB-2025-004", "req-004"),
                &mut NoTransaction,
            )
            .await
            .expect("不同编号与幂等身份可正常注册");
    })
}

#[tokio::test]
#[ignore]
async fn selection_item_target_duplicate_within_snapshot_is_rejected() {
    require_mongo!(async {
        let test_db = TestDb::new("bulkjob_item_dup").await.unwrap();
        let db = test_db.db();
        assert_domain_indexes(db).await;

        db.bulk_selection_snapshots()
            .create(&sample_snapshot("snap-1", 1), &mut NoTransaction)
            .await
            .unwrap();
        db.bulk_selection_items()
            .create(
                &sample_selection_item("si-1", "snap-1", "SO-1"),
                &mut NoTransaction,
            )
            .await
            .unwrap();

        let duplicate = sample_selection_item("si-2", "snap-1", "SO-1");
        let error = db
            .bulk_selection_items()
            .create(&duplicate, &mut NoTransaction)
            .await
            .expect_err("同一快照下重复 (类型, 对象) 必须被唯一索引拒绝");
        assert!(
            matches!(error, database::Error::DuplicateKey(_)),
            "期望 DuplicateKey，实际为 {error:?}"
        );
    })
}

#[tokio::test]
#[ignore]
async fn soft_delete_and_restore_match_deleted_state() {
    require_mongo!(async {
        let test_db = TestDb::new("bulkjob_soft").await.unwrap();
        let db = test_db.db();
        assert_domain_indexes(db).await;

        let mut snapshot = sample_snapshot("snap-1", 1);
        db.bulk_selection_snapshots()
            .create(&snapshot, &mut NoTransaction)
            .await
            .unwrap();

        db.bulk_selection_snapshots()
            .soft_delete(&mut snapshot, &mut NoTransaction)
            .await
            .unwrap();
        let after_delete = db
            .bulk_selection_snapshots()
            .find_by_id(&snapshot.base.id, &mut NoTransaction)
            .await
            .unwrap();
        assert!(after_delete.is_none(), "软删除后按 ID 不可见");

        db.bulk_selection_snapshots()
            .restore(&mut snapshot, &mut NoTransaction)
            .await
            .unwrap();
        let after_restore = db
            .bulk_selection_snapshots()
            .find_by_id(&snapshot.base.id, &mut NoTransaction)
            .await
            .unwrap();
        assert!(after_restore.is_some(), "恢复后按 ID 重新可见");

        let mut job = sample_job("job-1", "JOB-2025-001", "req-001");
        db.background_jobs()
            .create(&job, &mut NoTransaction)
            .await
            .unwrap();
        let rebind = sample_job("job-2", "JOB-2025-001", "req-002");
        db.background_jobs()
            .soft_delete(&mut job, &mut NoTransaction)
            .await
            .unwrap();
        let error = db
            .background_jobs()
            .create(&rebind, &mut NoTransaction)
            .await
            .expect_err("软删除后 job_no 身份仍被占用，不得复用");
        assert!(
            matches!(error, database::Error::DuplicateKey(_)),
            "软删除身份复用必须返回 DuplicateKey，实际为 {error:?}"
        );
    })
}

#[tokio::test]
#[ignore]
async fn search_respects_pagination_boundary_and_projection() {
    require_mongo!(async {
        let test_db = TestDb::new("bulkjob_list").await.unwrap();
        let db = test_db.db();
        assert_domain_indexes(db).await;

        db.bulk_selection_snapshots()
            .create(&sample_snapshot("snap-1", 2), &mut NoTransaction)
            .await
            .unwrap();
        let mut confirmed = sample_snapshot("snap-2", 1);
        confirmed.selection_type = SelectionType::OwnershipAssignment;
        db.bulk_selection_snapshots()
            .create(&confirmed, &mut NoTransaction)
            .await
            .unwrap();

        let filter = BulkSelectionSnapshotFilter {
            selection_type: Some(SelectionType::Export),
            status: None,
            created_by: Some("admin-1".to_string()),
            page: 1,
            page_size: 1,
            sort_by: Some("created_at".to_string()),
            sort_ascending: true,
        };
        let page = db
            .bulk_selection_snapshots()
            .search_snapshots(&filter, &mut NoTransaction)
            .await
            .unwrap();
        assert_eq!(page.total, 1, "导出类型快照只有一条");
        assert_eq!(page.items.len(), 1);
        let row = &page.items[0];
        assert_eq!(row.id, "snap-1");
        assert_eq!(row.selection_type, SelectionType::Export);
        assert_eq!(row.item_count, 2);
        assert_eq!(row.status, SelectionStatus::Pending);
        assert_eq!(row.expires_at, 1_700_604_800);
        assert_eq!(row.data_cutoff_at, 1_700_000_000);

        db.background_jobs()
            .create(
                &sample_job("job-1", "JOB-2025-001", "req-001"),
                &mut NoTransaction,
            )
            .await
            .unwrap();
        db.background_jobs()
            .create(
                &sample_job("job-2", "JOB-2025-002", "req-002"),
                &mut NoTransaction,
            )
            .await
            .unwrap();
        let job_filter = BackgroundJobFilter {
            job_no: Some("job-2025".to_string()),
            job_type: Some(JobType::Import),
            status: Some(JobStatus::Pending),
            requested_by: Some("admin-1".to_string()),
            page: 1,
            page_size: 1,
            sort_by: Some("created_at".to_string()),
            sort_ascending: true,
        };
        let jobs = db
            .background_jobs()
            .search_background_jobs(&job_filter, &mut NoTransaction)
            .await
            .unwrap();
        assert_eq!(jobs.total, 2, "编号模糊 + 类型 + 状态命中两条");
        assert_eq!(jobs.items.len(), 1, "第一页一条");
        assert!(jobs.items[0].status == JobStatus::Pending);
        assert_eq!(jobs.items[0].total_count, 2);
        assert_eq!(jobs.items[0].processed_count, 0);

        let second_page = BackgroundJobFilter {
            page: 2,
            ..job_filter.clone()
        };
        let jobs_two = db
            .background_jobs()
            .search_background_jobs(&second_page, &mut NoTransaction)
            .await
            .unwrap();
        assert_eq!(jobs_two.items.len(), 1, "第二页一条");
        assert_ne!(
            jobs_two.items[0].job_no, jobs.items[0].job_no,
            "同一秒创建的两条任务顺序不确定，两页必须各占一条"
        );
    })
}

#[tokio::test]
#[ignore]
async fn job_items_paginate_and_filter_by_status() {
    require_mongo!(async {
        let test_db = TestDb::new("bulkjob_items").await.unwrap();
        let db = test_db.db();
        assert_domain_indexes(db).await;

        db.background_jobs()
            .create(
                &sample_job("job-1", "JOB-2025-001", "req-001"),
                &mut NoTransaction,
            )
            .await
            .unwrap();
        let mut failed = sample_job_item("ji-1", "job-1", 1);
        failed
            .record_result(
                ItemStatus::Failed,
                Some("VERSION_MISMATCH".to_string()),
                None,
                None,
                None,
            )
            .unwrap();
        let mut success = sample_job_item("ji-2", "job-1", 2);
        success
            .record_result(
                ItemStatus::Success,
                None,
                Some("已创建".to_string()),
                Some("sales_order".to_string()),
                Some("SO-1".to_string()),
            )
            .unwrap();
        db.background_job_items()
            .create(&failed, &mut NoTransaction)
            .await
            .unwrap();
        db.background_job_items()
            .create(&success, &mut NoTransaction)
            .await
            .unwrap();

        let all = db
            .background_job_items()
            .list_by_job(&BackgroundJobId::new("job-1"), &mut NoTransaction)
            .await
            .unwrap();
        assert_eq!(all.len(), 2, "逐项行按任务批量取回");

        let page = db
            .background_job_items()
            .search_job_items(
                &BackgroundJobId::new("job-1"),
                Some(ItemStatus::Failed),
                1,
                20,
                &mut NoTransaction,
            )
            .await
            .unwrap();
        assert_eq!(page.total, 1, "失败逐项只有一条");
        assert_eq!(page.items[0].item_no, 1);
        assert_eq!(page.items[0].result_code.as_deref(), Some("VERSION_MISMATCH"));
        assert_eq!(page.items[0].object_id.as_deref(), Some("row-1"));

        let boundary = db
            .background_job_items()
            .search_job_items(&BackgroundJobId::new("job-1"), None, 2, 1, &mut NoTransaction)
            .await
            .unwrap();
        assert_eq!(boundary.items.len(), 1, "第二页一条");
        assert_eq!(boundary.items[0].item_no, 2);
        assert_eq!(boundary.items[0].status, Some(ItemStatus::Success));
        assert_eq!(boundary.items[0].result_object_id.as_deref(), Some("SO-1"));
    })
}

#[tokio::test]
#[ignore]
async fn create_snapshot_with_items_commits_atomically_inside_transaction() {
    require_mongo!(async {
        let test_db = TestDb::new("bulkjob_tx_commit").await.unwrap();
        let db = test_db.db();
        assert_domain_indexes(db).await;

        let snapshot = sample_snapshot("snap-1", 2);
        let items = vec![
            sample_selection_item("si-1", "snap-1", "SO-1"),
            sample_selection_item("si-2", "snap-1", "SO-2"),
        ];

        let db_clone = db.clone();
        let snapshot_for_tx = snapshot.clone();
        let items_for_tx = items.clone();
        test_db
            .client()
            .with_transaction::<_, (), database::Error>(move |session| {
                Box::pin(async move {
                    db_clone
                        .bulk_job()
                        .create_snapshot_with_items(&snapshot_for_tx, items_for_tx, session)
                        .await?;
                    Ok(())
                })
            })
            .await
            .expect("事务提交应成功");

        let snapshot_found = db
            .bulk_selection_snapshots()
            .find_by_id(&snapshot.base.id, &mut NoTransaction)
            .await
            .unwrap();
        assert!(snapshot_found.is_some(), "事务提交后快照可见");
        let items_found = db
            .bulk_selection_items()
            .list_by_snapshot(&BulkSelectionSnapshotId::new("snap-1"), &mut NoTransaction)
            .await
            .unwrap();
        assert_eq!(items_found.len(), 2, "事务提交后冻结目标全部可见");
    })
}

#[tokio::test]
#[ignore]
async fn create_snapshot_with_items_rolls_back_on_item_conflict() {
    require_mongo!(async {
        let test_db = TestDb::new("bulkjob_tx_conflict").await.unwrap();
        let db = test_db.db();
        assert_domain_indexes(db).await;

        db.bulk_selection_snapshots()
            .create(&sample_snapshot("snap-0", 1), &mut NoTransaction)
            .await
            .unwrap();
        db.bulk_selection_items()
            .create(
                &sample_selection_item("si-0", "snap-0", "SO-1"),
                &mut NoTransaction,
            )
            .await
            .unwrap();

        let snapshot = sample_snapshot("snap-1", 1);
        let conflicting = vec![
            sample_selection_item("si-1", "snap-1", "SO-1"),
            sample_selection_item("si-2", "snap-1", "SO-1"),
        ];

        let db_clone = db.clone();
        let snapshot_for_tx = snapshot.clone();
        let result: std::result::Result<(), database::Error> = test_db
            .client()
            .with_transaction(move |session| {
                let conflicting = conflicting.clone();
                Box::pin(async move {
                    db_clone
                        .bulk_job()
                        .create_snapshot_with_items(&snapshot_for_tx, conflicting, session)
                        .await?;
                    Ok(())
                })
            })
            .await;
        assert!(
            matches!(result, Err(database::Error::DuplicateKey(_))),
            "逐项目标冲突必须整体回滚并透出 DuplicateKey，实际为 {result:?}"
        );

        let snapshot_found = db
            .bulk_selection_snapshots()
            .find_by_id(&snapshot.base.id, &mut NoTransaction)
            .await
            .unwrap();
        assert!(snapshot_found.is_none(), "冲突回滚后快照不得残留");
        let items_found = db
            .bulk_selection_items()
            .list_by_snapshot(&BulkSelectionSnapshotId::new("snap-1"), &mut NoTransaction)
            .await
            .unwrap();
        assert!(items_found.is_empty(), "冲突回滚后逐项不得残留");
    })
}

#[tokio::test]
#[ignore]
async fn create_job_with_items_no_transaction_leaves_partial_write() {
    require_mongo!(async {
        let test_db = TestDb::new("bulkjob_notx").await.unwrap();
        let db = test_db.db();
        assert_domain_indexes(db).await;

        db.background_job_items()
            .create(&sample_job_item("ji-0", "job-2", 1), &mut NoTransaction)
            .await
            .unwrap();

        let new_job = sample_job("job-2", "JOB-2025-002", "req-002");
        let conflicting_items = vec![sample_job_item("ji-1", "job-2", 1)];
        let error = db
            .bulk_job()
            .create_job_with_items(&new_job, conflicting_items, &mut NoTransaction)
            .await
            .expect_err("第二笔逐项写入冲突必须返回错误");
        assert!(
            matches!(error, database::Error::DuplicateKey(_)),
            "期望 DuplicateKey，实际为 {error:?}"
        );

        let job_found = db
            .background_jobs()
            .find_by_id(&new_job.base.id, &mut NoTransaction)
            .await
            .unwrap();
        assert!(
            job_found.is_some(),
            "NoTransaction 下第一笔已自动提交，留下半成品（方法注释已声明该行为）"
        );
    })
}
