//! 发布修订最新序号投影、受检后继与唯一索引的真实 MongoDB 验收。

use std::str::FromStr;

use database::{ensure_indexes, NoTransaction, PublicationExt, Transactional};
use entities::common::time::Instant;
use entities::ids::{
    ProductCategoryId, ProductPublicationId, ProductPublicationRevisionId, SkuRevisionId,
    SupplierOfferingRevisionId,
};
use entities::money::{Amount, Quantity, Rate};
use entities::publication::{
    ProductCapability, ProductPublicationRevision, ProductPublicationRevisionData, SaleStatus,
};
use mongodb::bson::{doc, Bson, Document};
use test_support::{require_mongo, TestDb};

const REVISION_COLLECTION: &str = <mongodb::Database as PublicationExt>::PRODUCT_PUBLICATION_REVISIONS;
const REVISION_INDEX: &str = "uk_product_publication_revisions_publication_revision";

#[tokio::test]
#[ignore = "需要 ERP_TEST_MONGO_URI 指向 MongoDB 副本集"]
async fn latest_revision_no_is_bounded_transactional_and_uniquely_contended() {
    require_mongo!(async {
        let fixture = TestDb::new("publication_revision_layering")
            .await
            .expect("测试数据库创建失败");
        ensure_indexes(fixture.db()).await.expect("索引创建失败");

        let publication_id = ProductPublicationId::new("pub-latest-1");
        assert_eq!(
            fixture
                .db()
                .product_publication_revisions()
                .latest_revision_no(&publication_id, &mut NoTransaction)
                .await
                .expect("空历史查询失败"),
            None
        );

        fixture
            .db()
            .product_publication_revisions()
            .create(&revision("rev-single", "pub-latest-1", 4), &mut NoTransaction)
            .await
            .expect("单条修订写入失败");
        assert_eq!(
            fixture
                .db()
                .product_publication_revisions()
                .latest_revision_no(&publication_id, &mut NoTransaction)
                .await
                .expect("单条历史查询失败"),
            Some(4)
        );

        let unordered_publication = ProductPublicationId::new("pub-unordered");
        for (id, revision_no) in [("rev-3", 3_u32), ("rev-1", 1), ("rev-5", 5)] {
            fixture
                .db()
                .product_publication_revisions()
                .create(&revision(id, "pub-unordered", revision_no), &mut NoTransaction)
                .await
                .expect("乱序修订写入失败");
        }
        assert_eq!(
            fixture
                .db()
                .product_publication_revisions()
                .latest_revision_no(&unordered_publication, &mut NoTransaction)
                .await
                .expect("乱序历史查询失败"),
            Some(5)
        );

        let tx_publication = ProductPublicationId::new("pub-tx");
        let db = fixture.db().clone();
        let client = db.client().clone();
        client
            .with_transaction(move |session| {
                Box::pin(async move {
                    db.product_publication_revisions()
                        .create(&revision("rev-tx-2", "pub-tx", 2), session)
                        .await?;
                    let latest = db
                        .product_publication_revisions()
                        .latest_revision_no(&tx_publication, session)
                        .await?;
                    assert_eq!(latest, Some(2), "事务内必须读到同一会话刚写入的最大序号");
                    Ok::<(), database::Error>(())
                })
            })
            .await
            .expect("事务执行器路径失败");
        assert_eq!(
            fixture
                .db()
                .product_publication_revisions()
                .latest_revision_no(&ProductPublicationId::new("pub-tx"), &mut NoTransaction)
                .await
                .expect("事务提交后查询失败"),
            Some(2)
        );

        let first = revision("rev-race-1", "pub-race", 1);
        let second = revision("rev-race-2", "pub-race", 1);
        let first_db = fixture.db().clone();
        let second_db = fixture.db().clone();
        let create_first = async move {
            first_db
                .product_publication_revisions()
                .create(&first, &mut NoTransaction)
                .await
        };
        let create_second = async move {
            second_db
                .product_publication_revisions()
                .create(&second, &mut NoTransaction)
                .await
        };
        let (first_result, second_result) = tokio::join!(create_first, create_second);
        assert_eq!(
            usize::from(first_result.is_ok()) + usize::from(second_result.is_ok()),
            1,
            "同一发布修订序号并发写入必须恰有一个成功：first={first_result:?}, second={second_result:?}"
        );
        let failed = if first_result.is_err() {
            first_result
        } else {
            second_result
        };
        assert!(
            matches!(failed, Err(database::Error::DuplicateKey(_))),
            "失败方必须是唯一索引 DuplicateKey：{failed:?}"
        );
        assert_eq!(
            fixture
                .db()
                .product_publication_revisions()
                .latest_revision_no(&ProductPublicationId::new("pub-race"), &mut NoTransaction)
                .await
                .expect("并发后查询失败"),
            Some(1)
        );

        let explain = fixture
            .db()
            .run_command(doc! {
                "explain": {
                    "find": REVISION_COLLECTION,
                    "filter": {
                        "product_publication_id": "pub-unordered",
                        "deleted_at": 0_i64,
                    },
                    "sort": { "revision_no": -1_i64 },
                    "limit": 1_i64,
                    "projection": { "revision_no": 1_i64, "_id": 0_i64 },
                },
                "verbosity": "executionStats",
            })
            .await
            .expect("最新修订号查询 explain 失败");
        assert_explain_uses_index(&explain, REVISION_INDEX);
        let examined = numeric_field(
            explain
                .get_document("executionStats")
                .expect("explain 缺少 executionStats"),
            "totalDocsExamined",
        );
        assert!(examined <= 1, "最新修订号查询最多读取一行，实际 {examined}");
    });
}

/// 构造最小合法发布修订。
///
/// # 参数
/// * `id` - 修订主键
/// * `publication_id` - 所属稳定发布
/// * `revision_no` - 修订序号
///
/// # 返回
/// 返回可写入测试库的修订实体。
///
/// # 错误
/// 测试数据非法时 panic。
fn revision(id: &str, publication_id: &str, revision_no: u32) -> ProductPublicationRevision {
    ProductPublicationRevision::new(
        ProductPublicationRevisionId::new(id),
        revision_no,
        ProductPublicationRevisionData {
            product_publication_id: ProductPublicationId::new(publication_id),
            sku_revision_id: SkuRevisionId::new("sku-rev-1"),
            supplier_offering_revision_id: SupplierOfferingRevisionId::new("off-rev-1"),
            category_id: ProductCategoryId::new("cat-1"),
            name: "福利商城卡".to_string(),
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
        },
    )
    .expect("测试修订必须合法")
}

/// 断言 explain 命中指定索引且无集合扫描。
///
/// # 参数
/// * `explain` - MongoDB explain 文档
/// * `index_name` - 期望命中的索引名
///
/// # 返回
/// 无。
///
/// # 错误
/// 未命中 IXSCAN、出现 COLLSCAN 或未包含索引名时 panic。
fn assert_explain_uses_index(explain: &Document, index_name: &str) {
    let rendered = format!("{explain:?}");
    assert!(rendered.contains("IXSCAN"), "explain 未使用 IXSCAN：{rendered}");
    assert!(
        !rendered.contains("COLLSCAN"),
        "explain 出现 COLLSCAN：{rendered}"
    );
    assert!(
        rendered.contains(index_name),
        "explain 未命中索引 {index_name}：{rendered}"
    );
}

/// 读取 explain 数值字段。
///
/// # 参数
/// * `document` - 含数值字段的文档
/// * `field` - 字段名
///
/// # 返回
/// 返回 i64。
///
/// # 错误
/// 字段缺失或类型不受支持时 panic。
fn numeric_field(document: &Document, field: &str) -> i64 {
    match document
        .get(field)
        .unwrap_or_else(|| panic!("缺少数值字段 {field}"))
    {
        Bson::Int32(value) => i64::from(*value),
        Bson::Int64(value) => *value,
        other => panic!("字段 {field} 不是整数：{other:?}"),
    }
}
