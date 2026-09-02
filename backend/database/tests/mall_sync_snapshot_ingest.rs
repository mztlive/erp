//! INT-R16 商城销售单快照落盘批量范围、批量写入与单调水位的真实 MongoDB 验收。
//!
//! 覆盖：空输入不访问；请求内重复键去重后批量 exact；latest 取最大版本时间；
//! 缺失来源单不出现；软删除排除；0/1/多条 insert_many；等时唯一键冲突；
//! 混合页 exact 重复 + 新兄弟只落新行；水位唯一索引重复值门禁；
//! 并发同/不同版本在 E11000 后换新会话重放，旧版本不得成为 latest；
//! exact / latest `$or` / 水位 CAS 的代表性 explain 命中索引。

use database::{ensure_indexes, MallSyncExt, NoTransaction, Transactional};
use entities::common::time::Instant;
use entities::ids::{MallSalesOrderSnapshotId, MallSalesSyncJobId, SourceSystemId};
use entities::mall_sync::{
    ExternalOrderKey, MallSalesOrderSnapshot, MallSalesOrderSnapshotData, SnapshotFactIdentity,
    SnapshotIngestDecision, SnapshotIngestPlan,
};
use mongodb::bson::{doc, Document};
use mongodb::Database;
use test_support::{require_mongo, TestDb};

fn at(secs: i64) -> Instant {
    Instant::from_unix_secs(secs)
}

fn identity(order: &str, secs: i64) -> SnapshotFactIdentity {
    SnapshotFactIdentity::new(
        SourceSystemId::new("sys-mall"),
        ExternalOrderKey::from_trimmed(order),
        at(secs),
    )
}

fn snapshot(id: &str, order: &str, secs: i64, deleted: bool) -> MallSalesOrderSnapshot {
    let mut snapshot = MallSalesOrderSnapshot::new(
        MallSalesOrderSnapshotId::new(id),
        MallSalesOrderSnapshotData {
            source_system_id: SourceSystemId::new("sys-mall"),
            external_order_no: order.to_string(),
            source_updated_at: at(secs),
            content_hash: None,
            source_status_code: "EFFECTIVE".to_string(),
            normalized_snapshot: "{\"sell_order\":\"x\"}".to_string(),
            raw_payload_reference: None,
            observed_at: at(secs + 1),
            sync_job_id: MallSalesSyncJobId::new("j-1"),
        },
    )
    .expect("快照构造失败");
    if deleted {
        snapshot.base.deleted_at = (secs + 2) as u64;
    }
    snapshot
}

/// 空候选不访问数据库，返回空 exact 与空 latest。
#[tokio::test]
#[ignore = "需要 ERP_TEST_MONGO_URI 指向 MongoDB 副本集"]
async fn empty_scope_is_empty_without_error() {
    require_mongo!(async {
        let fixture = TestDb::new("int_r16_empty").await.expect("测试数据库创建失败");
        ensure_indexes(fixture.db()).await.expect("索引创建失败");
        let scope = fixture
            .db()
            .mall_sync()
            .snapshot_ingest_scope(&[], &mut NoTransaction)
            .await
            .expect("空范围查询失败");
        assert!(scope.exact_keys.is_empty());
        assert!(scope.latest.is_empty());
    });
}

/// 批内重复、缺失项、历史多版本与软删除：exact 只返回存在键，latest 取最大时间。
#[tokio::test]
#[ignore = "需要 ERP_TEST_MONGO_URI 指向 MongoDB 副本集"]
async fn scope_loads_exact_keys_and_latest_and_skips_missing_or_deleted() {
    require_mongo!(async {
        let fixture = TestDb::new("int_r16_scope").await.expect("测试数据库创建失败");
        ensure_indexes(fixture.db()).await.expect("索引创建失败");
        let db = fixture.db();
        db.mall_sync()
            .create_snapshots(
                &[
                    snapshot("s-old", "SO-1", 10, false),
                    snapshot("s-new", "SO-1", 30, false),
                    snapshot("s-other", "SO-2", 20, false),
                    snapshot("s-deleted", "SO-3", 40, true),
                    snapshot("s-cross", "SO-1", 50, false),
                ],
                &mut NoTransaction,
            )
            .await
            .expect("夹具插入失败");
        // 跨商城同单号不得串入：改写最后一条来源。
        db.collection::<MallSalesOrderSnapshot>(
            <mongodb::Database as MallSyncExt>::MALL_SALES_ORDER_SNAPSHOTS,
        )
        .update_one(
            doc! { "id": "s-cross" },
            doc! { "$set": { "source_system_id": "sys-other" } },
        )
        .await
        .expect("跨商城夹具更新失败");

        let candidates = [
            identity("SO-1", 10),
            identity("SO-1", 10),
            identity("SO-1", 30),
            identity("SO-2", 20),
            identity("SO-3", 40),
            identity("SO-missing", 1),
        ];
        let scope = db
            .mall_sync()
            .snapshot_ingest_scope(&candidates, &mut NoTransaction)
            .await
            .expect("范围查询失败");

        let mut exact = scope.exact_keys;
        exact.sort_by_key(|fact| {
            (
                fact.external_order_key.to_string(),
                fact.source_updated_at.unix_secs(),
            )
        });
        assert_eq!(
            exact,
            vec![identity("SO-1", 10), identity("SO-1", 30), identity("SO-2", 20)]
        );
        let mut latest = scope.latest;
        latest.sort_by_key(|fact| fact.external_order_key.to_string());
        assert_eq!(latest, vec![identity("SO-1", 30), identity("SO-2", 20)]);
        assert!(!exact
            .iter()
            .any(|fact| fact.external_order_key.to_string() == "SO-3"));
        assert!(!latest
            .iter()
            .any(|fact| fact.external_order_key.to_string() == "SO-missing"));
    });
}

/// 同一事务内未提交快照必须对 session 可见，回滚后对外不可见。
#[tokio::test]
#[ignore = "需要 ERP_TEST_MONGO_URI 指向 MongoDB 副本集"]
async fn scope_sees_session_writes_and_rollback_hides_them() {
    require_mongo!(async {
        let fixture = TestDb::new("int_r16_session").await.expect("测试数据库创建失败");
        ensure_indexes(fixture.db()).await.expect("索引创建失败");
        let db = fixture.db().clone();
        let client = db.client().clone();
        let aborted: Result<(), database::Error> = client
            .with_transaction(move |session| {
                let db = db.clone();
                Box::pin(async move {
                    db.mall_sync()
                        .create_snapshots(&[snapshot("s-tx", "SO-TX", 10, false)], session)
                        .await?;
                    let scope = db
                        .mall_sync()
                        .snapshot_ingest_scope(&[identity("SO-TX", 10)], session)
                        .await?;
                    assert_eq!(scope.exact_keys, vec![identity("SO-TX", 10)]);
                    assert_eq!(scope.latest, vec![identity("SO-TX", 10)]);
                    Err::<(), database::Error>(database::Error::OptimisticLockingError)
                })
            })
            .await;
        assert!(matches!(aborted, Err(database::Error::OptimisticLockingError)));
        let scope = fixture
            .db()
            .mall_sync()
            .snapshot_ingest_scope(&[identity("SO-TX", 10)], &mut NoTransaction)
            .await
            .expect("回滚后范围查询失败");
        assert!(scope.exact_keys.is_empty());
        assert!(scope.latest.is_empty());
    });
}

/// 空批量插入为 no-op；多条写入后可读；等时重复键冲突。
#[tokio::test]
#[ignore = "需要 ERP_TEST_MONGO_URI 指向 MongoDB 副本集"]
async fn create_snapshots_handles_empty_many_and_duplicate_key() {
    require_mongo!(async {
        let fixture = TestDb::new("int_r16_insert").await.expect("测试数据库创建失败");
        ensure_indexes(fixture.db()).await.expect("索引创建失败");
        let db = fixture.db();
        db.mall_sync()
            .create_snapshots(&[], &mut NoTransaction)
            .await
            .expect("空插入必须成功");
        db.mall_sync()
            .create_snapshots(
                &[
                    snapshot("s-a", "SO-A", 10, false),
                    snapshot("s-b", "SO-B", 11, false),
                ],
                &mut NoTransaction,
            )
            .await
            .expect("多条插入失败");
        let scope = db
            .mall_sync()
            .snapshot_ingest_scope(&[identity("SO-A", 10), identity("SO-B", 11)], &mut NoTransaction)
            .await
            .expect("插入后范围查询失败");
        assert_eq!(scope.exact_keys.len(), 2);

        let duplicate = db
            .mall_sync()
            .create_snapshots(&[snapshot("s-a-dup", "SO-A", 10, false)], &mut NoTransaction)
            .await;
        assert!(
            matches!(duplicate, Err(database::Error::DuplicateKey(_))),
            "等时重复必须撞唯一索引：{duplicate:?}"
        );
    });
}

/// 混合页：库内 exact 重复不得写入，新兄弟必须落盘。
#[tokio::test]
#[ignore = "需要 ERP_TEST_MONGO_URI 指向 MongoDB 副本集"]
async fn mixed_page_exact_duplicate_and_new_commits_only_new() {
    require_mongo!(async {
        let fixture = TestDb::new("int_r16_mixed").await.expect("测试数据库创建失败");
        ensure_indexes(fixture.db()).await.expect("索引创建失败");
        let db = fixture.db();
        db.mall_sync()
            .create_snapshots(&[snapshot("s-dup", "SO-1", 10, false)], &mut NoTransaction)
            .await
            .expect("已有事实插入失败");

        let candidates = [identity("SO-1", 10), identity("SO-2", 20)];
        let scope = db
            .mall_sync()
            .snapshot_ingest_scope(&candidates, &mut NoTransaction)
            .await
            .expect("混合页范围查询失败");
        let plan = SnapshotIngestPlan::classify(&candidates, &scope.exact_keys, &scope.latest);
        assert_eq!(
            plan.decisions(),
            &[SnapshotIngestDecision::Duplicate, SnapshotIngestDecision::Accept]
        );

        let sibling = snapshot("s-new", "SO-2", 20, false);
        let client = db.client().clone();
        persist_snapshot_once(&client, db, sibling)
            .await
            .expect("新兄弟落盘失败");

        let after = db
            .mall_sync()
            .snapshot_ingest_scope(&candidates, &mut NoTransaction)
            .await
            .expect("落盘后范围查询失败");
        assert_eq!(after.exact_keys.len(), 2);
        let so1 = db
            .collection::<MallSalesOrderSnapshot>(
                <mongodb::Database as MallSyncExt>::MALL_SALES_ORDER_SNAPSHOTS,
            )
            .count_documents(doc! {
                "source_system_id": "sys-mall",
                "external_order_key": ExternalOrderKey::from_trimmed("SO-1").to_bson_binary(),
            })
            .await
            .expect("计数失败");
        assert_eq!(so1, 1, "exact 重复不得再插入");
    });
}

/// 已有更新水位时，较旧版本 `$max` 不得落盘。
#[tokio::test]
#[ignore = "需要 ERP_TEST_MONGO_URI 指向 MongoDB 副本集"]
async fn older_claim_after_newer_watermark_is_skipped() {
    require_mongo!(async {
        let fixture = TestDb::new("int_r16_lost_cas").await.expect("测试数据库创建失败");
        ensure_indexes(fixture.db()).await.expect("索引创建失败");
        let db = fixture.db();
        let client = db.client().clone();
        persist_snapshot_once(&client, db, snapshot("s-new", "SO-1", 20, false))
            .await
            .expect("新版本落盘失败");
        persist_snapshot_once(&client, db, snapshot("s-old", "SO-1", 10, false))
            .await
            .expect("旧版本必须跳过而非失败");
        let latest = db
            .mall_sales_order_snapshots()
            .find_latest_by_order(
                &SourceSystemId::new("sys-mall"),
                &ExternalOrderKey::from_trimmed("SO-1"),
                &mut NoTransaction,
            )
            .await
            .expect("读取 latest 失败")
            .expect("必须存在 latest");
        assert_eq!(latest.source_updated_at, at(20));
        let older = db
            .collection::<MallSalesOrderSnapshot>(
                <mongodb::Database as MallSyncExt>::MALL_SALES_ORDER_SNAPSHOTS,
            )
            .count_documents(doc! {
                "source_system_id": "sys-mall",
                "source_updated_at": 10_i64,
            })
            .await
            .expect("计数失败");
        assert_eq!(older, 0, "水位未夺得不得落盘旧版本");
    });
}

/// 水位唯一索引拒绝同一来源单的第二行。
#[tokio::test]
#[ignore = "需要 ERP_TEST_MONGO_URI 指向 MongoDB 副本集"]
async fn watermark_unique_index_rejects_duplicate_order() {
    require_mongo!(async {
        let fixture = TestDb::new("int_r16_watermark_uk")
            .await
            .expect("测试数据库创建失败");
        ensure_indexes(fixture.db()).await.expect("索引创建失败");
        let coll = fixture
            .db()
            .collection::<Document>(<mongodb::Database as MallSyncExt>::MALL_SALES_ORDER_SNAPSHOT_WATERMARKS);
        let doc = doc! {
            "source_system_id": "sys-mall",
            "external_order_key": ExternalOrderKey::from_trimmed("SO-1").to_bson_binary(),
            "source_updated_at": 10_i64,
        };
        coll.insert_one(doc.clone()).await.expect("首行水位插入失败");
        let duplicate = coll
            .insert_one(doc! {
                "source_system_id": "sys-mall",
                "external_order_key": ExternalOrderKey::from_trimmed("SO-1").to_bson_binary(),
                "source_updated_at": 20_i64,
            })
            .await;
        let error = database::Error::from(duplicate.expect_err("重复水位必须失败"));
        assert!(
            matches!(error, database::Error::DuplicateKey(_)),
            "水位重复值门禁必须是 DuplicateKey：{error:?}"
        );
    });
}

/// 一次新会话内：`$max` upsert 夺得则插入，未夺得则跳过；E11000 原样冒泡。
async fn persist_snapshot_once(
    client: &mongodb::Client,
    db: &Database,
    snapshot: MallSalesOrderSnapshot,
) -> std::result::Result<(), database::Error> {
    let claim = SnapshotFactIdentity::from_snapshot(&snapshot);
    client
        .with_transaction(move |session| {
            let db = db.clone();
            let snapshot = snapshot.clone();
            let claim = claim.clone();
            Box::pin(async move {
                let won = db
                    .mall_sync()
                    .claim_snapshot_watermarks(std::slice::from_ref(&claim), session)
                    .await?;
                if won.first().copied().unwrap_or(false) {
                    db.mall_sync()
                        .create_snapshots(std::slice::from_ref(&snapshot), session)
                        .await?;
                }
                Ok(())
            })
        })
        .await
}

/// E11000 / 瞬态冲突后中止失败会话，换新会话重分类落盘。
async fn persist_snapshot_with_replay(
    client: &mongodb::Client,
    db: &Database,
    snapshot: MallSalesOrderSnapshot,
) -> std::result::Result<(), database::Error> {
    let mut attempts = 0;
    loop {
        attempts += 1;
        match persist_snapshot_once(client, db, snapshot.clone()).await {
            Ok(()) => return Ok(()),
            Err(database::Error::DuplicateKey(_)) | Err(database::Error::TransientTransactionConflict(_))
                if attempts < 8 =>
            {
                continue;
            }
            Err(error) => return Err(error),
        }
    }
}

/// 两个并发不同版本：失败会话不得复用；重放后旧版本不得成为 latest。
#[tokio::test]
#[ignore = "需要 ERP_TEST_MONGO_URI 指向 MongoDB 副本集"]
async fn concurrent_versions_replay_on_new_session_and_keep_newest() {
    require_mongo!(async {
        let fixture = TestDb::new("int_r16_concurrent")
            .await
            .expect("测试数据库创建失败");
        ensure_indexes(fixture.db()).await.expect("索引创建失败");
        let db = fixture.db().clone();
        let client = db.client().clone();

        let (older, newer) = tokio::join!(
            persist_snapshot_with_replay(&client, &db, snapshot("s-old", "SO-1", 10, false)),
            persist_snapshot_with_replay(&client, &db, snapshot("s-new", "SO-1", 20, false)),
        );
        older.expect("较旧版本重放后必须结束（落盘或跳过）");
        newer.expect("较新版本必须落盘");

        let latest = db
            .mall_sales_order_snapshots()
            .find_latest_by_order(
                &SourceSystemId::new("sys-mall"),
                &ExternalOrderKey::from_trimmed("SO-1"),
                &mut NoTransaction,
            )
            .await
            .expect("读取 latest 失败")
            .expect("必须存在 latest");
        assert_eq!(latest.source_updated_at, at(20), "旧版本不得成为 latest");
        assert!(latest.supersedes_candidate(at(10)));
    });
}

/// 两个并发相同版本：幂等重复经换新会话重放后只留一行。
#[tokio::test]
#[ignore = "需要 ERP_TEST_MONGO_URI 指向 MongoDB 副本集"]
async fn concurrent_same_version_replays_and_keeps_one_row() {
    require_mongo!(async {
        let fixture = TestDb::new("int_r16_concurrent_same")
            .await
            .expect("测试数据库创建失败");
        ensure_indexes(fixture.db()).await.expect("索引创建失败");
        let db = fixture.db().clone();
        let client = db.client().clone();

        let (first, second) = tokio::join!(
            persist_snapshot_with_replay(&client, &db, snapshot("s-a", "SO-1", 10, false)),
            persist_snapshot_with_replay(&client, &db, snapshot("s-b", "SO-1", 10, false)),
        );
        first.expect("首个等时写入必须结束");
        second.expect("幂等重复不得使整批失败");

        let count = db
            .collection::<MallSalesOrderSnapshot>(
                <mongodb::Database as MallSyncExt>::MALL_SALES_ORDER_SNAPSHOTS,
            )
            .count_documents(doc! {
                "source_system_id": "sys-mall",
                "external_order_key": ExternalOrderKey::from_trimmed("SO-1").to_bson_binary(),
                "source_updated_at": 10_i64,
            })
            .await
            .expect("计数失败");
        assert_eq!(count, 1, "等时并发只允许一行");
    });
}

/// 缺失来源单为空，exact 查询 explain 命中事实键唯一索引。
#[tokio::test]
#[ignore = "需要 ERP_TEST_MONGO_URI 指向 MongoDB 副本集"]
async fn missing_fact_is_empty_and_explain_uses_fact_key_index() {
    require_mongo!(async {
        let fixture = TestDb::new("int_r16_explain").await.expect("测试数据库创建失败");
        ensure_indexes(fixture.db()).await.expect("索引创建失败");
        let db = fixture.db();
        db.mall_sync()
            .create_snapshots(&[snapshot("s-1", "SO-1", 10, false)], &mut NoTransaction)
            .await
            .expect("夹具插入失败");

        let scope = db
            .mall_sync()
            .snapshot_ingest_scope(&[identity("SO-missing", 10)], &mut NoTransaction)
            .await
            .expect("缺失键查询失败");
        assert!(scope.exact_keys.is_empty());
        assert!(scope.latest.is_empty());

        let explained: Document = db
            .run_command(doc! {
                "explain": {
                    "find": <mongodb::Database as MallSyncExt>::MALL_SALES_ORDER_SNAPSHOTS,
                    "filter": {
                        "$or": [{
                            "source_system_id": "sys-mall",
                            "external_order_key": ExternalOrderKey::from_trimmed("SO-1").to_bson_binary(),
                            "source_updated_at": 10_i64,
                        }],
                        "deleted_at": 0_i64,
                    },
                },
                "verbosity": "executionStats",
            })
            .await
            .expect("explain 失败");
        let rendered = format!("{explained:?}");
        assert!(rendered.contains("IXSCAN"), "explain 未使用 IXSCAN：{rendered}");
        assert!(
            !rendered.contains("COLLSCAN"),
            "exact 事实键查询不得集合扫描：{rendered}"
        );
        assert!(
            rendered.contains("uk_mall_sales_order_snapshots_fact_key"),
            "explain 未命中事实键唯一索引：{rendered}"
        );

        let latest_explained: Document = db
            .run_command(doc! {
                "explain": {
                    "aggregate": <mongodb::Database as MallSyncExt>::MALL_SALES_ORDER_SNAPSHOTS,
                    "pipeline": [
                        { "$match": {
                            "$or": [{
                                "source_system_id": "sys-mall",
                                "external_order_key": ExternalOrderKey::from_trimmed("SO-1").to_bson_binary(),
                            }],
                            "deleted_at": 0_i64,
                        }},
                        { "$sort": { "source_updated_at": -1, "id": 1 } },
                        { "$group": {
                            "_id": {
                                "source_system_id": "$source_system_id",
                                "external_order_key": "$external_order_key",
                            },
                            "source_updated_at": { "$first": "$source_updated_at" },
                        }},
                    ],
                    "cursor": {},
                },
                "verbosity": "executionStats",
            })
            .await
            .expect("latest explain 失败");
        let latest_rendered = format!("{latest_explained:?}");
        assert!(
            latest_rendered.contains("IXSCAN"),
            "latest $or 未使用 IXSCAN：{latest_rendered}"
        );
        assert!(
            !latest_rendered.contains("COLLSCAN"),
            "latest $or 不得集合扫描：{latest_rendered}"
        );

        persist_snapshot_once(&db.client().clone(), db, snapshot("s-wm", "SO-WM", 10, false))
            .await
            .expect("水位 explain 夹具失败");
        let watermark_explained: Document = db
            .run_command(doc! {
                "explain": {
                    "update": <mongodb::Database as MallSyncExt>::MALL_SALES_ORDER_SNAPSHOT_WATERMARKS,
                    "updates": [{
                        "q": {
                            "source_system_id": "sys-mall",
                            "external_order_key": ExternalOrderKey::from_trimmed("SO-WM").to_bson_binary(),
                        },
                        "u": { "$max": { "source_updated_at": 20_i64 } },
                        "upsert": true,
                    }],
                },
                "verbosity": "executionStats",
            })
            .await
            .expect("水位 explain 失败");
        let watermark_rendered = format!("{watermark_explained:?}");
        assert!(
            watermark_rendered.contains("IXSCAN"),
            "水位 CAS 未使用 IXSCAN：{watermark_rendered}"
        );
        assert!(
            watermark_rendered.contains("uk_mall_sales_order_snapshot_watermarks_order"),
            "水位 CAS 未命中唯一索引：{watermark_rendered}"
        );
    });
}
