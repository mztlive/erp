//! 结算持久化 JSON 与 raw BSON 格式合同；只通过实体公开构造器构建夹具。

use crate::entity::supplier_settlement::*;
use erp_core::common::time::{BusinessDate, Instant};
use erp_core::ids::{SupplierAccountId, SupplierFulfillmentItemId, SupplierFulfillmentOrderId};
use erp_core::money::{Amount, Quantity};
use std::str::FromStr;

mod statement {
    use super::*;
    fn sample_data() -> SupplierSettlementStatementData {
        SupplierSettlementStatementData {
            statement_no: " ST-2026-001 ".to_string(),
            supplier_id: SupplierAccountId::new("supplier-1"),
            period_start: BusinessDate::from_ymd(2026, 7, 1).unwrap(),
            period_end: BusinessDate::from_ymd(2026, 7, 31).unwrap(),
            period_policy_id: "calendar-month".to_string(),
            period_policy_version: "1".to_string(),
            period_timezone: "Asia/Shanghai".to_string(),
            external_bill_no: None,
            external_bill_version: None,
            erp_amount: Amount::from_str("1000.00").unwrap(),
            supplier_amount: Amount::from_str("1023.45").unwrap(),
            subject_hash: "a".repeat(64),
            source_as_of: Instant::from_unix_secs(1_700_000_000),
            source_snapshot_at: Instant::from_unix_secs(1_700_000_000),
            source_snapshot_hash: "b".repeat(64),
            refresh_cutoff_policy_id: "supplier-settlement-review-cutoff".to_string(),
            refresh_cutoff_policy_version: "1".to_string(),
            prepared_by: " 经办人-a ".to_string(),
        }
    }

    /// raw BSON 金额、业务时间与 JSON 空值/状态沿原持久化协议。
    #[test]
    fn statement_json_and_raw_bson_keep_storage_contract() {
        let value = SupplierSettlementStatement::new(
            SupplierSettlementStatementId::new("statement-wire"),
            sample_data(),
        )
        .unwrap();
        let json = serde_json::to_value(&value).unwrap();
        assert_eq!(json["id"], "statement-wire");
        assert!(json.get("base").is_none());
        assert_eq!(json["status"], "DRAFT");
        assert_eq!(json["period_start"], "2026-07-01");
        assert_eq!(json["source_as_of"], 1_700_000_000_i64);
        assert_eq!(json["erp_amount"], "1000.00");
        for key in [
            "external_bill_no",
            "external_bill_version",
            "reviewed_by",
            "review_result",
            "reviewed_at",
            "confirmed_at",
            "payable_account_id",
        ] {
            assert!(json.as_object().unwrap().contains_key(key));
            assert!(json[key].is_null());
        }
        let bytes = bson::serialize_to_vec(&value).unwrap();
        let wire: bson::Document = bson::deserialize_from_slice(&bytes).unwrap();
        for key in ["erp_amount", "supplier_amount", "difference_amount"] {
            assert!(matches!(wire.get(key), Some(bson::Bson::Decimal128(_))));
        }
        assert_eq!(wire.get_i64("source_as_of").unwrap(), 1_700_000_000);
        let restored: SupplierSettlementStatement = bson::deserialize_from_slice(&bytes).unwrap();
        assert_eq!(restored, value);
    }
}

mod difference {
    use super::*;
    fn sample_data() -> SupplierSettlementDifferenceData {
        SupplierSettlementDifferenceData {
            statement_item_id: SupplierSettlementItemId::new("statement-item-1"),
            difference_type: SettlementDifferenceType::Amount,
            difference_amount: Amount::from_str("12.00").unwrap(),
            status: SettlementDifferenceStatus::Pending,
            resolution: None,
            resolved_by: None,
            resolved_at: None,
        }
    }

    /// 差异状态、空处理三元组及有符号金额保持原 wire。
    #[test]
    fn difference_json_and_raw_bson_keep_resolution_and_signed_amount_contract() {
        let mut input = sample_data();
        input.difference_amount = Amount::from_str("-12.00").unwrap();
        let value =
            SupplierSettlementDifference::new(SupplierSettlementDifferenceId::new("difference-wire"), input)
                .unwrap();
        let json = serde_json::to_value(&value).unwrap();
        assert_eq!(json["difference_type"], "AMOUNT");
        assert_eq!(json["status"], "PENDING");
        assert_eq!(json["difference_amount"], "-12.00");
        for key in ["resolution", "resolved_by", "resolved_at"] {
            assert!(json.as_object().unwrap().contains_key(key));
            assert!(json[key].is_null());
        }
        let bytes = bson::serialize_to_vec(&value).unwrap();
        let wire: bson::Document = bson::deserialize_from_slice(&bytes).unwrap();
        assert!(matches!(
            wire.get("difference_amount"),
            Some(bson::Bson::Decimal128(_))
        ));
        let restored: SupplierSettlementDifference = bson::deserialize_from_slice(&bytes).unwrap();
        assert_eq!(restored, value);
    }
}

mod item {
    use super::*;
    fn sample_data() -> SupplierSettlementItemData {
        SupplierSettlementItemData {
            statement_id: SupplierSettlementStatementId::new("statement-1"),
            supplier_fulfillment_order_id: SupplierFulfillmentOrderId::new("order-1"),
            supplier_fulfillment_item_id: SupplierFulfillmentItemId::new("item-1"),
            quantity: Quantity::from_str("2").unwrap(),
            order_amount: Amount::from_str("100.00").unwrap(),
            freight_amount: Amount::from_str("10.00").unwrap(),
            service_fee_amount: Amount::from_str("5.00").unwrap(),
            refund_amount: Amount::from_str("15.00").unwrap(),
            erp_calculated_amount: Amount::from_str("100.00").unwrap(),
            erp_calculated_net_amount: Amount::from_str("87.00").unwrap(),
            erp_calculated_tax_amount: Amount::from_str("13.00").unwrap(),
            supplier_billed_amount: Amount::from_str("99.50").unwrap(),
            supplier_billed_net_amount: Amount::from_str("86.57").unwrap(),
            supplier_billed_tax_amount: Amount::from_str("12.93").unwrap(),
        }
    }

    /// 原数量精度与全部金额字段在 raw BSON 使用 Decimal128。
    #[test]
    fn item_json_and_raw_bson_keep_quantity_and_amount_contract() {
        let value =
            SupplierSettlementItem::new(SupplierSettlementItemId::new("item-wire"), sample_data()).unwrap();
        let json = serde_json::to_value(&value).unwrap();
        assert_eq!(json["quantity"], "2");
        assert_eq!(json["supplier_billed_amount"], "99.50");
        let bytes = bson::serialize_to_vec(&value).unwrap();
        let wire: bson::Document = bson::deserialize_from_slice(&bytes).unwrap();
        for key in [
            "quantity",
            "order_amount",
            "freight_amount",
            "service_fee_amount",
            "refund_amount",
            "erp_calculated_amount",
            "erp_calculated_net_amount",
            "erp_calculated_tax_amount",
            "supplier_billed_amount",
            "supplier_billed_net_amount",
            "supplier_billed_tax_amount",
        ] {
            assert!(matches!(wire.get(key), Some(bson::Bson::Decimal128(_))));
        }
        let restored: SupplierSettlementItem = bson::deserialize_from_slice(&bytes).unwrap();
        assert_eq!(restored, value);
    }
}

mod evidence {
    use super::*;
    fn data() -> SupplierSettlementDifferenceEvidenceData {
        SupplierSettlementDifferenceEvidenceData {
            request_id: "evidence-1".to_string(),
            statement_id: SupplierSettlementStatementId::new("statement-1"),
            difference_id: SupplierSettlementDifferenceId::new("difference-1"),
            evidence_reference_ids: vec!["ticket://T-1".to_string()],
            opinion_code: Some("PROCUREMENT_NOTE".to_string()),
            comment: Some("供应商已确认".to_string()),
            provided_by: "buyer-1".to_string(),
            provided_at: Instant::from_unix_secs(1_700_000_000),
            command_hash: "a".repeat(64),
        }
    }

    /// 不可变补证保留秒级时间、显式空 Option 和基础字段 flatten。
    #[test]
    fn difference_evidence_json_and_raw_bson_keep_time_and_null_contract() {
        let mut input = data();
        input.opinion_code = None;
        input.comment = None;
        let value = SupplierSettlementDifferenceEvidence::new("evidence-wire", input).unwrap();
        let json = serde_json::to_value(&value).unwrap();
        assert_eq!(json["id"], "evidence-wire");
        assert!(json.get("base").is_none());
        assert_eq!(json["provided_at"], 1_700_000_000_i64);
        assert!(json.as_object().unwrap().contains_key("opinion_code"));
        assert!(json["opinion_code"].is_null());
        assert!(json.as_object().unwrap().contains_key("comment"));
        assert!(json["comment"].is_null());
        let bytes = bson::serialize_to_vec(&value).unwrap();
        let wire: bson::Document = bson::deserialize_from_slice(&bytes).unwrap();
        assert_eq!(wire.get_i64("provided_at").unwrap(), 1_700_000_000);
        let restored: SupplierSettlementDifferenceEvidence = bson::deserialize_from_slice(&bytes).unwrap();
        assert_eq!(restored, value);
    }
}

mod source_evidence {
    use super::*;
    fn amount(value: &str) -> Amount {
        Amount::from_str(value).unwrap()
    }

    fn line() -> SupplierSettlementSourceEvidenceLine {
        SupplierSettlementSourceEvidenceLine {
            supplier_fulfillment_order_id: SupplierFulfillmentOrderId::new("order-1"),
            supplier_fulfillment_item_id: SupplierFulfillmentItemId::new("item-1"),
            quantity: Quantity::from_str("1.000000").unwrap(),
            source_fact_types: vec![SettlementSourceFactType::FulfillmentCompleted],
            evidence_reference_ids: vec!["fulfillment://order-1/item-1".to_string()],
            order_gross: amount("100.00"),
            order_net: amount("87.00"),
            order_tax: amount("13.00"),
            freight_gross: amount("10.00"),
            freight_net: amount("8.70"),
            freight_tax: amount("1.30"),
            service_fee_gross: amount("0.00"),
            service_fee_net: amount("0.00"),
            service_fee_tax: amount("0.00"),
            refund_gross: amount("5.00"),
            refund_net: amount("4.35"),
            refund_tax: amount("0.65"),
            erp_gross: amount("105.00"),
            erp_net: amount("91.35"),
            erp_tax: amount("13.65"),
            supplier_billed_gross: amount("105.00"),
            supplier_billed_net: amount("91.35"),
            supplier_billed_tax: amount("13.65"),
        }
    }

    fn data() -> SupplierSettlementSourceEvidenceData {
        SupplierSettlementSourceEvidenceData {
            request_id: "source-1".to_string(),
            supplier_id: SupplierAccountId::new("supplier-1"),
            period_start: BusinessDate::from_ymd(2026, 7, 1).unwrap(),
            period_end: BusinessDate::from_ymd(2026, 7, 31).unwrap(),
            period_policy_id: "monthly".to_string(),
            period_policy_version: "1".to_string(),
            timezone: "Asia/Shanghai".to_string(),
            source_version: 1,
            external_bill_no: "BILL-1".to_string(),
            external_bill_version: "1".to_string(),
            external_bill_evidence_reference_id: "bill://BILL-1/1".to_string(),
            lines: vec![line()],
            source_as_of: Instant::from_unix_secs(1_700_000_000),
            recorded_by: "finance-1".to_string(),
            source_hash: "a".repeat(64),
            request_hash: "b".repeat(64),
        }
    }

    /// 来源冻结行的 18 个金额和数量按 raw BSON Decimal128 往返。
    #[test]
    fn source_evidence_json_and_raw_bson_keep_frozen_line_contract() {
        let value = SupplierSettlementSourceEvidence::new("source-wire", data()).unwrap();
        let json = serde_json::to_value(&value).unwrap();
        assert_eq!(json["period_start"], "2026-07-01");
        assert_eq!(json["source_as_of"], 1_700_000_000_i64);
        assert_eq!(json["lines"][0]["source_fact_types"][0], "FULFILLMENT_COMPLETED");
        assert_eq!(json["lines"][0]["quantity"], "1.000000");
        assert_eq!(json["lines"][0]["erp_gross"], "105.00");
        let bytes = bson::serialize_to_vec(&value).unwrap();
        let wire: bson::Document = bson::deserialize_from_slice(&bytes).unwrap();
        let line = wire.get_array("lines").unwrap()[0].as_document().unwrap();
        assert!(matches!(line.get("quantity"), Some(bson::Bson::Decimal128(_))));
        for prefix in [
            "order",
            "freight",
            "service_fee",
            "refund",
            "erp",
            "supplier_billed",
        ] {
            for component in ["gross", "net", "tax"] {
                assert!(matches!(
                    line.get(format!("{prefix}_{component}")),
                    Some(bson::Bson::Decimal128(_))
                ));
            }
        }
        let restored: SupplierSettlementSourceEvidence = bson::deserialize_from_slice(&bytes).unwrap();
        assert_eq!(restored, value);
    }
}
