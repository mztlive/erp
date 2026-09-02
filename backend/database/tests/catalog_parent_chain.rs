//! 分类祖先链投影的真实 MongoDB 验收：行为矩阵、事务可见性与主键索引 explain。

use database::{ensure_indexes, CatalogExt, NoTransaction, Transactional};
use entities::catalog::product_category::ProductCategoryData;
use entities::catalog::{EnableStatus, ProductCategory, ProductKind};
use entities::ids::ProductCategoryId;
use mongodb::bson::{doc, Document};
use test_support::{require_mongo, TestDb};

const CATEGORY_COLLECTION: &str = <mongodb::Database as CatalogExt>::PRODUCT_CATEGORIES;
const CATEGORY_ID_INDEX: &str = "uk_product_categories_id";

#[tokio::test]
#[ignore = "需要 ERP_TEST_MONGO_URI 指向 MongoDB 副本集"]
async fn parent_chain_is_bounded_fail_closed_and_uses_id_index() {
    require_mongo!(async {
        let fixture = TestDb::new("catalog_parent_chain")
            .await
            .expect("测试数据库创建失败");
        ensure_indexes(fixture.db()).await.expect("索引创建失败");
        let db = fixture.db();

        let root_fact = db
            .product_categories()
            .parent_chain(None, &mut NoTransaction)
            .await
            .expect("根节点查询失败");
        assert!(root_fact.start_parent_id.is_none());
        assert!(root_fact.links.is_empty());
        assert!(!root_fact.truncated);

        db.product_categories()
            .create(&category("root", "ROOT", None), &mut NoTransaction)
            .await
            .expect("根分类写入失败");
        db.product_categories()
            .create(&category("mid", "MID", Some("root")), &mut NoTransaction)
            .await
            .expect("中层分类写入失败");
        db.product_categories()
            .create(&category("leaf", "LEAF", Some("mid")), &mut NoTransaction)
            .await
            .expect("叶子分类写入失败");

        let multi = db
            .product_categories()
            .parent_chain(Some(&ProductCategoryId::new("leaf")), &mut NoTransaction)
            .await
            .expect("多级链查询失败");
        assert_eq!(multi.links.len(), 3);
        assert!(multi.missing_parent_id.is_none());
        assert!(!multi.cycle_detected);
        assert!(!multi.truncated);

        let missing = db
            .product_categories()
            .parent_chain(Some(&ProductCategoryId::new("ghost")), &mut NoTransaction)
            .await
            .expect("缺失父节点查询失败");
        assert_eq!(missing.missing_parent_id.as_deref(), Some("ghost"));

        let mut retired = category("retired", "RETIRED", None);
        db.product_categories()
            .create(&retired, &mut NoTransaction)
            .await
            .expect("待软删分类写入失败");
        db.product_categories()
            .soft_delete(&mut retired, &mut NoTransaction)
            .await
            .expect("软删除失败");
        let deleted = db
            .product_categories()
            .parent_chain(Some(&ProductCategoryId::new("retired")), &mut NoTransaction)
            .await
            .expect("软删除父节点查询失败");
        assert_eq!(deleted.missing_parent_id.as_deref(), Some("retired"));

        insert_raw_category(db, "loop", "LOOP", Some("loop")).await;
        let direct = db
            .product_categories()
            .parent_chain(Some(&ProductCategoryId::new("loop")), &mut NoTransaction)
            .await
            .expect("直接环查询失败");
        assert!(direct.cycle_detected);
        assert!(direct.missing_parent_id.is_none());

        db.product_categories()
            .create(&category("cyc-a", "CYCA", Some("cyc-b")), &mut NoTransaction)
            .await
            .expect("间接环 A 写入失败");
        db.product_categories()
            .create(&category("cyc-b", "CYCB", Some("cyc-a")), &mut NoTransaction)
            .await
            .expect("间接环 B 写入失败");
        let indirect = db
            .product_categories()
            .parent_chain(Some(&ProductCategoryId::new("cyc-a")), &mut NoTransaction)
            .await
            .expect("间接环查询失败");
        assert!(indirect.cycle_detected);
        assert!(indirect.missing_parent_id.is_none());

        let tx_db = db.clone();
        let client = tx_db.client().clone();
        client
            .with_transaction(move |session| {
                Box::pin(async move {
                    tx_db
                        .product_categories()
                        .create(&category("tx-root", "TXROOT", None), session)
                        .await?;
                    let fact = tx_db
                        .product_categories()
                        .parent_chain(Some(&ProductCategoryId::new("tx-root")), session)
                        .await?;
                    assert!(
                        fact.missing_parent_id.is_none(),
                        "事务内必须读到同一会话刚写入的父分类"
                    );
                    Ok::<(), database::Error>(())
                })
            })
            .await
            .expect("事务执行器路径失败");

        let pipeline = db.product_categories().parent_chain_aggregation_pipeline("leaf");
        let explain = db
            .run_command(doc! {
                "explain": {
                    "aggregate": CATEGORY_COLLECTION,
                    "pipeline": pipeline,
                    "cursor": {},
                },
                "verbosity": "executionStats",
            })
            .await
            .expect("祖先链聚合 explain 失败");
        assert_explain_uses_id_index(&explain);
    });
}

/// 构造最小合法分类。
///
/// # 参数
/// * `id` - 分类主键
/// * `code` - 稳定分类代码
/// * `parent` - 父分类主键；空表示根
///
/// # 返回
/// 返回可写入测试库的分类实体。
///
/// # 错误
/// 测试数据非法时 panic。
fn category(id: &str, code: &str, parent: Option<&str>) -> ProductCategory {
    ProductCategory::new(
        ProductCategoryId::new(id),
        ProductCategoryData {
            category_code: code.to_string(),
            parent_category_id: parent.map(ProductCategoryId::new),
            name: code.to_string(),
            product_kind: ProductKind::Physical,
            status: EnableStatus::Active,
        },
        "tester",
    )
    .expect("测试分类必须合法")
}

/// 写入实体构造会拒绝的分类文档（直接自环）。
///
/// # 参数
/// * `db` - 隔离测试库
/// * `id` - 分类主键
/// * `code` - 稳定分类代码
/// * `parent` - 父分类主键
///
/// # 错误
/// 插入失败时 panic。
async fn insert_raw_category(db: &mongodb::Database, id: &str, code: &str, parent: Option<&str>) {
    db.collection::<Document>(CATEGORY_COLLECTION)
        .insert_one(doc! {
            "id": id,
            "category_code": code,
            "parent_category_id": parent,
            "name": code,
            "product_kind": "PHYSICAL",
            "status": "active",
            "version": 1_i64,
            "created_at": 1_i64,
            "updated_at": 1_i64,
            "deleted_at": 0_i64,
            "created_by": "tester",
            "updated_by": "tester",
        })
        .await
        .expect("原始分类写入失败");
}

/// 断言祖先链 explain 命中稳定主键索引且无集合扫描或排序阶段。
///
/// # 参数
/// * `explain` - MongoDB explain 文档
///
/// # 错误
/// 未命中 IXSCAN、出现 COLLSCAN/SORT 或未包含主键索引名时 panic。
fn assert_explain_uses_id_index(explain: &Document) {
    let rendered = format!("{explain:?}");
    assert!(rendered.contains("IXSCAN"), "explain 未使用 IXSCAN：{rendered}");
    assert!(
        rendered.contains(CATEGORY_ID_INDEX),
        "explain 未命中 {CATEGORY_ID_INDEX}：{rendered}"
    );
    assert!(
        !rendered.contains("COLLSCAN"),
        "explain 出现 COLLSCAN：{rendered}"
    );
    assert!(!rendered.contains("SORT"), "explain 出现 SORT：{rendered}");
}
