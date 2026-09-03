//! 客户退款列表投影的 Mongo 分页/存在性验收（SALES-R06）。

use super::{CustomerRefundFilter, CustomerRefundRow};
use crate::executor::NoTransaction;
use crate::repository::extensions::ReturnsExt;
use entities::common::time::Instant;
use entities::ids::{CustomerAccountId, CustomerReceiptId, CustomerRefundId};
use entities::money::Amount;
use entities::returns::{CustomerRefund, CustomerRefundData, CustomerRefundStatus};
use std::str::FromStr;

fn test_refund(
    id: &str,
    refund_no: &str,
    customer_id: &str,
    amount: &str,
    occurred_at: i64,
    created_at: u64,
) -> CustomerRefund {
    let mut refund = CustomerRefund::new(
        CustomerRefundId::new(id),
        CustomerRefundData {
            refund_no: refund_no.to_string(),
            sales_return_case_id: None,
            customer_id: CustomerAccountId::new(customer_id),
            original_receipt_id: Some(CustomerReceiptId::new("cr-1")),
            original_receivable_entry_id: None,
            reason_code: Some("QUALITY".to_string()),
            reason_text: "质量退款".to_string(),
            amount: Amount::from_str(amount).expect("金额合法"),
            handled_by: "handler-1".to_string(),
            reviewed_by: "reviewer-1".to_string(),
            occurred_at: Instant::from_unix_secs(occurred_at),
            evidence_attachment_id: None,
        },
        "creator-1",
    )
    .expect("退款必须可构造");
    refund.base.created_at = created_at;
    refund
}

fn list_filter(
    customer_id: Option<&str>,
    page: u64,
    page_size: u32,
    sort_by: &str,
    sort_ascending: bool,
) -> CustomerRefundFilter {
    CustomerRefundFilter {
        refund_no: None,
        customer_id: customer_id.map(CustomerAccountId::new),
        status: None,
        page,
        page_size,
        sort_by: Some(sort_by.to_string()),
        sort_ascending,
    }
}

/// 插入实体后按投影反序列化，锁定 Amount/Instant/可选 ID 与分页边界。
#[tokio::test]
#[ignore = "需要 ERP_TEST_MONGO_URI 指向 MongoDB 副本集"]
async fn search_customer_refunds_pages_filters_and_projects_view_facts() {
    use test_support::{require_mongo, TestDb};

    require_mongo!(async {
        let fixture = TestDb::new("crf_search_page").await.expect("测试数据库创建失败");
        crate::ensure_indexes(fixture.db()).await.expect("索引创建失败");
        let repo = fixture.db().customer_refunds();
        let mut deleted = test_refund("crf-del", "RF-DEL", "cust-a", "9.00", 9, 9);
        deleted.base.deleted_at = 1;
        for refund in [
            test_refund("crf-a1", "RF-A1", "cust-a", "100.50", 30, 30),
            test_refund("crf-a2", "RF-A2", "cust-a", "200.00", 20, 20),
            test_refund("crf-a3", "RF-A3", "cust-a", "300.00", 10, 10),
            test_refund("crf-b1", "RF-B1", "cust-b", "50.00", 40, 40),
            deleted,
        ] {
            repo.create(&refund, &mut NoTransaction)
                .await
                .expect("退款写入失败");
        }

        let desc = repo
            .search_customer_refunds(
                &list_filter(Some("cust-a"), 1, 2, "occurred_at", false),
                &mut NoTransaction,
            )
            .await
            .expect("降序第一页必须成功");
        assert_eq!(desc.total, 3, "软删与其他客户不得计入 total");
        assert!(desc.items.len() as u32 <= 2);
        assert_eq!(desc.items.len(), 2);
        assert_eq!(
            desc.items.iter().map(|row| row.id.as_str()).collect::<Vec<_>>(),
            vec!["crf-a1", "crf-a2"]
        );
        assert_projected_view_facts(&desc.items[0], "crf-a1", "100.50", 30);

        let page_two = repo
            .search_customer_refunds(
                &list_filter(Some("cust-a"), 2, 2, "occurred_at", false),
                &mut NoTransaction,
            )
            .await
            .expect("第二页必须成功");
        assert_eq!(page_two.total, 3);
        assert!(page_two.items.len() as u32 <= 2);
        assert_eq!(page_two.items.len(), 1);
        assert_eq!(page_two.items[0].id, "crf-a3");

        let asc = repo
            .search_customer_refunds(
                &list_filter(Some("cust-a"), 1, 2, "occurred_at", true),
                &mut NoTransaction,
            )
            .await
            .expect("升序第一页必须成功");
        assert_eq!(
            asc.items.iter().map(|row| row.id.as_str()).collect::<Vec<_>>(),
            vec!["crf-a3", "crf-a2"]
        );

        let missing = repo
            .search_customer_refunds(
                &list_filter(Some("cust-missing"), 1, 20, "created_at", false),
                &mut NoTransaction,
            )
            .await
            .expect("缺客户必须成功返回空页");
        assert_eq!(missing.total, 0);
        assert!(missing.items.is_empty());
    });
}

/// 相同主排序字段时 `id` 次键必须稳定区分升序与降序。
#[tokio::test]
#[ignore = "需要 ERP_TEST_MONGO_URI 指向 MongoDB 副本集"]
async fn search_customer_refunds_id_tiebreaker_is_stable() {
    use test_support::{require_mongo, TestDb};

    require_mongo!(async {
        let fixture = TestDb::new("crf_search_id_sort")
            .await
            .expect("测试数据库创建失败");
        crate::ensure_indexes(fixture.db()).await.expect("索引创建失败");
        let repo = fixture.db().customer_refunds();
        repo.create(
            &test_refund("crf-z", "RF-Z", "cust-a", "10.00", 100, 100),
            &mut NoTransaction,
        )
        .await
        .expect("写入失败");
        repo.create(
            &test_refund("crf-a", "RF-A", "cust-a", "10.00", 100, 100),
            &mut NoTransaction,
        )
        .await
        .expect("写入失败");

        let desc = repo
            .search_customer_refunds(
                &list_filter(Some("cust-a"), 1, 10, "occurred_at", false),
                &mut NoTransaction,
            )
            .await
            .expect("降序必须成功");
        assert_eq!(
            desc.items.iter().map(|row| row.id.as_str()).collect::<Vec<_>>(),
            vec!["crf-z", "crf-a"]
        );
        let asc = repo
            .search_customer_refunds(
                &list_filter(Some("cust-a"), 1, 10, "occurred_at", true),
                &mut NoTransaction,
            )
            .await
            .expect("升序必须成功");
        assert_eq!(
            asc.items.iter().map(|row| row.id.as_str()).collect::<Vec<_>>(),
            vec!["crf-a", "crf-z"]
        );
    });
}

fn assert_projected_view_facts(row: &CustomerRefundRow, id: &str, amount: &str, occurred_at: i64) {
    assert_eq!(row.id, id);
    assert_eq!(row.original_receipt_id.as_deref(), Some("cr-1"));
    assert!(row.original_receivable_entry_id.is_none());
    assert_eq!(row.reason_code.as_deref(), Some("QUALITY"));
    assert_eq!(row.handled_by, "handler-1");
    assert_eq!(row.reviewed_by, "reviewer-1");
    assert_eq!(row.amount, Amount::from_str(amount).expect("金额合法"));
    assert_eq!(row.occurred_at, Instant::from_unix_secs(occurred_at));
    assert_eq!(row.status, CustomerRefundStatus::Draft);
}
