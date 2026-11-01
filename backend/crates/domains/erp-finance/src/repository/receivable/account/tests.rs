use std::str::FromStr;

use erp_core::ids::{CustomerAccountId, PartyId, SalesOrderId};
use erp_core::money::Amount;
use mongodb::bson::{Bson, doc};
use persistence_core::{NoTransaction, QueryFilter};

use super::super::sort_doc;
use super::write::{amount_bson, progress_pipeline};
use super::{ReceivableAccountFilter, ReceivableAccountInvoicingExt};
use crate::entity::receivable::ReceivableAccountStatus;
use crate::repository::ReceivableExt;
use crate::repository::owned::ReceivableAccountRepository;

#[test]
fn account_filter_applies_optional_fields_and_deleted_filter() {
    let mut filter = ReceivableAccountFilter {
        customer_id: Some(CustomerAccountId::new("cust-1")),
        counterparty_party_id: Some(PartyId::new("party-1")),
        status: Some(ReceivableAccountStatus::Open),
        ..Default::default()
    };

    let document = filter.to_doc();
    assert_eq!(document.get_i64("deleted_at").unwrap(), 0);
    assert_eq!(document.get_str("customer_id").unwrap(), "cust-1");
    assert_eq!(document.get_str("counterparty_party_id").unwrap(), "party-1");
    assert_eq!(document.get_str("status").unwrap(), "open");

    filter.keyword = Some("XS.1".to_string());
    filter.keyword_sales_order_ids = vec![SalesOrderId::new("sales-1")];
    filter.keyword_party_ids = vec![PartyId::new("party-1")];
    let searched = filter.to_doc();
    let alternatives = searched.get_array("$or").unwrap();
    assert_eq!(alternatives.len(), 6);
    assert_eq!(alternatives[4], Bson::Document(doc! { "sales_order_id": { "$in": ["sales-1"] } }));
    assert_eq!(alternatives[5], Bson::Document(doc! { "counterparty_party_id": { "$in": ["party-1"] } }));
    assert_eq!(searched.get_str("customer_id").unwrap(), "cust-1");
    assert_eq!(searched.get_str("status").unwrap(), "open");
    assert_eq!(searched.get_i64("deleted_at").unwrap(), 0);
}

#[test]
fn sort_doc_maps_whitelisted_fields_and_falls_back() {
    assert_eq!(sort_doc(Some("amount"), true, &["amount", "received_at"]), doc! { "amount": 1, "id": 1 });
    assert_eq!(sort_doc(Some("$where"), false, &["amount"]), doc! { "created_at": -1, "id": -1 });
    assert_eq!(sort_doc(None, true, &[]), doc! { "created_at": 1, "id": 1 });
}

#[test]
fn apply_pipeline_guards_status_and_keeps_decimal_fidelity() {
    let amount = Amount::from_str("100.50").unwrap();
    let pipeline =
        progress_pipeline("settled_total", "open_total", &amount_bson(&amount).unwrap(), true, "admin-1");

    let set = pipeline[0].get_document("$set").unwrap();
    let add = set.get_document("settled_total").unwrap().get_array("$add").unwrap();
    assert_eq!(add[0], Bson::String("$settled_total".to_string()));
    assert!(matches!(add[1], Bson::Decimal128(_)));
    assert!(set.get_document("status").unwrap().get("$cond").is_some());
}

#[test]
fn revert_pipeline_reduces_progress_without_status_cond_misuse() {
    let amount = Amount::from_str("50.00").unwrap();
    let pipeline = progress_pipeline(
        "invoiced_total",
        "open_invoiceable_total",
        &amount_bson(&amount).unwrap(),
        false,
        "sys",
    );

    let set = pipeline[0].get_document("$set").unwrap();
    assert!(set.contains_key("invoiced_total"));
    assert!(set.contains_key("open_invoiceable_total"));
    assert!(!set.contains_key("status"), "开票进度不派生状态");
}

#[test]
fn revert_pipeline_derives_open_when_progress_reaches_zero() {
    let amount = Amount::from_str("1000.00").unwrap();
    let pipeline =
        progress_pipeline("settled_total", "open_total", &amount_bson(&amount).unwrap(), false, "sys");

    let set = pipeline[0].get_document("$set").unwrap();
    let cond = set.get_document("status").unwrap().get_array("$cond").unwrap();
    assert!(cond[0].as_document().unwrap().get_array("$eq").is_ok());
    assert_eq!(cond[1], Bson::String("settled".to_string()), "开放余额归零为已结清");
    let nested = cond[2].as_document().unwrap().get_array("$cond").unwrap();
    assert!(nested[0].as_document().unwrap().get_array("$eq").is_ok());
    assert_eq!(nested[1], Bson::String("open".to_string()), "已核销归零为未结");
    assert_eq!(nested[2], Bson::String("partially_settled".to_string()));
}

/// 空输入批量回退直接成功且不访问数据库。
#[tokio::test]
async fn revert_invoicings_many_empty_input_returns_empty_without_db() {
    let client = mongodb::Client::with_uri_str("mongodb://127.0.0.1:1").await.expect("客户端句柄创建失败");
    let database = client.database("unused");
    let repository = ReceivableAccountRepository::new(
        &database,
        <mongodb::Database as ReceivableExt>::RECEIVABLE_ACCOUNTS,
    );
    let repository: ReceivableAccountRepository<'_> = repository;
    let result = repository
        .revert_invoicings_many(&[], "tester", &mut NoTransaction)
        .await
        .expect("空输入批量红冲必须成功");
    assert!(result.applied.is_empty());
    assert!(result.rejected.is_empty());
}
