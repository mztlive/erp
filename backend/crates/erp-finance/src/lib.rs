//! Financial accounts, invoices, payments and cost facts.

pub mod dto;
pub mod entity;
mod error;
pub mod indexes;
pub mod ports;
pub mod repository;
pub mod service;

pub use error::{Error, Result};

#[cfg(test)]
mod compile_probe_equivalence_tests {
    use erp_core::money::Amount;
    use mongodb::bson;
    use serde::Serialize;

    use crate::service::receivable::mapping::zero_amount;

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
