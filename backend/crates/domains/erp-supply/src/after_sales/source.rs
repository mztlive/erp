use std::collections::BTreeMap;

use erp_core::money::Amount;

use super::SettlementEvidence;
use crate::entity::supplier_settlement::{
    SettlementPeriod, SupplierSettlementSourceEvidence, SupplierSettlementSourceEvidenceLine,
};
use crate::ports::connector::settlement::SettlementMovement;
use crate::{Error, Result};

/// 核对 API 流水与正式来源批次的订单、供应商、金额及一次关联。
/// 所有涉及订单的扣费、退款及费用分别守恒。
/// 跨供应商、重复关联、未覆盖订单或金额不等时拒绝。
/// `source` 为领域已派生的完整来源批次；`rows` 为其引用的可靠 API 流水。
pub fn validate_settlement_source(
    source: &SupplierSettlementSourceEvidence,
    rows: &[SettlementEvidence],
) -> Result<()> {
    let period = SettlementPeriod::new(source.period_start, source.period_end, &source.timezone)?;
    for line in &source.lines {
        for id in line
            .evidence_reference_ids
            .iter()
            .filter_map(|reference| reference.strip_prefix("supplier-api-settlement:"))
        {
            if !rows.iter().any(|row| {
                row.base.id == id
                    && row.evidence.value.order.merchant_order_no
                        == line.supplier_fulfillment_order_id.as_ref()
            }) {
                return Err(Error::ConflictError("API 结算来源引用不属于当前明细订单".into()));
            }
        }
    }

    let mut groups: BTreeMap<&str, Vec<&SettlementEvidence>> = BTreeMap::new();
    for row in rows {
        if !period.contains(row.evidence.value.occurred_at)
            || row.supplier_id != source.supplier_id.as_ref()
            || row.source_evidence_id.as_ref().is_some_and(|id| id != &source.base.id)
        {
            return Err(Error::ConflictError("结算流水已关联另一来源或不属于当前供应商".into()));
        }
        groups.entry(&row.evidence.value.order.merchant_order_no).or_default().push(row);
    }

    for (order, entries) in groups {
        let lines: Vec<_> =
            source.lines.iter().filter(|line| line.supplier_fulfillment_order_id.as_ref() == order).collect();
        if lines.is_empty() {
            return Err(Error::ConflictError("结算流水缺少对应正式来源行".into()));
        }
        validate_totals(&entries, &lines)?;
        for row in entries {
            let reference = format!("supplier-api-settlement:{}", row.base.id);
            if !lines.iter().any(|line| line.evidence_reference_ids.contains(&reference)) {
                return Err(Error::ConflictError("结算流水被引用到其他订单".into()));
            }
        }
    }
    Ok(())
}

fn validate_totals(
    rows: &[&SettlementEvidence],
    lines: &[&SupplierSettlementSourceEvidenceLine],
) -> Result<()> {
    let mut goods = Vec::new();
    let mut freight = Vec::new();
    let mut service = Vec::new();
    let mut refunds = Vec::new();
    for row in rows {
        match &row.evidence.value.movement {
            SettlementMovement::Charge { goods: g, shipping: f, service: s } => {
                goods.push(*g);
                freight.push(*f);
                service.push(*s);
            },
            SettlementMovement::Refund { amount, .. } => refunds.push(*amount),
        }
    }

    let goods_total = sum_amounts(goods.into_iter())?;
    let freight_total = sum_amounts(freight.into_iter())?;
    let service_total = sum_amounts(service.into_iter())?;
    let refund_total = sum_amounts(refunds.into_iter())?;
    let gross = goods_total
        .to_decimal()
        .checked_add(freight_total.to_decimal())
        .and_then(|value| value.checked_add(service_total.to_decimal()))
        .and_then(|value| value.checked_sub(refund_total.to_decimal()))
        .ok_or_else(|| Error::ValidationError("结算金额溢出".into()))?;

    if Amount::try_from(gross)? != sum_amounts(lines.iter().map(|l| l.supplier_billed_gross))?
        || freight_total != sum_amounts(lines.iter().map(|l| l.freight_gross))?
        || service_total != sum_amounts(lines.iter().map(|l| l.service_fee_gross))?
        || refund_total != sum_amounts(lines.iter().map(|l| l.refund_gross))?
    {
        return Err(Error::ConflictError(
            "API 结算流水与正式来源的扣费、运费、服务费或退款金额不守恒".into(),
        ));
    }
    Ok(())
}
fn sum_amounts(mut values: impl Iterator<Item = Amount>) -> Result<Amount> {
    let value = values.try_fold(Amount::zero().to_decimal(), |sum, amount| {
        sum.checked_add(amount.to_decimal()).ok_or_else(|| Error::ValidationError("结算金额溢出".into()))
    })?;
    Ok(Amount::try_from(value)?)
}

#[cfg(test)]
mod tests {
    use entity_core::BaseModel;
    use erp_core::common::time::Instant;
    use erp_core::ids::{SupplierAccountId, SupplierFulfillmentItemId, SupplierFulfillmentOrderId};

    use super::*;
    use crate::after_sales::SettlementScan;
    use crate::entity::supplier_settlement::{
        SettlementAmountComponents, SettlementSourceFactType, SupplierSettlementSourceEvidenceLineData,
    };
    use crate::ports::connector::common::{Snapshot, SourceRevision, SourceStamp};
    use crate::ports::connector::order::OrderReference;
    use crate::ports::connector::settlement::SettlementEntry;

    fn row() -> SettlementEvidence {
        SettlementEvidence::new(
            &SettlementScan::new(
                "connection".into(),
                "supplier".into(),
                Instant::from_unix_secs(100),
                Instant::from_unix_secs(200),
            ),
            Snapshot {
                stamp: SourceStamp {
                    revision: SourceRevision::Unversioned,
                    changed_at: None,
                    observed_at: Instant::from_unix_secs(200),
                },
                value: SettlementEntry {
                    external_entry_id: "charge".into(),
                    order: OrderReference {
                        merchant_order_no: "order".into(),
                        external_order_no: "external".into(),
                    },
                    occurred_at: Instant::from_unix_secs(150),
                    movement: SettlementMovement::Charge {
                        goods: "10".parse().unwrap(),
                        shipping: "2".parse().unwrap(),
                        service: "1".parse().unwrap(),
                    },
                },
            },
        )
        .unwrap()
    }
    fn amount(value: &str) -> SettlementAmountComponents {
        SettlementAmountComponents::new(
            value.parse().unwrap(),
            value.parse().unwrap(),
            Amount::zero(),
            "test",
        )
        .unwrap()
    }
    fn source(row: &SettlementEvidence) -> SupplierSettlementSourceEvidence {
        let line =
            SupplierSettlementSourceEvidenceLine::from_components(SupplierSettlementSourceEvidenceLineData {
                supplier_fulfillment_order_id: SupplierFulfillmentOrderId::new("order"),
                supplier_fulfillment_item_id: SupplierFulfillmentItemId::new("item"),
                quantity: "1".parse().unwrap(),
                source_fact_types: vec![SettlementSourceFactType::FulfillmentCompleted],
                evidence_reference_ids: vec![format!("supplier-api-settlement:{}", row.base.id)],
                order: amount("10"),
                freight: amount("2"),
                service_fee: amount("1"),
                refund: amount("0"),
                supplier_billed: amount("13"),
            })
            .unwrap();
        SupplierSettlementSourceEvidence {
            base: BaseModel::fake(),
            request_id: "request".into(),
            supplier_id: SupplierAccountId::new("supplier"),
            period_start: "1970-01-01".parse().unwrap(),
            period_end: "1970-01-02".parse().unwrap(),
            period_policy_id: "policy".into(),
            period_policy_version: "1".into(),
            timezone: "Asia/Shanghai".into(),
            source_version: 1,
            external_bill_no: "bill".into(),
            external_bill_version: "1".into(),
            external_bill_evidence_reference_id: "bill-proof".into(),
            lines: vec![line],
            source_as_of: Instant::now(),
            recorded_by: "actor".into(),
            source_hash: "hash".into(),
            request_hash: "hash".into(),
        }
    }
    #[test]
    fn source_link_requires_account_order_period_amount_and_unconsumed_evidence() {
        let row = row();
        let source = source(&row);
        validate_settlement_source(&source, std::slice::from_ref(&row)).unwrap();

        for mode in 0..6 {
            let mut changed = row.clone();
            let mut source = source.clone();
            match mode {
                0 => changed.supplier_id = "other".into(),
                1 => changed.source_evidence_id = Some("other-source".into()),
                2 => changed.evidence.value.order.merchant_order_no = "other-order".into(),
                3 => changed.evidence.value.occurred_at = Instant::now(),
                4 => source.lines[0].supplier_billed_gross = "12".parse().unwrap(),
                _ => source.lines[0].freight_gross = "1".parse().unwrap(),
            }
            assert!(validate_settlement_source(&source, &[changed]).is_err());
        }

        let mut linked = row;
        linked.source_evidence_id = Some(source.base.id.clone());
        validate_settlement_source(&source, &[linked]).unwrap();
    }
}
