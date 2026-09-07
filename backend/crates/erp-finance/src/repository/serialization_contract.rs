//! In-memory BSON compatibility checks for finance persistence contracts.

use crate::entity::receivable::{
    CustomerReceipt, CustomerReceiptData, CustomerReceiptUpdate, Invoice, InvoiceData, InvoiceDirection,
    InvoiceKind,
};
use erp_core::common::time::{BusinessDate, Instant};
use erp_core::ids::{CustomerAccountId, CustomerReceiptId, InvoiceId, PartyId};
use erp_core::money::Amount;
use std::str::FromStr;

fn receipt_data() -> CustomerReceiptData {
    CustomerReceiptData {
        receipt_no: " RC-2026-001 ".to_string(),
        counterparty_party_id: PartyId::new("party-1"),
        customer_id: Some(CustomerAccountId::new("cust-1")),
        received_at: Instant::from_unix_secs(1_700_000_000),
        amount: Amount::from_str("1000.00").unwrap(),
        bank_reference: Some(" BANK-1 ".to_string()),
    }
}

#[test]
fn creator_is_normalized_and_legacy_missing_field_defaults_empty() {
    let mut receipt = CustomerReceipt::new(
        CustomerReceiptId::new("creator-cr"),
        receipt_data(),
        " creator-1 ",
    )
    .unwrap();
    receipt.update(CustomerReceiptUpdate::default()).unwrap();
    assert_eq!(receipt.created_by, "creator-1");
    assert!(CustomerReceipt::new(CustomerReceiptId::new("blank-creator-cr"), receipt_data(), "   ").is_err());

    let mut legacy = bson::serialize_to_document(&receipt).unwrap();
    legacy.remove("created_by");
    let legacy: CustomerReceipt = bson::deserialize_from_document(legacy).unwrap();
    assert!(legacy.created_by.is_empty());
}

fn invoice_data() -> InvoiceData {
    InvoiceData {
        invoice_direction: InvoiceDirection::Sales,
        invoice_kind: InvoiceKind::Blue,
        party_id: PartyId::new("party-1"),
        invoice_code: Some(" 1100199999 ".to_string()),
        invoice_no: " 01234567 ".to_string(),
        invoice_date: BusinessDate::from_ymd(2026, 8, 6).unwrap(),
        gross_amount: Amount::from_str("1000.00").unwrap(),
        net_amount: Amount::from_str("884.96").unwrap(),
        tax_amount: Amount::from_str("115.04").unwrap(),
        rounding_adjustment_amount: Amount::from_str("0.00").unwrap(),
        rounding_reason: None,
        original_invoice_id: None,
    }
}

#[test]
fn invoice_bson_roundtrip_preserves_fields() {
    let invoice = Invoice::new(InvoiceId::new("inv-1"), invoice_data(), "admin-1").unwrap();
    let back: Invoice =
        bson::deserialize_from_document(bson::serialize_to_document(&invoice).unwrap()).unwrap();
    assert_eq!(back, invoice);
}
