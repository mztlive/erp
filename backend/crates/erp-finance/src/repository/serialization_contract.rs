//! In-memory BSON compatibility checks for finance persistence contracts.

use std::str::FromStr;

use erp_core::common::time::{BusinessDate, Instant};
use erp_core::ids::{CustomerAccountId, CustomerReceiptId, InvoiceId, PartyId};
use erp_core::money::Amount;

use crate::entity::receivable::{
    CustomerReceipt, CustomerReceiptData, CustomerReceiptUpdate, Invoice, InvoiceData, InvoiceDirection,
    InvoiceKind,
};

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
    let mut receipt =
        CustomerReceipt::new(CustomerReceiptId::new("creator-cr"), receipt_data(), " creator-1 ").unwrap();
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

/// 金额精度 wire 探针（原 lib.rs 入口测试下沉到序列化契约旁，lib.rs 只剩模块声明）。
///
/// 锁定 `zero_amount` 的 `0.00` 解析拼写、JSON 与 Decimal128 wire 形态。
#[cfg(test)]
mod zero_amount_wire_tests {
    use erp_core::money::Amount;
    use serde::Serialize;

    use crate::entity::receivable::allocation_amount::zero_amount;

    #[derive(Serialize)]
    struct MoneyDocument {
        amount: Amount,
    }

    #[test]
    fn zero_amount_parse_spelling_preserves_value_json_and_decimal128_wire() {
        let original = MoneyDocument { amount: zero_amount() };
        let alternative = MoneyDocument { amount: "0.00".parse::<Amount>().unwrap() };
        assert_eq!(original.amount, alternative.amount);
        assert_eq!(original.amount.to_decimal().scale(), 2);
        assert_eq!(alternative.amount.to_decimal().scale(), 2);
        let original_json = serde_json::to_string(&original).unwrap();
        assert_eq!(original_json, r#"{"amount":"0.00"}"#);
        assert_eq!(serde_json::to_string(&alternative).unwrap(), original_json);

        let original_wire = bson::serialize_to_vec(&original).unwrap();
        assert_eq!(bson::serialize_to_vec(&alternative).unwrap(), original_wire);
        let document: bson::Document = bson::deserialize_from_slice(&original_wire).unwrap();
        match document.get("amount") {
            Some(bson::Bson::Decimal128(value)) => assert_eq!(value.to_string(), "0.00"),
            other => panic!("expected Decimal128 amount, got {other:?}"),
        }
    }

    #[test]
    fn former_zero_literal_probe_changes_scale_json_and_decimal128_wire() {
        let original = MoneyDocument { amount: zero_amount() };
        let invalid_probe = MoneyDocument { amount: "0".parse::<Amount>().unwrap() };
        assert_eq!(original.amount, invalid_probe.amount);
        assert_eq!(original.amount.to_decimal().scale(), 2);
        assert_eq!(invalid_probe.amount.to_decimal().scale(), 0);
        assert_eq!(serde_json::to_string(&invalid_probe).unwrap(), r#"{"amount":"0"}"#);
        assert_ne!(serde_json::to_string(&original).unwrap(), serde_json::to_string(&invalid_probe).unwrap());
        let original_wire = bson::serialize_to_vec(&original).unwrap();
        let invalid_wire = bson::serialize_to_vec(&invalid_probe).unwrap();
        assert_ne!(original_wire, invalid_wire);
        let original_doc: bson::Document = bson::deserialize_from_slice(&original_wire).unwrap();
        let invalid_doc: bson::Document = bson::deserialize_from_slice(&invalid_wire).unwrap();
        match (original_doc.get("amount"), invalid_doc.get("amount")) {
            (Some(bson::Bson::Decimal128(before)), Some(bson::Bson::Decimal128(after))) => {
                assert_eq!(before.to_string(), "0.00");
                assert_eq!(after.to_string(), "0");
                assert_ne!(before.bytes(), after.bytes());
            },
            other => panic!("expected two Decimal128 amounts, got {other:?}"),
        }
    }
}
