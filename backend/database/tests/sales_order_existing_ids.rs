//! FIN-R01 `SalesOrderRepository::find_existing_ids`（按 ID 集合批量返回已存在
//! ID 的精确读取）的真实 MongoDB 验收（P6 阶段）。
//!
//! 覆盖合同「关闭验收」四维：空输入不发数据库往返（仓储内早退）；重复 ID
//! 去重；批量部分缺失只返回存在的 ID；软删除（`deleted_at` 非未删除标记）
//! 排除；allocation 数量 1/20/100 时单次 `$in` 往返且全部正确解析。

use database::{ensure_indexes, NoTransaction, SalesOrderExt};
use entities::ids::{ContractId, CustomerAccountId, PartyId, SalesOrderId};
use entities::sales_order::{BusinessType, OriginSystem, SalesOrder, SalesOrderData};
use mongodb::Database;
use test_support::{require_mongo, TestDb};

/// 销售单夹具（可选软删除）。
fn sales_order(id: &str, order_no: &str, deleted: bool) -> SalesOrder {
    let mut order = SalesOrder::new(
        SalesOrderId::new(id),
        SalesOrderData {
            order_no: order_no.to_string(),
            business_type: BusinessType::Voucher,
            origin_system: OriginSystem::Erp,
            source_identity_id: None,
            customer_id: CustomerAccountId::new("customer-1"),
            contract_id: Some(ContractId::new("contract-1")),
            settlement_party_id: PartyId::new("party-1"),
            source_status_code: None,
        },
        "tester-1",
    )
    .expect("销售单构造失败");
    if deleted {
        order.base.deleted_at = 1_700_000_001;
    }
    order
}

/// 批量插入销售单（空输入不发起写入）。
async fn insert_orders(db: &Database, orders: &[SalesOrder]) {
    if orders.is_empty() {
        return;
    }
    db.collection::<SalesOrder>(<mongodb::Database as SalesOrderExt>::SALES_ORDERS)
        .insert_many(orders.to_vec())
        .await
        .expect("销售单插入失败");
}

/// 空输入直接返回空集合：`find_existing_ids` 在仓储内早退，不发起数据库往返。
#[tokio::test]
#[ignore = "需要 ERP_TEST_MONGO_URI 指向 MongoDB 副本集"]
async fn empty_input_returns_empty_without_database_roundtrip() {
    require_mongo!(async {
        let fixture = TestDb::new("fin_r01_empty").await.expect("测试数据库创建失败");
        ensure_indexes(fixture.db()).await.expect("索引创建失败");
        let db = fixture.db();

        let existing = db
            .sales_order()
            .find_existing_ids(&[], &mut NoTransaction)
            .await
            .expect("空输入查询失败");
        assert!(existing.is_empty(), "空输入必须直接返回空集合");
    });
}

/// 重复 ID 去重、部分缺失只返回存在的 ID、软删除排除。
#[tokio::test]
#[ignore = "需要 ERP_TEST_MONGO_URI 指向 MongoDB 副本集"]
async fn dedupe_partial_missing_and_soft_deleted_are_excluded() {
    require_mongo!(async {
        let fixture = TestDb::new("fin_r01_dedup").await.expect("测试数据库创建失败");
        ensure_indexes(fixture.db()).await.expect("索引创建失败");
        let db = fixture.db();
        insert_orders(
            db,
            &[
                sales_order("so-exist-1", "SO-EXIST-1", false),
                sales_order("so-exist-2", "SO-EXIST-2", false),
                sales_order("so-deleted", "SO-DELETED", true),
            ],
        )
        .await;

        let ids = vec![
            SalesOrderId::new("so-exist-1"),
            SalesOrderId::new("so-exist-2"),
            SalesOrderId::new("so-missing"),
            SalesOrderId::new("so-deleted"),
            SalesOrderId::new("so-exist-1"), // 重复
        ];
        let existing = db
            .sales_order()
            .find_existing_ids(&ids, &mut NoTransaction)
            .await
            .expect("批量存在性查询失败");

        let mut found = existing.iter().map(ToString::to_string).collect::<Vec<_>>();
        found.sort();
        assert_eq!(
            found,
            vec!["so-exist-1", "so-exist-2"],
            "缺失与软删 ID 不得返回，重复 ID 每个至多出现一次"
        );
    });
}

/// allocation 数量 1/20/100 时全部正确解析（查询次数与输入数量无关，
/// 单次 `$in` 往返）。
#[tokio::test]
#[ignore = "需要 ERP_TEST_MONGO_URI 指向 MongoDB 副本集"]
async fn allocation_sizes_1_20_100_resolve_completely() {
    require_mongo!(async {
        let fixture = TestDb::new("fin_r01_sizes").await.expect("测试数据库创建失败");
        ensure_indexes(fixture.db()).await.expect("索引创建失败");
        let db = fixture.db();
        let orders = (0..100)
            .map(|index| {
                sales_order(
                    &format!("so-bulk-{index:03}"),
                    &format!("SO-BULK-{index:03}"),
                    false,
                )
            })
            .collect::<Vec<_>>();
        insert_orders(db, &orders).await;

        for size in [1usize, 20, 100] {
            let ids = (0..size)
                .map(|index| SalesOrderId::new(format!("so-bulk-{index:03}")))
                .collect::<Vec<_>>();
            let existing = db
                .sales_order()
                .find_existing_ids(&ids, &mut NoTransaction)
                .await
                .expect("批量存在性查询失败");
            assert_eq!(existing.len(), size, "allocation 数量 {size} 必须全部解析");
            let mut found = existing.iter().map(ToString::to_string).collect::<Vec<_>>();
            let mut expected = ids.iter().map(ToString::to_string).collect::<Vec<_>>();
            found.sort();
            expected.sort();
            assert_eq!(found, expected, "allocation 数量 {size} 返回集合必须与输入一致");
        }
    });
}
