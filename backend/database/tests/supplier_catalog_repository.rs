//! 域 D24 `supplier_catalog` 仓储集成测试（真实 MongoDB 单节点副本集）。
//!
//! 统一 `#[ignore]` + `ERP_TEST_MONGO_URI` 门控（test-support `require_mongo!`）：
//! 无数据库环境 `cargo test` 跳过；CI 与验收执行
//! `cargo test -p database --test supplier_catalog_repository -- --include-ignored`。
//! 每个测试使用独立随机数据库名，结束自动 drop（TestDb）。

use database::repository::extensions::SupplierCatalogExt;
use database::{ensure_indexes, NoTransaction, Transactional};
use entities::common::time::{BusinessDate, Instant};
use entities::ids::{
    FileAssetId, SkuId, SupplierAccountId, SupplierCatalogIntakeBatchId, SupplierCatalogIntakeItemId,
    SupplierCatalogProductId, SupplierCatalogProductRevisionId, SupplierCatalogProductRevisionMediaId,
    SupplierCatalogSkuId, SupplierCatalogSkuRevisionId, SupplierOfferingId, SupplierOfferingRevisionId,
    SupplierProductMappingId,
};
use entities::money::{Amount, Quantity, Rate, UnitPrice};
use entities::supplier_catalog::{
    ArchiveStatus, AvailabilityStatus, CatalogItemStatus, CatalogSourceType, IntakeBatchStatus,
    IntakeItemClassification, IntakeItemResult, MappingStatus, MediaUsage, OfferingStatus, PrefillSourceRefs,
    SupplierCatalogIntakeBatch, SupplierCatalogIntakeBatchData, SupplierCatalogIntakeItem,
    SupplierCatalogIntakeItemData, SupplierCatalogProduct, SupplierCatalogProductData,
    SupplierCatalogProductRevision, SupplierCatalogProductRevisionData, SupplierCatalogProductRevisionMedia,
    SupplierCatalogProductRevisionMediaData, SupplierCatalogSku, SupplierCatalogSkuData,
    SupplierCatalogSkuRevision, SupplierCatalogSkuRevisionData, SupplierOffering, SupplierOfferingData,
    SupplierOfferingRevision, SupplierOfferingRevisionData, SupplierProductMapping,
    SupplierProductMappingData,
};
use mongodb::Database;
use std::str::FromStr;
use test_support::{assert_indexes, require_mongo, TestDb};

/// 供应商 SPU 列表筛选条件类型（经 `SupplierCatalogExt` 关联类型跨 crate 可达）。
type SupplierCatalogProductFilter = <Database as SupplierCatalogExt>::SupplierCatalogProductFilter;
/// 供应商 SKU 列表筛选条件类型。
type SupplierCatalogSkuFilter = <Database as SupplierCatalogExt>::SupplierCatalogSkuFilter;
/// 映射列表筛选条件类型。
type SupplierProductMappingFilter = <Database as SupplierCatalogExt>::SupplierProductMappingFilter;
/// 入库批次列表筛选条件类型。
type SupplierCatalogIntakeBatchFilter = <Database as SupplierCatalogExt>::SupplierCatalogIntakeBatchFilter;
/// 供给列表筛选条件类型。
type SupplierOfferingFilter = <Database as SupplierCatalogExt>::SupplierOfferingFilter;

/// 构造可复用的供应商 SPU 实体。
fn sample_product(supplier_id: &str, spu_code: &str) -> SupplierCatalogProduct {
    SupplierCatalogProduct::new(
        SupplierCatalogProductId::new(format!("scp-{spu_code}")),
        SupplierCatalogProductData {
            supplier_id: SupplierAccountId::new(supplier_id),
            source_type: CatalogSourceType::Excel,
            source_connection_id: None,
            supplier_spu_code: spu_code.to_string(),
        },
        "admin-1",
    )
    .unwrap()
}

/// 构造可复用的 SPU 来源修订。
fn sample_product_revision(
    product_id: &SupplierCatalogProductId,
    revision_no: u32,
) -> SupplierCatalogProductRevision {
    SupplierCatalogProductRevision::new(
        SupplierCatalogProductRevisionId::new(format!("scpr-{product_id}-{revision_no}")),
        SupplierCatalogProductRevisionData {
            supplier_catalog_product_id: product_id.clone(),
            revision_no,
            name: "慰问礼包".to_string(),
            description: Some("年节慰问组合".to_string()),
            source_product_kind: None,
            source_category: Some("食品".to_string()),
            source_brand: Some("华联".to_string()),
            structured_attributes: Vec::new(),
            source_revision_token: Some(format!("v{revision_no}")),
            source_updated_at: Instant::from_unix_secs(1_700_000_000),
            payload_hmac: format!("hmac-{revision_no}"),
            valid_from: Some(BusinessDate::from_ymd(2026, 1, 1).unwrap()),
            valid_to: Some(BusinessDate::from_ymd(2026, 12, 31).unwrap()),
        },
        CatalogSourceType::Excel,
    )
    .unwrap()
}

/// 构造可复用的来源图文（已归档媒体必须绑定受控文件）。
fn sample_media(revision_id: &SupplierCatalogProductRevisionId) -> SupplierCatalogProductRevisionMedia {
    SupplierCatalogProductRevisionMedia::new(
        SupplierCatalogProductRevisionMediaId::new(format!("scprm-{revision_id}-1")),
        SupplierCatalogProductRevisionMediaData {
            supplier_catalog_product_revision_id: revision_id.clone(),
            media_usage: MediaUsage::SpuCarousel,
            file_asset_id: Some(FileAssetId::new("file-1")),
            source_url_snapshot: Some("https://src.example.com/a.jpg".to_string()),
            archive_status: ArchiveStatus::Archived,
            sort_order: 1,
        },
    )
    .unwrap()
}

/// 构造可复用的供应商 SKU 实体。
fn sample_sku(product_id: &SupplierCatalogProductId, sku_code: &str) -> SupplierCatalogSku {
    SupplierCatalogSku::new(
        SupplierCatalogSkuId::new(format!("scs-{sku_code}")),
        SupplierCatalogSkuData {
            supplier_catalog_product_id: product_id.clone(),
            supplier_sku_code: sku_code.to_string(),
        },
        "admin-1",
    )
    .unwrap()
}

/// 构造可复用的来源 SKU 修订（含 Decimal128 观察价）。
fn sample_sku_revision(sku_id: &SupplierCatalogSkuId, revision_no: u32) -> SupplierCatalogSkuRevision {
    SupplierCatalogSkuRevision::new(
        SupplierCatalogSkuRevisionId::new(format!("scsr-{sku_id}-{revision_no}")),
        SupplierCatalogSkuRevisionData {
            supplier_catalog_sku_id: sku_id.clone(),
            revision_no,
            source_revision_token: Some(format!("v{revision_no}")),
            name: "慰问礼包·标准".to_string(),
            specification: "500g×2".to_string(),
            source_base_unit: Some("箱".to_string()),
            barcode: Some("690000000001".to_string()),
            structured_attributes: Vec::new(),
            source_main_image_asset_id: None,
            main_image_archive_status: None,
            dropship_floor_price_gross: Some(Amount::from_str("12.00").unwrap()),
            bulk_floor_price_gross: Some(Amount::from_str("10.00").unwrap()),
            bulk_minimum_order_quantity: Some(Quantity::from_str("10.000000").unwrap()),
            available_quantity: Some(Quantity::from_str("500.000000").unwrap()),
            availability_status: AvailabilityStatus::Available,
            source_updated_at: Instant::from_unix_secs(1_700_000_000),
            received_at: Instant::from_unix_secs(1_700_000_100),
            source_payload_hmac: None,
        },
    )
    .unwrap()
}

/// 构造可复用的供应商 SKU → 公司 SKU 映射。
fn sample_mapping(sku_id: &SupplierCatalogSkuId, target_sku_id: &str) -> SupplierProductMapping {
    SupplierProductMapping::new(
        SupplierProductMappingId::new(format!("spm-{sku_id}-{target_sku_id}")),
        SupplierProductMappingData {
            supplier_catalog_sku_id: sku_id.clone(),
            sku_id: SkuId::new(target_sku_id),
            status: MappingStatus::Pending,
            approved_by: None,
            approved_at: None,
            reason: Some("同款同规格".to_string()),
        },
    )
    .unwrap()
}

/// 构造可复用的来源入库批次。
fn sample_batch(supplier_id: &str, source_reference: &str) -> SupplierCatalogIntakeBatch {
    SupplierCatalogIntakeBatch::new(
        SupplierCatalogIntakeBatchId::new(format!("scib-{source_reference}")),
        SupplierCatalogIntakeBatchData {
            source_type: CatalogSourceType::Excel,
            supplier_id: SupplierAccountId::new(supplier_id),
            source_reference: source_reference.to_string(),
            source_connection_id: None,
            file_asset_id: Some(FileAssetId::new("file-1")),
        },
    )
    .unwrap()
}

/// 构造可复用的入库明细。
fn sample_item(batch_id: &SupplierCatalogIntakeBatchId, sku_code: &str) -> SupplierCatalogIntakeItem {
    SupplierCatalogIntakeItem::new(
        SupplierCatalogIntakeItemId::new(format!("scii-{batch_id}-{sku_code}")),
        SupplierCatalogIntakeItemData {
            supplier_catalog_intake_batch_id: batch_id.clone(),
            row_no: 1,
            supplier_sku_code: sku_code.to_string(),
            source_revision_token: Some("v1".to_string()),
            classification: IntakeItemClassification::New,
            result: IntakeItemResult::Success,
            error_text: None,
            supplier_catalog_sku_id: Some(SupplierCatalogSkuId::new(format!("scs-{sku_code}"))),
        },
    )
    .unwrap()
}

/// 构造可复用的供给稳定身份。
fn sample_offering(sku_id: &str, supplier_sku_id: &str) -> SupplierOffering {
    SupplierOffering::new(
        SupplierOfferingId::new(format!("so-{sku_id}-{supplier_sku_id}")),
        SupplierOfferingData {
            sku_id: SkuId::new(sku_id),
            supplier_id: SupplierAccountId::new("sup-1"),
            supplier_catalog_sku_id: SupplierCatalogSkuId::new(supplier_sku_id),
        },
        "admin-1",
    )
    .unwrap()
}

/// 构造一对换算一致的供给价（9.99 @13% → tax 1.30 → net 8.69）。
fn price_pair() -> (UnitPrice, UnitPrice) {
    (
        UnitPrice::from_str("9.9900").unwrap(),
        UnitPrice::from_str("8.6900").unwrap(),
    )
}

/// 构造可复用的供给修订（双价完整、区域 fail-closed 必填）。
fn sample_offering_revision(offering_id: &SupplierOfferingId, revision_no: u32) -> SupplierOfferingRevision {
    let (dropship_gross, dropship_net) = price_pair();
    let (bulk_gross, bulk_net) = price_pair();
    SupplierOfferingRevision::new(
        SupplierOfferingRevisionId::new(format!("sor-{offering_id}-{revision_no}")),
        SupplierOfferingRevisionData {
            supplier_offering_id: offering_id.clone(),
            revision_no,
            dropship_supply_price_gross: dropship_gross,
            dropship_supply_price_net: dropship_net,
            bulk_supply_price_gross: bulk_gross,
            bulk_supply_price_net: bulk_net,
            input_tax_rate: Rate::from_str("0.130000").unwrap(),
            dropship_express: Some("次日达".to_string()),
            freight_amount: Some(Amount::from_str("5.00").unwrap()),
            service_fee_amount: None,
            bulk_minimum_order_quantity: Quantity::from_str("10.000000").unwrap(),
            supply_region: vec!["全国".to_string()],
            availability_status: AvailabilityStatus::Available,
            available_quantity: Some(Quantity::from_str("100.000000").unwrap()),
            product_capabilities: vec!["CANCEL".to_string(), "REFUND".to_string()],
            valid_from: BusinessDate::from_ymd(2026, 1, 1).unwrap(),
            valid_to: Some(BusinessDate::from_ymd(2026, 12, 31).unwrap()),
            prefill_source_refs: PrefillSourceRefs::default(),
        },
    )
    .unwrap()
}

/// 执行 `ensure_indexes` 并断言本域全部必需索引就位。
async fn assert_domain_indexes(db: &Database) {
    ensure_indexes(db).await.expect("ensure_indexes 应成功");
    assert_indexes(
        db,
        <Database as SupplierCatalogExt>::SUPPLIER_CATALOG_PRODUCTS,
        &[
            "uk_supplier_catalog_products_supplier_code",
            "idx_supplier_catalog_products_supplier_status",
        ],
    )
    .await
    .expect("supplier_catalog_products 索引缺失");
    assert_indexes(
        db,
        <Database as SupplierCatalogExt>::SUPPLIER_CATALOG_PRODUCT_REVISIONS,
        &["uk_supplier_catalog_product_revisions_product_no"],
    )
    .await
    .expect("supplier_catalog_product_revisions 索引缺失");
    assert_indexes(
        db,
        <Database as SupplierCatalogExt>::SUPPLIER_CATALOG_PRODUCT_REVISION_MEDIA,
        &["uk_supplier_catalog_product_revision_media_usage_order"],
    )
    .await
    .expect("supplier_catalog_product_revision_media 索引缺失");
    assert_indexes(
        db,
        <Database as SupplierCatalogExt>::SUPPLIER_CATALOG_SKUS,
        &[
            "uk_supplier_catalog_skus_product_code",
            "idx_supplier_catalog_skus_product_status",
        ],
    )
    .await
    .expect("supplier_catalog_skus 索引缺失");
    assert_indexes(
        db,
        <Database as SupplierCatalogExt>::SUPPLIER_CATALOG_SKU_REVISIONS,
        &[
            "uk_supplier_catalog_sku_revisions_sku_no",
            "idx_supplier_catalog_sku_revisions_availability_freshness",
        ],
    )
    .await
    .expect("supplier_catalog_sku_revisions 索引缺失");
    assert_indexes(
        db,
        <Database as SupplierCatalogExt>::SUPPLIER_PRODUCT_MAPPINGS,
        &[
            "uk_supplier_product_mappings_active_sku",
            "idx_supplier_product_mappings_sku_status",
            "idx_supplier_product_mappings_target_sku_status",
        ],
    )
    .await
    .expect("supplier_product_mappings 索引缺失");
    assert_indexes(
        db,
        <Database as SupplierCatalogExt>::SUPPLIER_CATALOG_INTAKE_BATCHES,
        &[
            "uk_supplier_catalog_intake_batches_source_key",
            "idx_supplier_catalog_intake_batches_status_created",
        ],
    )
    .await
    .expect("supplier_catalog_intake_batches 索引缺失");
    assert_indexes(
        db,
        <Database as SupplierCatalogExt>::SUPPLIER_CATALOG_INTAKE_ITEMS,
        &["uk_supplier_catalog_intake_items_batch_sku_version"],
    )
    .await
    .expect("supplier_catalog_intake_items 索引缺失");
    assert_indexes(
        db,
        <Database as SupplierCatalogExt>::SUPPLIER_OFFERINGS,
        &[
            "uk_supplier_offerings_sku_supplier_sku",
            "idx_supplier_offerings_sku_status_validity",
            "idx_supplier_offerings_supplier_status",
        ],
    )
    .await
    .expect("supplier_offerings 索引缺失");
    assert_indexes(
        db,
        <Database as SupplierCatalogExt>::SUPPLIER_OFFERING_REVISIONS,
        &["uk_supplier_offering_revisions_offering_no"],
    )
    .await
    .expect("supplier_offering_revisions 索引缺失");
}

#[tokio::test]
#[ignore]
async fn create_and_read_roundtrip_preserves_decimal_amounts() {
    require_mongo!(async {
        let test_db = TestDb::new("sc_repo_roundtrip").await.unwrap();
        let db = test_db.db();
        assert_domain_indexes(db).await;

        let mut product = sample_product("sup-1", "SPU-001");
        db.supplier_catalog_products()
            .create(&product, &mut NoTransaction)
            .await
            .unwrap();
        let product_id = product.base.id.clone().into();
        let revision = sample_product_revision(&product_id, 1);
        let revision_id = revision.base.id.clone().into();
        product.stable.current_revision_id = Some(revision.base.id.clone());
        db.supplier_catalog()
            .create_product_with_revision(&mut product, &revision, &mut NoTransaction)
            .await
            .unwrap();
        assert_eq!(product.base.version, 2, "稳定表 CAS 成功后 version 递增");

        let media = sample_media(&revision_id);
        db.supplier_catalog_product_revision_media()
            .create(&media, &mut NoTransaction)
            .await
            .unwrap();

        let mut sku = sample_sku(&product_id, "SKU-001");
        db.supplier_catalog_skus()
            .create(&sku, &mut NoTransaction)
            .await
            .unwrap();
        let sku_id = sku.base.id.clone().into();
        let sku_revision = sample_sku_revision(&sku_id, 1);
        sku.stable.current_revision_id = Some(sku_revision.base.id.clone());
        db.supplier_catalog()
            .create_sku_with_revision(&mut sku, &sku_revision, &mut NoTransaction)
            .await
            .unwrap();

        let mut offering = sample_offering("sku-1", "scs-SKU-001");
        db.supplier_offerings()
            .create(&offering, &mut NoTransaction)
            .await
            .unwrap();
        let offering_id = offering.base.id.clone().into();
        let offering_revision = sample_offering_revision(&offering_id, 1);
        offering.stable.current_revision_id = Some(offering_revision.base.id.clone());
        db.supplier_catalog()
            .create_offering_with_revision(&mut offering, &offering_revision, &mut NoTransaction)
            .await
            .unwrap();

        let found_product = db
            .supplier_catalog_products()
            .find_by_id(&product.base.id, &mut NoTransaction)
            .await
            .unwrap()
            .expect("SPU 应可读回");
        assert_eq!(found_product.supplier_spu_code, "SPU-001");
        assert_eq!(found_product.supplier_id, SupplierAccountId::new("sup-1"));

        let found_revision = db
            .supplier_catalog_product_revisions()
            .find_by_id(&revision.base.id, &mut NoTransaction)
            .await
            .unwrap()
            .expect("来源修订应可读回");
        assert_eq!(found_revision.revision.revision_no, 1);
        assert_eq!(found_revision.name, "慰问礼包");
        assert_eq!(
            found_revision.source_updated_at,
            Instant::from_unix_secs(1_700_000_000)
        );

        let found_media = db
            .supplier_catalog_product_revision_media()
            .find_by_id(&media.base.id, &mut NoTransaction)
            .await
            .unwrap()
            .expect("图文应可读回");
        assert_eq!(found_media.media_usage, MediaUsage::SpuCarousel);
        assert_eq!(found_media.sort_order, 1);

        let found_sku_revision = db
            .supplier_catalog_sku_revisions()
            .find_by_id(&sku_revision.base.id, &mut NoTransaction)
            .await
            .unwrap()
            .expect("来源 SKU 修订应可读回");
        assert_eq!(
            found_sku_revision.dropship_floor_price_gross,
            Some(Amount::from_str("12.00").unwrap()),
            "Decimal128 观察价必须往返一致"
        );
        assert_eq!(
            found_sku_revision.bulk_floor_price_gross,
            Some(Amount::from_str("10.00").unwrap())
        );
        assert_eq!(
            found_sku_revision.available_quantity,
            Some(Quantity::from_str("500.000000").unwrap())
        );

        let found_offering_revision = db
            .supplier_offering_revisions()
            .find_by_id(&offering_revision.base.id, &mut NoTransaction)
            .await
            .unwrap()
            .expect("供给修订应可读回");
        assert_eq!(
            found_offering_revision.dropship_supply_price_gross,
            UnitPrice::from_str("9.9900").unwrap()
        );
        assert_eq!(
            found_offering_revision.dropship_supply_price_net,
            UnitPrice::from_str("8.6900").unwrap()
        );
        assert_eq!(
            found_offering_revision.bulk_minimum_order_quantity,
            Quantity::from_str("10.000000").unwrap()
        );
        assert_eq!(
            found_offering_revision.valid_from,
            BusinessDate::from_ymd(2026, 1, 1).unwrap()
        );
    })
}

#[tokio::test]
#[ignore]
async fn optimistic_lock_success_and_stale_version_conflict() {
    require_mongo!(async {
        let test_db = TestDb::new("sc_repo_optlock").await.unwrap();
        let db = test_db.db();
        assert_domain_indexes(db).await;

        let mut product = sample_product("sup-1", "SPU-010");
        db.supplier_catalog_products()
            .create(&product, &mut NoTransaction)
            .await
            .unwrap();
        let mut stale = product.clone();

        product.update(CatalogItemStatus::Stopped, "admin-2").unwrap();
        db.supplier_catalog_products()
            .update(&mut product, &mut NoTransaction)
            .await
            .unwrap();
        assert_eq!(product.base.version, 2, "乐观锁成功后 version 递增");
        assert_eq!(product.stable.status(), CatalogItemStatus::Stopped);
        stale.update(CatalogItemStatus::Exception, "admin-3").unwrap();
        let error = db
            .supplier_catalog_products()
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
async fn soft_delete_and_restore_keeps_identity_unique() {
    require_mongo!(async {
        let test_db = TestDb::new("sc_repo_softdel").await.unwrap();
        let db = test_db.db();
        assert_domain_indexes(db).await;

        let mut product = sample_product("sup-1", "SPU-011");
        db.supplier_catalog_products()
            .create(&product, &mut NoTransaction)
            .await
            .unwrap();

        db.supplier_catalog_products()
            .soft_delete(&mut product, &mut NoTransaction)
            .await
            .unwrap();
        let after_delete = db
            .supplier_catalog_products()
            .find_by_id(&product.base.id, &mut NoTransaction)
            .await
            .unwrap();
        assert!(after_delete.is_none(), "软删除后按 ID 不可见");

        let reuse = sample_product("sup-1", "SPU-011");
        let error = db
            .supplier_catalog_products()
            .create(&reuse, &mut NoTransaction)
            .await
            .expect_err("软删除后复用身份编码必须被全局唯一索引拒绝");
        assert!(
            matches!(error, database::Error::DuplicateKey(_)),
            "期望 DuplicateKey，实际为 {error:?}"
        );

        db.supplier_catalog_products()
            .restore(&mut product, &mut NoTransaction)
            .await
            .unwrap();
        let after_restore = db
            .supplier_catalog_products()
            .find_by_id(&product.base.id, &mut NoTransaction)
            .await
            .unwrap();
        assert!(after_restore.is_some(), "恢复后按 ID 重新可见");
    })
}

#[tokio::test]
#[ignore]
async fn unique_identity_conflicts_surface_duplicate_key() {
    require_mongo!(async {
        let test_db = TestDb::new("sc_repo_dup").await.unwrap();
        let db = test_db.db();
        assert_domain_indexes(db).await;

        let product = sample_product("sup-1", "SPU-012");
        db.supplier_catalog_products()
            .create(&product, &mut NoTransaction)
            .await
            .unwrap();

        let mut duplicate = sample_product("sup-1", "SPU-012");
        duplicate.base.id = "scp-dup".to_string();
        let error = db
            .supplier_catalog_products()
            .create(&duplicate, &mut NoTransaction)
            .await
            .expect_err("同一供应商重复 SPU 编码必须被唯一索引拒绝");
        assert!(
            matches!(error, database::Error::DuplicateKey(_)),
            "期望 DuplicateKey，实际为 {error:?}"
        );

        let other_supplier = sample_product("sup-2", "SPU-012");
        db.supplier_catalog_products()
            .create(&other_supplier, &mut NoTransaction)
            .await
            .unwrap();

        let sku = sample_sku(&product.base.id.clone().into(), "SKU-012");
        db.supplier_catalog_skus()
            .create(&sku, &mut NoTransaction)
            .await
            .unwrap();
        let mut sku_dup = sample_sku(&product.base.id.clone().into(), "SKU-012");
        sku_dup.base.id = "scs-dup".to_string();
        let error = db
            .supplier_catalog_skus()
            .create(&sku_dup, &mut NoTransaction)
            .await
            .expect_err("同一 SPU 重复 SKU 编码必须被唯一索引拒绝");
        assert!(
            matches!(error, database::Error::DuplicateKey(_)),
            "期望 DuplicateKey，实际为 {error:?}"
        );

        let offering = sample_offering("sku-1", "scs-SKU-012");
        db.supplier_offerings()
            .create(&offering, &mut NoTransaction)
            .await
            .unwrap();
        let mut offering_dup = sample_offering("sku-1", "scs-SKU-012");
        offering_dup.base.id = "so-dup".to_string();
        let error = db
            .supplier_offerings()
            .create(&offering_dup, &mut NoTransaction)
            .await
            .expect_err("重复稳定供给身份必须被唯一索引拒绝");
        assert!(
            matches!(error, database::Error::DuplicateKey(_)),
            "期望 DuplicateKey，实际为 {error:?}"
        );

        let batch = sample_batch("sup-1", "excel-2026-08-01.xlsx");
        db.supplier_catalog_intake_batches()
            .create(&batch, &mut NoTransaction)
            .await
            .unwrap();
        let mut batch_dup = sample_batch("sup-1", "excel-2026-08-01.xlsx");
        batch_dup.base.id = "scib-dup".to_string();
        let error = db
            .supplier_catalog_intake_batches()
            .create(&batch_dup, &mut NoTransaction)
            .await
            .expect_err("重复来源批次键必须被唯一索引拒绝");
        assert!(
            matches!(error, database::Error::DuplicateKey(_)),
            "期望 DuplicateKey，实际为 {error:?}"
        );
    })
}

#[tokio::test]
#[ignore]
async fn active_mapping_unique_is_scoped_to_status() {
    require_mongo!(async {
        let test_db = TestDb::new("sc_repo_mapping").await.unwrap();
        let db = test_db.db();
        assert_domain_indexes(db).await;

        let product = sample_product("sup-1", "SPU-013");
        db.supplier_catalog_products()
            .create(&product, &mut NoTransaction)
            .await
            .unwrap();
        let sku = sample_sku(&product.base.id.clone().into(), "SKU-013");
        db.supplier_catalog_skus()
            .create(&sku, &mut NoTransaction)
            .await
            .unwrap();
        let sku_id = sku.base.id.clone().into();

        let mut pending = sample_mapping(&sku_id, "sku-100");
        db.supplier_product_mappings()
            .create(&pending, &mut NoTransaction)
            .await
            .unwrap();
        let pending_again = sample_mapping(&sku_id, "sku-101");
        db.supplier_product_mappings()
            .create(&pending_again, &mut NoTransaction)
            .await
            .unwrap();
        let second_pending = sample_mapping(&sku_id, "sku-102");
        db.supplier_product_mappings()
            .create(&second_pending, &mut NoTransaction)
            .await
            .unwrap();

        pending
            .update(
                MappingStatus::Active,
                Some("buyer-1".to_string()),
                Some(Instant::from_unix_secs(1_700_000_000)),
            )
            .unwrap();
        db.supplier_product_mappings()
            .update(&mut pending, &mut NoTransaction)
            .await
            .unwrap();

        let duplicate_active = SupplierProductMapping::new(
            SupplierProductMappingId::new("spm-dup-active"),
            SupplierProductMappingData {
                supplier_catalog_sku_id: sku_id.clone(),
                sku_id: SkuId::new("sku-200"),
                status: MappingStatus::Active,
                approved_by: Some("buyer-2".to_string()),
                approved_at: Some(Instant::from_unix_secs(1_700_000_100)),
                reason: Some("重复生效".to_string()),
            },
        )
        .unwrap();
        let error = db
            .supplier_product_mappings()
            .create(&duplicate_active, &mut NoTransaction)
            .await
            .expect_err("同一供应商 SKU 同时两个生效映射必须被部分唯一索引拒绝");
        assert!(
            matches!(error, database::Error::DuplicateKey(_)),
            "期望 DuplicateKey，实际为 {error:?}"
        );

        let active_found = db
            .supplier_product_mappings()
            .find_active_by_supplier_sku(&sku_id, &mut NoTransaction)
            .await
            .unwrap()
            .expect("应命中唯一生效映射");
        assert_eq!(active_found.sku_id, SkuId::new("sku-100"));

        let filter = SupplierProductMappingFilter {
            supplier_catalog_sku_id: Some(sku_id),
            sku_id: None,
            status: Some(MappingStatus::Pending),
            page: 1,
            page_size: 20,
            sort_by: None,
            sort_ascending: false,
        };
        let page = db
            .supplier_product_mappings()
            .search_supplier_product_mappings(&filter, &mut NoTransaction)
            .await
            .unwrap();
        assert_eq!(page.total, 2, "待审核映射可多条并存");
    })
}

#[tokio::test]
#[ignore]
async fn product_list_search_pagination_projection_and_regex() {
    require_mongo!(async {
        let test_db = TestDb::new("sc_repo_list").await.unwrap();
        let db = test_db.db();
        assert_domain_indexes(db).await;

        db.supplier_catalog_products()
            .create(&sample_product("sup-1", "SPU-020"), &mut NoTransaction)
            .await
            .unwrap();
        db.supplier_catalog_products()
            .create(&sample_product("sup-1", "SPU-021"), &mut NoTransaction)
            .await
            .unwrap();
        let mut stopped = sample_product("sup-2", "SPU-022");
        stopped.update(CatalogItemStatus::Stopped, "admin-1").unwrap();
        db.supplier_catalog_products()
            .create(&stopped, &mut NoTransaction)
            .await
            .unwrap();

        let filter = SupplierCatalogProductFilter {
            supplier_id: Some(SupplierAccountId::new("sup-1")),
            source_type: Some(CatalogSourceType::Excel),
            status: Some(CatalogItemStatus::Active),
            supplier_spu_code: Some("spu-02".to_string()),
            page: 1,
            page_size: 1,
            sort_by: Some("supplier_spu_code".to_string()),
            sort_ascending: true,
        };
        let page = db
            .supplier_catalog_products()
            .search_supplier_catalog_products(&filter, &mut NoTransaction)
            .await
            .unwrap();
        assert_eq!(page.total, 2, "sup-1 且编码含 spu-02 共两条");
        assert_eq!(page.items.len(), 1, "单页一条");
        let row = &page.items[0];
        assert_eq!(row.supplier_spu_code, "SPU-020", "编码升序第一页应为最小编码");
        assert_eq!(row.supplier_id, SupplierAccountId::new("sup-1"));
        assert_eq!(row.source_type, CatalogSourceType::Excel);
        assert_eq!(row.status, CatalogItemStatus::Active);
        assert!(row.version >= 1);
        assert!(row.created_at > 0);

        let page_two = db
            .supplier_catalog_products()
            .search_supplier_catalog_products(
                &SupplierCatalogProductFilter { page: 2, ..filter },
                &mut NoTransaction,
            )
            .await
            .unwrap();
        assert_eq!(page_two.items.len(), 1, "第二页一条");
        assert_eq!(page_two.items[0].supplier_spu_code, "SPU-021");

        let bogus_sort = SupplierCatalogProductFilter {
            supplier_id: None,
            source_type: None,
            status: None,
            supplier_spu_code: None,
            page: 1,
            page_size: 20,
            sort_by: Some("arbitrary_field".to_string()),
            sort_ascending: true,
        };
        let all = db
            .supplier_catalog_products()
            .search_supplier_catalog_products(&bogus_sort, &mut NoTransaction)
            .await
            .unwrap();
        assert_eq!(all.total, 3, "未知排序字段不影响筛选");
        assert_eq!(
            all.items[0].supplier_spu_code, "SPU-020",
            "未知排序字段必须回退 created_at 升序（先建先出）"
        );

        let sku_filter = SupplierCatalogSkuFilter {
            supplier_catalog_product_id: Some(row.id.clone().into()),
            status: None,
            supplier_sku_code: None,
            page: 1,
            page_size: 20,
            sort_by: None,
            sort_ascending: false,
        };
        let sku = sample_sku(&row.id.clone().into(), "SKU-020");
        db.supplier_catalog_skus()
            .create(&sku, &mut NoTransaction)
            .await
            .unwrap();
        let sku_page = db
            .supplier_catalog_skus()
            .search_supplier_catalog_skus(&sku_filter, &mut NoTransaction)
            .await
            .unwrap();
        assert_eq!(sku_page.total, 1);
        assert_eq!(sku_page.items[0].supplier_sku_code, "SKU-020");
    })
}

#[tokio::test]
#[ignore]
async fn intake_batch_list_and_batch_item_queries() {
    require_mongo!(async {
        let test_db = TestDb::new("sc_repo_intake").await.unwrap();
        let db = test_db.db();
        assert_domain_indexes(db).await;

        let mut batch = sample_batch("sup-1", "excel-2026-08-01.xlsx");
        let batch_id = batch.base.id.clone().into();
        let items = vec![
            sample_item(&batch_id, "SKU-001"),
            sample_item(&batch_id, "SKU-002"),
        ];
        db.supplier_catalog()
            .create_intake_batch(&batch, &items, &mut NoTransaction)
            .await
            .unwrap();
        batch.update(IntakeBatchStatus::Completed, None).unwrap();
        db.supplier_catalog_intake_batches()
            .update(&mut batch, &mut NoTransaction)
            .await
            .unwrap();

        let mut failed = sample_batch("sup-1", "excel-2026-08-02.xlsx");
        let failed_id: SupplierCatalogIntakeBatchId = failed.base.id.clone().into();
        let failed_item = SupplierCatalogIntakeItem::new(
            SupplierCatalogIntakeItemId::new("scii-failed"),
            SupplierCatalogIntakeItemData {
                supplier_catalog_intake_batch_id: failed_id.clone(),
                row_no: 1,
                supplier_sku_code: "SKU-BAD".to_string(),
                source_revision_token: None,
                classification: IntakeItemClassification::Exception,
                result: IntakeItemResult::Failed,
                error_text: Some("编码无法解析".to_string()),
                supplier_catalog_sku_id: None,
            },
        )
        .unwrap();
        db.supplier_catalog()
            .create_intake_batch(&failed, &[failed_item], &mut NoTransaction)
            .await
            .unwrap();
        failed
            .update(IntakeBatchStatus::Failed, Some("解析失败".to_string()))
            .unwrap();
        db.supplier_catalog_intake_batches()
            .update(&mut failed, &mut NoTransaction)
            .await
            .unwrap();

        let filter = SupplierCatalogIntakeBatchFilter {
            source_type: Some(CatalogSourceType::Excel),
            supplier_id: Some(SupplierAccountId::new("sup-1")),
            status: None,
            page: 1,
            page_size: 1,
            sort_by: Some("status".to_string()),
            sort_ascending: true,
        };
        let page = db
            .supplier_catalog_intake_batches()
            .search_supplier_catalog_intake_batches(&filter, &mut NoTransaction)
            .await
            .unwrap();
        assert_eq!(page.total, 2);
        assert_eq!(page.items.len(), 1);
        assert_eq!(
            page.items[0].status,
            IntakeBatchStatus::Completed,
            "状态升序首条为 COMPLETED"
        );

        let items_found = db
            .supplier_catalog_intake_items()
            .find_items_by_batch_ids(&[batch_id.clone(), failed_id.clone()], &mut NoTransaction)
            .await
            .unwrap();
        assert_eq!(items_found.len(), 3, "两个批次明细一次 $in 取回");

        let failed_count = db
            .supplier_catalog_intake_items()
            .count_failed_items_by_batch_id(&failed_id, &mut NoTransaction)
            .await
            .unwrap();
        assert_eq!(failed_count, 1, "失败明细统计");

        let by_source = db
            .supplier_catalog_intake_batches()
            .find_by_source_key(
                CatalogSourceType::Excel,
                &SupplierAccountId::new("sup-1"),
                "excel-2026-08-01.xlsx",
                &mut NoTransaction,
            )
            .await
            .unwrap()
            .expect("来源键应命中批次");
        assert_eq!(by_source.status, IntakeBatchStatus::Completed);

        let dup = sample_batch("sup-1", "excel-2026-08-01.xlsx");
        let error = db
            .supplier_catalog_intake_batches()
            .create(&dup, &mut NoTransaction)
            .await
            .expect_err("同一来源键重复批次必须被唯一索引拒绝");
        assert!(
            matches!(error, database::Error::DuplicateKey(_)),
            "期望 DuplicateKey，实际为 {error:?}"
        );
    })
}

#[tokio::test]
#[ignore]
async fn offering_list_and_batch_queries_by_sku() {
    require_mongo!(async {
        let test_db = TestDb::new("sc_repo_offering").await.unwrap();
        let db = test_db.db();
        assert_domain_indexes(db).await;

        let offering_one = sample_offering("sku-1", "scs-1");
        db.supplier_offerings()
            .create(&offering_one, &mut NoTransaction)
            .await
            .unwrap();
        let mut offering_two = sample_offering("sku-2", "scs-2");
        offering_two.supplier_id = SupplierAccountId::new("sup-2");
        db.supplier_offerings()
            .create(&offering_two, &mut NoTransaction)
            .await
            .unwrap();
        let offering_three = sample_offering("sku-1", "scs-3");
        db.supplier_offerings()
            .create(&offering_three, &mut NoTransaction)
            .await
            .unwrap();

        let by_skus = db
            .supplier_offerings()
            .find_by_sku_ids(&[SkuId::new("sku-1"), SkuId::new("sku-2")], &mut NoTransaction)
            .await
            .unwrap();
        assert_eq!(by_skus.len(), 3, "按 SKU $in 批量取回全部供给");

        let filter = SupplierOfferingFilter {
            sku_id: Some(SkuId::new("sku-1")),
            supplier_id: None,
            status: Some(OfferingStatus::Active),
            page: 1,
            page_size: 20,
            sort_by: None,
            sort_ascending: false,
        };
        let page = db
            .supplier_offerings()
            .search_supplier_offerings(&filter, &mut NoTransaction)
            .await
            .unwrap();
        assert_eq!(page.total, 2, "sku-1 的启用供给两条");
        assert_eq!(page.items[0].supplier_id, SupplierAccountId::new("sup-1"));
        assert_eq!(page.items[0].sku_id, SkuId::new("sku-1"));
    })
}

#[tokio::test]
#[ignore]
async fn multi_step_intake_batch_rolls_back_with_items() {
    require_mongo!(async {
        let test_db = TestDb::new("sc_repo_tx").await.unwrap();
        let db = test_db.db();
        assert_domain_indexes(db).await;

        let batch = sample_batch("sup-1", "excel-2026-08-03.xlsx");
        let batch_id = batch.base.id.clone().into();
        let items = vec![
            sample_item(&batch_id, "SKU-003"),
            sample_item(&batch_id, "SKU-004"),
        ];

        let db_clone = db.clone();
        let batch_for_tx = batch.clone();
        let items_for_tx = items.clone();
        test_db
            .client()
            .with_transaction::<_, (), database::Error>(move |session| {
                Box::pin(async move {
                    db_clone
                        .supplier_catalog()
                        .create_intake_batch(&batch_for_tx, &items_for_tx, session)
                        .await?;
                    Ok(())
                })
            })
            .await
            .expect("事务提交应成功");

        let found = db
            .supplier_catalog_intake_batches()
            .find_by_id(&batch.base.id, &mut NoTransaction)
            .await
            .unwrap();
        assert!(found.is_some(), "事务提交后批次必须可见");
        let found_items = db
            .supplier_catalog_intake_items()
            .find_items_by_batch_ids(&[batch_id], &mut NoTransaction)
            .await
            .unwrap();
        assert_eq!(found_items.len(), 2, "事务提交后明细必须可见");

        let rollback_batch = sample_batch("sup-1", "excel-2026-08-04.xlsx");
        let rollback_id = rollback_batch.base.id.clone().into();
        let rollback_items = vec![sample_item(&rollback_id, "SKU-005")];
        let db_clone = db.clone();
        let batch_for_tx = rollback_batch.clone();
        let items_for_tx = rollback_items.clone();
        let result: std::result::Result<(), database::Error> = test_db
            .client()
            .with_transaction(move |session| {
                Box::pin(async move {
                    db_clone
                        .supplier_catalog()
                        .create_intake_batch(&batch_for_tx, &items_for_tx, session)
                        .await?;
                    Err(database::Error::OptimisticLockingError)
                })
            })
            .await;
        assert!(result.is_err(), "闭包返回错误必须整体回滚");

        let rolled_back = db
            .supplier_catalog_intake_batches()
            .find_by_id(&rollback_batch.base.id, &mut NoTransaction)
            .await
            .unwrap();
        assert!(rolled_back.is_none(), "回滚后批次不得残留");
        let rolled_back_items = db
            .supplier_catalog_intake_items()
            .find_items_by_batch_ids(&[rollback_id], &mut NoTransaction)
            .await
            .unwrap();
        assert!(rolled_back_items.is_empty(), "回滚后明细不得残留");
    })
}

#[tokio::test]
#[ignore]
async fn multi_step_product_revision_conflict_rolls_back_and_no_transaction_is_predictable() {
    require_mongo!(async {
        let test_db = TestDb::new("sc_repo_multi").await.unwrap();
        let db = test_db.db();
        assert_domain_indexes(db).await;

        let mut product = sample_product("sup-1", "SPU-030");
        db.supplier_catalog_products()
            .create(&product, &mut NoTransaction)
            .await
            .unwrap();
        let stale = product.clone();

        product.update(CatalogItemStatus::Stopped, "admin-2").unwrap();
        db.supplier_catalog_products()
            .update(&mut product, &mut NoTransaction)
            .await
            .unwrap();
        assert_eq!(product.base.version, 2);
        let product_id = product.base.id.clone().into();
        let revision = sample_product_revision(&product_id, 1);

        let db_clone = db.clone();
        let mut stale_for_tx = stale.clone();
        stale_for_tx.stable.current_revision_id = Some(revision.base.id.clone());
        let revision_for_tx = revision.clone();
        let result: std::result::Result<(), database::Error> = test_db
            .client()
            .with_transaction(move |session| {
                Box::pin(async move {
                    let error = db_clone
                        .supplier_catalog()
                        .create_product_with_revision(&mut stale_for_tx, &revision_for_tx, session)
                        .await
                        .expect_err("陈旧版本 CAS 必须失败");
                    assert!(
                        matches!(error, database::Error::OptimisticLockingError),
                        "期望 OptimisticLockingError，实际为 {error:?}"
                    );
                    Err(database::Error::OptimisticLockingError)
                })
            })
            .await;
        assert!(result.is_err(), "闭包返回错误必须整体回滚");

        let revision_found = db
            .supplier_catalog_product_revisions()
            .find_by_id(&revision.base.id, &mut NoTransaction)
            .await
            .unwrap();
        assert!(revision_found.is_none(), "回滚后修订不得残留");
        let product_found = db
            .supplier_catalog_products()
            .find_by_id(&product.base.id, &mut NoTransaction)
            .await
            .unwrap()
            .expect("SPU 仍应存在");
        assert_eq!(product_found.base.version, 2);
        assert!(
            product_found.stable.current_revision_id.is_none(),
            "指针未被半成品改动"
        );

        let mut no_tx_stale = stale.clone();
        no_tx_stale.stable.current_revision_id = Some(revision.base.id.clone());
        let error = db
            .supplier_catalog()
            .create_product_with_revision(&mut no_tx_stale, &revision, &mut NoTransaction)
            .await
            .expect_err("无事务时陈旧版本 CAS 同样失败");
        assert!(
            matches!(error, database::Error::OptimisticLockingError),
            "期望 OptimisticLockingError，实际为 {error:?}"
        );
        let revision_found = db
            .supplier_catalog_product_revisions()
            .find_by_id(&revision.base.id, &mut NoTransaction)
            .await
            .unwrap();
        assert!(
            revision_found.is_some(),
            "NoTransaction 下修订已自动提交（可预期半成品）"
        );
    })
}

#[tokio::test]
#[ignore]
async fn media_and_revision_batch_queries() {
    require_mongo!(async {
        let test_db = TestDb::new("sc_repo_batch").await.unwrap();
        let db = test_db.db();
        assert_domain_indexes(db).await;

        let product = sample_product("sup-1", "SPU-040");
        db.supplier_catalog_products()
            .create(&product, &mut NoTransaction)
            .await
            .unwrap();
        let product_id = product.base.id.clone().into();
        let revision_one = sample_product_revision(&product_id, 1);
        let revision_two = sample_product_revision(&product_id, 2);
        let revision_one_id = revision_one.base.id.clone().into();
        let revision_two_id = revision_two.base.id.clone().into();
        db.supplier_catalog_product_revisions()
            .create(&revision_one, &mut NoTransaction)
            .await
            .unwrap();
        db.supplier_catalog_product_revisions()
            .create(&revision_two, &mut NoTransaction)
            .await
            .unwrap();

        let media_one = sample_media(&revision_one_id);
        db.supplier_catalog_product_revision_media()
            .create(&media_one, &mut NoTransaction)
            .await
            .unwrap();
        let media_two = sample_media(&revision_two_id);
        db.supplier_catalog_product_revision_media()
            .create(&media_two, &mut NoTransaction)
            .await
            .unwrap();

        let revisions = db
            .supplier_catalog_product_revisions()
            .find_revisions_by_product_ids(&[product_id], &mut NoTransaction)
            .await
            .unwrap();
        assert_eq!(revisions.len(), 2, "同 SPU 两个修订一次 $in 取回");

        let media = db
            .supplier_catalog_product_revision_media()
            .find_media_by_revision_ids(&[revision_one_id.clone(), revision_two_id], &mut NoTransaction)
            .await
            .unwrap();
        assert_eq!(media.len(), 2, "两个修订的图文一次 $in 取回");

        let duplicate_media = SupplierCatalogProductRevisionMedia::new(
            SupplierCatalogProductRevisionMediaId::new("scprm-dup"),
            SupplierCatalogProductRevisionMediaData {
                supplier_catalog_product_revision_id: revision_one_id.clone(),
                media_usage: MediaUsage::SpuCarousel,
                file_asset_id: Some(FileAssetId::new("file-2")),
                source_url_snapshot: None,
                archive_status: ArchiveStatus::Archived,
                sort_order: 1,
            },
        )
        .unwrap();
        let error = db
            .supplier_catalog_product_revision_media()
            .create(&duplicate_media, &mut NoTransaction)
            .await
            .expect_err("同一修订同用途同顺序图文必须被唯一索引拒绝");
        assert!(
            matches!(error, database::Error::DuplicateKey(_)),
            "期望 DuplicateKey，实际为 {error:?}"
        );
    })
}

#[tokio::test]
#[ignore]
async fn sku_revision_batch_and_freshness_read_paths() {
    require_mongo!(async {
        let test_db = TestDb::new("sc_repo_sku_rev").await.unwrap();
        let db = test_db.db();
        assert_domain_indexes(db).await;

        let product = sample_product("sup-1", "SPU-050");
        db.supplier_catalog_products()
            .create(&product, &mut NoTransaction)
            .await
            .unwrap();
        let product_id = product.base.id.clone().into();
        let sku = sample_sku(&product_id, "SKU-050");
        db.supplier_catalog_skus()
            .create(&sku, &mut NoTransaction)
            .await
            .unwrap();
        let sku_id = sku.base.id.clone().into();
        let sku_revision_one = sample_sku_revision(&sku_id, 1);
        let sku_revision_two = sample_sku_revision(&sku_id, 2);
        db.supplier_catalog_sku_revisions()
            .create(&sku_revision_one, &mut NoTransaction)
            .await
            .unwrap();
        db.supplier_catalog_sku_revisions()
            .create(&sku_revision_two, &mut NoTransaction)
            .await
            .unwrap();

        let revisions = db
            .supplier_catalog_sku_revisions()
            .find_revisions_by_sku_ids(std::slice::from_ref(&sku_id), &mut NoTransaction)
            .await
            .unwrap();
        assert_eq!(revisions.len(), 2, "同 SKU 两个来源修订一次 $in 取回");

        let duplicate = sample_sku_revision(&sku_id, 2);
        let error = db
            .supplier_catalog_sku_revisions()
            .create(&duplicate, &mut NoTransaction)
            .await
            .expect_err("同一 SKU 重复修订号必须被唯一索引拒绝");
        assert!(
            matches!(error, database::Error::DuplicateKey(_)),
            "期望 DuplicateKey，实际为 {error:?}"
        );

        let offering = sample_offering("sku-1", "scs-SKU-050");
        db.supplier_offerings()
            .create(&offering, &mut NoTransaction)
            .await
            .unwrap();
        let offering_id = offering.base.id.clone().into();
        let offering_revision = sample_offering_revision(&offering_id, 1);
        db.supplier_offering_revisions()
            .create(&offering_revision, &mut NoTransaction)
            .await
            .unwrap();

        let offering_revisions = db
            .supplier_offering_revisions()
            .find_revisions_by_offering_ids(&[offering_id], &mut NoTransaction)
            .await
            .unwrap();
        assert_eq!(offering_revisions.len(), 1);
        assert_eq!(
            offering_revisions[0].bulk_supply_price_gross,
            UnitPrice::from_str("9.9900").unwrap()
        );
    })
}
