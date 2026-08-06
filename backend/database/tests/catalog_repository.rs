//! 域 D10 `catalog` 仓储集成测试（真实 MongoDB 单节点副本集）。
//!
//! 统一 `#[ignore]` + `ERP_TEST_MONGO_URI` 门控（test-support `require_mongo!`）：
//! 无数据库环境 `cargo test` 跳过；CI 与验收执行
//! `cargo test -p database --test catalog_repository -- --include-ignored`。
//! 每个测试使用独立随机数据库名，结束自动 drop（TestDb）。

use std::str::FromStr;

use database::repository::extensions::CatalogExt;
use database::{ensure_indexes, NoTransaction, Transactional};
use entities::catalog::product::ProductData;
use entities::catalog::product_brand::ProductBrandData;
use entities::catalog::product_category::{ProductCategoryData, ProductCategoryUpdate};
use entities::catalog::product_revision::ProductRevisionData;
use entities::catalog::product_revision_media::MediaRole;
use entities::catalog::product_revision_media::ProductRevisionMediaData;
use entities::catalog::sku::SkuData;
use entities::catalog::sku_attribute::AttributeValueType;
use entities::catalog::sku_attribute::SkuAttributeData;
use entities::catalog::sku_attribute_value::SkuAttributeValueData;
use entities::catalog::sku_revision::SkuRevisionData;
use entities::catalog::sku_revision_attribute_value::SkuRevisionAttributeValueData;
use entities::catalog::unit_of_measure::UnitOfMeasureData;
use entities::catalog::voucher_category_profile_revision::VoucherCategoryProfileRevisionData;
use entities::catalog::{
    EnableStatus, Product, ProductBrand, ProductCategory, ProductKind, ProductRevision, ProductRevisionMedia,
    Sku, SkuAttribute, SkuAttributeValue, SkuRevision, SkuRevisionAttributeValue, UnitOfMeasure,
    VoucherCategoryProfileRevision,
};
use entities::common::time::BusinessDate;
use entities::ids::{
    ProductBrandId, ProductCategoryId, ProductId, ProductRevisionId, ProductRevisionMediaId, SkuAttributeId,
    SkuAttributeValueId, SkuId, SkuRevisionAttributeValueId, SkuRevisionId, UnitOfMeasureId,
    VoucherCategoryProfileRevisionId,
};
use entities::money::{Amount, Quantity};
use mongodb::Database;
use test_support::{assert_indexes, require_mongo, TestDb};

/// 商品分类列表筛选条件类型（经 `CatalogExt` 关联类型跨 crate 可达）。
type ProductCategoryFilter = <Database as CatalogExt>::ProductCategoryFilter;
/// 商品列表筛选条件类型。
type ProductFilter = <Database as CatalogExt>::ProductFilter;
/// SKU 列表筛选条件类型。
type SkuFilter = <Database as CatalogExt>::SkuFilter;
/// SKU 修订列表筛选条件类型。
type SkuRevisionFilter = <Database as CatalogExt>::SkuRevisionFilter;

/// 执行 `ensure_indexes` 并断言本域全部必需索引就位。
async fn assert_domain_indexes(db: &Database) {
    ensure_indexes(db).await.expect("ensure_indexes 应成功");
    assert_indexes(
        db,
        <Database as CatalogExt>::PRODUCT_CATEGORIES,
        &[
            "uk_product_categories_category_code",
            "idx_product_categories_tree",
            "idx_product_categories_status_tree",
        ],
    )
    .await
    .expect("product_categories 索引缺失");
    assert_indexes(
        db,
        <Database as CatalogExt>::PRODUCT_BRANDS,
        &["uk_product_brands_brand_code", "idx_product_brands_status"],
    )
    .await
    .expect("product_brands 索引缺失");
    assert_indexes(
        db,
        <Database as CatalogExt>::UNIT_OF_MEASURES,
        &["uk_unit_of_measures_unit_code", "idx_unit_of_measures_status"],
    )
    .await
    .expect("unit_of_measures 索引缺失");
    assert_indexes(
        db,
        <Database as CatalogExt>::SKU_ATTRIBUTES,
        &[
            "uk_sku_attributes_attribute_code",
            "idx_sku_attributes_status_type",
        ],
    )
    .await
    .expect("sku_attributes 索引缺失");
    assert_indexes(
        db,
        <Database as CatalogExt>::SKU_ATTRIBUTE_VALUES,
        &[
            "uk_sku_attribute_values_attribute_value",
            "idx_sku_attribute_values_attribute_sort",
        ],
    )
    .await
    .expect("sku_attribute_values 索引缺失");
    assert_indexes(
        db,
        <Database as CatalogExt>::PRODUCT_CATEGORY_ATTRIBUTES,
        &[
            "uk_product_category_attributes_relation",
            "idx_product_category_attributes_category",
        ],
    )
    .await
    .expect("product_category_attributes 索引缺失");
    assert_indexes(
        db,
        <Database as CatalogExt>::PRODUCTS,
        &["uk_products_product_no", "idx_products_status_kind"],
    )
    .await
    .expect("products 索引缺失");
    assert_indexes(
        db,
        <Database as CatalogExt>::PRODUCT_REVISIONS,
        &["uk_product_revisions_revision"],
    )
    .await
    .expect("product_revisions 索引缺失");
    assert_indexes(
        db,
        <Database as CatalogExt>::PRODUCT_REVISION_MEDIAS,
        &["uk_product_revision_medias_media"],
    )
    .await
    .expect("product_revision_medias 索引缺失");
    assert_indexes(
        db,
        <Database as CatalogExt>::SKUS,
        &["uk_skus_sku_no", "uk_skus_product_spec"],
    )
    .await
    .expect("skus 索引缺失");
    assert_indexes(
        db,
        <Database as CatalogExt>::SKU_REVISIONS,
        &[
            "uk_sku_revisions_revision",
            "idx_sku_revisions_barcode",
            "idx_sku_revisions_search",
        ],
    )
    .await
    .expect("sku_revisions 索引缺失");
    assert_indexes(
        db,
        <Database as CatalogExt>::SKU_REVISION_ATTRIBUTE_VALUES,
        &[
            "uk_sku_revision_attribute_values_relation",
            "idx_sku_revision_attribute_values_revision",
            "idx_sku_revision_attribute_values_reverse",
        ],
    )
    .await
    .expect("sku_revision_attribute_values 索引缺失");
    assert_indexes(
        db,
        <Database as CatalogExt>::VOUCHER_CATEGORY_PROFILE_REVISIONS,
        &["uk_voucher_category_profile_revisions_revision"],
    )
    .await
    .expect("voucher_category_profile_revisions 索引缺失");
}

/// 构造可复用的根分类实体。
fn sample_category(id: &str, code: &str, parent: Option<&str>) -> ProductCategory {
    ProductCategory::new(
        ProductCategoryId::new(id),
        ProductCategoryData {
            category_code: code.to_string(),
            parent_category_id: parent.map(ProductCategoryId::new),
            name: format!("分类-{code}"),
            product_kind: ProductKind::Physical,
            status: EnableStatus::Active,
        },
        "admin-1",
    )
    .unwrap()
}

/// 构造可复用的品牌实体。
fn sample_brand(id: &str, code: &str) -> ProductBrand {
    ProductBrand::new(
        ProductBrandId::new(id),
        ProductBrandData {
            brand_code: code.to_string(),
            name: format!("品牌-{code}"),
            status: EnableStatus::Active,
        },
        "admin-1",
    )
    .unwrap()
}

/// 构造可复用的单位实体。
fn sample_unit(id: &str, code: &str) -> UnitOfMeasure {
    UnitOfMeasure::new(
        UnitOfMeasureId::new(id),
        UnitOfMeasureData {
            unit_code: code.to_string(),
            name: format!("单位-{code}"),
            symbol: code.to_string(),
            quantity_scale: 3,
            status: EnableStatus::Active,
        },
        "admin-1",
    )
    .unwrap()
}

/// 构造可复用的规格属性实体。
fn sample_attribute(id: &str, code: &str) -> SkuAttribute {
    SkuAttribute::new(
        SkuAttributeId::new(id),
        SkuAttributeData {
            attribute_code: code.to_string(),
            name: format!("属性-{code}"),
            value_type: AttributeValueType::Enum,
            status: EnableStatus::Active,
        },
        "admin-1",
    )
    .unwrap()
}

/// 构造可复用的规格属性值实体。
fn sample_attribute_value(id: &str, attribute_id: &str, code: &str) -> SkuAttributeValue {
    SkuAttributeValue::new(
        SkuAttributeValueId::new(id),
        SkuAttributeValueData {
            attribute_id: SkuAttributeId::new(attribute_id),
            value_code: code.to_string(),
            display_value: format!("值-{code}"),
            sort_order: 1,
            status: EnableStatus::Active,
        },
        "admin-1",
    )
    .unwrap()
}

/// 构造可复用的商品实体。
fn sample_product(id: &str, product_no: &str, kind: ProductKind) -> Product {
    Product::new(
        ProductId::new(id),
        ProductData {
            product_no: product_no.to_string(),
            product_kind: kind,
            status: EnableStatus::Active,
        },
        "admin-1",
    )
    .unwrap()
}

/// 构造可复用的商品修订实体。
fn sample_product_revision(
    id: &str,
    product_id: &str,
    revision_no: u32,
    category_id: &str,
    brand_id: &str,
) -> ProductRevision {
    ProductRevision::new(
        ProductRevisionId::new(id),
        ProductRevisionData {
            product_id: ProductId::new(product_id),
            revision_no,
            name: format!("商品修订-{revision_no}"),
            description: None,
            specification: None,
            category_id: ProductCategoryId::new(category_id),
            brand_id: ProductBrandId::new(brand_id),
            status: EnableStatus::Active,
            effective_from: BusinessDate::from_ymd(2026, 1, 1).unwrap(),
            effective_to: None,
        },
    )
    .unwrap()
}

/// 构造可复用的 SKU 实体。
fn sample_sku(id: &str, sku_no: &str, product_id: &str, signature: &str) -> Sku {
    Sku::new(
        SkuId::new(id),
        SkuData {
            sku_no: sku_no.to_string(),
            product_id: ProductId::new(product_id),
            base_unit_id: UnitOfMeasureId::new("uom-1"),
            specification_signature: signature.to_string(),
            status: EnableStatus::Active,
        },
        "admin-1",
    )
    .unwrap()
}

/// 构造可复用的 SKU 修订实体（含 Decimal128 价格与条码）。
fn sample_sku_revision(
    id: &str,
    sku_id: &str,
    revision_no: u32,
    barcode: Option<&str>,
    price: &str,
) -> SkuRevision {
    SkuRevision::new(
        SkuRevisionId::new(id),
        SkuRevisionData {
            sku_id: SkuId::new(sku_id),
            revision_no,
            name: format!("SKU修订-{revision_no}"),
            description: None,
            specification: None,
            barcode: barcode.map(|value| value.to_string()),
            weight_kg: Some(Quantity::from_str("0.500000").unwrap()),
            volume_m3: None,
            sales_visible_price_gross: Some(Amount::from_str(price).unwrap()),
            market_price: Some(Amount::from_str("129.00").unwrap()),
            status: EnableStatus::Active,
            effective_from: BusinessDate::from_ymd(2026, 1, 1).unwrap(),
            effective_to: None,
        },
    )
    .unwrap()
}

#[tokio::test]
#[ignore]
async fn category_crud_soft_delete_restore_roundtrip() {
    require_mongo!(async {
        let test_db = TestDb::new("cat_crud").await.unwrap();
        let db = test_db.db();
        assert_domain_indexes(db).await;

        let mut category = sample_category("cat-1", "ROOT", None);
        db.product_categories()
            .create(&category, &mut NoTransaction)
            .await
            .unwrap();
        assert_eq!(category.base.version, 1);

        let found = db
            .product_categories()
            .find_by_id(&category.base.id, &mut NoTransaction)
            .await
            .unwrap()
            .expect("创建后应可读回");
        assert_eq!(found.category_code, "ROOT");
        assert_eq!(found.stable.created_by, "admin-1");

        category
            .update(
                ProductCategoryUpdate {
                    name: Some("新分类名".to_string()),
                    status: Some(EnableStatus::Disabled),
                    product_kind: None,
                },
                "admin-2",
            )
            .unwrap();
        db.product_categories()
            .update(&mut category, &mut NoTransaction)
            .await
            .unwrap();
        assert_eq!(category.base.version, 2, "乐观锁成功后 version 递增");

        let mut stale = category.clone();
        category
            .update(
                ProductCategoryUpdate {
                    name: Some("再次更新".to_string()),
                    status: None,
                    product_kind: None,
                },
                "admin-3",
            )
            .unwrap();
        db.product_categories()
            .update(&mut category, &mut NoTransaction)
            .await
            .unwrap();

        stale
            .update(
                ProductCategoryUpdate {
                    name: Some("陈旧更新".to_string()),
                    status: None,
                    product_kind: None,
                },
                "admin-4",
            )
            .unwrap();
        let error = db
            .product_categories()
            .update(&mut stale, &mut NoTransaction)
            .await
            .expect_err("陈旧 version 更新必须被 CAS 拒绝");
        assert!(
            matches!(error, database::Error::OptimisticLockingError),
            "期望 OptimisticLockingError，实际为 {error:?}"
        );

        db.product_categories()
            .soft_delete(&mut category, &mut NoTransaction)
            .await
            .unwrap();
        let after_delete = db
            .product_categories()
            .find_by_id(&category.base.id, &mut NoTransaction)
            .await
            .unwrap();
        assert!(after_delete.is_none(), "软删除后按 ID 不可见");

        db.product_categories()
            .restore(&mut category, &mut NoTransaction)
            .await
            .unwrap();
        let after_restore = db
            .product_categories()
            .find_by_id(&category.base.id, &mut NoTransaction)
            .await
            .unwrap();
        assert!(after_restore.is_some(), "恢复后按 ID 重新可见");
    })
}

#[tokio::test]
#[ignore]
async fn category_unique_code_conflict_surfaces_duplicate_key() {
    require_mongo!(async {
        let test_db = TestDb::new("cat_dup").await.unwrap();
        let db = test_db.db();
        assert_domain_indexes(db).await;

        let category = sample_category("cat-1", "ROOT", None);
        db.product_categories()
            .create(&category, &mut NoTransaction)
            .await
            .unwrap();

        let duplicate = sample_category("cat-2", "ROOT", None);
        let error = db
            .product_categories()
            .create(&duplicate, &mut NoTransaction)
            .await
            .expect_err("重复分类代码必须被唯一索引拒绝");
        assert!(
            matches!(error, database::Error::DuplicateKey(_)),
            "期望 DuplicateKey，实际为 {error:?}"
        );
    })
}

#[tokio::test]
#[ignore]
async fn category_tree_children_and_subtree_queries() {
    require_mongo!(async {
        let test_db = TestDb::new("cat_tree").await.unwrap();
        let db = test_db.db();
        assert_domain_indexes(db).await;

        db.product_categories()
            .create(&sample_category("root-1", "ROOT", None), &mut NoTransaction)
            .await
            .unwrap();
        db.product_categories()
            .create(
                &sample_category("cat-1", "C01", Some("root-1")),
                &mut NoTransaction,
            )
            .await
            .unwrap();
        db.product_categories()
            .create(
                &sample_category("cat-1-1", "C011", Some("cat-1")),
                &mut NoTransaction,
            )
            .await
            .unwrap();

        let roots = db
            .product_categories()
            .find_children(None, &mut NoTransaction)
            .await
            .unwrap();
        assert_eq!(roots.len(), 1, "根分类只有一个");
        assert_eq!(roots[0].id, "root-1");

        let children = db
            .product_categories()
            .find_children(Some("root-1"), &mut NoTransaction)
            .await
            .unwrap();
        assert_eq!(children.len(), 1);
        assert_eq!(children[0].category_code, "C01");

        let subtree = db
            .product_categories()
            .find_subtree("root-1", &mut NoTransaction)
            .await
            .unwrap();
        assert_eq!(subtree.len(), 2, "子树包含全部后代层级");
        let codes: Vec<&str> = subtree.iter().map(|row| row.category_code.as_str()).collect();
        assert!(codes.contains(&"C01"));
        assert!(codes.contains(&"C011"));

        let root_filter = ProductCategoryFilter {
            category_code: None,
            name: None,
            parent_category_id: Some(None),
            status: None,
            page: 1,
            page_size: 20,
            sort_by: Some("category_code".to_string()),
            sort_ascending: true,
        };
        let root_page = db
            .product_categories()
            .search_product_categories(&root_filter, &mut NoTransaction)
            .await
            .unwrap();
        assert_eq!(root_page.total, 1, "根节点筛选只返回根分类");

        let child_filter = ProductCategoryFilter {
            category_code: None,
            name: None,
            parent_category_id: Some(Some("cat-1".to_string())),
            status: None,
            page: 1,
            page_size: 20,
            sort_by: None,
            sort_ascending: false,
        };
        let child_page = db
            .product_categories()
            .search_product_categories(&child_filter, &mut NoTransaction)
            .await
            .unwrap();
        assert_eq!(child_page.total, 1);
        assert_eq!(child_page.items[0].category_code, "C011");
    })
}

#[tokio::test]
#[ignore]
async fn category_list_pagination_projection_and_sort_whitelist() {
    require_mongo!(async {
        let test_db = TestDb::new("cat_list").await.unwrap();
        let db = test_db.db();
        assert_domain_indexes(db).await;

        db.product_categories()
            .create(&sample_category("cat-1", "C-B", None), &mut NoTransaction)
            .await
            .unwrap();
        db.product_categories()
            .create(&sample_category("cat-2", "C-A", None), &mut NoTransaction)
            .await
            .unwrap();
        db.product_categories()
            .create(&sample_category("cat-3", "C-C", None), &mut NoTransaction)
            .await
            .unwrap();

        let filter = ProductCategoryFilter {
            category_code: None,
            name: None,
            parent_category_id: Some(None),
            status: None,
            page: 1,
            page_size: 2,
            sort_by: Some("category_code".to_string()),
            sort_ascending: true,
        };
        let page = db
            .product_categories()
            .search_product_categories(&filter, &mut NoTransaction)
            .await
            .unwrap();
        assert_eq!(page.total, 3);
        assert_eq!(page.items.len(), 2, "分页边界：第一页 2 条");
        assert_eq!(page.items[0].category_code, "C-A");
        assert_eq!(page.items[0].name, "分类-C-A");
        assert_eq!(page.items[0].status, EnableStatus::Active);
        assert!(page.items[0].version >= 1);
        assert!(page.items[0].created_at > 0);
        assert_eq!(page.items[0].parent_category_id, None);

        let page_two = ProductCategoryFilter { page: 2, ..filter };
        let second = db
            .product_categories()
            .search_product_categories(&page_two, &mut NoTransaction)
            .await
            .unwrap();
        assert_eq!(second.items.len(), 1, "分页边界：第二页 1 条");
        assert_eq!(second.items[0].category_code, "C-C");
    })
}

#[tokio::test]
#[ignore]
async fn dictionary_unique_codes_conflict_surfaces_duplicate_key() {
    require_mongo!(async {
        let test_db = TestDb::new("cat_dict").await.unwrap();
        let db = test_db.db();
        assert_domain_indexes(db).await;

        db.product_brands()
            .create(&sample_brand("brand-1", "BR-001"), &mut NoTransaction)
            .await
            .unwrap();
        let brand_dup = sample_brand("brand-2", "BR-001");
        let error = db
            .product_brands()
            .create(&brand_dup, &mut NoTransaction)
            .await
            .expect_err("重复品牌代码必须被唯一索引拒绝");
        assert!(matches!(error, database::Error::DuplicateKey(_)));

        db.unit_of_measures()
            .create(&sample_unit("uom-1", "KG"), &mut NoTransaction)
            .await
            .unwrap();
        let unit_dup = sample_unit("uom-2", "KG");
        let error = db
            .unit_of_measures()
            .create(&unit_dup, &mut NoTransaction)
            .await
            .expect_err("重复单位代码必须被唯一索引拒绝");
        assert!(matches!(error, database::Error::DuplicateKey(_)));

        db.sku_attributes()
            .create(&sample_attribute("attr-1", "SIZE"), &mut NoTransaction)
            .await
            .unwrap();
        let attribute_dup = sample_attribute("attr-2", "SIZE");
        let error = db
            .sku_attributes()
            .create(&attribute_dup, &mut NoTransaction)
            .await
            .expect_err("重复属性代码必须被唯一索引拒绝");
        assert!(matches!(error, database::Error::DuplicateKey(_)));

        db.sku_attribute_values()
            .create(
                &sample_attribute_value("val-1", "attr-1", "L"),
                &mut NoTransaction,
            )
            .await
            .unwrap();
        let value_dup = sample_attribute_value("val-2", "attr-1", "L");
        let error = db
            .sku_attribute_values()
            .create(&value_dup, &mut NoTransaction)
            .await
            .expect_err("同一属性下重复属性值代码必须被唯一索引拒绝");
        assert!(matches!(error, database::Error::DuplicateKey(_)));

        let other_attribute = sample_attribute_value("val-3", "attr-2", "L");
        db.sku_attribute_values()
            .create(&other_attribute, &mut NoTransaction)
            .await
            .expect("不同属性下相同属性值代码应允许");
    })
}

#[tokio::test]
#[ignore]
async fn product_and_sku_unique_constraints_surface_duplicate_key() {
    require_mongo!(async {
        let test_db = TestDb::new("cat_prod_uniq").await.unwrap();
        let db = test_db.db();
        assert_domain_indexes(db).await;

        let product = sample_product("prod-1", "P-001", ProductKind::Physical);
        db.products().create(&product, &mut NoTransaction).await.unwrap();

        let product_dup = sample_product("prod-2", "P-001", ProductKind::Physical);
        let error = db
            .products()
            .create(&product_dup, &mut NoTransaction)
            .await
            .expect_err("重复商品编号必须被唯一索引拒绝");
        assert!(matches!(error, database::Error::DuplicateKey(_)));

        db.skus()
            .create(
                &sample_sku("sku-1", "SKU-001", "prod-1", "size=L"),
                &mut NoTransaction,
            )
            .await
            .unwrap();

        let sku_no_dup = sample_sku("sku-2", "SKU-001", "prod-1", "size=M");
        let error = db
            .skus()
            .create(&sku_no_dup, &mut NoTransaction)
            .await
            .expect_err("重复 SKU 编号必须被唯一索引拒绝");
        assert!(matches!(error, database::Error::DuplicateKey(_)));

        let spec_dup = sample_sku("sku-3", "SKU-003", "prod-1", "size=L");
        let error = db
            .skus()
            .create(&spec_dup, &mut NoTransaction)
            .await
            .expect_err("同商品同规格签名必须被唯一索引拒绝");
        assert!(matches!(error, database::Error::DuplicateKey(_)));

        let other_product = sample_sku("sku-4", "SKU-004", "prod-2", "size=L");
        db.skus()
            .create(&other_product, &mut NoTransaction)
            .await
            .expect("不同商品下相同规格签名应允许");
    })
}

#[tokio::test]
#[ignore]
async fn sku_revision_barcode_exact_query_allows_multiple_active() {
    require_mongo!(async {
        let test_db = TestDb::new("cat_barcode").await.unwrap();
        let db = test_db.db();
        assert_domain_indexes(db).await;

        db.skus()
            .create(
                &sample_sku("sku-1", "SKU-001", "prod-1", "size=L"),
                &mut NoTransaction,
            )
            .await
            .unwrap();
        db.skus()
            .create(
                &sample_sku("sku-2", "SKU-002", "prod-1", "size=M"),
                &mut NoTransaction,
            )
            .await
            .unwrap();

        db.sku_revisions()
            .create(
                &sample_sku_revision("rev-1", "sku-1", 1, Some("6901234567890"), "99.90"),
                &mut NoTransaction,
            )
            .await
            .unwrap();
        db.sku_revisions()
            .create(
                &sample_sku_revision("rev-2", "sku-2", 1, Some("6901234567890"), "88.00"),
                &mut NoTransaction,
            )
            .await
            .unwrap();

        let hits = db
            .sku_revisions()
            .find_active_by_barcode(" 6901234567890 ", &mut NoTransaction)
            .await
            .unwrap();
        assert_eq!(hits.len(), 2, "同一条码允许多个在用 SKU 修订");
        let ids: Vec<&str> = hits.iter().map(|revision| revision.sku_id.as_ref()).collect();
        assert!(ids.contains(&"sku-1"));
        assert!(ids.contains(&"sku-2"));
        assert_eq!(
            hits[0].sales_visible_price_gross,
            Some(Amount::from_str("99.90").unwrap()),
            "Decimal128 金额往返一致"
        );

        let filter = SkuRevisionFilter {
            sku_id: None,
            name: None,
            barcode: Some("6901234567890".to_string()),
            status: Some(EnableStatus::Active),
            page: 1,
            page_size: 20,
            sort_by: None,
            sort_ascending: false,
        };
        let page = db
            .sku_revisions()
            .search_sku_revisions(&filter, &mut NoTransaction)
            .await
            .unwrap();
        assert_eq!(page.total, 2, "列表条码精确查询命中 2 条");

        let no_hit = SkuRevisionFilter {
            barcode: Some("nonexistent".to_string()),
            ..filter
        };
        let empty = db
            .sku_revisions()
            .search_sku_revisions(&no_hit, &mut NoTransaction)
            .await
            .unwrap();
        assert_eq!(empty.total, 0);
    })
}

#[tokio::test]
#[ignore]
async fn create_sku_with_revision_commits_atomically_in_transaction() {
    require_mongo!(async {
        let test_db = TestDb::new("cat_tx_commit").await.unwrap();
        let db = test_db.db();
        assert_domain_indexes(db).await;

        let sku = sample_sku("sku-1", "SKU-001", "prod-1", "size=L");
        let revision = sample_sku_revision("rev-1", "sku-1", 1, Some("6901234567890"), "99.90");
        let attribute_value = SkuRevisionAttributeValue::new(
            SkuRevisionAttributeValueId::new("rav-1"),
            SkuRevisionAttributeValueData {
                sku_revision_id: SkuRevisionId::new("rev-1"),
                sku_attribute_id: SkuAttributeId::new("attr-1"),
                sku_attribute_value_id: Some(SkuAttributeValueId::new("val-1")),
                normalized_text_value: None,
                identity_position: 0,
            },
        )
        .unwrap();

        let db_clone = db.clone();
        let sku_for_tx = sku.clone();
        let revision_for_tx = revision.clone();
        let values_for_tx = vec![attribute_value.clone()];
        test_db
            .client()
            .with_transaction::<_, (), database::Error>(move |session| {
                Box::pin(async move {
                    db_clone
                        .catalog()
                        .create_sku_with_revision(&sku_for_tx, &revision_for_tx, &values_for_tx, session)
                        .await?;
                    Ok(())
                })
            })
            .await
            .expect("事务提交应成功");

        let sku_found = db
            .skus()
            .find_by_id(&sku.base.id, &mut NoTransaction)
            .await
            .unwrap();
        assert!(sku_found.is_some(), "事务提交后 SKU 必须可见");
        let revision_found = db
            .sku_revisions()
            .find_by_id(&revision.base.id, &mut NoTransaction)
            .await
            .unwrap();
        assert!(revision_found.is_some(), "事务提交后修订必须可见");
        let value_found = db
            .sku_revision_attribute_values()
            .find_by_id(&attribute_value.base.id, &mut NoTransaction)
            .await
            .unwrap();
        assert!(value_found.is_some(), "事务提交后规格值必须可见");
    })
}

#[tokio::test]
#[ignore]
async fn create_sku_with_revision_rolls_back_whole_on_conflict() {
    require_mongo!(async {
        let test_db = TestDb::new("cat_tx_abort").await.unwrap();
        let db = test_db.db();
        assert_domain_indexes(db).await;

        let sku = sample_sku("sku-1", "SKU-001", "prod-1", "size=L");
        let revision = sample_sku_revision("rev-1", "sku-1", 1, Some("6901234567890"), "99.90");
        let attribute_value = SkuRevisionAttributeValue::new(
            SkuRevisionAttributeValueId::new("rav-1"),
            SkuRevisionAttributeValueData {
                sku_revision_id: SkuRevisionId::new("rev-1"),
                sku_attribute_id: SkuAttributeId::new("attr-1"),
                sku_attribute_value_id: Some(SkuAttributeValueId::new("val-1")),
                normalized_text_value: None,
                identity_position: 0,
            },
        )
        .unwrap();

        let db_clone = db.clone();
        let sku_for_tx = sku.clone();
        let revision_for_tx = revision.clone();
        let values_for_tx = vec![attribute_value.clone()];
        let sku_again = sku.clone();
        let result: std::result::Result<(), database::Error> = test_db
            .client()
            .with_transaction(move |session| {
                Box::pin(async move {
                    db_clone
                        .catalog()
                        .create_sku_with_revision(&sku_for_tx, &revision_for_tx, &values_for_tx, session)
                        .await?;
                    let _ = db_clone
                        .skus()
                        .create(&sku_again, session)
                        .await
                        .expect_err("重复 SKU 编号必须在事务内触发唯一冲突");
                    Ok(())
                })
            })
            .await;
        assert!(result.is_err(), "闭包返回错误必须整体回滚");

        let sku_found = db
            .skus()
            .find_by_id(&sku.base.id, &mut NoTransaction)
            .await
            .unwrap();
        assert!(sku_found.is_none(), "回滚后 SKU 不得残留");
        let revision_found = db
            .sku_revisions()
            .find_by_id(&revision.base.id, &mut NoTransaction)
            .await
            .unwrap();
        assert!(revision_found.is_none(), "回滚后修订不得残留");
        let value_found = db
            .sku_revision_attribute_values()
            .find_by_id(&attribute_value.base.id, &mut NoTransaction)
            .await
            .unwrap();
        assert!(value_found.is_none(), "回滚后规格值不得残留");
    })
}

#[tokio::test]
#[ignore]
async fn multi_step_with_no_transaction_commits_parts_independently() {
    require_mongo!(async {
        let test_db = TestDb::new("cat_ntx").await.unwrap();
        let db = test_db.db();
        assert_domain_indexes(db).await;

        let sku = sample_sku("sku-1", "SKU-001", "prod-1", "size=L");
        let revision = sample_sku_revision("rev-1", "sku-1", 1, None, "99.90");
        let attribute_value = SkuRevisionAttributeValue::new(
            SkuRevisionAttributeValueId::new("rav-1"),
            SkuRevisionAttributeValueData {
                sku_revision_id: SkuRevisionId::new("rev-1"),
                sku_attribute_id: SkuAttributeId::new("attr-1"),
                sku_attribute_value_id: Some(SkuAttributeValueId::new("val-1")),
                normalized_text_value: None,
                identity_position: 0,
            },
        )
        .unwrap();

        db.catalog()
            .create_sku_with_revision(
                &sku,
                &revision,
                std::slice::from_ref(&attribute_value),
                &mut NoTransaction,
            )
            .await
            .expect("NoTransaction 下多步写入各自自动提交");

        let sku_found = db
            .skus()
            .find_by_id(&sku.base.id, &mut NoTransaction)
            .await
            .unwrap();
        assert!(sku_found.is_some(), "非事务执行器下第一步写入已提交");
        let revision_found = db
            .sku_revisions()
            .find_by_id(&revision.base.id, &mut NoTransaction)
            .await
            .unwrap();
        assert!(revision_found.is_some(), "非事务执行器下第二步写入已提交");
        let value_found = db
            .sku_revision_attribute_values()
            .find_by_id(&attribute_value.base.id, &mut NoTransaction)
            .await
            .unwrap();
        assert!(value_found.is_some(), "非事务执行器下第三步写入已提交");
    })
}

#[tokio::test]
#[ignore]
async fn product_revision_with_media_and_unique_relation() {
    require_mongo!(async {
        let test_db = TestDb::new("cat_rev_media").await.unwrap();
        let db = test_db.db();
        assert_domain_indexes(db).await;

        let revision = sample_product_revision("rev-1", "prod-1", 1, "cat-1", "brand-1");
        let carousel = ProductRevisionMedia::new(
            ProductRevisionMediaId::new("media-1"),
            ProductRevisionMediaData {
                product_revision_id: ProductRevisionId::new("rev-1"),
                file_asset_id: entities::ids::FileAssetId::new("asset-1"),
                media_role: MediaRole::Carousel,
                sort_order: 0,
                alt_text: Some("轮播图".to_string()),
            },
        )
        .unwrap();
        let detail = ProductRevisionMedia::new(
            ProductRevisionMediaId::new("media-2"),
            ProductRevisionMediaData {
                product_revision_id: ProductRevisionId::new("rev-1"),
                file_asset_id: entities::ids::FileAssetId::new("asset-2"),
                media_role: MediaRole::Detail,
                sort_order: 0,
                alt_text: None,
            },
        )
        .unwrap();

        let db_clone = db.clone();
        let revision_for_tx = revision.clone();
        let medias_for_tx = vec![carousel.clone(), detail.clone()];
        test_db
            .client()
            .with_transaction::<_, (), database::Error>(move |session| {
                Box::pin(async move {
                    db_clone
                        .catalog()
                        .create_product_revision_with_media(&revision_for_tx, &medias_for_tx, session)
                        .await?;
                    Ok(())
                })
            })
            .await
            .expect("事务提交应成功");

        let revision_found = db
            .product_revisions()
            .find_by_id(&revision.base.id, &mut NoTransaction)
            .await
            .unwrap();
        assert!(revision_found.is_some(), "事务提交后修订必须可见");
        let media_found = db
            .product_revision_medias()
            .find_by_id(&carousel.base.id, &mut NoTransaction)
            .await
            .unwrap();
        assert!(media_found.is_some(), "事务提交后媒体行必须可见");

        let duplicate_media = ProductRevisionMedia::new(
            ProductRevisionMediaId::new("media-3"),
            ProductRevisionMediaData {
                product_revision_id: ProductRevisionId::new("rev-1"),
                file_asset_id: entities::ids::FileAssetId::new("asset-3"),
                media_role: MediaRole::Carousel,
                sort_order: 0,
                alt_text: None,
            },
        )
        .unwrap();
        let error = db
            .product_revision_medias()
            .create(&duplicate_media, &mut NoTransaction)
            .await
            .expect_err("同修订同角色同排序媒体必须被唯一索引拒绝");
        assert!(matches!(error, database::Error::DuplicateKey(_)));

        let revision_dup = sample_product_revision("rev-2", "prod-1", 1, "cat-1", "brand-1");
        let error = db
            .product_revisions()
            .create(&revision_dup, &mut NoTransaction)
            .await
            .expect_err("同商品同修订序号必须被唯一索引拒绝");
        assert!(matches!(error, database::Error::DuplicateKey(_)));
    })
}

#[tokio::test]
#[ignore]
async fn sku_revision_attribute_value_uniqueness_and_reverse_query() {
    require_mongo!(async {
        let test_db = TestDb::new("cat_rav").await.unwrap();
        let db = test_db.db();
        assert_domain_indexes(db).await;

        let row = SkuRevisionAttributeValue::new(
            SkuRevisionAttributeValueId::new("rav-1"),
            SkuRevisionAttributeValueData {
                sku_revision_id: SkuRevisionId::new("rev-1"),
                sku_attribute_id: SkuAttributeId::new("attr-1"),
                sku_attribute_value_id: Some(SkuAttributeValueId::new("val-1")),
                normalized_text_value: None,
                identity_position: 0,
            },
        )
        .unwrap();
        db.sku_revision_attribute_values()
            .create(&row, &mut NoTransaction)
            .await
            .unwrap();

        let duplicate = SkuRevisionAttributeValue::new(
            SkuRevisionAttributeValueId::new("rav-2"),
            SkuRevisionAttributeValueData {
                sku_revision_id: SkuRevisionId::new("rev-1"),
                sku_attribute_id: SkuAttributeId::new("attr-1"),
                sku_attribute_value_id: Some(SkuAttributeValueId::new("val-2")),
                normalized_text_value: None,
                identity_position: 1,
            },
        )
        .unwrap();
        let error = db
            .sku_revision_attribute_values()
            .create(&duplicate, &mut NoTransaction)
            .await
            .expect_err("同修订同属性必须被唯一索引拒绝");
        assert!(matches!(error, database::Error::DuplicateKey(_)));

        let reverse = db
            .sku_revision_attribute_values()
            .find_many(
                mongodb::bson::doc! { "sku_attribute_value_id": "val-1" },
                &mut NoTransaction,
            )
            .await
            .unwrap();
        assert_eq!(reverse.len(), 1, "反向查询索引按属性值定位修订");
        assert_eq!(reverse[0].sku_revision_id.as_ref(), "rev-1");
    })
}

#[tokio::test]
#[ignore]
async fn product_search_filters_kind_status_and_pagination() {
    require_mongo!(async {
        let test_db = TestDb::new("cat_prod_list").await.unwrap();
        let db = test_db.db();
        assert_domain_indexes(db).await;

        let mut disabled = sample_product("prod-2", "P-002", ProductKind::Physical);
        disabled.stable.status = EnableStatus::Disabled;
        db.products()
            .create(
                &sample_product("prod-1", "P-001", ProductKind::Physical),
                &mut NoTransaction,
            )
            .await
            .unwrap();
        db.products().create(&disabled, &mut NoTransaction).await.unwrap();
        db.products()
            .create(
                &sample_product("prod-3", "V-001", ProductKind::Virtual),
                &mut NoTransaction,
            )
            .await
            .unwrap();

        let filter = ProductFilter {
            product_no: Some("p-".to_string()),
            product_kind: Some(ProductKind::Physical),
            status: Some(EnableStatus::Active),
            page: 1,
            page_size: 1,
            sort_by: Some("product_no".to_string()),
            sort_ascending: true,
        };
        let page = db
            .products()
            .search_products(&filter, &mut NoTransaction)
            .await
            .unwrap();
        assert_eq!(page.total, 1, "实物且启用且编号含 p- 只有一条");
        assert_eq!(page.items.len(), 1);
        assert_eq!(page.items[0].product_no, "P-001");
        assert_eq!(page.items[0].product_kind, ProductKind::Physical);
        assert_eq!(page.items[0].status, EnableStatus::Active);

        let all_filter = ProductFilter {
            product_no: None,
            product_kind: None,
            status: None,
            page: 1,
            page_size: 2,
            sort_by: None,
            sort_ascending: false,
        };
        let all_page = db
            .products()
            .search_products(&all_filter, &mut NoTransaction)
            .await
            .unwrap();
        assert_eq!(all_page.total, 3);
        assert_eq!(all_page.items.len(), 2, "分页边界：第一页 2 条");

        let sku_filter = SkuFilter {
            sku_no: Some("sku".to_string()),
            product_id: None,
            status: None,
            page: 1,
            page_size: 20,
            sort_by: None,
            sort_ascending: false,
        };
        let empty_skus = db
            .skus()
            .search_skus(&sku_filter, &mut NoTransaction)
            .await
            .unwrap();
        assert_eq!(empty_skus.total, 0, "未创建 SKU 时列表为空");
    })
}

#[tokio::test]
#[ignore]
async fn transaction_abort_rolls_back_cross_collection_writes() {
    require_mongo!(async {
        let test_db = TestDb::new("cat_tx_rollback").await.unwrap();
        let db = test_db.db();
        assert_domain_indexes(db).await;

        let category = sample_category("cat-1", "ROOT", None);
        let brand = sample_brand("brand-1", "BR-001");

        let db_clone = db.clone();
        let category_for_tx = category.clone();
        let brand_for_tx = brand.clone();
        let result: std::result::Result<(), database::Error> = test_db
            .client()
            .with_transaction(move |session| {
                Box::pin(async move {
                    db_clone
                        .product_categories()
                        .create(&category_for_tx, session)
                        .await?;
                    db_clone.product_brands().create(&brand_for_tx, session).await?;
                    Err(database::Error::OptimisticLockingError)
                })
            })
            .await;
        assert!(result.is_err(), "闭包返回错误必须整体回滚");

        let category_found = db
            .product_categories()
            .find_by_id(&category.base.id, &mut NoTransaction)
            .await
            .unwrap();
        assert!(category_found.is_none(), "回滚后分类不得残留");
        let brand_found = db
            .product_brands()
            .find_by_id(&brand.base.id, &mut NoTransaction)
            .await
            .unwrap();
        assert!(brand_found.is_none(), "回滚后品牌不得残留");
    })
}

#[tokio::test]
#[ignore]
async fn voucher_category_profile_revision_append_only_roundtrip() {
    require_mongo!(async {
        let test_db = TestDb::new("cat_vcp").await.unwrap();
        let db = test_db.db();
        assert_domain_indexes(db).await;

        let revision = VoucherCategoryProfileRevision::new(
            VoucherCategoryProfileRevisionId::new("vcp-1"),
            VoucherCategoryProfileRevisionData {
                sku_id: SkuId::new("sku-voucher-1"),
                revision_no: 1,
                description: "中国通卡券类目".to_string(),
                status: EnableStatus::Active,
            },
        )
        .unwrap();
        db.voucher_category_profile_revisions()
            .create(&revision, &mut NoTransaction)
            .await
            .unwrap();

        let found = db
            .voucher_category_profile_revisions()
            .find_by_id(&revision.base.id, &mut NoTransaction)
            .await
            .unwrap()
            .expect("创建后应可读回");
        assert_eq!(found.revision.revision_no, 1);
        assert!(found.is_active());

        let duplicate = VoucherCategoryProfileRevision::new(
            VoucherCategoryProfileRevisionId::new("vcp-2"),
            VoucherCategoryProfileRevisionData {
                sku_id: SkuId::new("sku-voucher-1"),
                revision_no: 1,
                description: "重复修订".to_string(),
                status: EnableStatus::Active,
            },
        )
        .unwrap();
        let error = db
            .voucher_category_profile_revisions()
            .create(&duplicate, &mut NoTransaction)
            .await
            .expect_err("同 SKU 同修订序号必须被唯一索引拒绝");
        assert!(matches!(error, database::Error::DuplicateKey(_)));
    })
}
