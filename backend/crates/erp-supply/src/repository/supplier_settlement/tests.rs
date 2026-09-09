use std::str::FromStr;

use super::projection::{sort_doc, supplier_settlement_statement_projection};
use super::source_scope::order_scope_filter;
use super::{SupplierSettlementDifferenceFilter, SupplierSettlementStatementFilter};
use crate::entity::supplier_settlement::{SettlementDifferenceStatus, SettlementPeriod, SettlementStatus};
use erp_core::ids::SupplierSettlementItemId;
use mongodb::bson::doc;
use persistence_core::QueryFilter;

#[test]
fn statement_filter_applies_optional_fields_and_deleted_filter() {
    let filter = SupplierSettlementStatementFilter {
        q: None,
        keyword_supplier_ids: Vec::new(),
        statement_no: Some("ST-2026".to_string()),
        supplier_id: None,
        status: Some(SettlementStatus::Confirmed),
        period_from: None,
        period_to: None,
        page: 1,
        page_size: 20,
        sort_by: None,
        sort_ascending: false,
    };

    let document = filter.to_doc();
    assert_eq!(document.get_i64("deleted_at").unwrap(), 0);
    assert_eq!(document.get_str("status").unwrap(), "CONFIRMED");
    assert_eq!(
        document
            .get_document("statement_no")
            .unwrap()
            .get_str("$regex")
            .unwrap(),
        r"ST\-2026"
    );
}

#[test]
fn difference_filter_applies_statement_item_and_status() {
    let filter = SupplierSettlementDifferenceFilter {
        statement_item_id: Some(SupplierSettlementItemId::new("settlement-item-1")),
        status: Some(SettlementDifferenceStatus::Pending),
        page: 1,
        page_size: 20,
        sort_by: None,
        sort_ascending: false,
    };

    let document = filter.to_doc();
    assert_eq!(
        document.get_str("statement_item_id").unwrap(),
        "settlement-item-1"
    );
    assert_eq!(document.get_str("status").unwrap(), "PENDING");
}

#[test]
fn settlement_sort_doc_rejects_fields_outside_whitelist() {
    let whitelist = ["created_at", "period_start", "confirmed_at"];
    assert_eq!(
        sort_doc(&whitelist, None, false),
        doc! { "created_at": -1, "id": -1 }
    );
    assert_eq!(
        sort_doc(&whitelist, Some("status"), false),
        doc! { "created_at": -1, "id": -1 },
        "白名单外的排序字段必须回退 created_at"
    );
    assert_eq!(
        sort_doc(&whitelist, Some("period_start"), true),
        doc! { "period_start": 1, "id": 1 }
    );
    assert_eq!(
        sort_doc(&whitelist, Some("confirmed_at"), false),
        doc! { "confirmed_at": -1, "id": -1 }
    );
}

#[test]
fn statement_projection_contains_frozen_review_and_audit_facts() {
    let projection = supplier_settlement_statement_projection();
    for field in [
        "subject_hash",
        "source_as_of",
        "source_snapshot_at",
        "source_snapshot_hash",
        "refresh_cutoff_policy_id",
        "refresh_cutoff_policy_version",
        "review_result",
        "review_reason_code",
        "review_comment",
        "reviewed_at",
    ] {
        assert_eq!(projection.get_i32(field).unwrap(), 1, "缺少字段 {field}");
    }
}

#[test]
fn settlement_period_bounds_use_shanghai_inclusive_start_exclusive_end() {
    use chrono::DateTime;
    use erp_core::common::time::BusinessDate;

    // 边界口径必须来自领域 SettlementPeriod::secs_bounds（与 contains 同源），
    // 仓储不再复制第二份计算。
    for (start_text, end_text, first_secs_text, last_secs_text) in [
        (
            "2026-07-01",
            "2026-07-31",
            "2026-07-01T00:00:00+08:00",
            "2026-08-01T00:00:00+08:00",
        ),
        (
            "2025-12-01",
            "2026-02-28",
            "2025-12-01T00:00:00+08:00",
            "2026-03-01T00:00:00+08:00",
        ),
        (
            "2028-02-01",
            "2028-02-29",
            "2028-02-01T00:00:00+08:00",
            "2028-03-01T00:00:00+08:00",
        ),
    ] {
        let start = BusinessDate::from_str(start_text).unwrap();
        let end = BusinessDate::from_str(end_text).unwrap();
        let (start_secs, end_secs) = SettlementPeriod::secs_bounds(start, end);
        assert_eq!(
            start_secs,
            DateTime::parse_from_rfc3339(first_secs_text).unwrap().timestamp(),
            "{start_text} 开始边界错误"
        );
        assert_eq!(
            end_secs,
            DateTime::parse_from_rfc3339(last_secs_text).unwrap().timestamp(),
            "{start_text} 结束边界错误"
        );
    }
    let start = BusinessDate::from_ymd(2026, 7, 1).unwrap();
    let end = BusinessDate::from_ymd(2026, 7, 31).unwrap();
    let (start_secs, end_secs) = SettlementPeriod::secs_bounds(start, end);
    let last_second = DateTime::parse_from_rfc3339("2026-07-31T23:59:59+08:00")
        .unwrap()
        .timestamp();
    assert!(
        (start_secs..end_secs).contains(&last_second),
        "结束日 23:59:59 +08:00 必须落在期间内"
    );
}

#[test]
fn settlement_scope_filters_cover_supplier_period_and_requested_ids() {
    use erp_core::ids::{SupplierAccountId, SupplierFulfillmentItemId};
    use std::collections::BTreeSet;

    use super::source_scope::{item_scope_filter, refund_fact_scope_filter};

    let supplier = SupplierAccountId::new("supplier-1");
    let facts = refund_fact_scope_filter(&supplier, 100, 200);
    assert_eq!(facts.get_str("supplier_id").unwrap(), "supplier-1");
    let range = facts.get_document("refunded_at").unwrap();
    assert_eq!(range.get_i64("$gte").unwrap(), 100);
    assert_eq!(range.get_i64("$lt").unwrap(), 200);

    let order_ids = BTreeSet::from(["order-1".to_string()]);
    let orders = order_scope_filter(&supplier, &order_ids, 100, 200);
    assert_eq!(orders.get_str("supplier_id").unwrap(), "supplier-1");
    let branches = orders.get_array("$or").unwrap();
    assert_eq!(branches.len(), 3);
    let id_branch = branches[0].as_document().unwrap();
    assert_eq!(
        id_branch
            .get_document("id")
            .unwrap()
            .get_array("$in")
            .expect("id 分支必须是 $in")
            .iter()
            .map(|value| value.as_str().unwrap())
            .collect::<Vec<_>>(),
        vec!["order-1"]
    );
    let completed_branch = branches[1].as_document().unwrap();
    assert_eq!(
        completed_branch
            .get_document("completed_at")
            .unwrap()
            .get_i64("$gte")
            .unwrap(),
        100
    );
    assert_eq!(
        completed_branch
            .get_document("completed_at")
            .unwrap()
            .get_i64("$lt")
            .unwrap(),
        200
    );
    // 损坏行分支：已完成但缺少完成时间（仅直接改库可产生）必须纳入，
    // 使 Service 的 confirmed_completed_at 校验 fail-closed。
    let tampered_branch = branches[2].as_document().unwrap();
    assert_eq!(
        tampered_branch.get_str("fulfillment_status").unwrap(),
        "COMPLETED"
    );
    assert!(matches!(
        tampered_branch.get("completed_at").unwrap(),
        mongodb::bson::Bson::Null
    ));
    let item_ids = vec![SupplierFulfillmentItemId::new("item-1")];
    let items = item_scope_filter(&order_ids, &item_ids);
    let branches = items.get_array("$or").unwrap();
    assert_eq!(branches.len(), 2);
    assert_eq!(
        branches[0]
            .as_document()
            .unwrap()
            .get_document("supplier_fulfillment_order_id")
            .unwrap()
            .get_array("$in")
            .expect("订单分支必须是 $in")
            .iter()
            .map(|value| value.as_str().unwrap())
            .collect::<Vec<_>>(),
        vec!["order-1"]
    );
    assert_eq!(
        branches[1]
            .as_document()
            .unwrap()
            .get_document("id")
            .unwrap()
            .get_array("$in")
            .expect("明细分支必须是 $in")
            .iter()
            .map(|value| value.as_str().unwrap())
            .collect::<Vec<_>>(),
        vec!["item-1"]
    );
}
