//! INT-R02 `MallOrderFactRepository::list_by_mall_and_external_order_no` 真实 MongoDB 验收。
//!
//! 覆盖：目标事实仅位于后续页且前页零命中时不得遗漏；混合订单隔离；同秒事实按
//! 稳定 `id` 排序；软删除排除；代表性 explain 命中
//! `idx_mall_order_facts_mall_external_order_occurred`。

use database::{ensure_indexes, MallOrderExt, NoTransaction};
use entities::common::time::Instant;
use entities::ids::{InboxMessageId, MallOrderFactId};
use entities::mall_order::{DataSource, FactType, MallOrderFact, MallOrderFactData};
use mongodb::bson::{doc, Document};
use mongodb::Database;
use test_support::{require_mongo, TestDb};

/// 支付成功关键事实夹具（可选软删除；可指定商城订单号与发生时间/ID）。
fn payment_fact(
    id: &str,
    mall_id: &str,
    external_order_no: &str,
    occurred_at: i64,
    deleted: bool,
) -> MallOrderFact {
    let mut fact = MallOrderFact::new(
        MallOrderFactId::new(id),
        MallOrderFactData {
            mall_id: mall_id.to_string(),
            source_event_id: format!("evt-{id}"),
            inbox_message_id: InboxMessageId::new(format!("inbox-{id}")),
            fact_type: FactType::PaymentSucceeded,
            business_fact_key: format!("bfk-{id}"),
            external_order_no: external_order_no.to_string(),
            external_order_version: "1".to_string(),
            after_sales_request_id: None,
            original_payment_fact_id: None,
            occurred_at: Instant::from_unix_secs(occurred_at),
            received_at: Instant::from_unix_secs(occurred_at + 1),
            data_source: DataSource::Realtime,
            raw_payload_reference: None,
        },
    )
    .expect("关键事实构造失败");
    if deleted {
        fact.base.deleted_at = (occurred_at + 2) as u64;
    }
    fact
}

/// 批量插入关键事实。
async fn insert_facts(db: &Database, facts: &[MallOrderFact]) {
    if facts.is_empty() {
        return;
    }
    db.collection::<MallOrderFact>(<mongodb::Database as MallOrderExt>::MALL_ORDER_FACTS)
        .insert_many(facts.to_vec())
        .await
        .expect("关键事实插入失败");
}

/// 目标订单事实夹在大量其他订单之后时仍被精确取回；混合订单与软删除被隔离。
#[tokio::test]
#[ignore = "需要 ERP_TEST_MONGO_URI 指向 MongoDB 副本集"]
async fn exact_order_query_does_not_miss_facts_after_unrelated_prefix() {
    require_mongo!(async {
        let fixture = TestDb::new("int_r02_exact_order")
            .await
            .expect("测试数据库创建失败");
        ensure_indexes(fixture.db()).await.expect("索引创建失败");
        let db = fixture.db();

        let mut facts = Vec::new();
        // 前缀：>100 条其他订单事实，模拟旧分页前页零命中场景。
        for index in 0..120 {
            facts.push(payment_fact(
                &format!("other-{index:04}"),
                "mall-a",
                &format!("SO-OTHER-{index:04}"),
                1_700_000_000 + index,
                false,
            ));
        }
        // 目标订单：两条同秒事实 + 一条更晚事实 + 一条软删除。
        facts.push(payment_fact(
            "target-b",
            "mall-a",
            "SO-TARGET",
            1_700_000_500,
            false,
        ));
        facts.push(payment_fact(
            "target-a",
            "mall-a",
            "SO-TARGET",
            1_700_000_500,
            false,
        ));
        facts.push(payment_fact(
            "target-c",
            "mall-a",
            "SO-TARGET",
            1_700_000_600,
            false,
        ));
        facts.push(payment_fact(
            "target-deleted",
            "mall-a",
            "SO-TARGET",
            1_700_000_700,
            true,
        ));
        // 其他商城同订单号不得串入。
        facts.push(payment_fact(
            "other-mall",
            "mall-b",
            "SO-TARGET",
            1_700_000_500,
            false,
        ));
        insert_facts(db, &facts).await;

        let loaded = db
            .mall_order_facts()
            .list_by_mall_and_external_order_no("mall-a", "SO-TARGET", &mut NoTransaction)
            .await
            .expect("精确订单事实查询失败");

        let ids: Vec<&str> = loaded.iter().map(|fact| fact.base.id.as_str()).collect();
        assert_eq!(ids, vec!["target-a", "target-b", "target-c"]);
        assert!(loaded.iter().all(|fact| fact.external_order_no == "SO-TARGET"));
        assert!(loaded.iter().all(|fact| fact.mall_id == "mall-a"));
    });
}

/// 空结果与代表性 explain 命中组合索引。
#[tokio::test]
#[ignore = "需要 ERP_TEST_MONGO_URI 指向 MongoDB 副本集"]
async fn missing_order_is_empty_and_explain_uses_composite_index() {
    require_mongo!(async {
        let fixture = TestDb::new("int_r02_explain").await.expect("测试数据库创建失败");
        ensure_indexes(fixture.db()).await.expect("索引创建失败");
        let db = fixture.db();
        insert_facts(
            db,
            &[payment_fact("only-1", "mall-a", "SO-1", 1_700_000_000, false)],
        )
        .await;

        let missing = db
            .mall_order_facts()
            .list_by_mall_and_external_order_no("mall-a", "SO-MISSING", &mut NoTransaction)
            .await
            .expect("缺失订单查询失败");
        assert!(missing.is_empty());

        let explained: Document = db
            .run_command(doc! {
                "explain": {
                    "find": <mongodb::Database as MallOrderExt>::MALL_ORDER_FACTS,
                    "filter": {
                        "mall_id": "mall-a",
                        "external_order_no": "SO-1",
                        "deleted_at": 0_i64,
                    },
                    "sort": { "occurred_at": 1, "id": 1 },
                },
                "verbosity": "executionStats",
            })
            .await
            .expect("explain 失败");
        let rendered = format!("{explained:?}");
        assert!(rendered.contains("IXSCAN"), "explain 未使用 IXSCAN：{rendered}");
        assert!(
            !rendered.contains("COLLSCAN"),
            "精确订单事实查询不得集合扫描：{rendered}"
        );
        assert!(
            rendered.contains("idx_mall_order_facts_mall_external_order_occurred"),
            "explain 未命中组合索引：{rendered}"
        );
    });
}
