//! 原实体 BSON 兼容测试在持久化层执行，复用实体测试中的原始数据。

mod customer_refund {
    use erp_core::ids::CustomerRefundId;
    use mongodb::bson;

    use crate::entity::returns::customer_refund::tests::data;
    use crate::entity::returns::{CustomerRefund, CustomerRefundUpdate};

    #[test]
    fn creator_is_normalized_and_legacy_missing_field_defaults_empty() {
        let mut refund =
            CustomerRefund::new(CustomerRefundId::new("creator-crf"), data(), " creator-1 ").unwrap();
        refund.update(CustomerRefundUpdate::default()).unwrap();
        assert_eq!(refund.created_by, "creator-1");
        assert!(CustomerRefund::new(CustomerRefundId::new("blank-creator-crf"), data(), "   ").is_err());

        let mut legacy = bson::serialize_to_document(&refund).unwrap();
        legacy.remove("created_by");
        let legacy: CustomerRefund = bson::deserialize_from_document(legacy).unwrap();
        assert!(legacy.created_by.is_empty());
    }
}

mod supplier_refund {
    use erp_core::ids::SupplierRefundId;
    use mongodb::bson;

    use crate::entity::returns::supplier_refund::tests::data;
    use crate::entity::returns::{SupplierRefund, SupplierRefundUpdate};

    #[test]
    fn creator_is_normalized_and_legacy_missing_field_defaults_empty() {
        let mut refund =
            SupplierRefund::new(SupplierRefundId::new("creator-srf"), data(), " creator-1 ").unwrap();
        refund.update(SupplierRefundUpdate::default()).unwrap();
        assert_eq!(refund.created_by, "creator-1");
        assert!(SupplierRefund::new(SupplierRefundId::new("blank-creator-srf"), data(), "   ").is_err());

        let mut legacy = bson::serialize_to_document(&refund).unwrap();
        legacy.remove("created_by");
        let legacy: SupplierRefund = bson::deserialize_from_document(legacy).unwrap();
        assert!(legacy.created_by.is_empty());
    }
}

mod receipt_reversal {
    use erp_core::ids::ReceiptReversalId;
    use mongodb::bson;

    use crate::entity::returns::receipt_reversal::tests::data;
    use crate::entity::returns::{ReceiptReversal, ReceiptReversalUpdate};

    #[test]
    fn creator_is_normalized_and_legacy_missing_field_defaults_empty() {
        let mut reversal =
            ReceiptReversal::new(ReceiptReversalId::new("creator-rr"), data(), " creator-1 ").unwrap();
        reversal.update(ReceiptReversalUpdate::default()).unwrap();
        assert_eq!(reversal.created_by, "creator-1");
        assert!(ReceiptReversal::new(ReceiptReversalId::new("blank-creator-rr"), data(), "   ").is_err());

        let mut legacy = bson::serialize_to_document(&reversal).unwrap();
        legacy.remove("created_by");
        let legacy: ReceiptReversal = bson::deserialize_from_document(legacy).unwrap();
        assert!(legacy.created_by.is_empty());
    }
}

mod payment_reversal {
    use erp_core::ids::PaymentReversalId;
    use mongodb::bson;

    use crate::entity::returns::payment_reversal::tests::data;
    use crate::entity::returns::{PaymentReversal, PaymentReversalUpdate};

    #[test]
    fn creator_is_normalized_and_legacy_missing_field_defaults_empty() {
        let mut reversal =
            PaymentReversal::new(PaymentReversalId::new("creator-prr"), data(), " creator-1 ").unwrap();
        reversal.update(PaymentReversalUpdate::default()).unwrap();
        assert_eq!(reversal.created_by, "creator-1");
        assert!(PaymentReversal::new(PaymentReversalId::new("blank-creator-prr"), data(), "   ").is_err());

        let mut legacy = bson::serialize_to_document(&reversal).unwrap();
        legacy.remove("created_by");
        let legacy: PaymentReversal = bson::deserialize_from_document(legacy).unwrap();
        assert!(legacy.created_by.is_empty());
    }
}

/// 资金纠错事实同时冻结 JSON 与 MongoDB wire 形态。
mod wire {
    use std::fmt::Debug;

    use erp_core::ids::{CustomerRefundId, PaymentReversalId, ReceiptReversalId, SupplierRefundId};
    use mongodb::bson::{self, Bson, Document};
    use serde::Serialize;
    use serde::de::DeserializeOwned;

    use crate::entity::returns::{
        CustomerRefund, CustomerRefundStatus, PaymentReversal, PaymentReversalStatus, ReceiptReversal,
        ReceiptReversalStatus, SupplierRefund, SupplierRefundStatus,
    };

    fn roundtrip<T: Serialize + DeserializeOwned + PartialEq + Debug>(value: &T) -> Document {
        let bytes = bson::serialize_to_vec(value).unwrap();
        let back: T = bson::deserialize_from_slice(&bytes).unwrap();
        assert_eq!(&back, value);
        let json = serde_json::to_value(value).unwrap();
        assert_eq!(json["amount"], "1000.00");
        assert_eq!(json["occurred_at"], 1_700_000_000);
        assert!(json["evidence_attachment_id"].is_null());
        let document: Document = bson::deserialize_from_slice(&bytes).unwrap();
        assert!(matches!(document.get("amount"), Some(Bson::Decimal128(_))));
        assert_eq!(document.get_i64("occurred_at").unwrap(), 1_700_000_000);
        assert_eq!(document.get("evidence_attachment_id"), Some(&Bson::Null));
        assert_eq!(document.get_str("status").unwrap(), "draft");
        document
    }

    #[test]
    fn four_funds_documents_keep_amount_time_option_and_bson_roundtrip() {
        let customer = CustomerRefund::new(
            CustomerRefundId::new("wire-crf"),
            crate::entity::returns::customer_refund::tests::data(),
            "creator",
        )
        .unwrap();
        let supplier = SupplierRefund::new(
            SupplierRefundId::new("wire-srf"),
            crate::entity::returns::supplier_refund::tests::data(),
            "creator",
        )
        .unwrap();
        let receipt = ReceiptReversal::new(
            ReceiptReversalId::new("wire-rr"),
            crate::entity::returns::receipt_reversal::tests::data(),
            "creator",
        )
        .unwrap();
        let payment = PaymentReversal::new(
            PaymentReversalId::new("wire-pr"),
            crate::entity::returns::payment_reversal::tests::data(),
            "creator",
        )
        .unwrap();
        assert_eq!(roundtrip(&customer).get_str("original_receipt_id").unwrap(), "cr-1");
        assert_eq!(roundtrip(&supplier).get_str("original_payment_id").unwrap(), "sp-1");
        assert_eq!(roundtrip(&receipt).get_str("original_customer_receipt_id").unwrap(), "cr-1");
        assert_eq!(roundtrip(&payment).get_str("original_supplier_payment_id").unwrap(), "sp-1");
    }

    fn status_codes<T: Serialize + DeserializeOwned + PartialEq + Debug>(values: [T; 4]) {
        for (value, expected) in values.into_iter().zip(["draft", "IN_APPROVAL", "posted", "reversed"]) {
            let json = serde_json::to_string(&value).unwrap();
            assert_eq!(json, format!("\"{expected}\""));
            assert_eq!(serde_json::from_str::<T>(&json).unwrap(), value);
            let bson_value = bson::serialize_to_bson(&value).unwrap();
            assert_eq!(bson_value, Bson::String(expected.to_string()));
            assert_eq!(bson::deserialize_from_bson::<T>(bson_value).unwrap(), value);
        }
    }

    #[test]
    fn four_status_enums_keep_mixed_case_json_and_bson_codes() {
        status_codes([
            CustomerRefundStatus::Draft,
            CustomerRefundStatus::InApproval,
            CustomerRefundStatus::Posted,
            CustomerRefundStatus::Reversed,
        ]);
        status_codes([
            SupplierRefundStatus::Draft,
            SupplierRefundStatus::InApproval,
            SupplierRefundStatus::Posted,
            SupplierRefundStatus::Reversed,
        ]);
        status_codes([
            ReceiptReversalStatus::Draft,
            ReceiptReversalStatus::InApproval,
            ReceiptReversalStatus::Posted,
            ReceiptReversalStatus::Reversed,
        ]);
        status_codes([
            PaymentReversalStatus::Draft,
            PaymentReversalStatus::InApproval,
            PaymentReversalStatus::Posted,
            PaymentReversalStatus::Reversed,
        ]);
    }
}
