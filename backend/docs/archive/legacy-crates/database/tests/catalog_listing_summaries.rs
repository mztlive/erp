//! 商品上架汇总投影的真实 MongoDB 验收：批量口径、缺失 listing_status 兼容、
//! 与 `product_list_pipeline` 对拍，以及 `$in` 聚合 explain。

use database::{ensure_indexes, CatalogExt, NoTransaction};
use entities::catalog::product::ProductData;
use entities::catalog::sku::SkuData;
use entities::catalog::{EnableStatus, ListingStatus, Product, ProductKind, Sku};
use entities::ids::{ProductId, SkuId, UnitOfMeasureId};
use mongodb::bson::{doc, Document};
use test_support::{require_mongo, TestDb};

const SKU_COLLECTION: &str = <mongodb::Database as CatalogExt>::SKUS;
const PRODUCT_SPEC_INDEX: &str = "uk_skus_product_spec";

type ProductFilter = <mongodb::Database as CatalogExt>::ProductFilter;

#[tokio::test]
#[ignore = "需要 ERP_TEST_MONGO_URI 指向 MongoDB 副本集"]
async fn listing_summaries_match_product_list_pipeline_and_skip_empty_input() {
    require_mongo!(async {
        let fixture = TestDb::new("catalog_listing_summaries")
            .await
            .expect("测试数据库创建失败");
        ensure_indexes(fixture.db()).await.expect("索引创建失败");
        let db = fixture.db();

        let empty = db
            .catalog()
            .listing_summaries(&[], &mut NoTransaction)
            .await
            .expect("空输入查询失败");
        assert!(empty.is_empty(), "空输入必须 0 次查询并返回空集合");

        insert_product(db, "zero", "ZERO").await;
        insert_product(db, "disabled", "DISABLED").await;
        insert_product(db, "listed", "LISTED").await;
        insert_product(db, "partial", "PARTIAL").await;
        insert_product(db, "unlisted", "UNLISTED").await;
        insert_product(db, "legacy", "LEGACY").await;

        insert_sku(
            db,
            "dis-1",
            "disabled",
            EnableStatus::Disabled,
            ListingStatus::Unlisted,
            "",
        )
        .await;
        insert_sku(
            db,
            "lst-1",
            "listed",
            EnableStatus::Active,
            ListingStatus::Listed,
            "颜色=红",
        )
        .await;
        insert_sku(
            db,
            "lst-2",
            "listed",
            EnableStatus::Active,
            ListingStatus::Listed,
            "尺码=L",
        )
        .await;
        insert_sku(
            db,
            "par-1",
            "partial",
            EnableStatus::Active,
            ListingStatus::Listed,
            "颜色=红",
        )
        .await;
        insert_sku(
            db,
            "par-2",
            "partial",
            EnableStatus::Active,
            ListingStatus::Unlisted,
            "尺码=L",
        )
        .await;
        insert_sku(
            db,
            "un-1",
            "unlisted",
            EnableStatus::Active,
            ListingStatus::Unlisted,
            "颜色=红",
        )
        .await;
        insert_sku(
            db,
            "un-2",
            "unlisted",
            EnableStatus::Active,
            ListingStatus::Unlisted,
            "尺码=L",
        )
        .await;
        insert_legacy_listed_sku(db, "leg-1", "legacy").await;

        let requested = [
            ProductId::new("listed"),
            ProductId::new("zero"),
            ProductId::new("listed"),
            ProductId::new("partial"),
            ProductId::new("unlisted"),
            ProductId::new("disabled"),
            ProductId::new("legacy"),
        ];
        let summaries = db
            .catalog()
            .listing_summaries(&requested, &mut NoTransaction)
            .await
            .expect("上架汇总查询失败");
        assert_eq!(summaries.len(), 6, "重复 ProductId 必须按首次出现去重");
        assert_eq!(summaries[0].product_id, "listed");
        assert_eq!((summaries[0].listed_sku_count, summaries[0].sku_count), (2, 2));
        assert_eq!((summaries[1].listed_sku_count, summaries[1].sku_count), (0, 0));
        assert_eq!((summaries[2].listed_sku_count, summaries[2].sku_count), (1, 2));
        assert_eq!((summaries[3].listed_sku_count, summaries[3].sku_count), (0, 2));
        assert_eq!((summaries[4].listed_sku_count, summaries[4].sku_count), (0, 0));
        assert_eq!(
            (summaries[5].listed_sku_count, summaries[5].sku_count),
            (1, 1),
            "缺失 listing_status 必须兼容为已上架"
        );

        let page = db
            .catalog()
            .search_products(&product_filter(), &mut NoTransaction)
            .await
            .expect("商品列表聚合失败");
        for summary in &summaries {
            let row = page
                .items
                .iter()
                .find(|item| item.id == summary.product_id)
                .unwrap_or_else(|| panic!("商品列表缺少 {}", summary.product_id));
            assert_eq!(
                (row.listed_sku_count, row.sku_count),
                (summary.listed_sku_count, summary.sku_count),
                "上架汇总必须与 product_list_pipeline 对拍 {}",
                summary.product_id
            );
        }

        let many_ids: Vec<ProductId> = summaries
            .iter()
            .map(|item| ProductId::new(item.product_id.as_str()))
            .collect();
        let pipeline = db.catalog().listing_summary_aggregation_pipeline(&many_ids);
        let explain = db
            .run_command(doc! {
                "explain": {
                    "aggregate": SKU_COLLECTION,
                    "pipeline": pipeline,
                    "cursor": {},
                },
                "verbosity": "executionStats",
            })
            .await
            .expect("上架汇总聚合 explain 失败");
        let rendered = format!("{explain:?}");
        assert!(!rendered.contains("COLLSCAN"), "上架汇总不得集合扫描：{rendered}");
        assert!(rendered.contains("IXSCAN"), "上架汇总必须使用索引：{rendered}");
        assert!(
            rendered.contains(PRODUCT_SPEC_INDEX) || rendered.contains("idx_skus_listing_status"),
            "上架汇总应命中 product_id 前缀或 listing 索引：{rendered}"
        );
    });
}

#[tokio::test]
#[ignore = "需要 ERP_TEST_MONGO_URI 指向 MongoDB 副本集"]
async fn specification_signature_audit_finds_only_noncanonical_rows() {
    require_mongo!(async {
        let fixture = TestDb::new("catalog_spec_audit")
            .await
            .expect("测试数据库创建失败");
        ensure_indexes(fixture.db()).await.expect("索引创建失败");
        let db = fixture.db();
        insert_product(db, "audit-p", "AUDIT").await;
        insert_sku(
            db,
            "ok-1",
            "audit-p",
            EnableStatus::Active,
            ListingStatus::Unlisted,
            "尺码=L|颜色=红色",
        )
        .await;
        insert_sku(
            db,
            "bad-1",
            "audit-p",
            EnableStatus::Active,
            ListingStatus::Unlisted,
            "尺码L",
        )
        .await;
        insert_sku(
            db,
            "bad-2",
            "audit-p",
            EnableStatus::Active,
            ListingStatus::Unlisted,
            "颜色=红色|尺码=L",
        )
        .await;

        let hits = db
            .catalog()
            .noncanonical_specification_signature_sku_ids(&mut NoTransaction)
            .await
            .expect("规格签名审计失败");
        assert_eq!(hits.len(), 2, "审计必须只命中非规范签名");
        assert!(hits.contains(&"bad-1".to_string()));
        assert!(hits.contains(&"bad-2".to_string()));
        assert!(!hits.contains(&"ok-1".to_string()));
    });
}

/// 构造商品列表对拍用的宽分页筛选。
///
/// # 参数
/// 无。
///
/// # 返回
/// 返回不附加业务筛选的第一页。
///
/// # 错误
/// 无。
fn product_filter() -> ProductFilter {
    ProductFilter {
        product_no: None,
        keyword: None,
        product_kind: None,
        category_id: None,
        brand_id: None,
        supplier_id: None,
        status: None,
        listing_status: None,
        supply_coverage: None,
        sales_price_min: None,
        sales_price_max: None,
        page: 1,
        page_size: 50,
        sort_by: Some("product_no".to_string()),
        sort_ascending: true,
    }
}

/// 写入最小商品身份。
///
/// # 参数
/// * `db` - 隔离测试库
/// * `id` - 商品主键
/// * `product_no` - 商品编号
///
/// # 错误
/// 写入失败时 panic。
async fn insert_product(db: &mongodb::Database, id: &str, product_no: &str) {
    let product = Product::new(
        ProductId::new(id),
        ProductData {
            product_no: product_no.to_string(),
            product_kind: ProductKind::Physical,
            status: EnableStatus::Active,
        },
        "tester",
    )
    .expect("测试商品必须合法");
    db.products()
        .create(&product, &mut NoTransaction)
        .await
        .expect("商品写入失败");
}

/// 写入最小 SKU 身份。
///
/// # 参数
/// * `db` - 隔离测试库
/// * `id` - SKU 主键
/// * `product_id` - 所属商品
/// * `status` - 启停状态
/// * `listing_status` - 上架状态
/// * `signature` - 规格签名
///
/// # 错误
/// 写入失败时 panic。
async fn insert_sku(
    db: &mongodb::Database,
    id: &str,
    product_id: &str,
    status: EnableStatus,
    listing_status: ListingStatus,
    signature: &str,
) {
    let sku = Sku::new(
        SkuId::new(id),
        SkuData {
            sku_no: id.to_string(),
            product_id: ProductId::new(product_id),
            base_unit_id: UnitOfMeasureId::new("unit-1"),
            specification_signature: signature.to_string(),
            status,
            listing_status,
        },
        "tester",
    )
    .expect("测试 SKU 必须可写入");
    db.skus()
        .create(&sku, &mut NoTransaction)
        .await
        .expect("SKU 写入失败");
}

/// 写入缺失 `listing_status` 的历史 SKU 文档。
///
/// # 参数
/// * `db` - 隔离测试库
/// * `id` - SKU 主键
/// * `product_id` - 所属商品
///
/// # 错误
/// 写入失败时 panic。
async fn insert_legacy_listed_sku(db: &mongodb::Database, id: &str, product_id: &str) {
    db.collection::<Document>(SKU_COLLECTION)
        .insert_one(doc! {
            "id": id,
            "sku_no": id,
            "product_id": product_id,
            "base_unit_id": "unit-1",
            "specification_signature": "",
            "status": "active",
            "version": 1_i64,
            "created_at": 1_i64,
            "updated_at": 1_i64,
            "deleted_at": 0_i64,
            "created_by": "tester",
            "updated_by": "tester",
        })
        .await
        .expect("历史 SKU 写入失败");
}
