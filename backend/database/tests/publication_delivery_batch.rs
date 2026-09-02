//! MASTER-R04 待处理发布投递批次上下文的真实 MongoDB 验收。
//!
//! 覆盖：空批次、单条、多条、缺失修订、缺失发布、处理顺序、`limit`、到期重试
//! 筛选；代表性 `explain` 命中 `idx_product_publication_deliveries_processable`
//! 与修订/发布 `id` 唯一索引。

use std::str::FromStr;

use database::{ensure_indexes, NoTransaction, PublicationExt};
use entities::common::time::Instant;
use entities::ids::{
    ProductCategoryId, ProductPublicationDeliveryId, ProductPublicationId, ProductPublicationRevisionId,
    SkuId, SkuRevisionId, SourceSystemId, SupplierOfferingRevisionId,
};
use entities::integration_ops::ErrorClass;
use entities::money::{Amount, Quantity, Rate};
use entities::publication::{
    ProductCapability, ProductPublication, ProductPublicationData, ProductPublicationDelivery,
    ProductPublicationDeliveryData, ProductPublicationRevision, ProductPublicationRevisionData,
    ProductPublicationStatus, PublicationDeliveryStatus, SaleStatus,
};
use mongodb::bson::doc;
use test_support::{require_mongo, TestDb};

const DELIVERY_COLLECTION: &str = <mongodb::Database as PublicationExt>::PRODUCT_PUBLICATION_DELIVERIES;
const REVISION_COLLECTION: &str = <mongodb::Database as PublicationExt>::PRODUCT_PUBLICATION_REVISIONS;
const PUBLICATION_COLLECTION: &str = <mongodb::Database as PublicationExt>::PRODUCT_PUBLICATIONS;
const PROCESSABLE_INDEX: &str = "idx_product_publication_deliveries_processable";
const REVISION_ID_INDEX: &str = "uk_product_publication_revisions_id";
const PUBLICATION_ID_INDEX: &str = "uk_product_publications_id";

#[tokio::test]
#[ignore = "需要 ERP_TEST_MONGO_URI 指向 MongoDB 副本集"]
async fn processable_delivery_batch_is_bounded_ordered_and_index_backed() {
    require_mongo!(async {
        let fixture = TestDb::new("publication_delivery_batch")
            .await
            .expect("测试数据库创建失败");
        ensure_indexes(fixture.db()).await.expect("索引创建失败");

        let at = Instant::from_unix_secs(1_700_000_100);
        let empty = fixture
            .db()
            .publication()
            .list_processable_publication_delivery_contexts(at, 50, &mut NoTransaction)
            .await
            .expect("空批次查询失败");
        assert!(empty.is_empty(), "空集合必须返回空批次");

        insert_publication(fixture.db(), "pub-1", "mall-1").await;
        insert_publication(fixture.db(), "pub-2", "mall-2").await;
        insert_revision(fixture.db(), "rev-1", "pub-1").await;
        insert_revision(fixture.db(), "rev-2", "pub-2").await;
        insert_revision(fixture.db(), "rev-orphan", "pub-missing").await;

        let mut due_retry = retrying_delivery("del-due", "rev-2", "mall-2", 1_700_000_050);
        due_retry.base.created_at = 20;
        let mut pending_late = pending_delivery("del-pending-b", "rev-1", "mall-1b");
        pending_late.base.created_at = 30;
        let mut pending_early = pending_delivery("del-pending-a", "rev-1", "mall-1");
        pending_early.base.created_at = 10;
        let mut future_retry = retrying_delivery("del-future", "rev-1", "mall-future", 1_700_000_200);
        future_retry.base.created_at = 5;
        let mut sending = sending_delivery("del-sending", "rev-1", "mall-sending");
        sending.base.created_at = 1;
        let mut missing_revision = pending_delivery("del-missing-rev", "rev-absent", "mall-3");
        missing_revision.base.created_at = 40;
        let mut missing_publication = pending_delivery("del-missing-pub", "rev-orphan", "mall-4");
        missing_publication.base.created_at = 50;

        for delivery in [
            due_retry,
            pending_late,
            pending_early,
            future_retry,
            sending,
            missing_revision,
            missing_publication,
        ] {
            fixture
                .db()
                .product_publication_deliveries()
                .create(&delivery, &mut NoTransaction)
                .await
                .expect("投递写入失败");
        }

        let limited = fixture
            .db()
            .publication()
            .list_processable_publication_delivery_contexts(at, 2, &mut NoTransaction)
            .await
            .expect("limit 查询失败");
        assert_eq!(
            limited
                .iter()
                .map(|row| row.delivery.base.id.as_str())
                .collect::<Vec<_>>(),
            vec!["del-pending-a", "del-due"]
        );
        assert_eq!(
            limited[0].revision.as_ref().map(|row| row.base.id.as_str()),
            Some("rev-1")
        );
        assert_eq!(
            limited[0].publication.as_ref().map(|row| row.base.id.as_str()),
            Some("pub-1")
        );

        let rows = fixture
            .db()
            .publication()
            .list_processable_publication_delivery_contexts(at, 50, &mut NoTransaction)
            .await
            .expect("全量批次查询失败");
        let ids: Vec<_> = rows.iter().map(|row| row.delivery.base.id.as_str()).collect();
        assert_eq!(
            ids,
            vec![
                "del-pending-a",
                "del-due",
                "del-pending-b",
                "del-missing-rev",
                "del-missing-pub",
            ]
        );
        assert!(
            !ids.contains(&"del-future") && !ids.contains(&"del-sending"),
            "未到期重试与发送中不得入选：{ids:?}"
        );
        let missing_rev = rows
            .iter()
            .find(|row| row.delivery.base.id == "del-missing-rev")
            .expect("缺失修订投递必须保留在批次中");
        assert!(missing_rev.revision.is_none());
        assert!(missing_rev.publication.is_none());
        let missing_pub = rows
            .iter()
            .find(|row| row.delivery.base.id == "del-missing-pub")
            .expect("缺失发布投递必须保留在批次中");
        assert_eq!(
            missing_pub.revision.as_ref().map(|row| row.base.id.as_str()),
            Some("rev-orphan")
        );
        assert!(missing_pub.publication.is_none());

        let processable_explain = fixture
            .db()
            .run_command(doc! {
                "explain": {
                    "find": DELIVERY_COLLECTION,
                    "filter": {
                        "deleted_at": 0_i64,
                        "$or": [
                            { "delivery_status": "pending_send" },
                            {
                                "delivery_status": "retrying",
                                "next_attempt_at": { "$lte": at.unix_secs() },
                            },
                        ],
                    },
                    "sort": { "created_at": 1_i64, "id": 1_i64 },
                    "limit": 50_i64,
                },
                "verbosity": "executionStats",
            })
            .await
            .expect("待处理投递 explain 失败");
        assert_explain_uses_index(&processable_explain, PROCESSABLE_INDEX);

        let revision_explain = fixture
            .db()
            .run_command(doc! {
                "explain": {
                    "find": REVISION_COLLECTION,
                    "filter": {
                        "id": { "$in": ["rev-1", "rev-2", "rev-orphan", "rev-absent"] },
                        "deleted_at": 0_i64,
                    },
                },
                "verbosity": "executionStats",
            })
            .await
            .expect("修订 ID 批量读取 explain 失败");
        assert_explain_uses_index(&revision_explain, REVISION_ID_INDEX);

        let publication_explain = fixture
            .db()
            .run_command(doc! {
                "explain": {
                    "find": PUBLICATION_COLLECTION,
                    "filter": {
                        "id": { "$in": ["pub-1", "pub-2"] },
                        "deleted_at": 0_i64,
                    },
                },
                "verbosity": "executionStats",
            })
            .await
            .expect("发布 ID 批量读取 explain 失败");
        assert_explain_uses_index(&publication_explain, PUBLICATION_ID_INDEX);
    });
}

/// 写入最小合法稳定发布。
///
/// # 参数
/// * `db` - 测试数据库
/// * `id` - 发布主键
/// * `mall_id` - 目标商城
///
/// # 错误
/// 写入失败时 panic。
async fn insert_publication(db: &mongodb::Database, id: &str, mall_id: &str) {
    db.product_publications()
        .create(
            &ProductPublication::new(
                ProductPublicationId::new(id),
                ProductPublicationData {
                    sku_id: SkuId::new(format!("sku-{id}")),
                    target_mall_id: SourceSystemId::new(mall_id),
                    status: ProductPublicationStatus::PendingPublish,
                },
                "tester",
            )
            .expect("测试发布必须合法"),
            &mut NoTransaction,
        )
        .await
        .expect("发布写入失败");
}

/// 写入最小合法发布修订。
///
/// # 参数
/// * `db` - 测试数据库
/// * `id` - 修订主键
/// * `publication_id` - 所属稳定发布
///
/// # 错误
/// 写入失败时 panic。
async fn insert_revision(db: &mongodb::Database, id: &str, publication_id: &str) {
    db.product_publication_revisions()
        .create(&revision(id, publication_id), &mut NoTransaction)
        .await
        .expect("修订写入失败");
}

/// 构造最小合法发布修订。
///
/// # 参数
/// * `id` - 修订主键
/// * `publication_id` - 所属稳定发布
///
/// # 返回
/// 返回可写入测试库的修订实体。
///
/// # 错误
/// 测试数据非法时 panic。
fn revision(id: &str, publication_id: &str) -> ProductPublicationRevision {
    ProductPublicationRevision::new(
        ProductPublicationRevisionId::new(id),
        1,
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

/// 构造待发送投递。
///
/// # 参数
/// * `id` - 投递主键
/// * `revision_id` - 关联修订
/// * `mall_id` - 目标商城
///
/// # 返回
/// 返回待发送投递。
///
/// # 错误
/// 测试数据非法时 panic。
fn pending_delivery(id: &str, revision_id: &str, mall_id: &str) -> ProductPublicationDelivery {
    ProductPublicationDelivery::new(
        ProductPublicationDeliveryId::new(id),
        ProductPublicationDeliveryData {
            publication_revision_id: ProductPublicationRevisionId::new(revision_id),
            target_mall_id: SourceSystemId::new(mall_id),
            delivery_status: PublicationDeliveryStatus::PendingSend,
            attempt_count: 0,
            last_attempt_at: None,
            next_attempt_at: None,
            mall_ack_at: None,
            mall_version: None,
            error_class: None,
            error_code: None,
            error_summary: None,
        },
    )
    .expect("测试投递必须合法")
}

/// 构造重试中投递。
///
/// # 参数
/// * `id` - 投递主键
/// * `revision_id` - 关联修订
/// * `mall_id` - 目标商城
/// * `next_attempt_at` - 下次处理时间
///
/// # 返回
/// 返回重试中投递。
///
/// # 错误
/// 测试数据非法时 panic。
fn retrying_delivery(
    id: &str,
    revision_id: &str,
    mall_id: &str,
    next_attempt_at: i64,
) -> ProductPublicationDelivery {
    ProductPublicationDelivery::new(
        ProductPublicationDeliveryId::new(id),
        ProductPublicationDeliveryData {
            publication_revision_id: ProductPublicationRevisionId::new(revision_id),
            target_mall_id: SourceSystemId::new(mall_id),
            delivery_status: PublicationDeliveryStatus::Retrying,
            attempt_count: 1,
            last_attempt_at: Some(Instant::from_unix_secs(next_attempt_at - 60)),
            next_attempt_at: Some(Instant::from_unix_secs(next_attempt_at)),
            mall_ack_at: None,
            mall_version: None,
            error_class: Some(ErrorClass::TransientFailure),
            error_code: Some("MALL_TIMEOUT".to_string()),
            error_summary: Some("商城超时".to_string()),
        },
    )
    .expect("测试重试投递必须合法")
}

/// 构造发送中投递。
///
/// # 参数
/// * `id` - 投递主键
/// * `revision_id` - 关联修订
/// * `mall_id` - 目标商城
///
/// # 返回
/// 返回发送中投递。
///
/// # 错误
/// 测试数据非法时 panic。
fn sending_delivery(id: &str, revision_id: &str, mall_id: &str) -> ProductPublicationDelivery {
    ProductPublicationDelivery::new(
        ProductPublicationDeliveryId::new(id),
        ProductPublicationDeliveryData {
            publication_revision_id: ProductPublicationRevisionId::new(revision_id),
            target_mall_id: SourceSystemId::new(mall_id),
            delivery_status: PublicationDeliveryStatus::Sending,
            attempt_count: 1,
            last_attempt_at: Some(Instant::from_unix_secs(1_700_000_000)),
            next_attempt_at: None,
            mall_ack_at: None,
            mall_version: None,
            error_class: None,
            error_code: None,
            error_summary: None,
        },
    )
    .expect("测试发送中投递必须合法")
}

/// 断言 explain 命中指定索引且无集合扫描。
///
/// # 参数
/// * `explain` - MongoDB explain 文档
/// * `index_name` - 期望命中的索引名
///
/// # 错误
/// 未命中 IXSCAN、出现 COLLSCAN 或未包含索引名时 panic。
fn assert_explain_uses_index(explain: &mongodb::bson::Document, index_name: &str) {
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
