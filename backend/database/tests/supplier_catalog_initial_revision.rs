//! 供应商目录首次创建的稳定身份与首版修订持久化回归测试。

use database::{ensure_indexes, SupplierCatalogExt, Transactional};
use entities::common::time::Instant;
use entities::ids::{
    SupplierAccountId, SupplierCatalogProductId, SupplierCatalogProductRevisionId, SupplierCatalogSkuId,
    SupplierCatalogSkuRevisionId,
};
use entities::supplier_catalog::{
    AvailabilityStatus, CatalogSourceType, SupplierCatalogProduct, SupplierCatalogProductData,
    SupplierCatalogProductRevision, SupplierCatalogProductRevisionData, SupplierCatalogSku,
    SupplierCatalogSkuData, SupplierCatalogSkuRevision, SupplierCatalogSkuRevisionData,
};
use test_support::{require_mongo, TestDb};

#[tokio::test]
#[ignore = "需要 ERP_TEST_MONGO_URI 指向 MongoDB 副本集"]
async fn initial_revisions_create_stable_identities_and_current_pointers() {
    require_mongo!(async {
        let fixture = TestDb::new("supplier_catalog_initial")
            .await
            .expect("测试数据库创建失败");
        ensure_indexes(fixture.db()).await.expect("索引创建失败");

        let product_id = SupplierCatalogProductId::new("product-1".to_string());
        let product_revision_id = SupplierCatalogProductRevisionId::new("product-revision-1".to_string());
        let product = SupplierCatalogProduct::new(
            product_id.clone(),
            SupplierCatalogProductData {
                supplier_id: SupplierAccountId::new("supplier-1".to_string()),
                source_type: CatalogSourceType::Manual,
                source_connection_id: None,
                supplier_spu_code: "SPU-001".to_string(),
            },
            "tester",
        )
        .expect("SPU 构造失败");
        let product_revision = SupplierCatalogProductRevision::new(
            product_revision_id.clone(),
            SupplierCatalogProductRevisionData {
                supplier_catalog_product_id: product_id.clone(),
                revision_no: 1,
                name: "测试商品".to_string(),
                description: None,
                source_product_kind: Some("PHYSICAL".to_string()),
                source_category: Some("测试分类".to_string()),
                source_brand: None,
                structured_attributes: Vec::new(),
                source_revision_token: None,
                source_updated_at: Instant::now(),
                payload_hmac: "product-hmac".to_string(),
                valid_from: None,
                valid_to: None,
            },
            CatalogSourceType::Manual,
        )
        .expect("SPU 修订构造失败");

        let sku_id = SupplierCatalogSkuId::new("sku-1".to_string());
        let sku_revision_id = SupplierCatalogSkuRevisionId::new("sku-revision-1".to_string());
        let sku = SupplierCatalogSku::new(
            sku_id.clone(),
            SupplierCatalogSkuData {
                supplier_catalog_product_id: product_id,
                supplier_sku_code: "SKU-001".to_string(),
            },
            "tester",
        )
        .expect("SKU 构造失败");
        let sku_revision = SupplierCatalogSkuRevision::new(
            sku_revision_id.clone(),
            SupplierCatalogSkuRevisionData {
                supplier_catalog_sku_id: sku_id.clone(),
                revision_no: 1,
                source_revision_token: None,
                name: "测试商品".to_string(),
                specification: "默认规格".to_string(),
                source_base_unit: Some("件".to_string()),
                barcode: None,
                structured_attributes: Vec::new(),
                source_main_image_asset_id: None,
                source_main_image_url_snapshot: None,
                main_image_archive_status: None,
                dropship_floor_price_gross: None,
                bulk_floor_price_gross: None,
                bulk_minimum_order_quantity: None,
                available_quantity: None,
                availability_status: AvailabilityStatus::Available,
                source_updated_at: Instant::now(),
                received_at: Instant::now(),
                source_payload_hmac: Some("sku-hmac".to_string()),
            },
        )
        .expect("SKU 修订构造失败");

        let db = fixture.db().clone();
        let client = db.client().clone();
        client
            .with_transaction(move |session| {
                Box::pin(async move {
                    db.supplier_catalog()
                        .create_product_with_initial_revision(&product, &product_revision, session)
                        .await?;
                    db.supplier_catalog()
                        .create_sku_with_initial_revision(&sku, &sku_revision, session)
                        .await?;
                    Ok::<(), database::Error>(())
                })
            })
            .await
            .expect("首次创建事务失败");

        let stored_product = fixture
            .db()
            .supplier_catalog_products()
            .find_by_id("product-1", &mut database::NoTransaction)
            .await
            .expect("SPU 查询失败")
            .expect("SPU 未写入");
        let stored_sku = fixture
            .db()
            .supplier_catalog_skus()
            .find_by_id("sku-1", &mut database::NoTransaction)
            .await
            .expect("SKU 查询失败")
            .expect("SKU 未写入");

        assert_eq!(
            stored_product.stable.current_revision_id.as_deref(),
            Some(product_revision_id.to_string().as_str())
        );
        assert_eq!(
            stored_sku.stable.current_revision_id.as_deref(),
            Some(sku_revision_id.to_string().as_str())
        );
    });
}
