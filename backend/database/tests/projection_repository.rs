//! 域 D27 `projection` 仓储集成测试（真实 MongoDB 单节点副本集）。
//!
//! 统一 `#[ignore]` + `ERP_TEST_MONGO_URI` 门控（test-support `require_mongo!`）：
//! 无数据库环境 `cargo test` 跳过；CI 与验收执行
//! `cargo test -p database --test projection_repository -- --include-ignored`。
//! 每个测试使用独立随机数据库名，结束自动 drop（TestDb）。

use database::repository::extensions::ProjectionExt;
use database::{ensure_indexes, NoTransaction, Transactional};
use entities::common::time::Instant;
use entities::ids::{
    SalesOrderId, SalesOrderProjectionDeliveryId, SalesOrderProjectionId, SalesOrderProjectionRevisionId,
    SalesOrderRevisionId, SourceSystemId,
};
use entities::money::Amount;
use entities::projection::{
    CardForm, ProjectionDeliveryStatus, ProjectionSource, SalesOrderProjection, SalesOrderProjectionData,
    SalesOrderProjectionDelivery, SalesOrderProjectionDeliveryData, SalesOrderProjectionRevision,
    SalesOrderProjectionRevisionData, SalesOrderProjectionUpdate,
};
use mongodb::Database;
use std::str::FromStr;
use test_support::{assert_indexes, require_mongo, TestDb};

/// 投影列表筛选条件类型（经 `ProjectionExt` 关联类型跨 crate 可达）。
type SalesOrderProjectionFilter = <Database as ProjectionExt>::SalesOrderProjectionFilter;
/// 投影下发列表筛选条件类型。
type SalesOrderProjectionDeliveryFilter = <Database as ProjectionExt>::SalesOrderProjectionDeliveryFilter;

/// 构造可复用的投影稳定身份实体。
fn sample_projection(sales_order: &str, mall: &str) -> SalesOrderProjection {
    SalesOrderProjection::new(
        SalesOrderProjectionId::new(format!("proj-{sales_order}-{mall}")),
        SalesOrderProjectionData {
            sales_order_id: SalesOrderId::new(sales_order),
            target_mall_id: SourceSystemId::new(mall),
        },
    )
    .unwrap()
}

/// 构造可复用的投影修订实体（含 Decimal128 金额字段）。
fn sample_revision(
    projection_id: &SalesOrderProjectionId,
    revision_no: u32,
    sales_order_revision: &str,
) -> SalesOrderProjectionRevision {
    SalesOrderProjectionRevision::new(
        SalesOrderProjectionRevisionId::new(format!("proj-rev-{projection_id}-{revision_no}")),
        revision_no,
        SalesOrderProjectionRevisionData {
            projection_id: projection_id.clone(),
            projection_source: ProjectionSource::ErpRevision,
            sales_order_revision_id: SalesOrderRevisionId::new(sales_order_revision),
            customer_external_identity: format!("mall-customer-{sales_order_revision}"),
            voucher_category_external_identity: "mall-voucher-001".to_string(),
            voucher_expiry_at: Instant::from_unix_secs(1_800_000_000),
            face_value: Amount::from_str("100.00").unwrap(),
            card_count: 100,
            card_form: CardForm::Electronic,
            effective_at: Instant::from_unix_secs(1_700_000_000),
            content_hash: "0011aabbccdd".to_string(),
        },
    )
    .unwrap()
}

/// 构造可复用的投影下发记录实体。
fn sample_delivery(
    revision_id: &SalesOrderProjectionRevisionId,
    mall: &str,
    status: ProjectionDeliveryStatus,
) -> SalesOrderProjectionDelivery {
    SalesOrderProjectionDelivery::new(
        SalesOrderProjectionDeliveryId::new(format!("proj-del-{revision_id}-{mall}")),
        SalesOrderProjectionDeliveryData {
            projection_revision_id: revision_id.clone(),
            target_mall_id: SourceSystemId::new(mall),
            status,
            attempt_count: 0,
            next_attempt_at: None,
            mall_ack_at: None,
            mall_execution_baseline: None,
            error_code: None,
            error_summary: None,
        },
    )
    .unwrap()
}

/// 执行 `ensure_indexes` 并断言本域全部必需索引就位。
async fn assert_domain_indexes(db: &Database) {
    ensure_indexes(db).await.expect("ensure_indexes 应成功");
    assert_indexes(
        db,
        <Database as ProjectionExt>::SALES_ORDER_PROJECTIONS,
        &["uk_sales_order_projections_order_mall"],
    )
    .await
    .expect("sales_order_projections 索引缺失");
    assert_indexes(
        db,
        <Database as ProjectionExt>::SALES_ORDER_PROJECTION_REVISIONS,
        &["uk_sales_order_projection_revisions_projection_revision"],
    )
    .await
    .expect("sales_order_projection_revisions 索引缺失");
    assert_indexes(
        db,
        <Database as ProjectionExt>::SALES_ORDER_PROJECTION_DELIVERIES,
        &[
            "uk_sales_order_projection_deliveries_revision_mall",
            "idx_sales_order_projection_deliveries_status",
        ],
    )
    .await
    .expect("sales_order_projection_deliveries 索引缺失");
}

#[tokio::test]
#[ignore]
async fn create_update_roundtrip_with_optimistic_locking() {
    require_mongo!(async {
        let test_db = TestDb::new("proj_crud").await.unwrap();
        let db = test_db.db();
        assert_domain_indexes(db).await;

        let mut projection = sample_projection("so-1", "mall-1");
        db.sales_order_projections()
            .create(&projection, &mut NoTransaction)
            .await
            .unwrap();
        assert_eq!(projection.base.version, 1);

        let found = db
            .sales_order_projections()
            .find_by_id(&projection.base.id, &mut NoTransaction)
            .await
            .unwrap()
            .expect("创建后应可读回");
        assert_eq!(found.sales_order_id, SalesOrderId::new("so-1"));
        assert_eq!(found.target_mall_id, SourceSystemId::new("mall-1"));
        assert!(found.current_acked_revision_id.is_none());

        projection
            .update(SalesOrderProjectionUpdate {
                current_acked_revision_id: Some(SalesOrderProjectionRevisionId::new("proj-rev-1")),
            })
            .unwrap();
        db.sales_order_projections()
            .update(&mut projection, &mut NoTransaction)
            .await
            .unwrap();
        assert_eq!(projection.base.version, 2, "乐观锁成功后 version 递增");

        let mut stale = projection.clone();
        projection
            .update(SalesOrderProjectionUpdate {
                current_acked_revision_id: Some(SalesOrderProjectionRevisionId::new("proj-rev-2")),
            })
            .unwrap();
        db.sales_order_projections()
            .update(&mut projection, &mut NoTransaction)
            .await
            .unwrap();

        stale
            .update(SalesOrderProjectionUpdate {
                current_acked_revision_id: Some(SalesOrderProjectionRevisionId::new("proj-rev-stale")),
            })
            .unwrap();
        let error = db
            .sales_order_projections()
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
async fn unique_order_mall_conflict_surfaces_duplicate_key() {
    require_mongo!(async {
        let test_db = TestDb::new("proj_dup_order_mall").await.unwrap();
        let db = test_db.db();
        assert_domain_indexes(db).await;

        let projection = sample_projection("so-1", "mall-1");
        db.sales_order_projections()
            .create(&projection, &mut NoTransaction)
            .await
            .unwrap();

        let duplicate = sample_projection("so-1", "mall-1");
        let error = db
            .sales_order_projections()
            .create(&duplicate, &mut NoTransaction)
            .await
            .expect_err("同一 (sales_order_id, target_mall_id) 重复投影必须被唯一索引拒绝");
        assert!(
            matches!(error, database::Error::DuplicateKey(_)),
            "期望 DuplicateKey，实际为 {error:?}"
        );
    })
}

#[tokio::test]
#[ignore]
async fn revision_roundtrip_list_and_idempotency_key() {
    require_mongo!(async {
        let test_db = TestDb::new("proj_rev").await.unwrap();
        let db = test_db.db();
        assert_domain_indexes(db).await;

        let projection = sample_projection("so-1", "mall-1");
        db.sales_order_projections()
            .create(&projection, &mut NoTransaction)
            .await
            .unwrap();
        let projection_id = projection.base.id.clone().into();
        let revision = sample_revision(&projection_id, 1, "so-rev-1");
        db.sales_order_projection_revisions()
            .create(&revision, &mut NoTransaction)
            .await
            .unwrap();

        let found = db
            .sales_order_projection_revisions()
            .find_revision_by_no(&projection_id, 1, &mut NoTransaction)
            .await
            .unwrap()
            .expect("按 (投影, 修订序号) 应可读回");
        assert_eq!(found.revision.revision_no, 1);
        assert_eq!(found.face_value, Amount::from_str("100.00").unwrap());
        assert_eq!(found.card_count, 100);
        assert_eq!(found.card_form, CardForm::Electronic);
        assert_eq!(found.projection_source, ProjectionSource::ErpRevision);

        let rows = db
            .sales_order_projection_revisions()
            .list_revisions_by_projection(&projection_id, &mut NoTransaction)
            .await
            .unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].revision_no, 1);
        assert_eq!(rows[0].customer_external_identity, "mall-customer-so-rev-1");
        assert_eq!(rows[0].face_value, Amount::from_str("100.00").unwrap());
        assert_eq!(rows[0].card_count, 100);
        assert_eq!(rows[0].effective_at, 1_700_000_000);
        assert!(
            !mongodb::bson::to_document(&rows[0])
                .unwrap()
                .contains_key("content_hash"),
            "内容指纹不进入修订列表投影"
        );
    })
}

#[tokio::test]
#[ignore]
async fn revision_and_delivery_unique_conflicts_surface_duplicate_key() {
    require_mongo!(async {
        let test_db = TestDb::new("proj_dup_rev").await.unwrap();
        let db = test_db.db();
        assert_domain_indexes(db).await;

        let projection = sample_projection("so-1", "mall-1");
        db.sales_order_projections()
            .create(&projection, &mut NoTransaction)
            .await
            .unwrap();
        let projection_id = projection.base.id.clone().into();
        let revision = sample_revision(&projection_id, 1, "so-rev-1");
        db.sales_order_projection_revisions()
            .create(&revision, &mut NoTransaction)
            .await
            .unwrap();
        let revision_id = revision.base.id.clone().into();

        let duplicate_no = sample_revision(&projection_id, 1, "so-rev-2");
        let error = db
            .sales_order_projection_revisions()
            .create(&duplicate_no, &mut NoTransaction)
            .await
            .expect_err("同一 (projection_id, revision_no) 重复必须被唯一索引拒绝");
        assert!(
            matches!(error, database::Error::DuplicateKey(_)),
            "期望 DuplicateKey，实际为 {error:?}"
        );

        db.sales_order_projection_deliveries()
            .create(
                &sample_delivery(&revision_id, "mall-1", ProjectionDeliveryStatus::PendingSend),
                &mut NoTransaction,
            )
            .await
            .unwrap();
        let duplicate_delivery = sample_delivery(&revision_id, "mall-1", ProjectionDeliveryStatus::Retrying);
        let error = db
            .sales_order_projection_deliveries()
            .create(&duplicate_delivery, &mut NoTransaction)
            .await
            .expect_err("同一 (projection_revision_id, target_mall_id) 重复下发必须被唯一索引拒绝");
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
        let test_db = TestDb::new("proj_list").await.unwrap();
        let db = test_db.db();
        assert_domain_indexes(db).await;

        db.sales_order_projections()
            .create(&sample_projection("so-1", "mall-1"), &mut NoTransaction)
            .await
            .unwrap();
        db.sales_order_projections()
            .create(&sample_projection("so-2", "mall-1"), &mut NoTransaction)
            .await
            .unwrap();
        db.sales_order_projections()
            .create(&sample_projection("so-3", "mall-2"), &mut NoTransaction)
            .await
            .unwrap();

        let filter = SalesOrderProjectionFilter {
            sales_order_id: None,
            target_mall_id: Some(SourceSystemId::new("mall-1")),
            page: 2,
            page_size: 1,
            sort_by: Some("sales_order_id".to_string()),
            sort_ascending: true,
        };
        let page = db
            .sales_order_projections()
            .search_sales_order_projections(&filter, &mut NoTransaction)
            .await
            .unwrap();
        assert_eq!(page.total, 2, "mall-1 共两条投影");
        assert_eq!(page.items.len(), 1, "分页边界：第二页只剩一条");
        let row = &page.items[0];
        assert_eq!(row.sales_order_id, "so-2", "按 sales_order_id 升序取第二条");
        assert_eq!(row.target_mall_id, "mall-1");
        assert_eq!(row.current_acked_revision_id, None);
        assert!(row.version >= 1);

        let no_match = SalesOrderProjectionFilter {
            sales_order_id: Some(SalesOrderId::new("so-999")),
            target_mall_id: None,
            page: 1,
            page_size: 20,
            sort_by: Some("任意字段".to_string()),
            sort_ascending: false,
        };
        let empty = db
            .sales_order_projections()
            .search_sales_order_projections(&no_match, &mut NoTransaction)
            .await
            .unwrap();
        assert_eq!(empty.total, 0, "不存在的销售单筛选应返回空列表");
    })
}

#[tokio::test]
#[ignore]
async fn delivery_list_search_filters_by_status_and_mall() {
    require_mongo!(async {
        let test_db = TestDb::new("proj_delivery_list").await.unwrap();
        let db = test_db.db();
        assert_domain_indexes(db).await;

        let projection = sample_projection("so-1", "mall-1");
        db.sales_order_projections()
            .create(&projection, &mut NoTransaction)
            .await
            .unwrap();
        let projection_id = projection.base.id.clone().into();
        let revision = sample_revision(&projection_id, 1, "so-rev-1");
        let revision_2 = sample_revision(&projection_id, 2, "so-rev-2");
        db.sales_order_projection_revisions()
            .create(&revision, &mut NoTransaction)
            .await
            .unwrap();
        db.sales_order_projection_revisions()
            .create(&revision_2, &mut NoTransaction)
            .await
            .unwrap();
        let revision_id = revision.base.id.clone().into();
        let revision_2_id = revision_2.base.id.clone().into();
        let mut confirmed = sample_delivery(&revision_2_id, "mall-1", ProjectionDeliveryStatus::PendingSend);
        confirmed
            .update(entities::projection::SalesOrderProjectionDeliveryUpdate {
                status: Some(ProjectionDeliveryStatus::Confirmed),
                attempt_count: Some(2),
                mall_ack_at: Some(Instant::from_unix_secs(1_700_000_000)),
                mall_execution_baseline: Some("baseline-v1".to_string()),
                ..Default::default()
            })
            .unwrap();
        let mut failed = sample_delivery(&revision_id, "mall-2", ProjectionDeliveryStatus::PendingSend);
        failed
            .update(entities::projection::SalesOrderProjectionDeliveryUpdate {
                status: Some(ProjectionDeliveryStatus::Failed),
                attempt_count: Some(1),
                error_code: Some("TIMEOUT".to_string()),
                error_summary: Some("商城超时".to_string()),
                ..Default::default()
            })
            .unwrap();

        db.sales_order_projection_deliveries()
            .create(
                &sample_delivery(&revision_id, "mall-1", ProjectionDeliveryStatus::PendingSend),
                &mut NoTransaction,
            )
            .await
            .unwrap();
        db.sales_order_projection_deliveries()
            .create(&confirmed, &mut NoTransaction)
            .await
            .unwrap();
        db.sales_order_projection_deliveries()
            .create(&failed, &mut NoTransaction)
            .await
            .unwrap();

        let filter = SalesOrderProjectionDeliveryFilter {
            target_mall_id: None,
            status: Some(ProjectionDeliveryStatus::Confirmed),
            page: 1,
            page_size: 20,
            sort_by: Some("created_at".to_string()),
            sort_ascending: false,
        };
        let page = db
            .sales_order_projection_deliveries()
            .search_sales_order_projection_deliveries(&filter, &mut NoTransaction)
            .await
            .unwrap();
        assert_eq!(page.total, 1, "按状态筛选只命中已确认下发");
        let row = &page.items[0];
        assert_eq!(row.status, ProjectionDeliveryStatus::Confirmed);
        assert_eq!(row.attempt_count, 2);
        assert_eq!(row.mall_ack_at, Some(1_700_000_000));
        assert_eq!(row.projection_revision_id, revision_2.base.id);
        assert!(!mongodb::bson::to_document(row)
            .unwrap()
            .contains_key("mall_execution_baseline"));

        let by_mall = SalesOrderProjectionDeliveryFilter {
            target_mall_id: Some(SourceSystemId::new("mall-1")),
            status: None,
            page: 1,
            page_size: 20,
            sort_by: None,
            sort_ascending: false,
        };
        let page = db
            .sales_order_projection_deliveries()
            .search_sales_order_projection_deliveries(&by_mall, &mut NoTransaction)
            .await
            .unwrap();
        assert_eq!(page.total, 2, "mall-1 有两条下发记录");

        let duplicate_lookup = db
            .sales_order_projection_deliveries()
            .find_delivery_by_revision_and_mall(
                &revision_id,
                &SourceSystemId::new("mall-1"),
                &mut NoTransaction,
            )
            .await
            .unwrap();
        assert!(duplicate_lookup.is_some(), "按 (修订, 商城) 幂等判定应命中");
    })
}

#[tokio::test]
#[ignore]
async fn multi_step_projection_revision_commits_atomically() {
    require_mongo!(async {
        let test_db = TestDb::new("proj_tx_commit").await.unwrap();
        let db = test_db.db();
        assert_domain_indexes(db).await;

        let projection = sample_projection("so-1", "mall-1");
        let projection_id = projection.base.id.clone().into();
        let revision = sample_revision(&projection_id, 1, "so-rev-1");
        let revision_2 = sample_revision(&projection_id, 2, "so-rev-2");
        let delivery = sample_delivery(
            &revision_2.base.id.clone().into(),
            "mall-1",
            ProjectionDeliveryStatus::PendingSend,
        );

        let db_clone = db.clone();
        let projection_for_tx = projection.clone();
        let revision_for_tx = revision.clone();
        let revision_2_for_tx = revision_2.clone();
        let delivery_for_tx = delivery.clone();
        test_db
            .client()
            .with_transaction::<_, (), database::Error>(move |session| {
                Box::pin(async move {
                    db_clone
                        .projection()
                        .create_projection_revision(&projection_for_tx, &revision_for_tx, session)
                        .await?;
                    db_clone
                        .projection()
                        .create_projection_revision_with_delivery(
                            &revision_2_for_tx,
                            &delivery_for_tx,
                            session,
                        )
                        .await?;
                    Ok(())
                })
            })
            .await
            .expect("投影、修订与下发事务提交应成功");

        let projection_found = db
            .sales_order_projections()
            .find_by_sales_order_and_mall(
                &SalesOrderId::new("so-1"),
                &SourceSystemId::new("mall-1"),
                &mut NoTransaction,
            )
            .await
            .unwrap();
        assert!(projection_found.is_some(), "事务提交后投影必须可见");
        let revision_found = db
            .sales_order_projection_revisions()
            .find_revision_by_no(&projection_id, 1, &mut NoTransaction)
            .await
            .unwrap();
        assert!(revision_found.is_some(), "事务提交后修订必须可见");
        let revision_2_found = db
            .sales_order_projection_revisions()
            .find_revision_by_no(&projection_id, 2, &mut NoTransaction)
            .await
            .unwrap();
        assert!(revision_2_found.is_some(), "事务提交后第二修订必须可见");
        let delivery_found = db
            .sales_order_projection_deliveries()
            .find_by_id(&delivery.base.id, &mut NoTransaction)
            .await
            .unwrap();
        assert!(delivery_found.is_some(), "事务提交后下发记录必须可见");
    })
}

#[tokio::test]
#[ignore]
async fn transaction_abort_rolls_back_revision_and_delivery() {
    require_mongo!(async {
        let test_db = TestDb::new("proj_tx_abort").await.unwrap();
        let db = test_db.db();
        assert_domain_indexes(db).await;

        let projection = sample_projection("so-1", "mall-1");
        let projection_id = projection.base.id.clone().into();
        let revision = sample_revision(&projection_id, 1, "so-rev-1");
        let delivery = sample_delivery(
            &revision.base.id.clone().into(),
            "mall-1",
            ProjectionDeliveryStatus::PendingSend,
        );

        let db_clone = db.clone();
        let revision_for_tx = revision.clone();
        let delivery_for_tx = delivery.clone();
        let result: std::result::Result<(), database::Error> = test_db
            .client()
            .with_transaction(move |session| {
                Box::pin(async move {
                    db_clone
                        .projection()
                        .create_projection_revision_with_delivery(&revision_for_tx, &delivery_for_tx, session)
                        .await?;
                    Err(database::Error::OptimisticLockingError)
                })
            })
            .await;
        assert!(result.is_err(), "闭包返回错误必须整体回滚");

        let revision_found = db
            .sales_order_projection_revisions()
            .find_revision_by_no(&projection_id, 1, &mut NoTransaction)
            .await
            .unwrap();
        assert!(revision_found.is_none(), "回滚后修订不得残留");
        let delivery_found = db
            .sales_order_projection_deliveries()
            .find_by_id(&delivery.base.id, &mut NoTransaction)
            .await
            .unwrap();
        assert!(delivery_found.is_none(), "回滚后下发记录不得残留");
    })
}

#[tokio::test]
#[ignore]
async fn no_transaction_partial_write_leaves_projection_without_revision() {
    require_mongo!(async {
        let test_db = TestDb::new("proj_no_tx").await.unwrap();
        let db = test_db.db();
        assert_domain_indexes(db).await;

        let projection_a = sample_projection("so-1", "mall-1");
        let projection_id_a = projection_a.base.id.clone().into();
        db.projection()
            .create_projection_revision(
                &projection_a,
                &sample_revision(&projection_id_a, 1, "so-rev-1"),
                &mut NoTransaction,
            )
            .await
            .unwrap();

        let projection_b = sample_projection("so-2", "mall-2");
        let duplicate_revision = sample_revision(&projection_id_a, 1, "so-rev-1");
        let error = db
            .projection()
            .create_projection_revision(&projection_b, &duplicate_revision, &mut NoTransaction)
            .await
            .expect_err("重复投影修订必须被唯一索引拒绝");
        assert!(matches!(error, database::Error::DuplicateKey(_)));

        let projection_b_found = db
            .sales_order_projections()
            .find_by_sales_order_and_mall(
                &SalesOrderId::new("so-2"),
                &SourceSystemId::new("mall-2"),
                &mut NoTransaction,
            )
            .await
            .unwrap();
        assert!(
            projection_b_found.is_some(),
            "NoTransaction 下投影已自动提交，半成品状态可预期"
        );
        let revision_b_found = db
            .sales_order_projection_revisions()
            .find_revision_by_no(&projection_b.base.id.clone().into(), 1, &mut NoTransaction)
            .await
            .unwrap();
        assert!(revision_b_found.is_none(), "冲突修订未写入，只有投影没有版本");
    })
}
