//! 审核面按本申请采购范围展示的付款记录，不含其他来源金额或文件资格。

use std::collections::{BTreeMap, BTreeSet};

use erp_core::common::time::{BusinessDate, Instant};
use erp_core::money::Amount;
use erp_finance::entity::payable::{
    AllocationAction, PaymentAllocation, PaymentAllocationSource, PurchasePaymentRequest,
    PurchasePaymentRequestStatus,
};
use serde::Serialize;

use crate::{Error, Result};

/// 一张已授权采购单的当前审核依据。
#[derive(Debug, Clone, Serialize)]
pub struct PaymentRequestReviewLine {
    pub payable_account_id: String,
    pub due_dates: Vec<BusinessDate>,
    pub settlements: Vec<PaymentReviewSettlement>,
    pub other_requests: Vec<PaymentReviewReservation>,
}

/// 仅本采购分录上的净核销；不返回整张付款单金额或外部采购分配。
#[derive(Debug, Clone, Serialize)]
pub struct PaymentReviewSettlement {
    pub id: String,
    pub payment_id: Option<String>,
    pub document_no: Option<String>,
    pub method_label: String,
    pub occurred_at: Instant,
    pub amount: Amount,
    pub reversed_amount: Amount,
    pub effective_amount: Amount,
}

impl PaymentReviewSettlement {
    /// 按真实资金来源汇总指定采购分录的正向及冲正核销。
    ///
    /// # 参数
    /// * `allocations` / `entry_ids` - 核销事实与当前授权采购的分录集合。
    /// # 返回
    /// 返回按发生时间倒序的采购内净额，空集合返回空记录。
    /// # 错误
    /// 金额溢出或冲正超过本采购正向核销时拒绝。
    pub(super) fn from_allocations(
        allocations: &[PaymentAllocation],
        entry_ids: &BTreeSet<String>,
    ) -> Result<Vec<Self>> {
        let mut rows: BTreeMap<String, Self> = BTreeMap::new();
        for allocation in allocations.iter().filter(|row| entry_ids.contains(row.payable_entry_id.as_ref())) {
            let (key, payment_id, label) = match &allocation.source {
                PaymentAllocationSource::SupplierPayment(id) => {
                    (format!("bank:{id}"), Some(id.to_string()), "银行付款")
                },
                PaymentAllocationSource::SupplierDepositUsage(id) => {
                    (format!("deposit:{id}"), None, "预存款核销")
                },
            };
            let row = rows.entry(key.clone()).or_insert_with(|| Self {
                id: key,
                payment_id,
                document_no: None,
                method_label: label.into(),
                occurred_at: allocation.allocated_at,
                amount: Amount::zero(),
                reversed_amount: Amount::zero(),
                effective_amount: Amount::zero(),
            });
            row.occurred_at = row.occurred_at.min(allocation.allocated_at);
            let total = match allocation.allocation_action {
                AllocationAction::Apply => &mut row.amount,
                AllocationAction::Reverse => &mut row.reversed_amount,
            };
            *total = checked_sum(*total, allocation.allocated_amount)?;
        }
        let mut rows = rows.into_values().collect::<Vec<_>>();
        for row in &mut rows {
            if row.reversed_amount > row.amount {
                return Err(Error::ConflictError("采购历史冲正超过有效付款".into()));
            }
            row.effective_amount = row.amount.checked_sub(row.reversed_amount);
        }
        rows.sort_by(|left, right| right.occurred_at.cmp(&left.occurred_at).then(left.id.cmp(&right.id)));
        Ok(rows)
    }
}

/// 其他申请在当前采购上的占用；不得暴露申请整单总额、其他采购或收款账户。
#[derive(Debug, Clone, Serialize)]
pub struct PaymentReviewReservation {
    pub request_id: String,
    pub request_no: String,
    pub created_at: u64,
    pub status: PurchasePaymentRequestStatus,
    pub amount: Amount,
}

impl PaymentReviewReservation {
    /// 截取同主体、同供应商、同采购上的进行中占用并排除本申请。
    ///
    /// # 参数
    /// * `other` / `request` / `account_id` - 候选申请、已授权申请与应付身份。
    /// # 返回
    /// 仅返回符合范围且占用为正的局部记录。
    /// # 错误
    /// 不执行 I/O；不符合范围返回 None。
    pub(super) fn from_request(
        other: &PurchasePaymentRequest,
        request: &PurchasePaymentRequest,
        account_id: &str,
    ) -> Option<Self> {
        let source = request.lines.iter().find(|line| line.payable_account_id == account_id)?;
        if other.base.id == request.base.id
            || other.supplier_id != request.supplier_id
            || other.payer_party_id != request.payer_party_id
            || !matches!(
                other.status,
                PurchasePaymentRequestStatus::InApproval
                    | PurchasePaymentRequestStatus::Approved
                    | PurchasePaymentRequestStatus::PartiallyPaid
            )
        {
            return None;
        }
        let line = other.lines.iter().find(|line| {
            line.payable_account_id == account_id && line.purchase_order_id == source.purchase_order_id
        })?;
        let amount = line.remaining();
        (amount > Amount::zero()).then(|| Self {
            request_id: other.base.id.clone(),
            request_no: other.request_no.clone(),
            created_at: other.base.created_at,
            status: other.status,
            amount,
        })
    }
}

/// 累加展示金额时显式拒绝十进制溢出。
fn checked_sum(left: Amount, right: Amount) -> Result<Amount> {
    let sum = left
        .to_decimal()
        .checked_add(right.to_decimal())
        .ok_or_else(|| Error::ConflictError("采购历史金额汇总溢出".into()))?;
    Ok(Amount::try_from(sum)?)
}

#[cfg(test)]
mod tests {
    use entity_core::BaseModel;
    use erp_core::ids::{PayableEntryId, PaymentAllocationId, SupplierPaymentId};

    use super::*;

    fn allocation(entry: &str, action: AllocationAction, amount: &str) -> PaymentAllocation {
        PaymentAllocation {
            base: BaseModel::fake(),
            source: SupplierPaymentId::new("payment").into(),
            payable_entry_id: PayableEntryId::new(entry),
            allocation_seq: 1,
            allocation_action: action,
            allocated_amount: amount.parse().unwrap(),
            allocated_at: Instant::from_unix_secs(100),
            reverses_allocation_id: (action == AllocationAction::Reverse)
                .then(|| PaymentAllocationId::new("original")),
        }
    }

    #[test]
    fn history_scopes_shared_payments_and_nets_partial_reversals() {
        let rows = PaymentReviewSettlement::from_allocations(
            &[
                allocation("allowed", AllocationAction::Apply, "150"),
                allocation("allowed", AllocationAction::Reverse, "20"),
                allocation("other-purchase", AllocationAction::Apply, "900"),
            ],
            &BTreeSet::from(["allowed".into()]),
        )
        .unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].amount, "150".parse().unwrap());
        assert_eq!(rows[0].effective_amount, "130".parse().unwrap());
    }

    #[test]
    fn deposit_is_separate_from_cash_and_empty_or_invalid_history_is_explicit() {
        let mut deposit = allocation("allowed", AllocationAction::Apply, "25");
        deposit.source = PaymentAllocationSource::deposit_usage("payment").unwrap();
        let scope = BTreeSet::from(["allowed".into()]);
        let rows = PaymentReviewSettlement::from_allocations(
            &[allocation("allowed", AllocationAction::Apply, "150"), deposit],
            &scope,
        )
        .unwrap();
        assert_eq!(rows.len(), 2);
        assert!(rows.iter().any(|row| row.payment_id.is_none() && row.method_label == "预存款核销"));
        assert!(PaymentReviewSettlement::from_allocations(&[], &scope).unwrap().is_empty());
        assert!(
            PaymentReviewSettlement::from_allocations(
                &[allocation("allowed", AllocationAction::Reverse, "1"),],
                &scope
            )
            .is_err()
        );
    }

    fn request(id: &str) -> PurchasePaymentRequest {
        use erp_finance::entity::payable::{
            PaymentRequestLineData, PaymentRequestSourceSnapshot, PurchasePaymentRequestData,
            PurchasePaymentRequestLine,
        };
        let mut request = PurchasePaymentRequest::new(
            id.into(),
            PurchasePaymentRequestData {
                supplier_id: "supplier".into(),
                payee_bank_account_id: "bank".into(),
                execution_method: Default::default(),
                lines: vec![PaymentRequestLineData {
                    payable_account_id: "allowed".into(),
                    amount: "100".parse().unwrap(),
                }],
                desired_payment_date: BusinessDate::from_ymd(2026, 11, 4).unwrap(),
                reason: "采购付款".into(),
                attachment_ids: vec![],
            },
            "buyer",
        )
        .unwrap();
        request.payer_party_id = "payer".into();
        request.status = PurchasePaymentRequestStatus::PartiallyPaid;
        request.lines = vec![PurchasePaymentRequestLine {
            payable_account_id: "allowed".into(),
            purchase_order_id: "purchase".into(),
            purchase_revision_id: "revision".into(),
            requested_amount: "100".parse().unwrap(),
            paid_amount: "30".parse().unwrap(),
            closed_amount: "20".parse().unwrap(),
            gross_total: "1000".parse().unwrap(),
            settled_total: Amount::zero(),
            open_total: "1000".parse().unwrap(),
            other_reserved_total: Amount::zero(),
            payment_terms: serde_json::json!({}),
            source_snapshot: PaymentRequestSourceSnapshot {
                purchase_preview: serde_json::json!({}),
                sales_sources: vec![],
                allowed_attachment_ids: vec![],
            },
        }];
        request
    }

    #[test]
    fn reservations_exclude_self_and_unrelated_parties_and_show_only_scoped_unpaid() {
        let current = request("current");
        let mut other = request("other");
        let mut unrelated = other.lines[0].clone();
        unrelated.payable_account_id = "unrelated".into();
        unrelated.requested_amount = "900".parse().unwrap();
        other.lines.push(unrelated);
        let row = PaymentReviewReservation::from_request(&other, &current, "allowed").unwrap();
        assert_eq!(row.amount, "50".parse().unwrap());
        assert!(PaymentReviewReservation::from_request(&current, &current, "allowed").is_none());
        assert!(PaymentReviewReservation::from_request(&other, &current, "missing").is_none());
        other.payer_party_id = "different".into();
        assert!(PaymentReviewReservation::from_request(&other, &current, "allowed").is_none());
        other.payer_party_id = current.payer_party_id.clone();
        other.supplier_id = "different".into();
        assert!(PaymentReviewReservation::from_request(&other, &current, "allowed").is_none());
    }

    #[test]
    fn reservations_exclude_rejected_closed_paid_and_zero_remaining_rows() {
        let current = request("current");
        for status in [
            PurchasePaymentRequestStatus::Rejected,
            PurchasePaymentRequestStatus::Closed,
            PurchasePaymentRequestStatus::Paid,
            PurchasePaymentRequestStatus::Withdrawn,
            PurchasePaymentRequestStatus::Draft,
        ] {
            let mut other = request("other");
            other.status = status;
            assert!(PaymentReviewReservation::from_request(&other, &current, "allowed").is_none());
        }
        let mut other = request("other");
        other.lines[0].paid_amount = "80".parse().unwrap();
        assert!(PaymentReviewReservation::from_request(&other, &current, "allowed").is_none());
    }
}
