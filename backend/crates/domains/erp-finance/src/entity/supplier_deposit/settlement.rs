//! 真实使用的核销范围、返款分配和不可变执行回执。

use entity_core::BaseModel;
use entity_macros::Entity;
use erp_core::common::time::Instant;
use erp_core::money::Amount;
use erp_core::{Error, Result};
use serde::{Deserialize, Serialize};

use super::{
    DepositBatchAllocation, DepositTransactionEvidence, SupplierDepositAccount, SupplierDepositReservation,
    SupplierDepositUsage, add, positive, subtract,
};
use crate::entity::payable::{
    AllocationAction, EntryDirection, PayableAccount, PayableEntry, PayableEntryOffset, PayableSourceType,
    PaymentAllocation, PaymentAllocationLedger, PaymentAllocationSource, PendingPaymentAllocation,
    PurchasePaymentRequest,
};

/// 每次核销使用独立操作身份，支持同一扣款经补充审批继续核销。
#[derive(Debug, Clone, Serialize, Deserialize, Entity, PartialEq, Eq)]
pub struct SupplierDepositSettlement {
    #[serde(flatten)]
    pub base: BaseModel,
    pub account_id: String,
    pub operation_key: String,
    pub payload_fingerprint: String,
    pub usage_id: String,
    pub amount: Amount,
    pub allocation_ids: Vec<String>,
    pub actor_id: String,
    pub occurred_at: Instant,
}

/// 同一事务读取的核销归属依据，不从客户端推断主体或采购范围。
pub(crate) struct DepositSettlementScope<'a> {
    pub usage: &'a SupplierDepositUsage,
    pub reservation: &'a SupplierDepositReservation,
    pub account: &'a SupplierDepositAccount,
    pub request: &'a PurchasePaymentRequest,
    pub payable: &'a PayableAccount,
    pub entries: &'a [PayableEntry],
}

impl DepositSettlementScope<'_> {
    /// 验证资金与采购申请、正式应付的唯一归属。
    /// # 参数
    /// 使用同一事务读取的当前批准修订及其原采购增加分录。
    /// # 返回
    /// 当前批准来源严格匹配，原增加分录可保留其历史非空修订身份。
    /// # 错误
    /// 跨主体、采购、账户、批准修订或缺少分录来源修订时拒绝。
    pub(crate) fn validate(&self) -> Result<()> {
        self.request.require_deposit_account(&self.account.base.id)?;
        let basis = &self.reservation.basis;
        let identity = &self.account.identity;
        if self.usage.account_id != self.account.base.id
            || self.reservation.account_id != self.account.base.id
            || self.usage.reservation_id != self.reservation.base.id
            || self.request.base.id != basis.payment_request_id
            || self.request.submission_version != basis.payment_request_version
            || self.request.supplier_id != identity.supplier_id
            || self.request.payer_party_id != identity.payer_party_id
            || self.payable.base.id != basis.payable_account_id
            || self.payable.supplier_id.as_ref() != identity.supplier_id
            || self.payable.settlement_party_id.as_deref() != Some(identity.payer_party_id.as_str())
            || self.payable.source_type != PayableSourceType::PurchaseOrder
            || self.payable.source_document_id != basis.purchase_order_id
            || self.payable.purchase_revision_id.as_deref() != Some(basis.purchase_revision_id.as_str())
        {
            return Err(Error::from("预存款核销与原付款主体、供应商、采购或修订不一致"));
        }
        let line = self
            .request
            .lines
            .iter()
            .find(|line| line.payable_account_id == basis.payable_account_id)
            .ok_or_else(|| Error::from("原付款申请缺少执行采购"))?;
        if line.purchase_order_id != basis.purchase_order_id
            || line.purchase_revision_id != basis.purchase_revision_id
        {
            return Err(Error::from("原付款批准采购版本与资金占用不一致"));
        }
        for entry in self.entries {
            if entry.payable_account_id.as_ref() != basis.payable_account_id
                || entry.source_document_id != basis.purchase_order_id
                || entry.source_revision_id.trim().is_empty()
                || entry.direction != EntryDirection::Increase
            {
                return Err(Error::from("核销分录不属于原采购账户或缺少增加应付来源修订"));
            }
        }
        Ok(())
    }

    /// 按真实未返款金额构建来源隔离的核销账本。
    pub(crate) fn ledger(
        &self,
        existing: &[PaymentAllocation],
        pending: &[PendingPaymentAllocation],
    ) -> Result<PaymentAllocationLedger> {
        self.validate()?;
        if pending.is_empty() || pending.len() > 100 {
            return Err(Error::from("一次预存款核销须包含1至100条应付分配"));
        }
        for line in pending {
            positive(line.allocated_amount)?;
        }
        let available = subtract(self.usage.amount, self.usage.returned_amount)?;
        let source = PaymentAllocationSource::deposit_usage(&self.usage.base.id)?;
        PaymentAllocationLedger::for_source(source, available, existing, pending)
    }
}

impl SupplierDepositUsage {
    /// 计算实际返款中可冲回的已核销部分；其余部分保留原申请锁待核实。
    /// # 参数
    /// * `amount` - 已到账返款金额。
    /// # 返回
    /// 返回可冲回的正式核销金额。
    /// # 错误
    /// 非正、超原使用或持久化累计不守恒时拒绝。
    pub fn return_settlement_amount(&self, amount: Amount) -> Result<Amount> {
        positive(amount)?;
        let net_settled = subtract(self.settled_amount, self.reversed_settlement_amount)?;
        if net_settled < Amount::zero() {
            return Err(Error::from("原预存款核销冲回不守恒"));
        }
        let reversed = amount.min(net_settled);
        let mut candidate = self.clone();
        candidate.record_return(amount, reversed)?;
        Ok(reversed)
    }

    /// 将返款稳定归还原使用批次，不把无来源差异伪造为充值。
    /// # 参数
    /// * `amount` - 实际到账返款。
    /// # 返回
    /// 按原使用分配顺序返回可证明的批次金额；超原批次部分仍由账户保留差异。
    /// # 错误
    /// 非正、超返或金额溢出时拒绝。
    pub fn return_batches(&self, amount: Amount) -> Result<Vec<DepositBatchAllocation>> {
        self.return_settlement_amount(amount)?;
        let mut offset = self.returned_amount;
        let mut remaining = amount;
        let mut batches = Vec::new();
        for batch in &self.batches {
            if remaining == Amount::zero() {
                break;
            }
            if offset >= batch.amount {
                offset = subtract(offset, batch.amount)?;
                continue;
            }
            let returned = remaining.min(subtract(batch.amount, offset)?);
            offset = Amount::zero();
            remaining = subtract(remaining, returned)?;
            batches.push(DepositBatchAllocation { batch_id: batch.batch_id.clone(), amount: returned });
        }
        Ok(batches)
    }

    /// 返回当前来源的正式净核销，核对账本和使用投影是否一致。
    /// # 参数
    /// * `allocations` - 同一资金使用全部核销。
    /// # 返回
    /// 返回已核实一致的正式净额。
    /// # 错误
    /// 来源混入、净额溢出或投影不一致时拒绝。
    pub fn settlement_total(&self, allocations: &[PaymentAllocation]) -> Result<Amount> {
        let source = PaymentAllocationSource::deposit_usage(&self.base.id)?;
        let mut total = Amount::zero();
        for allocation in allocations {
            if allocation.source != source {
                return Err(Error::from("核销资金使用来源不一致"));
            }
            total = match allocation.allocation_action {
                AllocationAction::Apply => add(total, allocation.allocated_amount)?,
                AllocationAction::Reverse => subtract(total, allocation.allocated_amount)?,
            };
        }
        if total != subtract(self.settled_amount, self.reversed_settlement_amount)? {
            return Err(Error::from("预存款使用净核销与正式分配不一致"));
        }
        Ok(total)
    }
}

impl SupplierDepositSettlement {
    pub(crate) fn allocation_total(allocations: &[PaymentAllocation]) -> Result<Amount> {
        allocations.iter().try_fold(Amount::zero(), |sum, row| add(sum, row.allocated_amount))
    }

    pub(crate) fn batch_total(allocations: &[DepositBatchAllocation]) -> Result<Amount> {
        allocations.iter().try_fold(Amount::zero(), |sum, row| add(sum, row.amount))
    }
}

impl DepositTransactionEvidence {
    /// 核验真实资金证据身份完整，不依据订单状态推断到账。
    /// # 参数
    /// 无。
    /// # 返回
    /// 三项身份已规范化且有界时成功。
    /// # 错误
    /// 缺稳定交易、原单或到账依据时拒绝。
    pub fn validate(&self) -> Result<()> {
        for value in [&self.external_transaction_id, &self.external_order_id, &self.evidence_reference] {
            if value.trim().is_empty() || value.trim() != value || value.len() > 256 {
                return Err(Error::from("资金交易须有稳定交易、原单及到账核验依据"));
            }
        }
        Ok(())
    }
}

pub(crate) fn validate_deposit_actor(actor: &str) -> Result<()> {
    if actor.trim().is_empty() || actor.trim() != actor || actor.len() > 128 {
        return Err(Error::from("资金核验人身份无效"));
    }
    Ok(())
}

pub(crate) fn ensure_deposit_entry_balances(
    entries: &[PayableEntry],
    pending: &[PendingPaymentAllocation],
    existing: &[PaymentAllocation],
    offsets: &[PayableEntryOffset],
) -> Result<()> {
    for line in pending {
        let entry = entries
            .iter()
            .find(|entry| entry.base.id == line.payable_entry_id.as_ref())
            .ok_or_else(|| Error::from("原核销应付分录不存在"))?;
        let mut settled = Amount::zero();
        for allocation in existing.iter().filter(|row| row.payable_entry_id == line.payable_entry_id) {
            settled = match allocation.allocation_action {
                AllocationAction::Apply => add(settled, allocation.allocated_amount)?,
                AllocationAction::Reverse => subtract(settled, allocation.allocated_amount)?,
            };
        }
        let decreased = offsets
            .iter()
            .filter(|row| row.increase_entry_id == line.payable_entry_id)
            .try_fold(Amount::zero(), |sum, row| add(sum, row.offset_amount))?;
        let requested = pending.iter().filter(|row| row.payable_entry_id == line.payable_entry_id).try_fold(
            Amount::zero(),
            |sum, row| {
                positive(row.allocated_amount)?;
                add(sum, row.allocated_amount)
            },
        )?;
        if settled < Amount::zero() || add(add(settled, decreased)?, requested)? > entry.amount {
            return Err(Error::from("预存款核销超过原应付分录的正式开放余额"));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use erp_core::common::time::BusinessDate;
    use erp_core::ids::{
        PayableAccountId, PayableEntryId, PayableEntryOffsetId, PaymentAllocationId, SupplierAccountId,
        SupplierPaymentId,
    };
    use serde_json::json;

    use super::*;
    use crate::entity::payable::{
        PayableAccountData, PayableEntryData, PayableEntryOffsetData, PayableEntryType,
        PaymentAllocationData, PaymentRequestDepositAccountSnapshot, PaymentRequestExecutionMethod,
        PaymentRequestLineData, PaymentRequestSalesSource, PaymentRequestSourceSnapshot,
        PaymentRequestSubmission, PurchasePaymentRequestData, PurchasePaymentRequestLine,
    };
    use crate::entity::supplier_deposit::{
        DepositAccountIdentity, DepositCurrency, DepositExposure, DepositReservationBasis,
    };

    struct ScopeFacts {
        usage: SupplierDepositUsage,
        reservation: SupplierDepositReservation,
        account: SupplierDepositAccount,
        request: PurchasePaymentRequest,
        payable: PayableAccount,
        entries: Vec<PayableEntry>,
    }

    impl ScopeFacts {
        fn scope(&self) -> DepositSettlementScope<'_> {
            DepositSettlementScope {
                usage: &self.usage,
                reservation: &self.reservation,
                account: &self.account,
                request: &self.request,
                payable: &self.payable,
                entries: &self.entries,
            }
        }
    }

    fn amount(value: &str) -> Amount {
        value.parse().unwrap()
    }

    fn approved_request(account: &SupplierDepositAccount) -> PurchasePaymentRequest {
        let mut request = PurchasePaymentRequest::new(
            "request".into(),
            PurchasePaymentRequestData {
                execution_method: PaymentRequestExecutionMethod::SupplierDeposit {
                    account_id: account.base.id.clone(),
                },
                supplier_id: "supplier".into(),
                payee_bank_account_id: String::new(),
                lines: vec![PaymentRequestLineData {
                    payable_account_id: "payable".into(),
                    amount: amount("100"),
                }],
                desired_payment_date: BusinessDate::from_ymd(2026, 10, 29).unwrap(),
                reason: "按当前采购修订付款".into(),
                attachment_ids: vec![],
            },
            "buyer",
        )
        .unwrap();
        request
            .submit(PaymentRequestSubmission {
                supplier_id: "supplier".into(),
                payer_party_id: "payer".into(),
                payer_party_name: "我方公司".into(),
                org_unit_id: None,
                payee_account: None,
                deposit_account: Some(PaymentRequestDepositAccountSnapshot {
                    account_id: account.base.id.clone(),
                    identity: account.identity.clone(),
                }),
                lines: vec![PurchasePaymentRequestLine {
                    payable_account_id: "payable".into(),
                    purchase_order_id: "purchase".into(),
                    purchase_revision_id: "revision-current".into(),
                    requested_amount: amount("100"),
                    paid_amount: Amount::zero(),
                    closed_amount: Amount::zero(),
                    gross_total: amount("100"),
                    settled_total: Amount::zero(),
                    open_total: amount("100"),
                    other_reserved_total: Amount::zero(),
                    payment_terms: json!({"type":"monthly"}),
                    source_snapshot: PaymentRequestSourceSnapshot {
                        purchase_preview: json!({"revision_id":"revision-current"}),
                        allowed_attachment_ids: vec![],
                        sales_sources: vec![PaymentRequestSalesSource {
                            sales_order_id: "sales".into(),
                            sales_revision_id: "sales-revision".into(),
                            customer_id: "customer".into(),
                            payer_party_id: "payer".into(),
                            payer_party_name: "我方公司".into(),
                            preview: json!({"order_no":"XS-1"}),
                        }],
                    },
                }],
            })
            .unwrap();
        request.approve().unwrap();
        request
    }

    fn old_increase_entry() -> PayableEntry {
        PayableEntry::new(
            PayableEntryId::new("old-entry"),
            PayableEntryData {
                payable_account_id: PayableAccountId::new("payable"),
                entry_type: PayableEntryType::Original,
                direction: EntryDirection::Increase,
                amount: amount("100"),
                due_date: BusinessDate::from_ymd(2026, 9, 1).unwrap(),
                source_fact_type: "purchase_order".into(),
                source_document_id: "purchase".into(),
                source_revision_id: "revision-original".into(),
                source_sequence: 1,
                posted_at: Instant::from_unix_secs(1),
            },
        )
        .unwrap()
    }

    fn scope_facts() -> ScopeFacts {
        let account = SupplierDepositAccount::new(
            "deposit".into(),
            DepositAccountIdentity {
                payer_party_id: "payer".into(),
                supplier_id: "supplier".into(),
                external_account_ref: "supplier-account".into(),
                currency: DepositCurrency::CNY,
            },
        )
        .unwrap();
        let request = approved_request(&account);
        let mut payable = PayableAccount::new(
            PayableAccountId::new("payable"),
            PayableAccountData {
                source_document_id: "purchase".into(),
                supplier_id: SupplierAccountId::new("supplier"),
                source_type: PayableSourceType::PurchaseOrder,
                gross_total: amount("100"),
                settled_total: Amount::zero(),
                invoiceable_total: amount("100"),
                invoiced_total: Amount::zero(),
            },
            "buyer",
        )
        .unwrap();
        payable.settlement_party_id = Some("payer".into());
        payable.purchase_revision_id = Some("revision-current".into());
        let reservation = SupplierDepositReservation {
            base: BaseModel::new("reservation".into()),
            account_id: account.base.id.clone(),
            operation_key: "reserve".into(),
            payload_fingerprint: "payload".into(),
            basis: DepositReservationBasis {
                payment_request_id: request.base.id.clone(),
                payment_request_version: request.submission_version,
                payable_account_id: payable.base.id.clone(),
                purchase_order_id: "purchase".into(),
                purchase_revision_id: "revision-current".into(),
                execution_group_id: "group".into(),
                approved_goods_amount: amount("100"),
                approved_freight: Amount::zero(),
                authorization_id: "grant".into(),
                authorization_version: 1,
            },
            amount: amount("100"),
            consumed_amount: amount("100"),
            released_amount: Amount::zero(),
            batches: vec![],
            exposure: DepositExposure::PossiblySent,
            clearance_evidence_reference: None,
            actor_id: "finance".into(),
            release_receipts: Default::default(),
        };
        let usage = SupplierDepositUsage {
            base: BaseModel::new("usage".into()),
            account_id: account.base.id.clone(),
            reservation_id: reservation.base.id.clone(),
            ledger_entry_id: "debit".into(),
            amount: amount("100"),
            settled_amount: Amount::zero(),
            returned_amount: Amount::zero(),
            reversed_settlement_amount: Amount::zero(),
            batches: vec![],
            payment_allocation_ids: vec![],
        };
        ScopeFacts { usage, reservation, account, request, payable, entries: vec![old_increase_entry()] }
    }

    fn pending(value: &str) -> PendingPaymentAllocation {
        PendingPaymentAllocation::new(PayableEntryId::new("old-entry"), amount(value)).unwrap()
    }

    fn usage() -> SupplierDepositUsage {
        SupplierDepositUsage {
            base: BaseModel::fake(),
            account_id: "a".into(),
            reservation_id: "r".into(),
            ledger_entry_id: "l".into(),
            amount: "120".parse().unwrap(),
            settled_amount: "80".parse().unwrap(),
            returned_amount: "20".parse().unwrap(),
            reversed_settlement_amount: "20".parse().unwrap(),
            batches: vec![
                DepositBatchAllocation { batch_id: "b1".into(), amount: "50".parse().unwrap() },
                DepositBatchAllocation { batch_id: "b2".into(), amount: "50".parse().unwrap() },
            ],
            payment_allocation_ids: vec![],
        }
    }

    #[test]
    fn partial_return_preserves_batch_order_and_formal_reversal_limit() {
        let usage = usage();
        assert_eq!(usage.return_settlement_amount("90".parse().unwrap()).unwrap(), "60".parse().unwrap());
        assert_eq!(
            usage.return_batches("90".parse().unwrap()).unwrap(),
            vec![
                DepositBatchAllocation { batch_id: "b1".into(), amount: "30".parse().unwrap() },
                DepositBatchAllocation { batch_id: "b2".into(), amount: "50".parse().unwrap() },
            ]
        );
        assert!(usage.return_batches("101".parse().unwrap()).is_err());
        assert!(usage.return_batches(Amount::zero()).is_err());
        assert_eq!(usage.returned_amount, "20".parse().unwrap());
    }

    #[test]
    fn approved_current_revision_can_settle_open_original_increase_entry() {
        let facts = scope_facts();
        let scope = facts.scope();
        assert_ne!(facts.entries[0].source_revision_id, facts.reservation.basis.purchase_revision_id);
        scope.validate().unwrap();
        let pending = vec![pending("60")];
        ensure_deposit_entry_balances(&facts.entries, &pending, &[], &[]).unwrap();
        let mut ledger = scope.ledger(&[], &pending).unwrap();
        ledger
            .apply(
                &pending[0],
                &facts.entries[0],
                PaymentAllocationId::new("new-allocation"),
                Instant::from_unix_secs(2),
            )
            .unwrap();
        assert_eq!(ledger.new_allocations().len(), 1);
        assert_eq!(
            ledger.new_allocations()[0].source,
            PaymentAllocationSource::deposit_usage("usage").unwrap()
        );
        assert_eq!(ledger.new_allocations()[0].allocated_amount, amount("60"));
    }

    #[test]
    fn historical_entry_still_requires_original_purchase_account_and_source_revision() {
        for mutation in 0..5 {
            let mut facts = scope_facts();
            match mutation {
                0 => facts.entries[0].source_document_id = "another-purchase".into(),
                1 => facts.entries[0].payable_account_id = PayableAccountId::new("another-payable"),
                2 => facts.entries[0].source_revision_id.clear(),
                3 => facts.entries[0].source_revision_id = "   ".into(),
                _ => facts.entries[0].direction = EntryDirection::Decrease,
            }
            assert!(facts.scope().ledger(&[], &[pending("1")]).is_err(), "mutation {mutation}");
        }
    }

    #[test]
    fn approved_request_basis_and_current_payable_revision_cannot_be_downgraded_to_old_entry_revision() {
        for mutation in 0..4 {
            let mut facts = scope_facts();
            match mutation {
                0 => facts.request.lines[0].purchase_revision_id = "revision-original".into(),
                1 => facts.reservation.basis.purchase_revision_id = "revision-original".into(),
                2 => facts.payable.purchase_revision_id = Some("revision-original".into()),
                _ => facts.reservation.basis.payment_request_version += 1,
            }
            assert!(facts.scope().validate().is_err(), "mutation {mutation}");
        }
    }

    fn existing_allocation(
        id: &str,
        source: PaymentAllocationSource,
        action: AllocationAction,
        value: &str,
        reverses: Option<&str>,
    ) -> PaymentAllocation {
        PaymentAllocation::new(
            PaymentAllocationId::new(id),
            PaymentAllocationData {
                source,
                payable_entry_id: PayableEntryId::new("old-entry"),
                allocation_seq: if reverses.is_some() { 2 } else { 1 },
                allocation_action: action,
                allocated_amount: amount(value),
                allocated_at: Instant::from_unix_secs(1),
                reverses_allocation_id: reverses.map(PaymentAllocationId::new),
            },
        )
        .unwrap()
    }

    #[test]
    fn open_original_entry_balance_counts_all_payment_sources_reversals_and_offsets() {
        let facts = scope_facts();
        let bank: PaymentAllocationSource = SupplierPaymentId::new("bank-payment").into();
        let existing = vec![
            existing_allocation("bank", bank.clone(), AllocationAction::Apply, "30", None),
            existing_allocation(
                "other-deposit",
                PaymentAllocationSource::deposit_usage("another-usage").unwrap(),
                AllocationAction::Apply,
                "20",
                None,
            ),
            existing_allocation("bank-reversal", bank, AllocationAction::Reverse, "5", Some("bank")),
        ];
        let offsets = vec![
            PayableEntryOffset::new(
                PayableEntryOffsetId::new("offset"),
                PayableEntryOffsetData {
                    decrease_entry_id: PayableEntryId::new("change-decrease"),
                    increase_entry_id: PayableEntryId::new("old-entry"),
                    offset_sequence: 1,
                    offset_amount: amount("10"),
                },
            )
            .unwrap(),
        ];
        facts.scope().validate().unwrap();
        ensure_deposit_entry_balances(&facts.entries, &[pending("45")], &existing, &offsets).unwrap();
        assert!(
            ensure_deposit_entry_balances(&facts.entries, &[pending("45.01")], &existing, &offsets).is_err()
        );
        assert!(
            ensure_deposit_entry_balances(
                &facts.entries,
                &[pending("20"), pending("25.01")],
                &existing,
                &offsets
            )
            .is_err()
        );
    }
}
