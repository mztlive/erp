//! Core / Access / WorkItem 分层整改的真实 MongoDB 验收。

use std::str::FromStr;

use database::{
    ensure_indexes, BackgroundJobRegistration, BulkJobExt, CardBaselineRegistration, CardInstanceExt,
    NoTransaction, PayableExt, ReceivableExt, Transactional,
};
use entities::bulk_job::{
    BackgroundJobAggregate, BackgroundJobAggregateData, BackgroundJobItemDraft, JobType,
};
use entities::card_instance::{CardSourceType, MallCardBaselineAggregate, MallCardInstanceData};
use entities::common::time::{BusinessDate, Instant};
use entities::ids::{
    BackgroundJobId, BackgroundJobItemId, ExternalIdentityMapId, MallBalanceSnapshotId, MallCardInstanceId,
    PayableAccountId, PayableEntryId, ReceivableAccountId, ReceivableEntryId, SalesOrderId,
    SalesOrderRevisionId,
};
use entities::money::Amount;
use entities::payable::{
    EntryDirection as PayableDirection, PayableEntry, PayableEntryData, PayableEntryType,
};
use entities::receivable::{
    EntryDirection as ReceivableDirection, ReceivableEntry, ReceivableEntryData, ReceivableEntryType,
};
use mongodb::bson::{doc, Bson, Document};
use mongodb::Database;
use test_support::{require_mongo, TestDb};

#[tokio::test]
#[ignore = "需要 ERP_TEST_MONGO_URI 指向 MongoDB 副本集"]
async fn card_baseline_unique_competition_is_replayable_and_atomic() {
    require_mongo!(async {
        let fixture = TestDb::new("core_card_baseline")
            .await
            .expect("测试数据库创建失败");
        ensure_indexes(fixture.db()).await.expect("索引创建失败");

        let first = card_baseline("card-1", "snapshot-1", "100.00");
        let second = card_baseline("card-2", "snapshot-2", "100.00");
        assert!(first.0.same_baseline_as(&second.0));

        let db_first = fixture.db().clone();
        let client_first = db_first.client().clone();
        let db_second = fixture.db().clone();
        let client_second = db_second.client().clone();
        let first_for_tx = first.clone();
        let second_for_tx = second.clone();
        let create_first = client_first.with_transaction(move |session| {
            Box::pin(async move {
                db_first
                    .card_instance()
                    .create_card_instance_with_initial_snapshot(&first_for_tx.0, &first_for_tx.1, session)
                    .await
            })
        });
        let create_second = client_second.with_transaction(move |session| {
            Box::pin(async move {
                db_second
                    .card_instance()
                    .create_card_instance_with_initial_snapshot(&second_for_tx.0, &second_for_tx.1, session)
                    .await
            })
        });

        let (first_result, second_result) = tokio::join!(create_first, create_second);
        assert_eq!(
            usize::from(first_result.is_ok()) + usize::from(second_result.is_ok()),
            1,
            "并发唯一竞争必须恰有一个事务成功：first={first_result:?}, second={second_result:?}"
        );
        let loser = if first_result.is_err() {
            &first.0
        } else {
            &second.0
        };
        let replay = fixture
            .db()
            .card_instance()
            .registration_by_identity(loser, &mut NoTransaction)
            .await
            .expect("失败事务退出后的基线复核失败")
            .expect("唯一竞争后必须存在胜者");
        assert!(matches!(replay, CardBaselineRegistration::ExistingSame(_)));
        assert_eq!(count(fixture.db(), "mall_card_instances").await, 1);
        assert_eq!(count(fixture.db(), "mall_balance_snapshots").await, 1);

        let conflict = card_baseline("card-conflict", "snapshot-conflict", "101.00");
        let conflict_result = fixture
            .db()
            .card_instance()
            .registration_by_identity(&conflict.0, &mut NoTransaction)
            .await
            .expect("冲突基线复核失败")
            .expect("冲突基线必须命中既有身份");
        assert!(matches!(
            conflict_result,
            CardBaselineRegistration::ExistingConflict(_)
        ));
    });
}

#[tokio::test]
#[ignore = "需要 ERP_TEST_MONGO_URI 指向 MongoDB 副本集"]
async fn background_job_unique_competition_checks_fingerprint_and_legacy_rows() {
    require_mongo!(async {
        let fixture = TestDb::new("core_background_job")
            .await
            .expect("测试数据库创建失败");
        ensure_indexes(fixture.db()).await.expect("索引创建失败");

        let first = background_job("job-1", "item-1", "JOB-001", "request-1", "object-1");
        let second = background_job("job-2", "item-2", "JOB-001", "request-1", "object-1");
        assert_eq!(first.0.request_fingerprint, second.0.request_fingerprint);

        let db_first = fixture.db().clone();
        let client_first = db_first.client().clone();
        let db_second = fixture.db().clone();
        let client_second = db_second.client().clone();
        let first_for_tx = first.clone();
        let second_for_tx = second.clone();
        let create_first = client_first.with_transaction(move |session| {
            Box::pin(async move {
                db_first
                    .bulk_job()
                    .create_job_with_items(&first_for_tx.0, first_for_tx.1, session)
                    .await
            })
        });
        let create_second = client_second.with_transaction(move |session| {
            Box::pin(async move {
                db_second
                    .bulk_job()
                    .create_job_with_items(&second_for_tx.0, second_for_tx.1, session)
                    .await
            })
        });

        let (first_result, second_result) = tokio::join!(create_first, create_second);
        assert_eq!(
            usize::from(first_result.is_ok()) + usize::from(second_result.is_ok()),
            1,
            "并发唯一竞争必须恰有一个事务成功：first={first_result:?}, second={second_result:?}"
        );
        let loser = if first_result.is_err() {
            &first.0
        } else {
            &second.0
        };
        let replay = fixture
            .db()
            .bulk_job()
            .registration_by_request_id(loser, &mut NoTransaction)
            .await
            .expect("失败事务退出后的任务复核失败")
            .expect("唯一竞争后必须存在胜者");
        assert!(matches!(replay, BackgroundJobRegistration::ReplaySame(_)));
        assert_eq!(count(fixture.db(), "background_jobs").await, 1);
        assert_eq!(count(fixture.db(), "background_job_items").await, 1);

        let different = background_job(
            "job-different",
            "item-different",
            "JOB-001",
            "request-1",
            "object-2",
        );
        let conflict = fixture
            .db()
            .bulk_job()
            .registration_by_request_id(&different.0, &mut NoTransaction)
            .await
            .expect("异载荷任务复核失败")
            .expect("异载荷必须命中既有请求身份");
        assert!(matches!(
            conflict,
            BackgroundJobRegistration::ConflictDifferentPayload(_)
        ));

        let legacy = background_job(
            "legacy-stored",
            "legacy-stored-item",
            "JOB-LEGACY",
            "request-legacy",
            "legacy-object",
        );
        let mut legacy_document =
            mongodb::bson::serialize_to_document(&legacy.0).expect("历史任务序列化失败");
        legacy_document.remove("request_fingerprint");
        fixture
            .db()
            .collection::<Document>("background_jobs")
            .insert_one(legacy_document)
            .await
            .expect("历史无指纹任务写入失败");
        let requested = background_job(
            "legacy-requested",
            "legacy-requested-item",
            "JOB-LEGACY-REQUESTED",
            "request-legacy",
            "legacy-object",
        );
        let legacy_result = fixture
            .db()
            .bulk_job()
            .registration_by_request_id(&requested.0, &mut NoTransaction)
            .await
            .expect("历史任务复核失败")
            .expect("历史任务必须命中请求身份");
        assert!(matches!(
            legacy_result,
            BackgroundJobRegistration::ConflictDifferentPayload(_)
        ));
    });
}

#[tokio::test]
#[ignore = "需要 ERP_TEST_MONGO_URI 指向 MongoDB 副本集"]
async fn minimum_due_date_aggregation_matches_contract_and_uses_indexes() {
    require_mongo!(async {
        let fixture = TestDb::new("core_due_date_aggregation")
            .await
            .expect("测试数据库创建失败");
        ensure_indexes(fixture.db()).await.expect("索引创建失败");

        for entry in [
            payable_entry("pe-1", "pa-1", 1, "2026-09-30"),
            payable_entry("pe-2", "pa-1", 2, "2026-09-10"),
            payable_entry("pe-3", "pa-2", 1, "2026-10-01"),
        ] {
            fixture
                .db()
                .payable_entries()
                .create(&entry, &mut NoTransaction)
                .await
                .expect("应付分录写入失败");
        }
        let payable = fixture
            .db()
            .payable_entries()
            .minimum_due_dates_by_accounts(
                &[
                    PayableAccountId::new("pa-1"),
                    PayableAccountId::new("pa-2"),
                    PayableAccountId::new("pa-empty"),
                ],
                &mut NoTransaction,
            )
            .await
            .expect("应付最早到期日聚合失败");
        assert_eq!(payable.len(), 2);
        assert_eq!(payable["pa-1"], BusinessDate::from_str("2026-09-10").unwrap());
        assert_eq!(payable["pa-2"], BusinessDate::from_str("2026-10-01").unwrap());
        assert!(!payable.contains_key("pa-empty"));

        for entry in [
            receivable_entry(
                "re-1",
                "ra-1",
                1,
                "2026-09-20",
                ReceivableEntryType::Original,
                ReceivableDirection::Increase,
            ),
            receivable_entry(
                "re-2",
                "ra-1",
                2,
                "2026-09-01",
                ReceivableEntryType::SalesChangeDelta,
                ReceivableDirection::Decrease,
            ),
            receivable_entry(
                "re-3",
                "ra-1",
                3,
                "2026-09-15",
                ReceivableEntryType::SalesChangeDelta,
                ReceivableDirection::Increase,
            ),
        ] {
            fixture
                .db()
                .receivable_entries()
                .create(&entry, &mut NoTransaction)
                .await
                .expect("应收分录写入失败");
        }
        let receivable = fixture
            .db()
            .receivable_entries()
            .minimum_increase_due_dates_by_accounts(
                &[
                    ReceivableAccountId::new("ra-1"),
                    ReceivableAccountId::new("ra-empty"),
                ],
                &mut NoTransaction,
            )
            .await
            .expect("应收最早到期日聚合失败");
        assert_eq!(receivable.len(), 1);
        assert_eq!(receivable["ra-1"], BusinessDate::from_str("2026-09-15").unwrap());
        assert!(!receivable.contains_key("ra-empty"));

        let receivable_explain = fixture
            .db()
            .run_command(doc! {
                "explain": {
                    "aggregate": "receivable_entries",
                    "pipeline": [
                        { "$match": {
                            "receivable_account_id": { "$in": ["ra-1"] },
                            "direction": "increase",
                        }},
                        { "$group": {
                            "_id": "$receivable_account_id",
                            "due_date": { "$min": "$due_date" },
                        }},
                    ],
                    "cursor": {},
                    "hint": "idx_receivable_entries_account_due",
                },
                "verbosity": "executionStats",
            })
            .await
            .expect("应收聚合 explain 失败");
        assert_explain_uses_index(&receivable_explain, "idx_receivable_entries_account_due");

        let scope_explain = fixture
            .db()
            .run_command(doc! {
                "explain": {
                    "find": "data_scopes",
                    "filter": {
                        "subject_type": "role",
                        "subject_id": "role-1",
                    },
                    "limit": 1_i64,
                    "hint": "uk_data_scopes_subject_scope",
                },
                "verbosity": "executionStats",
            })
            .await
            .expect("DataScope exists explain 失败");
        assert_explain_uses_index(&scope_explain, "uk_data_scopes_subject_scope");
        let total_examined = numeric_field(
            scope_explain
                .get_document("executionStats")
                .expect("DataScope explain 缺少 executionStats"),
            "totalDocsExamined",
        );
        assert!(
            total_examined <= 1,
            "exists 查询最多读取一条，实际 {total_examined}"
        );
    });
}

fn card_baseline(
    instance_id: &str,
    snapshot_id: &str,
    initial_balance: &str,
) -> (
    entities::card_instance::MallCardInstance,
    entities::card_instance::MallBalanceSnapshot,
) {
    MallCardBaselineAggregate::new(
        MallCardInstanceId::new(instance_id),
        MallBalanceSnapshotId::new(snapshot_id),
        MallCardInstanceData {
            mall_id: "mall-1".to_string(),
            opaque_instance_ref: "opaque-1".to_string(),
            origin_sales_order_source_identity_id: ExternalIdentityMapId::new("external-1"),
            origin_sales_order_id: SalesOrderId::new("sales-order-1"),
            origin_sales_order_revision_id: SalesOrderRevisionId::new("sales-order-revision-1"),
            source_baseline_version: Some("baseline-v1".to_string()),
            initial_balance: Amount::from_str(initial_balance).unwrap(),
            baseline_at: Instant::from_unix_secs(1_788_112_000),
            source_type: CardSourceType::Realtime,
        },
    )
    .expect("卡基线聚合构造失败")
    .into_parts()
}

fn background_job(
    job_id: &str,
    item_id: &str,
    job_no: &str,
    request_id: &str,
    object_id: &str,
) -> (
    entities::bulk_job::BackgroundJob,
    Vec<entities::bulk_job::BackgroundJobItem>,
) {
    BackgroundJobAggregate::new(
        BackgroundJobId::new(job_id),
        BackgroundJobAggregateData {
            job_no: job_no.to_string(),
            job_type: JobType::Batch,
            domain_job_type: Some("sales_order".to_string()),
            domain_job_id: Some("batch-1".to_string()),
            selection_snapshot_id: None,
            requested_by: "tester".to_string(),
            request_id: request_id.to_string(),
            input_file_asset_id: None,
            result_file_asset_id: None,
            declared_total_count: 1,
        },
        vec![BackgroundJobItemDraft {
            id: BackgroundJobItemId::new(item_id),
            object_type: Some("sales_order".to_string()),
            object_id: Some(object_id.to_string()),
            expected_version: Some("1".to_string()),
            expected_hash: Some("hash-1".to_string()),
            worksheet_name: None,
            source_row_no: None,
            source_column_name: None,
        }],
    )
    .expect("后台任务聚合构造失败")
    .into_parts()
}

fn payable_entry(id: &str, account_id: &str, sequence: u32, due_date: &str) -> PayableEntry {
    PayableEntry::new(
        PayableEntryId::new(id),
        PayableEntryData {
            payable_account_id: PayableAccountId::new(account_id),
            entry_type: PayableEntryType::Original,
            direction: PayableDirection::Increase,
            amount: Amount::from_str("100.00").unwrap(),
            due_date: BusinessDate::from_str(due_date).unwrap(),
            source_fact_type: "PURCHASE_ORDER".to_string(),
            source_document_id: format!("po-{sequence}"),
            source_revision_id: format!("po-revision-{sequence}"),
            source_sequence: sequence,
            posted_at: Instant::from_unix_secs(1_788_112_000 + i64::from(sequence)),
        },
    )
    .expect("应付分录构造失败")
}

fn receivable_entry(
    id: &str,
    account_id: &str,
    sequence: u32,
    due_date: &str,
    entry_type: ReceivableEntryType,
    direction: ReceivableDirection,
) -> ReceivableEntry {
    ReceivableEntry::new(
        ReceivableEntryId::new(id),
        ReceivableEntryData {
            receivable_account_id: ReceivableAccountId::new(account_id),
            entry_type,
            direction,
            amount: Amount::from_str("100.00").unwrap(),
            due_date: BusinessDate::from_str(due_date).unwrap(),
            source_fact_type: "SALES_ORDER".to_string(),
            source_document_id: format!("so-{sequence}"),
            source_revision_id: format!("so-revision-{sequence}"),
            source_sequence: sequence,
            posted_at: Instant::from_unix_secs(1_788_112_000 + i64::from(sequence)),
        },
    )
    .expect("应收分录构造失败")
}

async fn count(db: &Database, collection: &str) -> u64 {
    db.collection::<Document>(collection)
        .count_documents(doc! {})
        .await
        .expect("集合计数失败")
}

fn assert_explain_uses_index(explain: &Document, index_name: &str) {
    let rendered = format!("{explain:?}");
    assert!(rendered.contains("IXSCAN"), "explain 未使用 IXSCAN：{rendered}");
    assert!(
        rendered.contains(index_name),
        "explain 未命中索引 {index_name}：{rendered}"
    );
}

fn numeric_field(document: &Document, field: &str) -> i64 {
    match document
        .get(field)
        .unwrap_or_else(|| panic!("缺少数值字段 {field}"))
    {
        Bson::Int32(value) => i64::from(*value),
        Bson::Int64(value) => *value,
        value => panic!("字段 {field} 不是整数：{value:?}"),
    }
}
