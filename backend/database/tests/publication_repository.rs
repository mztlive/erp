//! 域 D26 `publication` 仓储集成测试（真实 MongoDB 单节点副本集）。
//!
//! 统一 `#[ignore]` + `ERP_TEST_MONGO_URI` 门控（test-support `require_mongo!`）：
//! 无数据库环境 `cargo test` 跳过；CI 与验收执行
//! `cargo test -p database --test publication_repository -- --include-ignored`。
//! 每个测试使用独立随机数据库名，结束自动 drop（TestDb）。

use database::repository::extensions::PublicationExt;
use database::{ensure_indexes, NoTransaction, Transactional};
use entities::common::time::Instant;
use entities::ids::{
    FileAssetId, ProductCategoryId, ProductPublicationId, ProductPublicationRevisionId,
    ProductPublicationRevisionMediaId, SkuId, SkuRevisionId, SourceSystemId, SupplierOfferingRevisionId,
};
use entities::money::{Amount, Quantity, Rate};
use entities::publication::{
    MediaRole, ProductCapability, ProductPublication, ProductPublicationData, ProductPublicationDelivery,
    ProductPublicationDeliveryData, ProductPublicationRevision, ProductPublicationRevisionData,
    ProductPublicationRevisionMedia, ProductPublicationRevisionMediaData, ProductPublicationStatus,
    ProductPublicationUpdate, PublicationDeliveryStatus, SaleStatus,
};
use mongodb::Database;
use std::str::FromStr;
use test_support::{assert_indexes, require_mongo, TestDb};

/// 发布列表筛选条件类型（经 `PublicationExt` 关联类型跨 crate 可达）。
type ProductPublicationFilter = <Database as PublicationExt>::ProductPublicationFilter;
/// 发布投递列表筛选条件类型。
type ProductPublicationDeliveryFilter = <Database as PublicationExt>::ProductPublicationDeliveryFilter;

/// 构造可复用的发布主表实体。
fn sample_publication(sku: &str, mall: &str, created_by: &str) -> ProductPublication {
    ProductPublication::new(
        ProductPublicationId::new(format!("pub-{sku}-{mall}")),
        ProductPublicationData {
            sku_id: SkuId::new(sku),
            target_mall_id: SourceSystemId::new(mall),
            status: ProductPublicationStatus::Draft,
        },
        created_by,
    )
    .unwrap()
}

/// 构造可复用的发布修订实体（含 Decimal128 金额字段）。
fn sample_revision(
    publication_id: &ProductPublicationId,
    revision_no: u32,
    name: &str,
) -> ProductPublicationRevision {
    ProductPublicationRevision::new(
        ProductPublicationRevisionId::new(format!("rev-{publication_id}-{revision_no}")),
        revision_no,
        ProductPublicationRevisionData {
            product_publication_id: publication_id.clone(),
            sku_revision_id: SkuRevisionId::new("sku-rev-1"),
            supplier_offering_revision_id: SupplierOfferingRevisionId::new("offer-rev-1"),
            category_id: ProductCategoryId::new("cat-1"),
            name: name.to_string(),
            specification: None,
            sales_description: "员工福利采购".to_string(),
            minimum_purchase_quantity: Quantity::from_str("1.000000").unwrap(),
            sales_price_gross: Amount::from_str("100.00").unwrap(),
            sales_tax_rate: Rate::from_str("0.130000").unwrap(),
            base_unit_code: "张".to_string(),
            sales_region: None,
            sale_status: SaleStatus::OnSale,
            product_capabilities: vec![ProductCapability::Cancel],
            valid_from: Instant::from_unix_secs(1_700_000_000),
            valid_to: Some(Instant::from_unix_secs(1_800_000_000)),
            content_hash: "aabbccddeeff".to_string(),
        },
    )
    .unwrap()
}

/// 构造可复用的发布修订媒体实体。
fn sample_media(
    revision_id: &ProductPublicationRevisionId,
    media_role: MediaRole,
    sort_no: u32,
) -> ProductPublicationRevisionMedia {
    ProductPublicationRevisionMedia::new(
        ProductPublicationRevisionMediaId::new(format!("media-{revision_id}-{sort_no}")),
        revision_id.clone(),
        ProductPublicationRevisionMediaData {
            file_asset_id: FileAssetId::new("file-1"),
            media_role,
            sort_no,
            alt_text: None,
        },
    )
    .unwrap()
}

/// 构造可复用的发布投递实体。
fn sample_delivery(
    revision_id: &ProductPublicationRevisionId,
    mall: &str,
    delivery_status: PublicationDeliveryStatus,
) -> ProductPublicationDelivery {
    ProductPublicationDelivery::new(
        entities::ids::ProductPublicationDeliveryId::new(format!("del-{revision_id}-{mall}")),
        ProductPublicationDeliveryData {
            publication_revision_id: revision_id.clone(),
            target_mall_id: SourceSystemId::new(mall),
            delivery_status,
            attempt_count: 0,
            last_attempt_at: None,
            mall_ack_at: None,
            mall_version: None,
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
        <Database as PublicationExt>::PRODUCT_PUBLICATIONS,
        &[
            "uk_product_publications_sku_mall",
            "idx_product_publications_status",
        ],
    )
    .await
    .expect("product_publications 索引缺失");
    assert_indexes(
        db,
        <Database as PublicationExt>::PRODUCT_PUBLICATION_REVISIONS,
        &["uk_product_publication_revisions_publication_revision"],
    )
    .await
    .expect("product_publication_revisions 索引缺失");
    assert_indexes(
        db,
        <Database as PublicationExt>::PRODUCT_PUBLICATION_REVISION_MEDIA,
        &["uk_product_publication_revision_media_revision_role_sort"],
    )
    .await
    .expect("product_publication_revision_media 索引缺失");
    assert_indexes(
        db,
        <Database as PublicationExt>::PRODUCT_PUBLICATION_DELIVERIES,
        &["idx_product_publication_deliveries_status"],
    )
    .await
    .expect("product_publication_deliveries 索引缺失");
}

#[tokio::test]
#[ignore]
async fn create_update_roundtrip_with_optimistic_locking() {
    require_mongo!(async {
        let test_db = TestDb::new("pub_crud").await.unwrap();
        let db = test_db.db();
        assert_domain_indexes(db).await;

        let mut publication = sample_publication("sku-1", "mall-1", "admin-1");
        db.product_publications()
            .create(&publication, &mut NoTransaction)
            .await
            .unwrap();
        assert_eq!(publication.base.version, 1);

        let found = db
            .product_publications()
            .find_by_id(&publication.base.id, &mut NoTransaction)
            .await
            .unwrap()
            .expect("创建后应可读回");
        assert_eq!(found.sku_id, SkuId::new("sku-1"));
        assert_eq!(found.target_mall_id, SourceSystemId::new("mall-1"));
        assert_eq!(found.stable.created_by, "admin-1");

        publication
            .update(
                ProductPublicationUpdate {
                    status: Some(ProductPublicationStatus::PendingPublish),
                    current_revision_id: None,
                },
                "admin-2",
            )
            .unwrap();
        db.product_publications()
            .update(&mut publication, &mut NoTransaction)
            .await
            .unwrap();
        assert_eq!(publication.base.version, 2, "乐观锁成功后 version 递增");

        let mut stale = publication.clone();
        publication
            .update(
                ProductPublicationUpdate {
                    status: Some(ProductPublicationStatus::MallEffective),
                    current_revision_id: Some("rev-1".to_string()),
                },
                "admin-3",
            )
            .unwrap();
        db.product_publications()
            .update(&mut publication, &mut NoTransaction)
            .await
            .unwrap();
        assert!(publication.is_mall_effective());

        stale
            .update(
                ProductPublicationUpdate {
                    status: Some(ProductPublicationStatus::Paused),
                    current_revision_id: None,
                },
                "admin-4",
            )
            .unwrap();
        let error = db
            .product_publications()
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
async fn unique_sku_mall_conflict_surfaces_duplicate_key() {
    require_mongo!(async {
        let test_db = TestDb::new("pub_dup_sku_mall").await.unwrap();
        let db = test_db.db();
        assert_domain_indexes(db).await;

        let publication = sample_publication("sku-1", "mall-1", "admin-1");
        db.product_publications()
            .create(&publication, &mut NoTransaction)
            .await
            .unwrap();

        let duplicate = sample_publication("sku-1", "mall-1", "admin-2");
        let error = db
            .product_publications()
            .create(&duplicate, &mut NoTransaction)
            .await
            .expect_err("同一 (sku_id, target_mall_id) 重复发布必须被唯一索引拒绝");
        assert!(
            matches!(error, database::Error::DuplicateKey(_)),
            "期望 DuplicateKey，实际为 {error:?}"
        );
    })
}

#[tokio::test]
#[ignore]
async fn revision_and_media_roundtrip_with_decimal_amounts() {
    require_mongo!(async {
        let test_db = TestDb::new("pub_rev_media").await.unwrap();
        let db = test_db.db();
        assert_domain_indexes(db).await;

        let publication = sample_publication("sku-1", "mall-1", "admin-1");
        db.product_publications()
            .create(&publication, &mut NoTransaction)
            .await
            .unwrap();
        let publication_id = publication.base.id.clone().into();
        let revision = sample_revision(&publication_id, 1, "福利商城卡");
        let revision_id = revision.base.id.clone().into();
        let media = vec![
            sample_media(&revision_id, MediaRole::Main, 1),
            sample_media(&revision_id, MediaRole::Carousel, 1),
            sample_media(&revision_id, MediaRole::Carousel, 2),
        ];

        let db_clone = db.clone();
        let revision_for_tx = revision.clone();
        let media_for_tx = media.clone();
        test_db
            .client()
            .with_transaction::<_, (), database::Error>(move |session| {
                Box::pin(async move {
                    db_clone
                        .publication()
                        .create_revision_with_media(&revision_for_tx, &media_for_tx, session)
                        .await?;
                    Ok(())
                })
            })
            .await
            .expect("修订与媒体事务提交应成功");

        let found = db
            .product_publication_revisions()
            .find_revision_by_no(&publication_id, 1, &mut NoTransaction)
            .await
            .unwrap()
            .expect("按 (发布, 修订序号) 应可读回");
        assert_eq!(found.name, "福利商城卡");
        assert_eq!(found.revision.revision_no, 1);
        assert_eq!(found.sales_price_gross, Amount::from_str("100.00").unwrap());
        assert_eq!(
            found.minimum_purchase_quantity,
            Quantity::from_str("1.000000").unwrap()
        );

        let media_found = db
            .product_publication_revision_media()
            .find_media_by_revision(&revision_id, &mut NoTransaction)
            .await
            .unwrap();
        assert_eq!(media_found.len(), 3, "修订媒体行全部可读回");
        assert_eq!(
            media_found[0].media_role,
            MediaRole::Carousel,
            "media_role 按存储代码字典序升序（carousel < detail < main）"
        );

        let rows = db
            .product_publication_revisions()
            .list_revisions_by_publication(&publication_id, &mut NoTransaction)
            .await
            .unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].revision_no, 1);
        assert_eq!(rows[0].sale_status, SaleStatus::OnSale);
        assert_eq!(rows[0].sales_price_gross, Amount::from_str("100.00").unwrap());
        assert_eq!(rows[0].valid_from, 1_700_000_000);
        assert_eq!(rows[0].valid_to, Some(1_800_000_000));
        assert!(
            !mongodb::bson::to_document(&rows[0])
                .unwrap()
                .contains_key("sales_description"),
            "长文本快照不进入修订列表投影"
        );
    })
}

#[tokio::test]
#[ignore]
async fn revision_unique_conflict_surfaces_duplicate_key() {
    require_mongo!(async {
        let test_db = TestDb::new("pub_dup_rev").await.unwrap();
        let db = test_db.db();
        assert_domain_indexes(db).await;

        let publication = sample_publication("sku-1", "mall-1", "admin-1");
        db.product_publications()
            .create(&publication, &mut NoTransaction)
            .await
            .unwrap();
        let publication_id = publication.base.id.clone().into();
        let revision = sample_revision(&publication_id, 1, "第一版");
        db.product_publication_revisions()
            .create(&revision, &mut NoTransaction)
            .await
            .unwrap();

        let duplicate = sample_revision(&publication_id, 1, "重复版本");
        let error = db
            .product_publication_revisions()
            .create(&duplicate, &mut NoTransaction)
            .await
            .expect_err("同一 (发布, 修订序号) 重复必须被唯一索引拒绝");
        assert!(
            matches!(error, database::Error::DuplicateKey(_)),
            "期望 DuplicateKey，实际为 {error:?}"
        );
    })
}

#[tokio::test]
#[ignore]
async fn media_unique_conflict_surfaces_duplicate_key() {
    require_mongo!(async {
        let test_db = TestDb::new("pub_dup_media").await.unwrap();
        let db = test_db.db();
        assert_domain_indexes(db).await;

        let publication = sample_publication("sku-1", "mall-1", "admin-1");
        let publication_id = publication.base.id.clone().into();
        let revision = sample_revision(&publication_id, 1, "第一版");
        db.product_publication_revisions()
            .create(&revision, &mut NoTransaction)
            .await
            .unwrap();
        let revision_id = revision.base.id.clone().into();
        db.product_publication_revision_media()
            .create(
                &sample_media(&revision_id, MediaRole::Main, 1),
                &mut NoTransaction,
            )
            .await
            .unwrap();

        let duplicate = sample_media(&revision_id, MediaRole::Main, 1);
        let error = db
            .product_publication_revision_media()
            .create(&duplicate, &mut NoTransaction)
            .await
            .expect_err("同一 (修订, 角色, 序号) 重复媒体必须被唯一索引拒绝");
        assert!(
            matches!(error, database::Error::DuplicateKey(_)),
            "期望 DuplicateKey，实际为 {error:?}"
        );
    })
}

#[tokio::test]
#[ignore]
async fn publication_list_search_respects_filters_and_pagination() {
    require_mongo!(async {
        let test_db = TestDb::new("pub_list").await.unwrap();
        let db = test_db.db();
        assert_domain_indexes(db).await;

        let mut effective = sample_publication("sku-2", "mall-1", "admin-1");
        effective.stable.status = ProductPublicationStatus::MallEffective;
        effective.stable.current_revision_id = Some("rev-1".to_string());
        db.product_publications()
            .create(
                &sample_publication("sku-1", "mall-1", "admin-1"),
                &mut NoTransaction,
            )
            .await
            .unwrap();
        db.product_publications()
            .create(&effective, &mut NoTransaction)
            .await
            .unwrap();
        db.product_publications()
            .create(
                &sample_publication("sku-3", "mall-2", "admin-1"),
                &mut NoTransaction,
            )
            .await
            .unwrap();

        let filter = ProductPublicationFilter {
            sku_id: None,
            target_mall_id: Some(SourceSystemId::new("mall-1")),
            status: Some(ProductPublicationStatus::Draft),
            page: 1,
            page_size: 1,
            sort_by: Some("sku_id".to_string()),
            sort_ascending: true,
        };
        let page = db
            .product_publications()
            .search_product_publications(&filter, &mut NoTransaction)
            .await
            .unwrap();
        assert_eq!(page.total, 1, "mall-1 下草稿状态只有一条");
        assert_eq!(page.items.len(), 1);
        let row = &page.items[0];
        assert_eq!(row.sku_id, "sku-1", "按 sku_id 升序取第一条");
        assert_eq!(row.status, ProductPublicationStatus::Draft);
        assert_eq!(row.target_mall_id, "mall-1");
        assert_eq!(row.current_revision_id, None);
        assert!(row.version >= 1);
        assert!(row.created_at > 0);

        let page_2 = ProductPublicationFilter {
            sku_id: None,
            target_mall_id: Some(SourceSystemId::new("mall-1")),
            status: None,
            page: 2,
            page_size: 1,
            sort_by: Some("任意字段".to_string()),
            sort_ascending: false,
        };
        let page = db
            .product_publications()
            .search_product_publications(&page_2, &mut NoTransaction)
            .await
            .unwrap();
        assert_eq!(page.total, 2);
        assert_eq!(page.items.len(), 1, "分页边界：第二页仍只有一条");
    })
}

#[tokio::test]
#[ignore]
async fn delivery_list_search_filters_by_status_and_mall() {
    require_mongo!(async {
        let test_db = TestDb::new("pub_delivery_list").await.unwrap();
        let db = test_db.db();
        assert_domain_indexes(db).await;

        let publication = sample_publication("sku-1", "mall-1", "admin-1");
        let publication_id = publication.base.id.clone().into();
        let revision = sample_revision(&publication_id, 1, "第一版");
        db.product_publication_revisions()
            .create(&revision, &mut NoTransaction)
            .await
            .unwrap();
        let revision_id = revision.base.id.clone().into();
        let mut confirmed = sample_delivery(&revision_id, "mall-1", PublicationDeliveryStatus::PendingSend);
        confirmed
            .update(entities::publication::ProductPublicationDeliveryUpdate {
                delivery_status: Some(PublicationDeliveryStatus::Confirmed),
                attempt_count: Some(2),
                last_attempt_at: Some(Instant::from_unix_secs(1_699_999_000)),
                mall_ack_at: Some(Instant::from_unix_secs(1_700_000_000)),
                mall_version: Some("v2".to_string()),
                ..Default::default()
            })
            .unwrap();
        let mut failed = sample_delivery(&revision_id, "mall-2", PublicationDeliveryStatus::PendingSend);
        failed
            .update(entities::publication::ProductPublicationDeliveryUpdate {
                delivery_status: Some(PublicationDeliveryStatus::Failed),
                attempt_count: Some(1),
                last_attempt_at: Some(Instant::from_unix_secs(1_700_000_000)),
                error_code: Some("TIMEOUT".to_string()),
                error_summary: Some("商城超时".to_string()),
                ..Default::default()
            })
            .unwrap();

        db.product_publication_deliveries()
            .create(
                &sample_delivery(&revision_id, "mall-1", PublicationDeliveryStatus::PendingSend),
                &mut NoTransaction,
            )
            .await
            .unwrap();
        db.product_publication_deliveries()
            .create(&confirmed, &mut NoTransaction)
            .await
            .unwrap();
        db.product_publication_deliveries()
            .create(&failed, &mut NoTransaction)
            .await
            .unwrap();

        let filter = ProductPublicationDeliveryFilter {
            target_mall_id: None,
            delivery_status: Some(PublicationDeliveryStatus::Confirmed),
            page: 1,
            page_size: 20,
            sort_by: Some("created_at".to_string()),
            sort_ascending: false,
        };
        let page = db
            .product_publication_deliveries()
            .search_product_publication_deliveries(&filter, &mut NoTransaction)
            .await
            .unwrap();
        assert_eq!(page.total, 1, "按状态筛选只命中已确认投递");
        let row = &page.items[0];
        assert_eq!(row.delivery_status, PublicationDeliveryStatus::Confirmed);
        assert_eq!(row.mall_version.as_deref(), Some("v2"));
        assert_eq!(row.attempt_count, 2);
        assert_eq!(row.publication_revision_id, revision.base.id);
        assert!(!mongodb::bson::to_document(row)
            .unwrap()
            .contains_key("error_summary"));

        let by_mall = ProductPublicationDeliveryFilter {
            target_mall_id: Some(SourceSystemId::new("mall-1")),
            delivery_status: None,
            page: 1,
            page_size: 20,
            sort_by: None,
            sort_ascending: false,
        };
        let page = db
            .product_publication_deliveries()
            .search_product_publication_deliveries(&by_mall, &mut NoTransaction)
            .await
            .unwrap();
        assert_eq!(page.total, 2, "mall-1 有两条投递");
    })
}

#[tokio::test]
#[ignore]
async fn multi_step_publication_revision_commits_atomically() {
    require_mongo!(async {
        let test_db = TestDb::new("pub_tx_commit").await.unwrap();
        let db = test_db.db();
        assert_domain_indexes(db).await;

        let publication = sample_publication("sku-1", "mall-1", "admin-1");
        let publication_id = publication.base.id.clone().into();
        let revision = sample_revision(&publication_id, 1, "第一版");

        let db_clone = db.clone();
        let publication_for_tx = publication.clone();
        let revision_for_tx = revision.clone();
        test_db
            .client()
            .with_transaction::<_, (), database::Error>(move |session| {
                Box::pin(async move {
                    db_clone
                        .publication()
                        .create_publication_revision(&publication_for_tx, &revision_for_tx, session)
                        .await?;
                    Ok(())
                })
            })
            .await
            .expect("发布与修订事务提交应成功");

        let publication_found = db
            .product_publications()
            .find_by_id(&publication.base.id, &mut NoTransaction)
            .await
            .unwrap();
        assert!(publication_found.is_some(), "事务提交后发布必须可见");
        let revision_found = db
            .product_publication_revisions()
            .find_revision_by_no(&publication_id, 1, &mut NoTransaction)
            .await
            .unwrap();
        assert!(revision_found.is_some(), "事务提交后修订必须可见");
    })
}

#[tokio::test]
#[ignore]
async fn transaction_abort_rolls_back_publication_and_revision() {
    require_mongo!(async {
        let test_db = TestDb::new("pub_tx_abort").await.unwrap();
        let db = test_db.db();
        assert_domain_indexes(db).await;

        let publication = sample_publication("sku-1", "mall-1", "admin-1");
        let publication_id = publication.base.id.clone().into();
        let revision = sample_revision(&publication_id, 1, "第一版");

        let db_clone = db.clone();
        let publication_for_tx = publication.clone();
        let revision_for_tx = revision.clone();
        let result: std::result::Result<(), database::Error> = test_db
            .client()
            .with_transaction(move |session| {
                Box::pin(async move {
                    db_clone
                        .publication()
                        .create_publication_revision(&publication_for_tx, &revision_for_tx, session)
                        .await?;
                    Err(database::Error::OptimisticLockingError)
                })
            })
            .await;
        assert!(result.is_err(), "闭包返回错误必须整体回滚");

        let publication_found = db
            .product_publications()
            .find_by_id(&publication.base.id, &mut NoTransaction)
            .await
            .unwrap();
        assert!(publication_found.is_none(), "回滚后发布不得残留");
        let revision_found = db
            .product_publication_revisions()
            .find_revision_by_no(&publication_id, 1, &mut NoTransaction)
            .await
            .unwrap();
        assert!(revision_found.is_none(), "回滚后修订不得残留");
    })
}

#[tokio::test]
#[ignore]
async fn no_transaction_partial_write_leaves_publication_without_revision() {
    require_mongo!(async {
        let test_db = TestDb::new("pub_no_tx").await.unwrap();
        let db = test_db.db();
        assert_domain_indexes(db).await;

        let publication_a = sample_publication("sku-1", "mall-1", "admin-1");
        let publication_id_a = publication_a.base.id.clone().into();
        db.publication()
            .create_publication_revision(
                &publication_a,
                &sample_revision(&publication_id_a, 1, "第一版"),
                &mut NoTransaction,
            )
            .await
            .unwrap();

        let publication_b = sample_publication("sku-2", "mall-2", "admin-1");
        let duplicate_revision = sample_revision(&publication_id_a, 1, "引用已占用修订序号");
        let error = db
            .publication()
            .create_publication_revision(&publication_b, &duplicate_revision, &mut NoTransaction)
            .await
            .expect_err("重复修订序号必须被唯一索引拒绝");
        assert!(matches!(error, database::Error::DuplicateKey(_)));

        let publication_b_found = db
            .product_publications()
            .find_by_id(&publication_b.base.id, &mut NoTransaction)
            .await
            .unwrap();
        assert!(
            publication_b_found.is_some(),
            "NoTransaction 下发布已自动提交，半成品状态可预期"
        );
        let revision_b_found = db
            .product_publication_revisions()
            .find_revision_by_no(&publication_b.base.id.clone().into(), 1, &mut NoTransaction)
            .await
            .unwrap();
        assert!(revision_b_found.is_none(), "冲突修订未写入，只有发布没有版本");
    })
}
