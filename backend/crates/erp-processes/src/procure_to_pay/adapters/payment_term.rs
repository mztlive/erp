//! 在采购原解析时点调用供应商唯一付款条件规则。
use erp_procurement::entity::facts::PaymentTermFact;
use erp_supplier::{split_encoded_payment_term_snapshot, SupplierPaymentTerm};
/// 解析采购单付款条件；保持提供方原校验与错误。
pub(crate) fn parse(code: &str) -> erp_core::Result<PaymentTermFact> {
    let term = SupplierPaymentTerm::parse(code)?;
    Ok(PaymentTermFact {
        canonical_code: term.code().to_string(),
        prepay_gate: term.prepay_gate(),
        days_after_delivery: term.days_after_delivery(),
    })
}
/// 先分离历史快照附带的经营类目，再按提供方付款条件规则解析。
pub(crate) fn parse_snapshot(code: &str) -> erp_core::Result<PaymentTermFact> {
    parse(&split_encoded_payment_term_snapshot(code).payment_term_code)
}

#[cfg(test)]
mod tests {
    use super::*;
    use erp_core::common::time::BusinessDate;
    use erp_procurement::entity::purchase_order::PaymentTermSnapshot;

    #[test]
    fn payment_fact_preserves_all_provider_codes_gates_and_due_rules() {
        for (raw, canonical, gate, days) in [
            ("预付款", "PREPAY_100", true, None),
            ("PREPAY-50", "PREPAY_50", true, None),
            ("PREPAY-30", "PREPAY_30", true, None),
            ("现结", "CASH_ON_APPROVAL", false, None),
            ("NET-15", "POSTPAY_NET15", false, Some(15)),
            (" NET-30 ", "POSTPAY_NET30", false, Some(30)),
        ] {
            let fact = parse(raw).unwrap();
            assert_eq!(fact.canonical_code, canonical);
            assert_eq!(fact.prepay_gate, gate);
            assert_eq!(fact.days_after_delivery, days);
        }
    }

    #[test]
    fn historical_snapshot_separators_use_provider_rules_and_frozen_due_date() {
        let approved = BusinessDate::from_ymd(2026, 8, 26).unwrap();
        let delivery = BusinessDate::from_ymd(2026, 8, 31).unwrap();
        for encoded in ["NET-30｜经营类目：礼盒", " NET-30 | 经营类目：礼盒 "] {
            let snapshot = PaymentTermSnapshot {
                payment_term_code: encoded.to_string(),
                prepay_gate: false,
                prepay_minimum_amount: None,
                prepay_minimum_ratio: None,
            };
            assert_eq!(
                snapshot
                    .payable_due_date(approved, Some(delivery), parse_snapshot)
                    .unwrap(),
                BusinessDate::from_ymd(2026, 9, 30).unwrap()
            );
            assert!(parse(encoded).is_err());
        }
    }

    #[test]
    fn unknown_snapshot_terms_preserve_provider_error_before_missing_delivery() {
        let encoded = "先用后付｜经营类目：礼盒";
        let provider_error = SupplierPaymentTerm::parse("先用后付").unwrap_err().to_string();
        assert_eq!(parse_snapshot(encoded).unwrap_err().to_string(), provider_error);
        let snapshot = PaymentTermSnapshot {
            payment_term_code: encoded.to_string(),
            prepay_gate: false,
            prepay_minimum_amount: None,
            prepay_minimum_ratio: None,
        };
        assert_eq!(
            snapshot
                .payable_due_date(BusinessDate::from_ymd(2026, 8, 26).unwrap(), None, parse_snapshot)
                .unwrap_err()
                .to_string(),
            provider_error
        );
    }
}
