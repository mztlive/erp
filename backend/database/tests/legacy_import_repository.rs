//! 域 D22 `legacy_import` 仓储集成测试（真实 MongoDB 单节点副本集）。
//!
//! 统一 `#[ignore]` + `ERP_TEST_MONGO_URI` 门控（test-support `require_mongo!`）：
//! 无数据库环境 `cargo test` 跳过；CI 与验收执行
//! `cargo test -p database --test legacy_import_repository -- --include-ignored`。
//! 每个测试使用独立随机数据库名，结束自动 drop（TestDb）。

use database::repository::extensions::LegacyImportExt;
use database::{ensure_indexes, NoTransaction, Transactional};
use entities::common::time::{BusinessDate, Instant};
use entities::ids::{
    LegacyImportBatchId, LegacyImportConfirmationId, LegacyImportRowId, SourceSystemId, WorkItemId,
};
use entities::legacy_import::{
    ConfirmationDecision, ConfirmationStatus, LegacyImportBatch, LegacyImportBatchData,
    LegacyImportBatchStatus, LegacyImportConfirmation, LegacyImportConfirmationData, LegacyImportRow,
    LegacyImportRowData, ParseStatus,
};
use mongodb::Database;
use test_support::{assert_indexes, require_mongo, TestDb};

/// 失败诊断 TTL 保留秒数（30 天，与 `indexes::legacy_import` 一致）。
const DIAGNOSTIC_RETENTION_SECONDS: i64 = 30 * 24 * 60 * 60;

/// 导入批次列表筛选条件类型（经 `LegacyImportExt` 关联类型跨 crate 可达）。
type LegacyImportBatchFilter = <Database as LegacyImportExt>::LegacyImportBatchFilter;
/// 导入行列表筛选条件类型。
type LegacyImportRowFilter = <Database as LegacyImportExt>::LegacyImportRowFilter;
/// 导入确认列表筛选条件类型。
type LegacyImportConfirmationFilter = <Database as LegacyImportExt>::LegacyImportConfirmationFilter;

/// 构造可复用的导入批次实体。
fn sample_batch(batch_no: &str) -> LegacyImportBatch {
    LegacyImportBatch::new(
        LegacyImportBatchId::new(format!("batch-{batch_no}")),
        LegacyImportBatchData {
            batch_no: batch_no.to_string(),
            source_system_id: SourceSystemId::new("sys-mall"),
            source_object_set: "客户,卡券销售".to_string(),
            baseline_date: BusinessDate::from_ymd(2026, 1, 1).unwrap(),
            import_rule_version: "v1".to_string(),
            source_file_hmac: Some(format!("hmac-{batch_no}")),
            status: LegacyImportBatchStatus::PendingValidation,
            total_rows: 2,
            success_rows: 0,
            failed_rows: 0,
            failure_code_summary: None,
            confirmation_status_summary: None,
        },
    )
    .unwrap()
}

/// 构造属于指定批次的可复用导入行。
fn sample_row(batch_id: &LegacyImportBatchId, row_key: &str) -> LegacyImportRow {
    LegacyImportRow::new(
        LegacyImportRowId::new(format!("row-{row_key}")),
        LegacyImportRowData {
            batch_id: batch_id.clone(),
            source_object_type: "卡券销售".to_string(),
            source_row_key: row_key.to_string(),
            normalized_payload_reference: format!("{{\"sell_order\":\"{row_key}\"}}"),
        },
    )
    .unwrap()
}

/// 构造属于指定批次的可复用确认事实。
fn sample_confirmation(batch_id: &LegacyImportBatchId, scope: &str) -> LegacyImportConfirmation {
    LegacyImportConfirmation::new(
        LegacyImportConfirmationId::new(format!("conf-{scope}")),
        LegacyImportConfirmationData {
            batch_id: batch_id.clone(),
            confirmation_scope: scope.to_string(),
            owner_role: "销售领导".to_string(),
            batch_version: 1,
            trial_version: 1,
            import_rule_version: "v1".to_string(),
            work_item_id: WorkItemId::new(format!("wi-{scope}")),
        },
    )
    .unwrap()
}

/// 执行 `ensure_indexes` 并断言本域全部必需索引就位。
async fn assert_domain_indexes(db: &Database) {
    ensure_indexes(db).await.expect("ensure_indexes 应成功");
    assert_indexes(
        db,
        <Database as LegacyImportExt>::LEGACY_IMPORT_BATCHES,
        &[
            "uk_legacy_import_batches_batch_no",
            "idx_legacy_import_batches_reimport_warning",
        ],
    )
    .await
    .expect("legacy_import_batches 索引缺失");
    assert_indexes(
        db,
        <Database as LegacyImportExt>::LEGACY_IMPORT_ROWS,
        &[
            "uk_legacy_import_rows_batch_identity",
            "idx_legacy_import_rows_process_queue",
            "idx_legacy_import_rows_batch_id_created",
            "ttl_legacy_import_rows_diagnostics_30d",
        ],
    )
    .await
    .expect("legacy_import_rows 索引缺失");
    assert_indexes(
        db,
        <Database as LegacyImportExt>::LEGACY_IMPORT_CONFIRMATIONS,
        &[
            "uk_legacy_import_confirmations_scope_trial",
            "uk_legacy_import_confirmations_work_item",
            "idx_legacy_import_confirmations_batch_status",
        ],
    )
    .await
    .expect("legacy_import_confirmations 索引缺失");
}

/// 断言失败诊断 TTL 索引携带 30 天 `expire_after_seconds` 选项。
async fn assert_diagnostic_ttl_index(db: &Database) {
    use futures_util::StreamExt;

    let mut cursor = db
        .collection::<mongodb::bson::Document>(<Database as LegacyImportExt>::LEGACY_IMPORT_ROWS)
        .list_indexes()
        .await
        .expect("listIndexes 应成功");
    let mut existing = Vec::new();
    while let Some(index) = cursor.next().await {
        existing.push(index.expect("索引描述应可读取"));
    }
    let ttl = existing
        .iter()
        .find(|index| {
            index.options.as_ref().and_then(|options| options.name.as_deref())
                == Some("ttl_legacy_import_rows_diagnostics_30d")
        })
        .expect("失败诊断 TTL 索引必须存在");
    let expire = ttl
        .options
        .as_ref()
        .and_then(|options| options.expire_after)
        .expect("TTL 索引必须声明 expire_after");
    assert_eq!(
        expire.as_secs() as i64,
        DIAGNOSTIC_RETENTION_SECONDS,
        "失败诊断保留 30 天"
    );
}

#[tokio::test]
#[ignore]
async fn create_update_soft_delete_restore_roundtrip() {
    require_mongo!(async {
        let test_db = TestDb::new("legacy_imp_crud").await.unwrap();
        let db = test_db.db();
        assert_domain_indexes(db).await;

        let mut batch = sample_batch("IMP-2026-001");
        db.legacy_import_batches()
            .create(&batch, &mut NoTransaction)
            .await
            .unwrap();
        assert_eq!(batch.base.version, 1);

        let found = db
            .legacy_import_batches()
            .find_by_id(&batch.base.id, &mut NoTransaction)
            .await
            .unwrap()
            .expect("创建后应可读回");
        assert_eq!(found.batch_no, "IMP-2026-001");
        assert_eq!(found.source_object_set, "客户,卡券销售");
        assert_eq!(found.total_rows, 2);

        batch.update_counts(2, 1, 1).unwrap();
        db.legacy_import_batches()
            .update(&mut batch, &mut NoTransaction)
            .await
            .unwrap();
        assert_eq!(batch.base.version, 2, "乐观锁成功后 version 递增");

        db.legacy_import_batches()
            .soft_delete(&mut batch, &mut NoTransaction)
            .await
            .unwrap();
        let after_delete = db
            .legacy_import_batches()
            .find_by_id(&batch.base.id, &mut NoTransaction)
            .await
            .unwrap();
        assert!(after_delete.is_none(), "软删除后按 ID 不可见");

        db.legacy_import_batches()
            .restore(&mut batch, &mut NoTransaction)
            .await
            .unwrap();
        let after_restore = db
            .legacy_import_batches()
            .find_by_id(&batch.base.id, &mut NoTransaction)
            .await
            .unwrap();
        assert!(after_restore.is_some(), "恢复后按 ID 重新可见");
    })
}

#[tokio::test]
#[ignore]
async fn batch_no_conflict_surfaces_duplicate_key() {
    require_mongo!(async {
        let test_db = TestDb::new("legacy_imp_dup_batch").await.unwrap();
        let db = test_db.db();
        assert_domain_indexes(db).await;

        let batch = sample_batch("IMP-2026-001");
        db.legacy_import_batches()
            .create(&batch, &mut NoTransaction)
            .await
            .unwrap();

        let duplicate = sample_batch("IMP-2026-001");
        let error = db
            .legacy_import_batches()
            .create(&duplicate, &mut NoTransaction)
            .await
            .expect_err("重复 batch_no 必须被唯一索引拒绝");
        assert!(
            matches!(error, database::Error::DuplicateKey(_)),
            "期望 DuplicateKey，实际为 {error:?}"
        );
    })
}

#[tokio::test]
#[ignore]
async fn row_identity_conflict_surfaces_duplicate_key() {
    require_mongo!(async {
        let test_db = TestDb::new("legacy_imp_dup_row").await.unwrap();
        let db = test_db.db();
        assert_domain_indexes(db).await;

        let batch = sample_batch("IMP-2026-002");
        db.legacy_import_batches()
            .create(&batch, &mut NoTransaction)
            .await
            .unwrap();
        let batch_id = batch.base.id.clone().into();
        let row = sample_row(&batch_id, "sell-001");
        db.legacy_import_rows()
            .create(&row, &mut NoTransaction)
            .await
            .unwrap();

        let duplicate = sample_row(&batch_id, "sell-001");
        let error = db
            .legacy_import_rows()
            .create(&duplicate, &mut NoTransaction)
            .await
            .expect_err("同批次同来源行身份重复必须被唯一索引拒绝");
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
        let test_db = TestDb::new("legacy_imp_optlock").await.unwrap();
        let db = test_db.db();

        let mut batch = sample_batch("IMP-2026-003");
        db.legacy_import_batches()
            .create(&batch, &mut NoTransaction)
            .await
            .unwrap();

        let mut stale = batch.clone();
        batch.update_counts(3, 2, 1).unwrap();
        db.legacy_import_batches()
            .update(&mut batch, &mut NoTransaction)
            .await
            .unwrap();

        stale.update_counts(3, 0, 3).unwrap();
        let error = db
            .legacy_import_batches()
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
async fn batch_projection_list_respects_filters_pagination_and_sort_whitelist() {
    require_mongo!(async {
        let test_db = TestDb::new("legacy_imp_list").await.unwrap();
        let db = test_db.db();
        assert_domain_indexes(db).await;

        let mut later = sample_batch("IMP-B");
        later.baseline_date = BusinessDate::from_ymd(2026, 2, 1).unwrap();
        let mut supplier = sample_batch("IMP-C");
        supplier.source_system_id = SourceSystemId::new("sys-supplier");
        db.legacy_import_batches()
            .create(&sample_batch("IMP-A"), &mut NoTransaction)
            .await
            .unwrap();
        db.legacy_import_batches()
            .create(&later, &mut NoTransaction)
            .await
            .unwrap();
        db.legacy_import_batches()
            .create(&supplier, &mut NoTransaction)
            .await
            .unwrap();

        let filter = LegacyImportBatchFilter {
            batch_no: Some("IMP".to_string()),
            source_system_id: Some(SourceSystemId::new("sys-mall")),
            status: None,
            baseline_date_from: None,
            baseline_date_to: None,
            page: 1,
            page_size: 1,
            sort_by: Some("baseline_date".to_string()),
            sort_ascending: true,
        };
        let page = db
            .legacy_import_batches()
            .search_legacy_import_batches(&filter, &mut NoTransaction)
            .await
            .unwrap();
        assert_eq!(page.total, 2, "sys-mall 下 batch_no 前缀 IMP 只有两条");
        assert_eq!(page.items.len(), 1);
        let row = &page.items[0];
        assert_eq!(row.batch_no, "IMP-A", "按 baseline_date 升序应先是 IMP-A");
        assert_eq!(row.baseline_date.to_string(), "2026-01-01");
        assert_eq!(row.status, LegacyImportBatchStatus::PendingValidation);
        assert_eq!(row.total_rows, 2);
        assert!(row.version >= 1);
        assert!(row.created_at > 0);

        let page2 = db
            .legacy_import_batches()
            .search_legacy_import_batches(
                &LegacyImportBatchFilter {
                    batch_no: Some("IMP".to_string()),
                    source_system_id: Some(SourceSystemId::new("sys-mall")),
                    status: None,
                    baseline_date_from: None,
                    baseline_date_to: None,
                    page: 2,
                    page_size: 1,
                    sort_by: Some("baseline_date".to_string()),
                    sort_ascending: true,
                },
                &mut NoTransaction,
            )
            .await
            .unwrap();
        assert_eq!(page2.items.len(), 1);
        assert_eq!(page2.items[0].batch_no, "IMP-B", "第二页应为 IMP-B");

        let unsorted = db
            .legacy_import_batches()
            .search_legacy_import_batches(
                &LegacyImportBatchFilter {
                    batch_no: Some("IMP".to_string()),
                    source_system_id: Some(SourceSystemId::new("sys-mall")),
                    status: None,
                    baseline_date_from: None,
                    baseline_date_to: None,
                    page: 1,
                    page_size: 1,
                    sort_by: Some("id".to_string()),
                    sort_ascending: true,
                },
                &mut NoTransaction,
            )
            .await
            .unwrap();
        assert_eq!(unsorted.total, 2, "非法排序字段回退 created_at 不报错");
    })
}

#[tokio::test]
#[ignore]
async fn row_search_filters_regex_and_batch_in_query() {
    require_mongo!(async {
        let test_db = TestDb::new("legacy_imp_row_list").await.unwrap();
        let db = test_db.db();
        assert_domain_indexes(db).await;

        let batch_a = sample_batch("IMP-2026-010");
        db.legacy_import_batches()
            .create(&batch_a, &mut NoTransaction)
            .await
            .unwrap();
        let batch_a_id: LegacyImportBatchId = batch_a.base.id.clone().into();
        let batch_b = sample_batch("IMP-2026-011");
        db.legacy_import_batches()
            .create(&batch_b, &mut NoTransaction)
            .await
            .unwrap();

        let mut row1 = sample_row(&batch_a_id, "sell-001");
        row1.mark_parse_result(ParseStatus::Valid, None, None).unwrap();
        db.legacy_import_rows()
            .create(&row1, &mut NoTransaction)
            .await
            .unwrap();
        let mut row2 = sample_row(&batch_a_id, "sell-002");
        row2.mark_parse_result(
            ParseStatus::Invalid,
            Some("AMOUNT_NOT_CONSERVED".to_string()),
            None,
        )
        .unwrap();
        db.legacy_import_rows()
            .create(&row2, &mut NoTransaction)
            .await
            .unwrap();
        db.legacy_import_rows()
            .create(
                &sample_row(&batch_b.base.id.clone().into(), "sell-003"),
                &mut NoTransaction,
            )
            .await
            .unwrap();

        let filter = LegacyImportRowFilter {
            batch_id: Some(batch_a_id.clone()),
            parse_status: None,
            mapping_status: None,
            import_status: None,
            source_row_key: Some("SELL-00".to_string()),
            page: 1,
            page_size: 20,
            sort_by: Some("source_row_key".to_string()),
            sort_ascending: true,
        };
        let page = db
            .legacy_import_rows()
            .search_legacy_import_rows(&filter, &mut NoTransaction)
            .await
            .unwrap();
        assert_eq!(page.total, 2, "regex 字面量忽略大小写命中 sell-001/002");
        assert_eq!(page.items[0].source_row_key, "sell-001");
        assert_eq!(page.items[0].parse_status, ParseStatus::Valid);
        assert_eq!(page.items[1].error_code.as_deref(), Some("AMOUNT_NOT_CONSERVED"));

        let by_ids = db
            .legacy_import_rows()
            .find_rows_by_batch_ids(
                &[batch_a_id.clone(), batch_b.base.id.clone().into()],
                &mut NoTransaction,
            )
            .await
            .unwrap();
        assert_eq!(by_ids.len(), 3, "$in 一次取回两个批次的全部行");
        assert_eq!(
            db.legacy_import_rows()
                .count_rows_by_batch(&batch_a_id, &mut NoTransaction)
                .await
                .unwrap(),
            2
        );
    })
}

#[tokio::test]
#[ignore]
async fn confirmation_search_and_work_item_lookup() {
    require_mongo!(async {
        let test_db = TestDb::new("legacy_imp_conf").await.unwrap();
        let db = test_db.db();
        assert_domain_indexes(db).await;

        let batch = sample_batch("IMP-2026-020");
        db.legacy_import_batches()
            .create(&batch, &mut NoTransaction)
            .await
            .unwrap();
        let batch_id: LegacyImportBatchId = batch.base.id.clone().into();

        let mut sales = sample_confirmation(&batch_id, "sales");
        let at = Instant::from_unix_secs(1_700_000_000);
        sales
            .decide(
                ConfirmationDecision::ConfirmScope,
                "运营-张三".to_string(),
                at,
                None,
                None,
            )
            .unwrap();
        db.legacy_import_confirmations()
            .create(&sales, &mut NoTransaction)
            .await
            .unwrap();
        let mut finance = sample_confirmation(&batch_id, "finance");
        finance
            .invalidate(LegacyImportConfirmationId::new("conf-sales-2"), at)
            .unwrap();
        db.legacy_import_confirmations()
            .create(&finance, &mut NoTransaction)
            .await
            .unwrap();

        let by_work_item = db
            .legacy_import_confirmations()
            .find_by_work_item(&WorkItemId::new("wi-sales"), &mut NoTransaction)
            .await
            .unwrap()
            .expect("按正式任务应找到确认事实");
        assert_eq!(by_work_item.confirmation_scope, "sales");
        assert_eq!(by_work_item.status, ConfirmationStatus::Confirmed);

        let by_trial = db
            .legacy_import_confirmations()
            .find_by_batch_scope_trial(&batch_id, "finance", 1, &mut NoTransaction)
            .await
            .unwrap()
            .expect("按批次+范围+试算版本应命中");
        assert_eq!(by_trial.status, ConfirmationStatus::Invalidated);

        let filter = LegacyImportConfirmationFilter {
            batch_id: Some(batch_id.clone()),
            confirmation_scope: None,
            status: None,
            page: 1,
            page_size: 20,
            sort_by: Some("trial_version".to_string()),
            sort_ascending: true,
        };
        let page = db
            .legacy_import_confirmations()
            .search_legacy_import_confirmations(&filter, &mut NoTransaction)
            .await
            .unwrap();
        assert_eq!(page.total, 2);
        assert_eq!(page.items[0].confirmation_scope, "sales");
        assert_eq!(page.items[0].decision, Some(ConfirmationDecision::ConfirmScope));
        assert_eq!(page.items[1].status, ConfirmationStatus::Invalidated);
    })
}

#[tokio::test]
#[ignore]
async fn reimport_warning_candidates_query() {
    require_mongo!(async {
        let test_db = TestDb::new("legacy_imp_warn").await.unwrap();
        let db = test_db.db();
        assert_domain_indexes(db).await;

        db.legacy_import_batches()
            .create(&sample_batch("IMP-2026-030"), &mut NoTransaction)
            .await
            .unwrap();
        db.legacy_import_batches()
            .create(&sample_batch("IMP-2026-031"), &mut NoTransaction)
            .await
            .unwrap();

        let candidates = db
            .legacy_import_batches()
            .find_reimport_warning_candidates(
                "客户,卡券销售",
                BusinessDate::from_ymd(2026, 1, 1).unwrap(),
                "hmac-IMP-2026-030",
                &mut NoTransaction,
            )
            .await
            .unwrap();
        assert_eq!(candidates.len(), 1, "HMAC 不同不算重复导入");
        assert_eq!(candidates[0].batch_no, "IMP-2026-030");

        let same_hmac = db
            .legacy_import_batches()
            .find_reimport_warning_candidates(
                "客户,卡券销售",
                BusinessDate::from_ymd(2026, 1, 1).unwrap(),
                "hmac-IMP-2026-031",
                &mut NoTransaction,
            )
            .await
            .unwrap();
        assert_eq!(same_hmac.len(), 1);
        assert_eq!(same_hmac[0].batch_no, "IMP-2026-031");
    })
}

#[tokio::test]
#[ignore]
async fn ttl_index_declares_thirty_day_expiration() {
    require_mongo!(async {
        let test_db = TestDb::new("legacy_imp_ttl").await.unwrap();
        let db = test_db.db();
        assert_domain_indexes(db).await;
        assert_diagnostic_ttl_index(db).await;
    })
}

#[tokio::test]
#[ignore]
async fn multi_step_batch_with_rows_commits_atomically_inside_transaction() {
    require_mongo!(async {
        let test_db = TestDb::new("legacy_imp_tx_commit").await.unwrap();
        let db = test_db.db();
        assert_domain_indexes(db).await;

        let batch = sample_batch("IMP-2026-040");
        let rows = vec![
            sample_row(&batch.base.id.clone().into(), "sell-001"),
            sample_row(&batch.base.id.clone().into(), "sell-002"),
        ];

        let db_clone = db.clone();
        let batch_for_tx = batch.clone();
        let rows_for_tx = rows.clone();
        test_db
            .client()
            .with_transaction::<_, (), database::Error>(move |session| {
                Box::pin(async move {
                    db_clone
                        .legacy_import()
                        .create_batch_with_rows(&batch_for_tx, &rows_for_tx, session)
                        .await?;
                    Ok(())
                })
            })
            .await
            .expect("事务提交应成功");

        let batch_found = db
            .legacy_import_batches()
            .find_by_id(&batch.base.id, &mut NoTransaction)
            .await
            .unwrap();
        assert!(batch_found.is_some(), "事务提交后批次必须可见");
        let rows_found = db
            .legacy_import_rows()
            .find_rows_by_batch_ids(&[batch.base.id.clone().into()], &mut NoTransaction)
            .await
            .unwrap();
        assert_eq!(rows_found.len(), 2, "事务提交后全部行必须可见");
    })
}

#[tokio::test]
#[ignore]
async fn multi_step_abort_rolls_back_both_collections() {
    require_mongo!(async {
        let test_db = TestDb::new("legacy_imp_tx_abort").await.unwrap();
        let db = test_db.db();
        assert_domain_indexes(db).await;

        let batch = sample_batch("IMP-2026-041");
        let rows = vec![
            sample_row(&batch.base.id.clone().into(), "sell-001"),
            sample_row(&batch.base.id.clone().into(), "sell-002"),
        ];

        let db_clone = db.clone();
        let batch_for_tx = batch.clone();
        let rows_for_tx = rows.clone();
        let result: std::result::Result<(), database::Error> = test_db
            .client()
            .with_transaction(move |session| {
                Box::pin(async move {
                    db_clone
                        .legacy_import()
                        .create_batch_with_rows(&batch_for_tx, &rows_for_tx, session)
                        .await?;
                    Err(database::Error::OptimisticLockingError)
                })
            })
            .await;
        assert!(result.is_err(), "闭包返回错误必须整体回滚");

        let batch_found = db
            .legacy_import_batches()
            .find_by_id(&batch.base.id, &mut NoTransaction)
            .await
            .unwrap();
        assert!(batch_found.is_none(), "回滚后批次不得残留");
        let rows_found = db
            .legacy_import_rows()
            .find_rows_by_batch_ids(&[batch.base.id.clone().into()], &mut NoTransaction)
            .await
            .unwrap();
        assert!(rows_found.is_empty(), "回滚后行不得残留");
    })
}

#[tokio::test]
#[ignore]
async fn multi_step_no_transaction_writes_both_independently() {
    require_mongo!(async {
        let test_db = TestDb::new("legacy_imp_notx").await.unwrap();
        let db = test_db.db();
        assert_domain_indexes(db).await;

        let batch = sample_batch("IMP-2026-042");
        let rows = vec![
            sample_row(&batch.base.id.clone().into(), "sell-001"),
            sample_row(&batch.base.id.clone().into(), "sell-002"),
        ];

        db.legacy_import()
            .create_batch_with_rows(&batch, &rows, &mut NoTransaction)
            .await
            .expect("NoTransaction 下两笔写入各自自动提交，应全部成功");

        let batch_found = db
            .legacy_import_batches()
            .find_by_id(&batch.base.id, &mut NoTransaction)
            .await
            .unwrap();
        assert!(batch_found.is_some());
        let rows_found = db
            .legacy_import_rows()
            .find_rows_by_batch_ids(&[batch.base.id.clone().into()], &mut NoTransaction)
            .await
            .unwrap();
        assert_eq!(rows_found.len(), 2, "非事务执行器写入行为可预期：全部落盘");
    })
}

#[tokio::test]
#[ignore]
async fn confirmation_scope_trial_conflict_surfaces_duplicate_key() {
    require_mongo!(async {
        let test_db = TestDb::new("legacy_imp_conf_dup").await.unwrap();
        let db = test_db.db();
        assert_domain_indexes(db).await;

        let batch = sample_batch("IMP-2026-050");
        db.legacy_import_batches()
            .create(&batch, &mut NoTransaction)
            .await
            .unwrap();
        let batch_id: LegacyImportBatchId = batch.base.id.clone().into();

        let confirmation = sample_confirmation(&batch_id, "sales");
        db.legacy_import_confirmations()
            .create(&confirmation, &mut NoTransaction)
            .await
            .unwrap();

        let duplicate = sample_confirmation(&batch_id, "sales");
        let error = db
            .legacy_import_confirmations()
            .create(&duplicate, &mut NoTransaction)
            .await
            .expect_err("同一批次+范围+试算版本重复必须被唯一索引拒绝");
        assert!(
            matches!(error, database::Error::DuplicateKey(_)),
            "期望 DuplicateKey，实际为 {error:?}"
        );

        let mut duplicate_work_item = sample_confirmation(&batch_id, "warehouse");
        duplicate_work_item.work_item_id = confirmation.work_item_id.clone();
        let error2 = db
            .legacy_import_confirmations()
            .create(&duplicate_work_item, &mut NoTransaction)
            .await
            .expect_err("同一 work_item 重复必须被唯一索引拒绝");
        assert!(
            matches!(error2, database::Error::DuplicateKey(_)),
            "期望 DuplicateKey，实际为 {error2:?}"
        );
    })
}
