//! W27 结算来源证据批次。
//!
//! 当前 D32 已提供不可变履约明细成本与退款分配，但尚无可直接结算的外部账单、
//! 运费、服务费和关联到订单的取消事实。本实体把一次受控补证冻结为不可变批次；
//! W27 草稿创建与刷新只能消费该批次，不能接收客户端拼装的结算明细金额。

use erp_core::money::Amount;
use erp_core::validation::normalize_required_text;
use erp_core::{Error, Result};
use rust_decimal::Decimal;

use crate::entity::supplier_settlement::normalize_statement_sha256;

mod batch;
mod components;
mod line;

pub use batch::{SupplierSettlementSourceEvidence, SupplierSettlementSourceEvidenceData};
pub use components::{
    SETTLEMENT_TIMEZONE, SettlementAmountComponents, SettlementCancelEvidence, SettlementPeriod,
    SettlementSourceFactType,
};
pub use line::{SupplierSettlementSourceEvidenceLine, SupplierSettlementSourceEvidenceLineData};

const COMMAND_ID_MAX_LEN: usize = 128;
const POLICY_VALUE_MAX_LEN: usize = 128;
const TIMEZONE_MAX_LEN: usize = 64;
const BILL_VALUE_MAX_LEN: usize = 128;
const EVIDENCE_REFERENCE_MAX_LEN: usize = 256;
const ACTOR_MAX_LEN: usize = 128;
const MAX_LINES: usize = 1_000;
const MAX_REFERENCES_PER_LINE: usize = 32;

/// 校验金额非负。
///
/// # 参数
/// * `value` - 待校验金额
/// * `field` - 错误消息使用的业务字段名称
///
/// # 返回
/// 金额非负时返回 `Ok(())`。
///
/// # 错误
/// 金额为负时返回领域错误。
fn ensure_non_negative(value: Amount, field: &str) -> Result<()> {
    if value.to_decimal() < Decimal::ZERO {
        return Err(Error::from(format!("{field}不得为负")));
    }
    Ok(())
}

fn ensure_triple(gross: Amount, net: Amount, tax: Amount, field: &str) -> Result<()> {
    if net.checked_add(tax) != gross {
        return Err(Error::from(format!("{field}必须满足含税等于不含税加税额")));
    }
    Ok(())
}

fn normalize_references(values: &mut Vec<String>, max: usize) -> Result<()> {
    if values.len() > max {
        return Err(Error::from("来源证据引用数量超限"));
    }
    for value in values.iter_mut() {
        *value = normalize_required_text(
            std::mem::take(value),
            "来源证据引用不能为空",
            EVIDENCE_REFERENCE_MAX_LEN,
            "来源证据引用过长",
        )?;
    }
    values.sort();
    values.dedup();
    Ok(())
}

fn normalize_hash(value: String) -> Result<String> {
    normalize_statement_sha256(value, "来源证据摘要")
        .map_err(|_| Error::from("来源证据摘要必须是64位SHA-256十六进制值"))
}

#[cfg(test)]
mod tests {
    use std::str::FromStr;

    use erp_core::common::time::{BusinessDate, Instant};
    use erp_core::ids::{SupplierAccountId, SupplierFulfillmentItemId, SupplierFulfillmentOrderId};
    use erp_core::money::{Amount, Quantity};

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

    #[test]
    fn source_evidence_accepts_complete_batch() {
        let evidence = SupplierSettlementSourceEvidence::new("source-1", data()).unwrap();
        assert_eq!(evidence.lines.len(), 1);
        assert_eq!(evidence.external_bill_no, "BILL-1");
    }

    #[test]
    fn source_evidence_rejects_duplicate_line_pair() {
        let mut input = data();
        input.lines.push(line());
        assert!(SupplierSettlementSourceEvidence::new("source-2", input).is_err());
    }

    #[test]
    fn source_line_rejects_guessed_tax_or_missing_reference() {
        let mut invalid = line();
        invalid.supplier_billed_tax = amount("0.00");
        assert!(invalid.validate().is_err());

        let mut missing = line();
        missing.evidence_reference_ids.clear();
        assert!(missing.validate().is_err());
    }

    #[test]
    fn period_and_cancel_evidence_own_pairing_and_timezone_rules() {
        let period = SettlementPeriod::new(
            BusinessDate::from_ymd(2026, 7, 1).unwrap(),
            BusinessDate::from_ymd(2026, 7, 31).unwrap(),
            SETTLEMENT_TIMEZONE,
        )
        .unwrap();
        let occurred_at = Instant::from_unix_secs(
            chrono::DateTime::parse_from_rfc3339("2026-07-01T00:00:00+08:00").unwrap().timestamp(),
        );
        assert!(period.contains(occurred_at));
        let cancel =
            SettlementCancelEvidence::from_optional(Some(occurred_at), Some(" proof-1 ".to_string()), period)
                .unwrap()
                .unwrap();
        assert_eq!(cancel.reference_id(), "proof-1");
        assert!(SettlementCancelEvidence::from_optional(Some(occurred_at), None, period).is_err());
        assert!(SettlementPeriod::new(period.end(), period.start(), SETTLEMENT_TIMEZONE).is_err());
        assert!(SettlementPeriod::new(period.start(), period.end(), "UTC").is_err());
    }

    #[test]
    fn period_secs_bounds_match_contains_across_dense_samples() {
        let start = BusinessDate::from_ymd(2026, 7, 1).unwrap();
        let end = BusinessDate::from_ymd(2026, 7, 31).unwrap();
        let period = SettlementPeriod::new(start, end, SETTLEMENT_TIMEZONE).unwrap();
        let (start_secs, end_secs) = SettlementPeriod::secs_bounds(start, end);
        // 以小时为步长覆盖期间前后各一天，秒级区间判定必须与上海业务日期
        // 判定完全一致（开始日 00:00 含、结束日次日 00:00 不含）。
        let first = chrono::DateTime::parse_from_rfc3339("2026-06-30T00:00:00+08:00").unwrap().timestamp();
        let last = chrono::DateTime::parse_from_rfc3339("2026-08-01T00:00:00+08:00").unwrap().timestamp();
        let mut cursor = first;
        while cursor <= last {
            let in_interval = (start_secs..end_secs).contains(&cursor);
            assert_eq!(
                period.contains(Instant::from_unix_secs(cursor)),
                in_interval,
                "秒级边界与业务日期判定在 {cursor} 不一致"
            );
            cursor += 3600;
        }
        assert!(start_secs < end_secs, "边界必须单调");
    }

    #[test]
    fn source_line_factory_derives_erp_components_and_rejects_excess_refund() {
        let data = SupplierSettlementSourceEvidenceLineData {
            supplier_fulfillment_order_id: SupplierFulfillmentOrderId::new("order-1"),
            supplier_fulfillment_item_id: SupplierFulfillmentItemId::new("item-1"),
            quantity: Quantity::from_str("1").unwrap(),
            source_fact_types: vec![SettlementSourceFactType::FulfillmentCompleted],
            evidence_reference_ids: vec!["proof-1".to_string()],
            order: SettlementAmountComponents::new(
                amount("100.00"),
                amount("87.00"),
                amount("13.00"),
                "订单金额",
            )
            .unwrap(),
            freight: SettlementAmountComponents::new(
                amount("10.00"),
                amount("8.70"),
                amount("1.30"),
                "运费金额",
            )
            .unwrap(),
            service_fee: SettlementAmountComponents::zero(),
            refund: SettlementAmountComponents::new(
                amount("5.00"),
                amount("4.35"),
                amount("0.65"),
                "退款金额",
            )
            .unwrap(),
            supplier_billed: SettlementAmountComponents::new(
                amount("105.00"),
                amount("91.35"),
                amount("13.65"),
                "供应商账单金额",
            )
            .unwrap(),
        };
        let built = SupplierSettlementSourceEvidenceLine::from_components(data.clone()).unwrap();
        assert_eq!(built.erp_gross, amount("105.00"));
        assert_eq!(built.erp_net, amount("91.35"));
        assert_eq!(built.erp_tax, amount("13.65"));

        let excess_refund = SupplierSettlementSourceEvidenceLineData {
            refund: SettlementAmountComponents::new(
                amount("111.00"),
                amount("96.57"),
                amount("14.43"),
                "退款金额",
            )
            .unwrap(),
            ..data
        };
        assert!(SupplierSettlementSourceEvidenceLine::from_components(excess_refund).is_err());
    }

    #[test]
    fn source_hash_and_source_version_rules_are_deterministic() {
        let mut first = data();
        let mut second_line = line();
        second_line.supplier_fulfillment_item_id = SupplierFulfillmentItemId::new("item-2");
        second_line.evidence_reference_ids = vec!["proof-2".to_string(), "proof-1".to_string()];
        first.lines.push(second_line);
        let first_hash = first.canonical_source_hash();

        let mut second = first.clone();
        second.lines.reverse();
        second.lines[0].evidence_reference_ids.reverse();
        assert_eq!(first_hash, second.canonical_source_hash());

        first.source_hash = first_hash;
        let evidence = SupplierSettlementSourceEvidence::new("source-1", first).unwrap();
        assert!(evidence.ensure_newer_source_version(2).is_ok());
        assert!(evidence.ensure_newer_source_version(1).is_err());
        assert!(evidence.matches_request_hash(&"b".repeat(64)));
        assert!(
            SupplierSettlementSourceEvidence::ensure_unique_item_ids(&[
                SupplierFulfillmentItemId::new("item-1"),
                SupplierFulfillmentItemId::new("item-1"),
            ])
            .is_err()
        );
    }
}
