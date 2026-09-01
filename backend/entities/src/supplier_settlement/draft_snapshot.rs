//! W27 结算草稿快照：由不可变来源证据确定性派生结算明细与差异。
//!
//! 草稿创建与刷新共用同一领域工厂（FUL-E12），保证两种入口对同一来源批次生成
//! 相同的业务快照；明细/差异主键由 Service 注入，本层不触碰全局 ID 生成器，
//! 也不做任何 I/O。

use rust_decimal::Decimal;

use crate::errors::{Error, Result};
use crate::ids::{SupplierSettlementDifferenceId, SupplierSettlementItemId, SupplierSettlementStatementId};
use crate::money::Amount;
use crate::supplier_settlement::{
    SettlementDifferenceStatus, SettlementDifferenceType, SupplierSettlementDifference,
    SupplierSettlementDifferenceData, SupplierSettlementItem, SupplierSettlementItemData,
    SupplierSettlementSourceEvidence,
};

/// 由不可变来源证据批次派生的完整草稿快照。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SupplierSettlementDraftSnapshot {
    /// 逐来源行冻结的结算明细（按来源行顺序，来源行已按履约明细主键稳定排序）。
    pub items: Vec<SupplierSettlementItem>,
    /// 非零金额差异（供应商账单金额 − ERP 计算金额，带符号）。
    pub differences: Vec<SupplierSettlementDifference>,
    /// ERP 计算含税总额（来源行 `erp_gross` 精确累加）。
    pub erp_amount: Amount,
    /// 供应商账单含税总额（来源行 `supplier_billed_gross` 精确累加）。
    pub supplier_amount: Amount,
}

impl SupplierSettlementDraftSnapshot {
    /// 从不可变来源证据批次确定性派生结算草稿快照。
    ///
    /// 每条来源行生成一条结算明细；`供应商账单 − ERP 计算` 非零时追加一条
    /// 待处理金额差异（负数表示 ERP 金额大于供应商金额），零差异不生成；
    /// `erp_amount` 与 `supplier_amount` 分别按来源行含税金额精确累加。
    ///
    /// # 参数
    /// * `statement_id` - 结算单主键（由 Service 注入）
    /// * `source` - 已冻结的权威来源证据批次
    /// * `next_item_id` - 按来源行顺序提供结算明细主键的注入器
    /// * `next_difference_id` - 按差异生成顺序提供差异主键的注入器
    ///
    /// # 返回
    /// 返回明细、差异与两侧总额的草稿快照。
    ///
    /// # 错误
    /// 来源批次没有可结算行，或明细/差异构造校验失败时返回领域错误。
    pub fn from_source(
        statement_id: &SupplierSettlementStatementId,
        source: &SupplierSettlementSourceEvidence,
        mut next_item_id: impl FnMut() -> SupplierSettlementItemId,
        mut next_difference_id: impl FnMut() -> SupplierSettlementDifferenceId,
    ) -> Result<Self> {
        if source.lines.is_empty() {
            return Err(Error::from(
                "SOURCE_EVIDENCE_INCOMPLETE: 来源证据批次没有可结算行",
            ));
        }
        let mut items = Vec::with_capacity(source.lines.len());
        let mut differences = Vec::new();
        let mut erp_amount = zero();
        let mut supplier_amount = zero();
        for line in &source.lines {
            let item = SupplierSettlementItem::new(
                next_item_id(),
                SupplierSettlementItemData {
                    statement_id: statement_id.clone(),
                    supplier_fulfillment_order_id: line.supplier_fulfillment_order_id.clone(),
                    supplier_fulfillment_item_id: line.supplier_fulfillment_item_id.clone(),
                    quantity: line.quantity,
                    order_amount: line.order_gross,
                    freight_amount: line.freight_gross,
                    service_fee_amount: line.service_fee_gross,
                    refund_amount: line.refund_gross,
                    erp_calculated_amount: line.erp_gross,
                    erp_calculated_net_amount: line.erp_net,
                    erp_calculated_tax_amount: line.erp_tax,
                    supplier_billed_amount: line.supplier_billed_gross,
                    supplier_billed_net_amount: line.supplier_billed_net,
                    supplier_billed_tax_amount: line.supplier_billed_tax,
                },
            )?;
            erp_amount = erp_amount.checked_add(line.erp_gross);
            supplier_amount = supplier_amount.checked_add(line.supplier_billed_gross);
            let difference_amount = line.supplier_billed_gross.checked_sub(line.erp_gross);
            if difference_amount != zero() {
                differences.push(SupplierSettlementDifference::new(
                    next_difference_id(),
                    SupplierSettlementDifferenceData {
                        statement_item_id: SupplierSettlementItemId::new(item.base.id.clone()),
                        difference_type: SettlementDifferenceType::Amount,
                        difference_amount,
                        status: SettlementDifferenceStatus::Pending,
                        resolution: None,
                        resolved_by: None,
                        resolved_at: None,
                    },
                )?);
            }
            items.push(item);
        }
        Ok(Self {
            items,
            differences,
            erp_amount,
            supplier_amount,
        })
    }
}

/// 返回草稿快照累加使用的零金额。
///
/// # 返回
/// 返回精确到分的零金额。
fn zero() -> Amount {
    Amount::try_from(Decimal::ZERO).expect("零是合法金额")
}

#[cfg(test)]
mod tests {
    use std::str::FromStr;

    use super::*;
    use crate::common::time::{BusinessDate, Instant};
    use crate::ids::{SupplierAccountId, SupplierFulfillmentItemId, SupplierFulfillmentOrderId};
    use crate::money::Quantity;
    use crate::supplier_settlement::{
        SettlementSourceFactType, SupplierSettlementSourceEvidenceData, SupplierSettlementSourceEvidenceLine,
        SETTLEMENT_TIMEZONE,
    };

    fn amount(value: &str) -> Amount {
        Amount::from_str(value).unwrap()
    }

    fn source_line(
        item_id: &str,
        erp_gross: &str,
        supplier_gross: &str,
    ) -> SupplierSettlementSourceEvidenceLine {
        SupplierSettlementSourceEvidenceLine {
            supplier_fulfillment_order_id: SupplierFulfillmentOrderId::new(format!("order-{item_id}")),
            supplier_fulfillment_item_id: SupplierFulfillmentItemId::new(item_id),
            quantity: Quantity::from_str("1").unwrap(),
            source_fact_types: vec![SettlementSourceFactType::FulfillmentCompleted],
            evidence_reference_ids: vec![format!("fulfillment://{item_id}")],
            order_gross: amount(erp_gross),
            order_net: amount(erp_gross),
            order_tax: amount("0.00"),
            freight_gross: amount("0.00"),
            freight_net: amount("0.00"),
            freight_tax: amount("0.00"),
            service_fee_gross: amount("0.00"),
            service_fee_net: amount("0.00"),
            service_fee_tax: amount("0.00"),
            refund_gross: amount("0.00"),
            refund_net: amount("0.00"),
            refund_tax: amount("0.00"),
            erp_gross: amount(erp_gross),
            erp_net: amount(erp_gross),
            erp_tax: amount("0.00"),
            supplier_billed_gross: amount(supplier_gross),
            supplier_billed_net: amount(supplier_gross),
            supplier_billed_tax: amount("0.00"),
        }
    }

    fn source(lines: Vec<SupplierSettlementSourceEvidenceLine>) -> SupplierSettlementSourceEvidence {
        SupplierSettlementSourceEvidence::new(
            "source-1",
            SupplierSettlementSourceEvidenceData {
                request_id: "source-request-1".to_string(),
                supplier_id: SupplierAccountId::new("supplier-1"),
                period_start: BusinessDate::from_str("2026-07-01").unwrap(),
                period_end: BusinessDate::from_str("2026-07-31").unwrap(),
                period_policy_id: "monthly".to_string(),
                period_policy_version: "1".to_string(),
                timezone: SETTLEMENT_TIMEZONE.to_string(),
                source_version: 1,
                external_bill_no: "BILL-1".to_string(),
                external_bill_version: "1".to_string(),
                external_bill_evidence_reference_id: "bill://1".to_string(),
                lines,
                source_as_of: Instant::from_unix_secs(1_700_000_000),
                recorded_by: "finance-1".to_string(),
                source_hash: "a".repeat(64),
                request_hash: "b".repeat(64),
            },
        )
        .unwrap()
    }

    fn sequential_item_ids() -> impl FnMut() -> SupplierSettlementItemId {
        let mut sequence = 0;
        move || {
            sequence += 1;
            SupplierSettlementItemId::new(format!("settlement-item-{sequence}"))
        }
    }

    fn sequential_difference_ids() -> impl FnMut() -> SupplierSettlementDifferenceId {
        let mut sequence = 0;
        move || {
            sequence += 1;
            SupplierSettlementDifferenceId::new(format!("settlement-difference-{sequence}"))
        }
    }

    fn snapshot(lines: Vec<SupplierSettlementSourceEvidenceLine>) -> Result<SupplierSettlementDraftSnapshot> {
        SupplierSettlementDraftSnapshot::from_source(
            &SupplierSettlementStatementId::new("statement-1"),
            &source(lines),
            sequential_item_ids(),
            sequential_difference_ids(),
        )
    }

    #[test]
    fn from_source_builds_item_difference_and_totals() {
        let snapshot = snapshot(vec![source_line("item-1", "113.00", "114.00")]).unwrap();
        assert_eq!(snapshot.items.len(), 1);
        assert_eq!(snapshot.differences.len(), 1);
        assert_eq!(snapshot.items[0].base.id, "settlement-item-1");
        assert_eq!(
            snapshot.items[0].statement_id,
            SupplierSettlementStatementId::new("statement-1")
        );
        assert_eq!(snapshot.items[0].supplier_fulfillment_item_id.as_ref(), "item-1");
        assert_eq!(
            snapshot.items[0].quantity.to_decimal(),
            Quantity::from_str("1").unwrap().to_decimal()
        );
        assert_eq!(snapshot.items[0].erp_calculated_amount, amount("113.00"));
        assert_eq!(snapshot.items[0].supplier_billed_amount, amount("114.00"));
        assert_eq!(snapshot.differences[0].base.id, "settlement-difference-1");
        assert_eq!(
            snapshot.differences[0].statement_item_id.as_ref(),
            "settlement-item-1"
        );
        assert_eq!(snapshot.differences[0].difference_amount, amount("1.00"));
        assert_eq!(snapshot.erp_amount, amount("113.00"));
        assert_eq!(snapshot.supplier_amount, amount("114.00"));
    }

    #[test]
    fn from_source_sums_multiple_lines_with_signed_differences() {
        let snapshot = snapshot(vec![
            source_line("item-1", "113.00", "114.00"),
            source_line("item-2", "105.00", "100.00"),
        ])
        .unwrap();
        assert_eq!(snapshot.items.len(), 2);
        assert_eq!(snapshot.differences.len(), 2);
        assert_eq!(snapshot.differences[0].difference_amount, amount("1.00"));
        assert_eq!(snapshot.differences[1].difference_amount, amount("-5.00"));
        assert_eq!(snapshot.erp_amount, amount("218.00"));
        assert_eq!(snapshot.supplier_amount, amount("214.00"));
        assert_eq!(snapshot.items[0].base.id, "settlement-item-1");
        assert_eq!(snapshot.items[1].base.id, "settlement-item-2");
    }

    #[test]
    fn from_source_skips_zero_difference() {
        let snapshot = snapshot(vec![
            source_line("item-1", "100.00", "100.00"),
            source_line("item-2", "50.00", "50.00"),
        ])
        .unwrap();
        assert_eq!(snapshot.items.len(), 2);
        assert!(snapshot.differences.is_empty());
        assert_eq!(snapshot.erp_amount, amount("150.00"));
        assert_eq!(snapshot.supplier_amount, amount("150.00"));
    }

    #[test]
    fn from_source_rejects_empty_source() {
        // 绕过 `new` 的行数校验，直接构造空行来源，验证工厂自身的空来源不变量。
        let empty = SupplierSettlementSourceEvidence {
            base: entity_core::BaseModel::new("source-empty".to_string()),
            request_id: "source-request-1".to_string(),
            supplier_id: SupplierAccountId::new("supplier-1"),
            period_start: BusinessDate::from_str("2026-07-01").unwrap(),
            period_end: BusinessDate::from_str("2026-07-31").unwrap(),
            period_policy_id: "monthly".to_string(),
            period_policy_version: "1".to_string(),
            timezone: SETTLEMENT_TIMEZONE.to_string(),
            source_version: 1,
            external_bill_no: "BILL-1".to_string(),
            external_bill_version: "1".to_string(),
            external_bill_evidence_reference_id: "bill://1".to_string(),
            lines: Vec::new(),
            source_as_of: Instant::from_unix_secs(1_700_000_000),
            recorded_by: "finance-1".to_string(),
            source_hash: "a".repeat(64),
            request_hash: "b".repeat(64),
        };
        let error = SupplierSettlementDraftSnapshot::from_source(
            &SupplierSettlementStatementId::new("statement-1"),
            &empty,
            sequential_item_ids(),
            sequential_difference_ids(),
        )
        .unwrap_err();
        assert!(error.to_string().contains("SOURCE_EVIDENCE_INCOMPLETE"));
    }

    #[test]
    fn from_source_is_deterministic_given_same_injected_ids() {
        let lines = vec![
            source_line("item-1", "113.00", "114.00"),
            source_line("item-2", "105.00", "100.00"),
        ];
        let first = SupplierSettlementDraftSnapshot::from_source(
            &SupplierSettlementStatementId::new("statement-1"),
            &source(lines.clone()),
            sequential_item_ids(),
            sequential_difference_ids(),
        )
        .unwrap();
        let second = SupplierSettlementDraftSnapshot::from_source(
            &SupplierSettlementStatementId::new("statement-1"),
            &source(lines),
            sequential_item_ids(),
            sequential_difference_ids(),
        )
        .unwrap();
        assert_eq!(first, second);
    }

    #[test]
    fn from_source_keeps_business_snapshot_across_id_injections() {
        let lines = vec![source_line("item-1", "113.00", "114.00")];
        let first = SupplierSettlementDraftSnapshot::from_source(
            &SupplierSettlementStatementId::new("statement-1"),
            &source(lines.clone()),
            sequential_item_ids(),
            sequential_difference_ids(),
        )
        .unwrap();
        let second = SupplierSettlementDraftSnapshot::from_source(
            &SupplierSettlementStatementId::new("statement-1"),
            &source(lines),
            || SupplierSettlementItemId::new("other-item-1"),
            || SupplierSettlementDifferenceId::new("other-difference-1"),
        )
        .unwrap();
        assert_ne!(first, second, "注入主键不同时快照身份不同");
        assert_eq!(first.erp_amount, second.erp_amount);
        assert_eq!(first.supplier_amount, second.supplier_amount);
        assert_eq!(
            first.items[0].erp_calculated_amount,
            second.items[0].erp_calculated_amount
        );
        assert_eq!(
            first.items[0].supplier_billed_amount,
            second.items[0].supplier_billed_amount
        );
        assert_eq!(
            first.items[0].supplier_fulfillment_item_id,
            second.items[0].supplier_fulfillment_item_id
        );
        assert_eq!(
            first.differences[0].difference_amount,
            second.differences[0].difference_amount
        );
    }
}
