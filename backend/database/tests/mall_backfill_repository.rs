//! 域 D31 `mall_backfill` 仓储集成测试（真实 MongoDB 单节点副本集）。
//!
//! 统一 `#[ignore]` + `ERP_TEST_MONGO_URI` 门控（test-support `require_mongo!`）：
//! 无数据库环境 `cargo test` 跳过；CI 与验收执行
//! `cargo test -p database --test mall_backfill_repository -- --include-ignored`。
//! 每个测试使用独立随机数据库名，结束自动 drop（TestDb）。

use database::repository::extensions::MallBackfillExt;
use database::{ensure_indexes, NoTransaction, Transactional};
use entities::common::time::Instant;
use entities::ids::{
    FileAssetId, InboxMessageId, MallConsumptionBackfillItemId, MallConsumptionBackfillJobId,
    MallConsumptionCutoverId, MallOrderFactId,
};
use entities::mall_backfill::{
    BackfillCostBasis, BackfillItemResult, BackfillJobStatus, MallConsumptionBackfillItem,
    MallConsumptionBackfillItemData, MallConsumptionBackfillJob, MallConsumptionBackfillJobData,
};
use entities::money::Amount;
use mongodb::Database;
use std::str::FromStr;
use test_support::{assert_indexes, require_mongo, TestDb};

/// 回填作业列表筛选条件类型（经 `MallBackfillExt` 关联类型跨 crate 可达）。
type MallConsumptionBackfillJobFilter = <Database as MallBackfillExt>::MallConsumptionBackfillJobFilter;
/// 回填明细列表筛选条件类型。
type MallConsumptionBackfillItemFilter = <Database as MallBackfillExt>::MallConsumptionBackfillItemFilter;

/// 构造可复用的回填作业实体。
fn sample_job(id: &str, mall_id: &str) -> MallConsumptionBackfillJob {
    MallConsumptionBackfillJob::new(
        MallConsumptionBackfillJobId::new(id),
        MallConsumptionBackfillJobData {
            mall_id: format!(" {mall_id} "),
            cutover_id: MallConsumptionCutoverId::new("cutover-1"),
            range_start: Instant::from_unix_secs(1_600_000_000),
            range_end: Instant::from_unix_secs(1_700_000_000),
            total_count: 100,
            total_amount: Amount::from_str("5000.00").unwrap(),
        },
    )
    .unwrap()
}

/// 构造可复用的回填明细实体。
fn sample_item(
    id: &str,
    job_id: &str,
    business_fact_key: &str,
    result: BackfillItemResult,
) -> MallConsumptionBackfillItem {
    MallConsumptionBackfillItem::new(
        MallConsumptionBackfillItemId::new(id),
        MallConsumptionBackfillItemData {
            job_id: MallConsumptionBackfillJobId::new(job_id),
            business_fact_key: business_fact_key.to_string(),
            source_event_reference: format!(" src-{id} "),
            inbox_message_id: InboxMessageId::new(format!("inbox-{id}")),
            mall_order_fact_id: match result {
                BackfillItemResult::New => Some(MallOrderFactId::new(format!("fact-{id}"))),
                _ => None,
            },
            result,
            cost_basis: BackfillCostBasis::Actual,
            error_code: match result {
                BackfillItemResult::Failed => Some(" E_9001 ".to_string()),
                _ => None,
            },
            error_detail: match result {
                BackfillItemResult::Failed => Some(" 事实键重复 ".to_string()),
                _ => None,
            },
        },
    )
    .unwrap()
}

/// 执行 `ensure_indexes` 并断言本域全部必需索引就位。
async fn assert_domain_indexes(db: &Database) {
    ensure_indexes(db).await.expect("ensure_indexes 应成功");
    assert_indexes(
        db,
        <Database as MallBackfillExt>::MALL_CONSUMPTION_BACKFILL_JOBS,
        &["idx_mall_consumption_backfill_jobs_status"],
    )
    .await
    .expect("mall_consumption_backfill_jobs 索引缺失");
    assert_indexes(
        db,
        <Database as MallBackfillExt>::MALL_CONSUMPTION_BACKFILL_ITEMS,
        &[
            "uk_mall_consumption_backfill_items_key",
            "idx_mall_consumption_backfill_items_result",
        ],
    )
    .await
    .expect("mall_consumption_backfill_items 索引缺失");
}

#[tokio::test]
#[ignore]
async fn job_create_update_soft_delete_restore_roundtrip() {
    require_mongo!(async {
        let test_db = TestDb::new("mback_job_crud").await.unwrap();
        let db = test_db.db();
        assert_domain_indexes(db).await;

        let mut job = sample_job("job-1", "mall-a");
        db.mall_consumption_backfill_jobs()
            .create(&job, &mut NoTransaction)
            .await
            .unwrap();

        let found = db
            .mall_consumption_backfill_jobs()
            .find_by_id(&job.base.id, &mut NoTransaction)
            .await
            .unwrap()
            .expect("创建后应可读回");
        assert_eq!(found.mall_id, "mall-a");
        assert_eq!(found.status, BackfillJobStatus::Pending);
        assert_eq!(found.total_amount, Amount::from_str("5000.00").unwrap());

        job.transition_to(BackfillJobStatus::Running).unwrap();
        job.update_progress(5, 60, 20, 10, 5, Some(FileAssetId::new("report-1")))
            .unwrap();
        db.mall_consumption_backfill_jobs()
            .update(&mut job, &mut NoTransaction)
            .await
            .unwrap();
        assert_eq!(job.base.version, 2, "乐观锁成功后 version 递增");
        assert_eq!(job.actual_count, 60);
        assert_eq!(job.report_file_id, Some(FileAssetId::new("report-1")));

        let mut stale = job.clone();
        job.transition_to(BackfillJobStatus::Completed).unwrap();
        db.mall_consumption_backfill_jobs()
            .update(&mut job, &mut NoTransaction)
            .await
            .unwrap();

        stale.transition_to(BackfillJobStatus::Failed).unwrap();
        let error = db
            .mall_consumption_backfill_jobs()
            .update(&mut stale, &mut NoTransaction)
            .await
            .expect_err("陈旧 version 更新必须被 CAS 拒绝");
        assert!(
            matches!(error, database::Error::OptimisticLockingError),
            "期望 OptimisticLockingError，实际为 {error:?}"
        );

        db.mall_consumption_backfill_jobs()
            .soft_delete(&mut job, &mut NoTransaction)
            .await
            .unwrap();
        assert!(
            db.mall_consumption_backfill_jobs()
                .find_by_id(&job.base.id, &mut NoTransaction)
                .await
                .unwrap()
                .is_none(),
            "软删除后按 ID 不可见"
        );

        db.mall_consumption_backfill_jobs()
            .restore(&mut job, &mut NoTransaction)
            .await
            .unwrap();
        assert!(
            db.mall_consumption_backfill_jobs()
                .find_by_id(&job.base.id, &mut NoTransaction)
                .await
                .unwrap()
                .is_some(),
            "恢复后按 ID 重新可见"
        );
    })
}

#[tokio::test]
#[ignore]
async fn backfill_item_key_uniqueness_surfaces_duplicate_key() {
    require_mongo!(async {
        let test_db = TestDb::new("mback_item_dup").await.unwrap();
        let db = test_db.db();
        assert_domain_indexes(db).await;

        let item = sample_item("bi-1", "job-1", "mall-a:PAYMENT:SO-9:v1", BackfillItemResult::New);
        db.mall_consumption_backfill_items()
            .create(&item, &mut NoTransaction)
            .await
            .unwrap();

        let duplicate = sample_item(
            "bi-2",
            "job-1",
            "mall-a:PAYMENT:SO-9:v1",
            BackfillItemResult::Duplicate,
        );
        let error = db
            .mall_consumption_backfill_items()
            .create(&duplicate, &mut NoTransaction)
            .await
            .expect_err("同一 (批次, 事实身份) 重复明细必须被唯一索引拒绝");
        assert!(
            matches!(error, database::Error::DuplicateKey(_)),
            "期望 DuplicateKey，实际为 {error:?}"
        );

        let same_key_other_job =
            sample_item("bi-3", "job-2", "mall-a:PAYMENT:SO-9:v1", BackfillItemResult::New);
        db.mall_consumption_backfill_items()
            .create(&same_key_other_job, &mut NoTransaction)
            .await
            .unwrap();
        let found = db
            .mall_consumption_backfill_items()
            .find_by_job_and_key(
                &MallConsumptionBackfillJobId::new("job-2"),
                "mall-a:PAYMENT:SO-9:v1",
                &mut NoTransaction,
            )
            .await
            .unwrap()
            .expect("不同批次同事实身份可共存");
        assert_eq!(found.base.id, "bi-3");
    })
}

#[tokio::test]
#[ignore]
async fn backfill_item_create_many_and_read_roundtrip() {
    require_mongo!(async {
        let test_db = TestDb::new("mback_item_bulk").await.unwrap();
        let db = test_db.db();
        assert_domain_indexes(db).await;

        let items = vec![
            sample_item("bi-1", "job-1", "k:PAYMENT:SO-1:v1", BackfillItemResult::New),
            sample_item("bi-2", "job-1", "k:PAYMENT:SO-2:v1", BackfillItemResult::New),
            sample_item("bi-3", "job-1", "k:PAYMENT:SO-3:v1", BackfillItemResult::Failed),
        ];
        db.mall_consumption_backfill_items()
            .create_many(items, &mut NoTransaction)
            .await
            .unwrap();

        let found = db
            .mall_consumption_backfill_items()
            .find_by_id("bi-1", &mut NoTransaction)
            .await
            .unwrap()
            .expect("批量创建后应可读回");
        assert_eq!(found.business_fact_key, "k:PAYMENT:SO-1:v1");
        assert_eq!(found.result, BackfillItemResult::New);
        assert_eq!(found.mall_order_fact_id, Some(MallOrderFactId::new("fact-bi-1")));

        let failed = db
            .mall_consumption_backfill_items()
            .find_by_id("bi-3", &mut NoTransaction)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(failed.error_code.as_deref(), Some("E_9001"));
        assert_eq!(failed.error_detail.as_deref(), Some("事实键重复"));
        assert!(failed.mall_order_fact_id.is_none());
    })
}

#[tokio::test]
#[ignore]
async fn job_projection_list_respects_filters_and_pagination() {
    require_mongo!(async {
        let test_db = TestDb::new("mback_job_list").await.unwrap();
        let db = test_db.db();
        assert_domain_indexes(db).await;

        let mut running = sample_job("job-1", "mall-a");
        running.transition_to(BackfillJobStatus::Running).unwrap();
        db.mall_consumption_backfill_jobs()
            .create(&running, &mut NoTransaction)
            .await
            .unwrap();
        db.mall_consumption_backfill_jobs()
            .create(&sample_job("job-2", "mall-b"), &mut NoTransaction)
            .await
            .unwrap();

        let filter = MallConsumptionBackfillJobFilter {
            mall_id: Some("mall-a".to_string()),
            status: Some(BackfillJobStatus::Running),
            page: 1,
            page_size: 10,
            sort_by: Some("range_start".to_string()),
            sort_ascending: true,
        };
        let page = db
            .mall_consumption_backfill_jobs()
            .search_backfill_jobs(&filter, &mut NoTransaction)
            .await
            .unwrap();
        assert_eq!(page.total, 1, "运行中且商城为 mall-a 只有一条");
        assert_eq!(page.items.len(), 1);
        let row = &page.items[0];
        assert_eq!(row.mall_id, "mall-a");
        assert_eq!(row.status, BackfillJobStatus::Running);
        assert_eq!(row.total_amount, Amount::from_str("5000.00").unwrap());
        assert_eq!(row.range_start, Instant::from_unix_secs(1_600_000_000));
        assert_eq!(row.range_end, Instant::from_unix_secs(1_700_000_000));
        assert!(row.version >= 1);

        let second_page = MallConsumptionBackfillJobFilter {
            page: 2,
            page_size: 1,
            ..filter
        };
        let empty = db
            .mall_consumption_backfill_jobs()
            .search_backfill_jobs(&second_page, &mut NoTransaction)
            .await
            .unwrap();
        assert_eq!(empty.items.len(), 0, "分页边界：第二页为空");
    })
}

#[tokio::test]
#[ignore]
async fn backfill_item_projection_list_respects_result_and_cost_basis_filters() {
    require_mongo!(async {
        let test_db = TestDb::new("mback_item_list").await.unwrap();
        let db = test_db.db();
        assert_domain_indexes(db).await;

        db.mall_consumption_backfill_items()
            .create_many(
                vec![
                    sample_item("bi-1", "job-1", "k:PAYMENT:SO-1:v1", BackfillItemResult::New),
                    sample_item("bi-2", "job-1", "k:PAYMENT:SO-2:v1", BackfillItemResult::New),
                    sample_item("bi-3", "job-1", "k:PAYMENT:SO-3:v1", BackfillItemResult::Failed),
                ],
                &mut NoTransaction,
            )
            .await
            .unwrap();

        let filter = MallConsumptionBackfillItemFilter {
            job_id: Some(MallConsumptionBackfillJobId::new("job-1")),
            result: Some(BackfillItemResult::New),
            cost_basis: Some(BackfillCostBasis::Actual),
            page: 1,
            page_size: 1,
            sort_by: Some("business_fact_key".to_string()),
            sort_ascending: true,
        };
        let page = db
            .mall_consumption_backfill_items()
            .search_backfill_items(&filter, &mut NoTransaction)
            .await
            .unwrap();
        assert_eq!(page.total, 2, "job-1 中新增且实际成本口径共两条");
        assert_eq!(page.items.len(), 1);
        let row = &page.items[0];
        assert_eq!(row.job_id, MallConsumptionBackfillJobId::new("job-1"));
        assert_eq!(row.business_fact_key, "k:PAYMENT:SO-1:v1");
        assert_eq!(row.result, BackfillItemResult::New);
        assert_eq!(row.cost_basis, BackfillCostBasis::Actual);
        assert_eq!(row.mall_order_fact_id, Some(MallOrderFactId::new("fact-bi-1")));
        assert_eq!(row.source_event_reference, "src-bi-1");
    })
}

#[tokio::test]
#[ignore]
async fn multi_step_writes_commits_atomically_inside_transaction() {
    require_mongo!(async {
        let test_db = TestDb::new("mback_tx_commit").await.unwrap();
        let db = test_db.db();
        assert_domain_indexes(db).await;

        let job = sample_job("job-1", "mall-a");
        let items = vec![
            sample_item("bi-1", "job-1", "k:PAYMENT:SO-1:v1", BackfillItemResult::New),
            sample_item("bi-2", "job-1", "k:PAYMENT:SO-2:v1", BackfillItemResult::New),
        ];

        let db_clone = db.clone();
        let job_for_tx = job.clone();
        let items_for_tx = items.clone();
        test_db
            .client()
            .with_transaction::<_, (), database::Error>(move |session| {
                Box::pin(async move {
                    db_clone
                        .mall_backfill()
                        .create_job_with_items(&job_for_tx, items_for_tx, session)
                        .await?;
                    Ok(())
                })
            })
            .await
            .expect("事务提交应成功");

        assert!(
            db.mall_consumption_backfill_jobs()
                .find_by_id(&job.base.id, &mut NoTransaction)
                .await
                .unwrap()
                .is_some(),
            "事务提交后作业必须可见"
        );
        assert!(
            db.mall_consumption_backfill_items()
                .find_by_id(&items[0].base.id, &mut NoTransaction)
                .await
                .unwrap()
                .is_some(),
            "事务提交后明细必须可见"
        );
    })
}

#[tokio::test]
#[ignore]
async fn transaction_abort_rolls_back_job_and_items() {
    require_mongo!(async {
        let test_db = TestDb::new("mback_tx_abort").await.unwrap();
        let db = test_db.db();
        assert_domain_indexes(db).await;

        let job = sample_job("job-1", "mall-a");
        let items = vec![sample_item(
            "bi-1",
            "job-1",
            "k:PAYMENT:SO-1:v1",
            BackfillItemResult::New,
        )];

        let db_clone = db.clone();
        let job_for_tx = job.clone();
        let items_for_tx = items.clone();
        let result: std::result::Result<(), database::Error> = test_db
            .client()
            .with_transaction(move |session| {
                Box::pin(async move {
                    db_clone
                        .mall_backfill()
                        .create_job_with_items(&job_for_tx, items_for_tx, session)
                        .await?;
                    Err(database::Error::OptimisticLockingError)
                })
            })
            .await;
        assert!(result.is_err(), "闭包返回错误必须整体回滚");

        assert!(
            db.mall_consumption_backfill_jobs()
                .find_by_id(&job.base.id, &mut NoTransaction)
                .await
                .unwrap()
                .is_none(),
            "回滚后作业不得残留"
        );
        assert!(
            db.mall_consumption_backfill_items()
                .find_by_id(&items[0].base.id, &mut NoTransaction)
                .await
                .unwrap()
                .is_none(),
            "回滚后明细不得残留"
        );
    })
}

#[tokio::test]
#[ignore]
async fn multi_step_conflict_rolls_back_whole_write() {
    require_mongo!(async {
        let test_db = TestDb::new("mback_tx_conflict").await.unwrap();
        let db = test_db.db();
        assert_domain_indexes(db).await;

        let existing = sample_item("bi-0", "job-1", "k:PAYMENT:SO-1:v1", BackfillItemResult::New);
        db.mall_consumption_backfill_items()
            .create(&existing, &mut NoTransaction)
            .await
            .unwrap();

        let job = sample_job("job-1", "mall-a");
        let items = vec![sample_item(
            "bi-9",
            "job-1",
            "k:PAYMENT:SO-1:v1",
            BackfillItemResult::Duplicate,
        )];

        let db_clone = db.clone();
        let job_for_tx = job.clone();
        let items_for_tx = items.clone();
        let result: std::result::Result<(), database::Error> = test_db
            .client()
            .with_transaction(move |session| {
                Box::pin(async move {
                    db_clone
                        .mall_backfill()
                        .create_job_with_items(&job_for_tx, items_for_tx, session)
                        .await?;
                    Ok(())
                })
            })
            .await;
        assert!(
            matches!(result, Err(database::Error::DuplicateKey(_))),
            "唯一冲突必须透出 DuplicateKey，实际为 {result:?}"
        );

        assert!(
            db.mall_consumption_backfill_jobs()
                .find_by_id(&job.base.id, &mut NoTransaction)
                .await
                .unwrap()
                .is_none(),
            "冲突回滚后作业不得残留"
        );
        assert!(
            db.mall_consumption_backfill_items()
                .find_by_id(&items[0].base.id, &mut NoTransaction)
                .await
                .unwrap()
                .is_none(),
            "冲突回滚后明细不得残留"
        );
    })
}
