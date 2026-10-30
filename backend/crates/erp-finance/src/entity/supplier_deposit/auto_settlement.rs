//! 已核实资金使用按真实开放应付分录核销，不由订单支付状态补造付款。

use std::collections::HashSet;

use erp_core::ids::PayableEntryId;
use erp_core::money::Amount;
use erp_core::{Error, Result};

use super::{
    SupplierDepositReservation, SupplierDepositSettlement, SupplierDepositUsage, add, positive, subtract,
};
use crate::entity::payable::{
    AllocationAction, EntryDirection, PayableEntry, PayableEntryOffset, PaymentAllocation,
    PaymentAllocationSource, PendingPaymentAllocation, PurchasePaymentRequest, PurchasePaymentRequestStatus,
};

/// 从同一财务事务内的真实分录、全部来源核销和抵销形成的完整计划。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DepositAutomaticSettlementPlan {
    pub amount: Amount,
    pub allocations: Vec<PendingPaymentAllocation>,
}

impl DepositAutomaticSettlementPlan {
    /// 按稳定入账时间和身份分配实际未核销扣款，不产生超批准的部分计划。
    /// # 参数
    /// * `usage` / `reservation` / `request` - 原真实使用、专属占用和逐单批准。
    /// * `entries` - 本采购应付的完整增加分录。
    /// * `allocations` / `offsets` - 各分录全部来源正反向核销及正式抵销。
    /// # 返回
    /// 返回覆盖本次真实未核销金额的完整分配。
    /// # 错误
    /// 归属、审批、锁定额度、净核销或分录开放金额不足及溢出时拒绝。
    pub fn from_facts(
        usage: &SupplierDepositUsage,
        reservation: &SupplierDepositReservation,
        request: &PurchasePaymentRequest,
        entries: &[PayableEntry],
        allocations: &[PaymentAllocation],
        offsets: &[PayableEntryOffset],
    ) -> Result<Self> {
        let amount = Self::unsettled_amount(usage, allocations)?;
        Self::validate_approval(usage, reservation, request, amount)?;
        Self::validate_records(reservation, entries, allocations, offsets)?;
        let mut ordered = entries.iter().collect::<Vec<_>>();
        ordered.sort_by(|left, right| {
            left.posted_at.cmp(&right.posted_at).then_with(|| left.base.id.cmp(&right.base.id))
        });
        let mut remaining = amount;
        let mut pending = Vec::new();
        for entry in ordered {
            if remaining == Amount::zero() {
                break;
            }
            let available = Self::entry_open(entry, allocations, offsets)?;
            let allocated = remaining.min(available);
            if allocated == Amount::zero() {
                continue;
            }
            if pending.len() >= 100 {
                return Err(Error::from("自动核销分录超过一百条，请核对原资金使用"));
            }
            pending.push(PendingPaymentAllocation::new(PayableEntryId::new(&entry.base.id), allocated)?);
            remaining = subtract(remaining, allocated)?;
        }
        if remaining != Amount::zero() {
            return Err(Error::from("原批准采购的正式开放分录不足，真实扣款保留待核销"));
        }
        Ok(Self { amount, allocations: pending })
    }

    fn unsettled_amount(usage: &SupplierDepositUsage, allocations: &[PaymentAllocation]) -> Result<Amount> {
        let source = PaymentAllocationSource::deposit_usage(&usage.base.id)?;
        let own = allocations.iter().filter(|row| row.source == source).cloned().collect::<Vec<_>>();
        let settled = usage.settlement_total(&own)?;
        let net_usage = subtract(usage.amount, usage.returned_amount)?;
        let covered = SupplierDepositSettlement::batch_total(&usage.batches)?;
        if net_usage < Amount::zero() || settled < Amount::zero() || covered != usage.amount {
            return Err(Error::from("实际扣款缺少完整原资金来源或使用金额不守恒"));
        }
        let amount = subtract(net_usage, settled)?;
        positive(amount)?;
        Ok(amount)
    }

    fn validate_approval(
        usage: &SupplierDepositUsage,
        reservation: &SupplierDepositReservation,
        request: &PurchasePaymentRequest,
        amount: Amount,
    ) -> Result<()> {
        let basis = &reservation.basis;
        request.require_deposit_account(&reservation.account_id)?;
        if usage.reservation_id != reservation.base.id
            || usage.account_id != reservation.account_id
            || request.base.id != basis.payment_request_id
            || request.submission_version != basis.payment_request_version
            || !matches!(
                request.status,
                PurchasePaymentRequestStatus::Approved | PurchasePaymentRequestStatus::PartiallyPaid
            )
        {
            return Err(Error::from("自动核销不属于原批准付款申请及执行组"));
        }
        let line = request
            .lines
            .iter()
            .find(|line| line.payable_account_id == basis.payable_account_id)
            .ok_or_else(|| Error::from("原付款申请缺少本采购批准金额"))?;
        let locked = request
            .deposit_reservations
            .get(&reservation.base.id)
            .ok_or_else(|| Error::from("原付款申请缺少本执行组资金占用"))?;
        let remaining = subtract(subtract(line.requested_amount, line.paid_amount)?, line.closed_amount)?;
        if line.purchase_order_id != basis.purchase_order_id
            || line.purchase_revision_id != basis.purchase_revision_id
            || locked.payable_account_id != basis.payable_account_id
            || locked.amount > remaining
            || amount > locked.amount
            || amount > remaining
        {
            return Err(Error::from("自动核销超过原逐采购批准和专属锁定额度"));
        }
        Ok(())
    }

    fn validate_records(
        reservation: &SupplierDepositReservation,
        entries: &[PayableEntry],
        allocations: &[PaymentAllocation],
        offsets: &[PayableEntryOffset],
    ) -> Result<()> {
        let basis = &reservation.basis;
        let mut ids = HashSet::new();
        for entry in entries {
            positive(entry.amount)?;
            if entry.payable_account_id.as_ref() != basis.payable_account_id
                || entry.source_document_id != basis.purchase_order_id
                || entry.source_revision_id.trim().is_empty()
                || entry.direction != EntryDirection::Increase
                || !ids.insert(entry.base.id.as_str())
            {
                return Err(Error::from("自动核销候选必须为本采购的唯一正式增加分录"));
            }
        }
        let mut allocation_ids = HashSet::new();
        for allocation in allocations {
            positive(allocation.allocated_amount)?;
            if !ids.contains(allocation.payable_entry_id.as_ref())
                || !allocation_ids.insert(&allocation.base.id)
            {
                return Err(Error::from("自动核销分配重复或不属于候选应付分录"));
            }
        }
        let mut offset_ids = HashSet::new();
        for offset in offsets {
            positive(offset.offset_amount)?;
            if !ids.contains(offset.increase_entry_id.as_ref()) || !offset_ids.insert(&offset.base.id) {
                return Err(Error::from("自动核销抵销重复或不属于候选应付分录"));
            }
        }
        Ok(())
    }

    fn entry_open(
        entry: &PayableEntry,
        allocations: &[PaymentAllocation],
        offsets: &[PayableEntryOffset],
    ) -> Result<Amount> {
        let mut settled = Amount::zero();
        for row in allocations.iter().filter(|row| row.payable_entry_id.as_ref() == entry.base.id) {
            settled = match row.allocation_action {
                AllocationAction::Apply => add(settled, row.allocated_amount)?,
                AllocationAction::Reverse => subtract(settled, row.allocated_amount)?,
            };
        }
        let decreased = offsets
            .iter()
            .filter(|row| row.increase_entry_id.as_ref() == entry.base.id)
            .try_fold(Amount::zero(), |sum, row| add(sum, row.offset_amount))?;
        let open = subtract(subtract(entry.amount, settled)?, decreased)?;
        if settled < Amount::zero() || open < Amount::zero() {
            return Err(Error::from("正式应付分录的核销或抵销金额不守恒"));
        }
        Ok(open)
    }
}

impl SupplierDepositSettlement {
    /// 验证自动核销原命令的已提交回执，不按当前余额重新分配。
    /// # 参数
    /// * `usage` - 原真实资金使用当前记录。
    /// * `operation_key` - 原自动核销命令身份。
    /// # 返回
    /// 身份和原正式核销金额仍可证明时成功。
    /// # 错误
    /// 跨使用、跨账户、异命令或回执损坏时拒绝。
    pub fn require_automatic_usage(&self, usage: &SupplierDepositUsage, operation_key: &str) -> Result<()> {
        if self.usage_id != usage.base.id
            || self.account_id != usage.account_id
            || self.operation_key != operation_key
            || self.amount <= Amount::zero()
            || self.amount > usage.settled_amount
            || self.allocation_ids.is_empty()
        {
            return Err(Error::from("自动核销命令与原资金使用回执不一致"));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use std::str::FromStr;

    use entity_core::BaseModel;
    use erp_core::common::time::{BusinessDate, Instant};
    use erp_core::ids::{PayableAccountId, PaymentAllocationId, SupplierPaymentId};
    use serde_json::json;

    use super::*;
    use crate::entity::payable::{
        PayableEntryType, PaymentAllocationData, PaymentRequestDepositAccountSnapshot,
        PaymentRequestDepositReservation, PaymentRequestExecutionMethod, PaymentRequestLineData,
        PaymentRequestSourceSnapshot, PurchasePaymentRequestData, PurchasePaymentRequestLine,
    };
    use crate::entity::supplier_deposit::{
        DepositAccountIdentity, DepositAuthorizationMode, DepositAutomaticReservationData,
        DepositBatchAllocation, DepositCurrency, DepositExposure, DepositReservationBasis,
        ManualDepositAuthorizationData, SupplierDepositAuthorization,
    };

    #[derive(Clone)]
    struct Facts {
        usage: SupplierDepositUsage,
        reservation: SupplierDepositReservation,
        request: PurchasePaymentRequest,
        entries: Vec<PayableEntry>,
        allocations: Vec<PaymentAllocation>,
        offsets: Vec<PayableEntryOffset>,
    }

    impl Facts {
        fn plan(&self) -> Result<DepositAutomaticSettlementPlan> {
            DepositAutomaticSettlementPlan::from_facts(
                &self.usage,
                &self.reservation,
                &self.request,
                &self.entries,
                &self.allocations,
                &self.offsets,
            )
        }
    }

    fn amount(value: &str) -> Amount {
        Amount::from_str(value).unwrap()
    }
    fn base(id: &str) -> BaseModel {
        let mut value = BaseModel::fake();
        value.id = id.into();
        value
    }

    fn fixture() -> Facts {
        let usage = SupplierDepositUsage {
            base: base("usage"),
            account_id: "deposit".into(),
            reservation_id: "reservation".into(),
            ledger_entry_id: "debit".into(),
            amount: amount("100"),
            settled_amount: Amount::zero(),
            returned_amount: Amount::zero(),
            reversed_settlement_amount: Amount::zero(),
            batches: vec![DepositBatchAllocation { batch_id: "batch".into(), amount: amount("100") }],
            payment_allocation_ids: vec![],
        };
        let reservation = SupplierDepositReservation {
            base: base("reservation"),
            account_id: "deposit".into(),
            operation_key: "reserve-operation".into(),
            payload_fingerprint: "payload".into(),
            basis: DepositReservationBasis {
                payment_request_id: "request".into(),
                payment_request_version: 2,
                payable_account_id: "payable".into(),
                purchase_order_id: "purchase".into(),
                purchase_revision_id: "current-revision".into(),
                execution_group_id: "execution".into(),
                approved_goods_amount: amount("180"),
                approved_freight: amount("20"),
                authorization_id: "authorization".into(),
                authorization_version: 1,
            },
            amount: amount("200"),
            consumed_amount: amount("100"),
            released_amount: Amount::zero(),
            batches: vec![DepositBatchAllocation { batch_id: "batch".into(), amount: amount("200") }],
            exposure: DepositExposure::PossiblySent,
            clearance_evidence_reference: None,
            actor_id: "system".into(),
            release_receipts: Default::default(),
        };
        let request = request();
        Facts {
            usage,
            reservation,
            request,
            entries: vec![entry("newer", "current-revision", 20), entry("older", "old-revision", 10)],
            allocations: vec![],
            offsets: vec![],
        }
    }

    fn request() -> PurchasePaymentRequest {
        let mut request = PurchasePaymentRequest::new(
            "request".into(),
            PurchasePaymentRequestData {
                supplier_id: "supplier".into(),
                execution_method: PaymentRequestExecutionMethod::SupplierDeposit {
                    account_id: "deposit".into(),
                },
                payee_bank_account_id: String::new(),
                lines: vec![PaymentRequestLineData {
                    payable_account_id: "payable".into(),
                    amount: amount("200"),
                }],
                desired_payment_date: BusinessDate::from_ymd(2026, 10, 29).unwrap(),
                reason: "合同商品及已批准运费".into(),
                attachment_ids: vec![],
            },
            "buyer",
        )
        .unwrap();
        request.status = PurchasePaymentRequestStatus::Approved;
        request.submission_version = 2;
        request.approval_subject_version = 2;
        request.payer_party_id = "payer".into();
        request.payer_party_name = "我方公司".into();
        request.deposit_account = Some(PaymentRequestDepositAccountSnapshot {
            account_id: "deposit".into(),
            identity: DepositAccountIdentity {
                payer_party_id: "payer".into(),
                supplier_id: "supplier".into(),
                external_account_ref: "verified-external-account".into(),
                currency: DepositCurrency::CNY,
            },
        });
        request.lines = vec![PurchasePaymentRequestLine {
            payable_account_id: "payable".into(),
            purchase_order_id: "purchase".into(),
            purchase_revision_id: "current-revision".into(),
            requested_amount: amount("200"),
            paid_amount: Amount::zero(),
            closed_amount: Amount::zero(),
            gross_total: amount("300"),
            settled_total: Amount::zero(),
            open_total: amount("300"),
            other_reserved_total: Amount::zero(),
            payment_terms: json!({"type":"monthly"}),
            source_snapshot: PaymentRequestSourceSnapshot {
                purchase_preview: json!({"purchase_no":"PO20261029-000001"}),
                sales_sources: vec![],
                allowed_attachment_ids: vec![],
            },
        }];
        request.deposit_reservations.insert(
            "reservation".into(),
            PaymentRequestDepositReservation { payable_account_id: "payable".into(), amount: amount("200") },
        );
        request
    }

    fn entry(id: &str, revision: &str, posted: i64) -> PayableEntry {
        PayableEntry {
            base: base(id),
            payable_account_id: PayableAccountId::new("payable"),
            entry_type: PayableEntryType::Original,
            direction: EntryDirection::Increase,
            amount: amount("100"),
            due_date: BusinessDate::from_ymd(2026, 10, 29).unwrap(),
            source_fact_type: "purchase_order".into(),
            source_document_id: "purchase".into(),
            source_revision_id: revision.into(),
            source_sequence: 1,
            posted_at: Instant::from_unix_secs(posted),
        }
    }

    fn allocation(
        id: &str,
        entry: &str,
        source: PaymentAllocationSource,
        seq: u32,
        action: AllocationAction,
        value: &str,
        original: Option<&str>,
    ) -> PaymentAllocation {
        PaymentAllocation::new(
            PaymentAllocationId::new(id),
            PaymentAllocationData {
                source,
                payable_entry_id: PayableEntryId::new(entry),
                allocation_seq: seq,
                allocation_action: action,
                allocated_amount: amount(value),
                allocated_at: Instant::from_unix_secs(30),
                reverses_allocation_id: original.map(PaymentAllocationId::new),
            },
        )
        .unwrap()
    }

    fn bank() -> PaymentAllocationSource {
        SupplierPaymentId::new("bank-payment").into()
    }
    fn own() -> PaymentAllocationSource {
        PaymentAllocationSource::deposit_usage("usage").unwrap()
    }

    #[test]
    fn automatic_plan_deducts_all_funding_sources_and_offsets_in_stable_order() {
        let mut facts = fixture();
        facts.allocations = vec![
            allocation("bank-apply", "older", bank(), 1, AllocationAction::Apply, "70", None),
            allocation(
                "bank-reverse",
                "older",
                bank(),
                2,
                AllocationAction::Reverse,
                "20",
                Some("bank-apply"),
            ),
            allocation(
                "other-use",
                "newer",
                PaymentAllocationSource::deposit_usage("other-usage").unwrap(),
                1,
                AllocationAction::Apply,
                "20",
                None,
            ),
        ];
        facts.offsets = vec![PayableEntryOffset {
            base: base("offset"),
            decrease_entry_id: PayableEntryId::new("decrease"),
            increase_entry_id: PayableEntryId::new("older"),
            offset_sequence: 1,
            offset_amount: amount("10"),
        }];
        let plan = facts.plan().unwrap();
        assert_eq!(plan.amount, amount("100"));
        assert_eq!(
            plan.allocations,
            [
                PendingPaymentAllocation::new(PayableEntryId::new("older"), amount("40")).unwrap(),
                PendingPaymentAllocation::new(PayableEntryId::new("newer"), amount("60")).unwrap()
            ]
        );
        facts.entries.reverse();
        assert_eq!(facts.plan().unwrap(), plan);
    }

    #[test]
    fn partial_use_only_plans_real_unreturned_and_unsettled_amount() {
        let mut facts = fixture();
        facts.usage.settled_amount = amount("30");
        facts.usage.returned_amount = amount("20");
        facts.usage.reversed_settlement_amount = amount("10");
        facts.request.lines[0].paid_amount = amount("20");
        facts.request.status = PurchasePaymentRequestStatus::PartiallyPaid;
        facts.request.deposit_reservations.get_mut("reservation").unwrap().amount = amount("170");
        facts.allocations = vec![
            allocation("own-apply", "older", own(), 1, AllocationAction::Apply, "30", None),
            allocation("own-reverse", "older", own(), 2, AllocationAction::Reverse, "10", Some("own-apply")),
        ];
        let plan = facts.plan().unwrap();
        assert_eq!(plan.amount, amount("60"));
        assert_eq!(
            plan.allocations,
            [PendingPaymentAllocation::new(PayableEntryId::new("older"), amount("60")).unwrap()]
        );
        assert_eq!(facts.usage.amount, amount("100"));
        assert_eq!(facts.usage.settled_amount, amount("30"));
    }

    #[test]
    fn insufficient_open_entries_never_return_a_partial_settlement_plan() {
        let mut facts = fixture();
        facts.allocations = vec![
            allocation("bank-apply", "older", bank(), 1, AllocationAction::Apply, "100", None),
            allocation("other-apply", "newer", bank(), 3, AllocationAction::Apply, "1", None),
        ];
        assert!(facts.plan().unwrap_err().to_string().contains("正式开放分录不足"));
        assert_eq!(facts.usage.settled_amount, Amount::zero());
        assert_eq!(facts.request.deposit_reservations["reservation"].amount, amount("200"));
    }

    #[test]
    fn current_approval_revision_keeps_historical_same_purchase_entries_eligible() {
        let mut facts = fixture();
        facts.entries[0].amount = amount("1");
        let plan = facts.plan().unwrap();
        assert_eq!(plan.allocations[0].payable_entry_id.as_ref(), "older");
        assert_eq!(plan.allocations[0].allocated_amount, amount("100"));
        facts.request.submission_version += 1;
        assert!(facts.plan().is_err());
    }

    #[test]
    fn cross_purchase_scope_and_insufficient_locked_approval_are_rejected() {
        let mut facts = fixture();
        facts.entries[0].source_document_id = "other-purchase".into();
        assert!(facts.plan().is_err());
        let mut facts = fixture();
        facts.request.deposit_reservations.get_mut("reservation").unwrap().amount = amount("99.99");
        assert!(facts.plan().is_err());
    }

    #[test]
    fn projection_mismatch_uncovered_excess_and_duplicate_facts_fail_closed() {
        let mut facts = fixture();
        facts.usage.settled_amount = amount("1");
        assert!(facts.plan().is_err());
        let mut facts = fixture();
        facts.usage.batches[0].amount = amount("99");
        assert!(facts.plan().is_err());
        let mut facts = fixture();
        facts.entries.push(facts.entries[0].clone());
        assert!(facts.plan().is_err());
        let mut facts = fixture();
        let row = allocation("bank-apply", "older", bank(), 1, AllocationAction::Apply, "1", None);
        facts.allocations = vec![row.clone(), row];
        assert!(facts.plan().is_err());
    }

    #[test]
    fn automatic_receipt_recovery_requires_original_usage_account_and_operation() {
        let facts = fixture();
        let mut usage = facts.usage;
        usage.settled_amount = amount("100");
        let receipt = SupplierDepositSettlement {
            base: base("receipt"),
            account_id: "deposit".into(),
            operation_key: "verified-usage:operation".into(),
            payload_fingerprint: "payload".into(),
            usage_id: "usage".into(),
            amount: amount("100"),
            allocation_ids: vec!["allocation".into()],
            actor_id: "system".into(),
            occurred_at: Instant::from_unix_secs(30),
        };
        receipt.require_automatic_usage(&usage, "verified-usage:operation").unwrap();
        assert!(receipt.require_automatic_usage(&usage, "different-operation").is_err());
        usage.account_id = "other-account".into();
        assert!(receipt.require_automatic_usage(&usage, "verified-usage:operation").is_err());
    }
    fn automatic_data() -> DepositAutomaticReservationData {
        DepositAutomaticReservationData {
            account_id: "deposit".into(),
            purchase_order_id: "purchase".into(),
            purchase_revision_id: "current-revision".into(),
            execution_group_id: "execution".into(),
            approved_goods_amount: amount("180"),
            approved_freight: amount("20"),
        }
    }

    #[test]
    fn automatic_reservation_requires_unique_full_approved_request() {
        let data = automatic_data();
        let mut request = request();
        request.deposit_reservations.clear();
        assert_eq!(data.request(std::slice::from_ref(&request)).unwrap().base.id, "request");
        let mut other = request.clone();
        other.base.id = "other-request".into();
        assert!(data.request(&[request.clone(), other]).is_err());
        request.status = PurchasePaymentRequestStatus::PartiallyPaid;
        request.lines[0].paid_amount = amount("1");
        assert!(data.request(&[request]).is_err());
        assert!(data.request(&[]).is_err());
    }

    #[test]
    fn automatic_reservation_cannot_treat_manual_grant_as_account_auto_permission() {
        let data = automatic_data();
        let manual = SupplierDepositAuthorization::manual(
            "manual".into(),
            ManualDepositAuthorizationData {
                account_id: "deposit".into(),
                purchase_order_id: "purchase".into(),
                work_item_id: "cashier-task".into(),
                task_version: 1,
                actor_id: "cashier".into(),
                approval_reference: "原付款任务本次执行".into(),
                amount: amount("200"),
                now: Instant::from_unix_secs(10),
            },
        )
        .unwrap();
        assert!(data.authorization(std::slice::from_ref(&manual), Instant::from_unix_secs(11)).is_err());
        let mut automatic = manual.clone();
        automatic.base.id = "automatic".into();
        automatic.mode = DepositAuthorizationMode::Automatic;
        assert_eq!(
            data.authorization(&[manual.clone(), automatic.clone()], Instant::from_unix_secs(11))
                .unwrap()
                .base
                .id,
            "automatic"
        );
        assert_eq!(automatic.committed_amount, Amount::zero());
        let mut request = request();
        request.deposit_reservations.clear();
        assert!(data.basis(&request, &manual).is_err());
        let basis = data.basis(&request, &automatic).unwrap();
        assert_eq!(basis.execution_group_id, "execution");
        assert_eq!(basis.total().unwrap(), amount("200"));
        automatic.revoked = true;
        assert!(data.authorization(&[automatic], Instant::from_unix_secs(11)).is_err());
    }
}
