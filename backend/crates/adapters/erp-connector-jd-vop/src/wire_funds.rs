//! 官方5853分页/交易与5854预存款余额；未核实字段保留未决证据，不猜测账务方向。

use std::collections::BTreeMap;
use std::num::NonZeroU32;

use chrono::{FixedOffset, NaiveDateTime, TimeZone};
use erp_core::common::time::Instant;
use erp_core::money::Amount;
use erp_supply::ports::connector::common::ConnectorResult;
use erp_supply::ports::connector::funds::{
    SupplierFundsEvidence, SupplierFundsObservation, SupplierFundsObservationResult, SupplierFundsQuery,
    SupplierFundsTransaction, SupplierFundsUnresolved,
};
use serde::Deserialize;
use serde_json::value::RawValue;

use super::funds::{JdVopFundsProfile, JdVopFundsSign, JdVopFundsTimeFormat};
use super::mapping;
use super::wire::{WireId, WireText};

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct BalanceChanges {
    pub(super) items: Vec<Box<RawValue>>,
    page_size: WireText,
    page_index: WireText,
    page_item_total: WireText,
    page_total: Option<WireText>,
}

impl BalanceChanges {
    pub(super) fn validate(&self, requested_page: u32, limit: NonZeroU32) -> ConnectorResult<bool> {
        let size = count(&self.page_size)?;
        let page = count(&self.page_index)?;
        let total = count(&self.page_item_total)?;
        if size != u64::from(limit.get()) || page != u64::from(requested_page) || page == 0 {
            return Err(mapping("京东资金分页与冻结请求不一致"));
        }

        let pages = total.checked_add(size - 1).ok_or_else(|| mapping("资金总页数超限"))? / size;
        if self.page_total.as_ref().is_some_and(|value| count(value).ok() != Some(pages))
            || (total > 0 && page > pages)
        {
            return Err(mapping("京东资金分页总数不一致"));
        }

        let offset = (page - 1).checked_mul(size).ok_or_else(|| mapping("资金分页位置超限"))?;
        let expected = total.saturating_sub(offset).min(size);
        if u64::try_from(self.items.len()).ok() != Some(expected) {
            return Err(mapping("京东资金分页存在缺项，保留原检查点继续核实"));
        }
        Ok(page < pages)
    }
}

fn count(value: &WireText) -> ConnectorResult<u64> {
    if !value.0.bytes().all(|byte| byte.is_ascii_digit()) {
        return Err(mapping("资金分页数量无效"));
    }
    value.0.parse().map_err(|_| mapping("资金分页数量超限"))
}

pub(super) fn balance_amount(raw: &RawValue) -> ConnectorResult<Amount> {
    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct Balance {
        balance_info: Option<BalanceInfo>,
    }
    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct BalanceInfo {
        remain_limit: Option<WireText>,
    }
    let balance: Balance = serde_json::from_str(raw.get()).map_err(|_| mapping("京东资金余额字段无效"))?;
    let amount = balance
        .balance_info
        .and_then(|balance| balance.remain_limit)
        .ok_or_else(|| mapping("京东未提供预存款余额，不能以账期或金采额度替代"))?;
    amount.0.parse().map_err(|_| mapping("京东预存款余额精度或金额无效"))
}

struct Candidate {
    transaction_id: Option<String>,
    order_id: Option<String>,
    signed_amount: Option<Amount>,
    occurred_at: Option<Instant>,
    business_code: Option<String>,
    pin: Option<String>,
    account_type: Option<String>,
}

pub(super) fn observation(
    raw: &RawValue,
    profile: &JdVopFundsProfile,
    evidence: SupplierFundsEvidence,
    query: &SupplierFundsQuery,
) -> SupplierFundsObservation {
    let fields = serde_json::from_str::<BTreeMap<String, Box<RawValue>>>(raw.get());
    let fields = fields.unwrap_or_default();
    let candidate = candidate(&fields, profile);
    let result = match transaction(&candidate, profile, &evidence, query) {
        Ok(transaction) => SupplierFundsObservationResult::Transaction(transaction),
        Err(reason) => SupplierFundsObservationResult::Unresolved(SupplierFundsUnresolved {
            observation_id: evidence.reference.clone(),
            evidence_reference: evidence.reference.clone(),
            external_transaction_id: candidate.transaction_id,
            external_order_id: candidate.order_id,
            amount: candidate.signed_amount,
            occurred_at: candidate.occurred_at,
            business_code: candidate.business_code,
            reason_code: reason.into(),
        }),
    };
    SupplierFundsObservation { evidence, result }
}

fn candidate(fields: &BTreeMap<String, Box<RawValue>>, profile: &JdVopFundsProfile) -> Candidate {
    let signed_amount = fields
        .get("amount")
        .and_then(|raw| serde_json::from_str::<WireText>(raw.get()).ok())
        .and_then(|value| value.0.parse().ok());
    Candidate {
        transaction_id: identifier(fields.get("tradeNo").map(Box::as_ref)),
        order_id: identifier(fields.get("jdOrderId").map(Box::as_ref)),
        signed_amount,
        occurred_at: plain(fields.get("createdDate").map(Box::as_ref))
            .and_then(|date| occurred_at(&date, profile)),
        business_code: scalar(fields.get("tradeType").map(Box::as_ref)),
        pin: plain(fields.get("pin").map(Box::as_ref)),
        account_type: scalar(fields.get("accountType").map(Box::as_ref)),
    }
}

fn transaction(
    candidate: &Candidate,
    profile: &JdVopFundsProfile,
    evidence: &SupplierFundsEvidence,
    query: &SupplierFundsQuery,
) -> Result<SupplierFundsTransaction, &'static str> {
    if candidate.pin.as_deref() != Some(profile.expected_pin.as_str())
        || candidate.account_type.as_deref() != Some(profile.account_type.as_str())
    {
        return Err("JD_VOP_FUNDS_ACCOUNT_UNVERIFIED");
    }

    let id = candidate.transaction_id.clone().ok_or("JD_VOP_FUNDS_STABLE_ID_MISSING")?;
    let order_id = candidate.order_id.clone().ok_or("JD_VOP_FUNDS_ORDER_ID_MISSING")?;

    let code = candidate.business_code.as_ref().ok_or("JD_VOP_FUNDS_BUSINESS_CODE_MISSING")?;
    let rule = profile.trade_types.get(code).ok_or("JD_VOP_FUNDS_BUSINESS_CODE_UNKNOWN")?;
    let signed = candidate.signed_amount.ok_or("JD_VOP_FUNDS_AMOUNT_INVALID")?;
    let amount = signed.to_decimal();
    if amount.is_zero() || ((rule.sign == JdVopFundsSign::Negative) != amount.is_sign_negative()) {
        return Err("JD_VOP_FUNDS_AMOUNT_SIGN_UNVERIFIED");
    }
    let amount = Amount::try_from(amount.abs()).map_err(|_| "JD_VOP_FUNDS_AMOUNT_INVALID")?;

    let occurred_at = candidate.occurred_at.ok_or("JD_VOP_FUNDS_TIME_UNVERIFIED")?;
    if occurred_at < query.since || occurred_at > query.through {
        return Err("JD_VOP_FUNDS_TIME_OUTSIDE_WINDOW");
    }
    Ok(SupplierFundsTransaction {
        stable_transaction_id: id,
        external_order_id: Some(order_id),
        direction: rule.direction,
        amount,
        currency: profile.currency,
        occurred_at,
        evidence_reference: evidence.reference.clone(),
        original_transaction_id: None,
        bank_reference: None,
        business_code: code.clone(),
    })
}

fn identifier(raw: Option<&RawValue>) -> Option<String> {
    let value = serde_json::from_str::<WireId>(raw?.get()).ok()?.0;
    (!value.bytes().all(|byte| byte == b'0')).then_some(value)
}

fn scalar(raw: Option<&RawValue>) -> Option<String> {
    serde_json::from_str::<WireText>(raw?.get()).ok().map(|value| value.0)
}

fn plain(raw: Option<&RawValue>) -> Option<String> {
    let value: String = serde_json::from_str(raw?.get()).ok()?;
    (!value.is_empty() && value.len() <= 256).then_some(value)
}

fn occurred_at(value: &str, profile: &JdVopFundsProfile) -> Option<Instant> {
    let format = match profile.time_format {
        JdVopFundsTimeFormat::LocalDateTimeSeconds => "%Y-%m-%d %H:%M:%S",
    };
    let date = NaiveDateTime::parse_from_str(value, format).ok()?;
    let offset = FixedOffset::east_opt(profile.timezone_offset_seconds)?;
    let date = offset.from_local_datetime(&date).single()?;
    Instant::try_from_unix_secs(date.timestamp()).ok()
}

#[cfg(test)]
pub(super) mod tests {
    use erp_supply::ports::connector::funds::{SupplierFundsCurrency, SupplierFundsDirection};

    use super::super::funds::JdVopFundsTradeRule;
    use super::*;

    pub(crate) fn profile() -> JdVopFundsProfile {
        JdVopFundsProfile {
            evidence_reference: "verified-ledger-protocol".into(),
            verified_account_reference: "account-ref".into(),
            expected_pin: "verified-pin".into(),
            account_type: "1".into(),
            currency: SupplierFundsCurrency::CNY,
            timezone_offset_seconds: 8 * 3600,
            time_format: JdVopFundsTimeFormat::LocalDateTimeSeconds,
            maximum_page_size: NonZeroU32::new(20).unwrap(),
            trade_types: BTreeMap::from([
                (
                    "1209".into(),
                    JdVopFundsTradeRule {
                        direction: SupplierFundsDirection::Debit,
                        sign: JdVopFundsSign::Negative,
                    },
                ),
                (
                    "758".into(),
                    JdVopFundsTradeRule {
                        direction: SupplierFundsDirection::Return,
                        sign: JdVopFundsSign::Positive,
                    },
                ),
            ]),
            message_effects: BTreeMap::from([("10".into(), SupplierFundsDirection::Return)]),
        }
    }

    pub(crate) fn query() -> SupplierFundsQuery {
        SupplierFundsQuery {
            since: Instant::from_unix_secs(0),
            through: Instant::from_unix_secs(3600),
            cursor: None,
            limit: NonZeroU32::new(1).unwrap(),
        }
    }

    fn evidence() -> SupplierFundsEvidence {
        SupplierFundsEvidence {
            reference: "evidence".into(),
            body_encrypted: "cipher".into(),
            content_fingerprint: "hash".into(),
        }
    }

    fn row(changes: serde_json::Value) -> Box<RawValue> {
        let mut value = serde_json::json!({"tradeNo":"9007199254740993","jdOrderId":"9007199254740995",
            "amount":"-21.9300","tradeType":"1209","accountType":"1","pin":"verified-pin",
            "createdDate":"1970-01-01 08:30:00"});
        for (key, value_change) in changes.as_object().unwrap() {
            value[key] = value_change.clone();
        }
        serde_json::from_str(&value.to_string()).unwrap()
    }

    #[test]
    fn verified_business_code_sign_account_and_timezone_produce_exact_positive_debit() {
        let observed = observation(&row(serde_json::json!({})), &profile(), evidence(), &query());
        let SupplierFundsObservationResult::Transaction(transaction) = observed.result else {
            panic!("verified transaction expected");
        };
        assert_eq!(transaction.stable_transaction_id, "9007199254740993");
        assert_eq!(transaction.external_order_id.as_deref(), Some("9007199254740995"));
        assert_eq!(transaction.amount.to_decimal(), "21.93".parse().unwrap());
        assert_eq!(transaction.direction, SupplierFundsDirection::Debit);
        assert_eq!(transaction.occurred_at, Instant::from_unix_secs(1800));
        assert_eq!(transaction.original_transaction_id, None);
        assert_eq!(transaction.bank_reference, None);
    }

    #[test]
    fn missing_identity_unknown_code_wrong_account_sign_and_time_only_preserve_unresolved_evidence() {
        for change in [
            serde_json::json!({"tradeNo":null}),
            serde_json::json!({"jdOrderId":null}),
            serde_json::json!({"tradeType":"9999"}),
            serde_json::json!({"pin":"another-pin"}),
            serde_json::json!({"amount":"21.93"}),
            serde_json::json!({"createdDate":"unknown"}),
            serde_json::json!({"amount":"-21.931"}),
        ] {
            let observed = observation(&row(change), &profile(), evidence(), &query());
            let SupplierFundsObservationResult::Unresolved(unresolved) = observed.result else {
                panic!("unresolved expected");
            };
            assert!(!unresolved.reason_code.is_empty());
            assert_eq!(unresolved.evidence_reference, "evidence");
            assert_eq!(observed.evidence.body_encrypted, "cipher");
        }
    }

    #[test]
    fn non_object_rows_preserve_unresolved_evidence_and_account_failure_priority() {
        for body in ["null", "true", "[]", "0", r#""not a transaction""#, "{}"] {
            let raw: Box<RawValue> = serde_json::from_str(body).unwrap();
            let evidence = evidence();
            let observed = observation(&raw, &profile(), evidence.clone(), &query());
            assert_eq!(observed.evidence, evidence);
            assert_eq!(
                observed.result,
                SupplierFundsObservationResult::Unresolved(SupplierFundsUnresolved {
                    observation_id: "evidence".into(),
                    evidence_reference: "evidence".into(),
                    external_transaction_id: None,
                    external_order_id: None,
                    amount: None,
                    occurred_at: None,
                    business_code: None,
                    reason_code: "JD_VOP_FUNDS_ACCOUNT_UNVERIFIED".into(),
                })
            );
        }
    }

    #[test]
    fn transaction_times_include_both_window_edges_and_retain_outside_observations() {
        for (date, timestamp, accepted) in [
            ("1970-01-01 07:59:59", -1, false),
            ("1970-01-01 08:00:00", 0, true),
            ("1970-01-01 09:00:00", 3600, true),
            ("1970-01-01 09:00:01", 3601, false),
        ] {
            let observed =
                observation(&row(serde_json::json!({"createdDate":date})), &profile(), evidence(), &query());
            match observed.result {
                SupplierFundsObservationResult::Transaction(transaction) => {
                    assert!(accepted);
                    assert_eq!(transaction.occurred_at, Instant::from_unix_secs(timestamp));
                    assert_eq!(transaction.amount, "21.93".parse().unwrap());
                },
                SupplierFundsObservationResult::Unresolved(unresolved) => {
                    assert!(!accepted);
                    assert_eq!(unresolved.reason_code, "JD_VOP_FUNDS_TIME_OUTSIDE_WINDOW");
                    assert_eq!(unresolved.occurred_at, Some(Instant::from_unix_secs(timestamp)));
                    assert_eq!(unresolved.amount, Some("-21.93".parse().unwrap()));
                    assert_eq!(unresolved.evidence_reference, "evidence");
                },
            }
        }
    }

    #[test]
    fn balance_uses_only_real_deposit_field_and_never_substitutes_credit_limits() {
        let valid: Box<RawValue> = serde_json::from_str(
            r#"{"balanceInfo":{"remainLimit":128.1000},"debtBalanceInfo":{"deptRemainLimit":"9999"}}"#,
        )
        .unwrap();
        assert_eq!(balance_amount(&valid).unwrap().to_decimal(), "128.10".parse().unwrap());

        let missing: Box<RawValue> =
            serde_json::from_str(r#"{"geiousInfo":{"geiousRemainLimit":"9999"}}"#).unwrap();
        assert!(balance_amount(&missing).is_err());
    }

    #[test]
    fn page_proves_only_consistent_page_completion_and_rejects_missing_items() {
        let valid: BalanceChanges = serde_json::from_str(
            r#"{"pageSize":"1","pageIndex":"1","pageItemTotal":"2","pageTotal":"2","items":[{}]}"#,
        )
        .unwrap();
        assert!(valid.validate(1, NonZeroU32::new(1).unwrap()).unwrap());

        let missing: BalanceChanges = serde_json::from_str(
            r#"{"pageSize":"1","pageIndex":"1","pageItemTotal":"2","pageTotal":"2","items":[]}"#,
        )
        .unwrap();
        assert!(missing.validate(1, NonZeroU32::new(1).unwrap()).is_err());

        let empty: BalanceChanges = serde_json::from_str(
            r#"{"pageSize":"1","pageIndex":"1","pageItemTotal":"0","pageTotal":"0","items":[]}"#,
        )
        .unwrap();
        assert!(!empty.validate(1, NonZeroU32::new(1).unwrap()).unwrap());
    }
}
