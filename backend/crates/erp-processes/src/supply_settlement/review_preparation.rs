//! 财务构造与结算决定的原准备顺序，保持非零成本失败在状态与任务变更之前。
use erp_core::{common::time::Instant, money::Amount};
use erp_finance::{
    entity::{
        cost::CostEntry,
        payable::{PayableAccount, PayableEntry},
    },
    service::{
        cost::supplier_settlement::{build_settlement_cost_delta, SettlementCostDeltaFact},
        payable::supplier_settlement::{build_settlement_payable, SettlementPayableSource},
    },
};
use erp_supply::{
    dto::supplier_settlement as dto,
    entity::supplier_settlement::{
        SettlementReviewDecision, SettlementReviewRejectReason, SettlementStatus,
        SupplierSettlementDifference, SupplierSettlementItem, SupplierSettlementStatement,
    },
};
use erp_workflow::entity::work_item::WorkItem;
use services::{Error, Result};
pub(super) struct ReviewInput<'a> {
    pub request: &'a dto::SettlementReviewCommand,
    pub reject_reason: Option<SettlementReviewRejectReason>,
    pub actor_id: &'a str,
    pub at: Instant,
}
pub(super) struct PreparedReview {
    pub payable: Option<PayableAccount>,
    pub payable_entry: Option<PayableEntry>,
    pub cost_entries: Vec<CostEntry>,
    pub cost_delta: Option<Amount>,
    pub result_status: dto::SettlementReviewDecisionStatus,
}
trait FinancePreparation {
    fn payable(
        &mut self,
        source: &SettlementPayableSource,
        amount: Amount,
        actor_id: &str,
        at: Instant,
    ) -> Result<(PayableAccount, PayableEntry)>;
    fn cost(&mut self, delta: &SettlementCostDeltaFact) -> Result<Vec<CostEntry>>;
}
struct Finance;
impl FinancePreparation for Finance {
    fn payable(
        &mut self,
        source: &SettlementPayableSource,
        amount: Amount,
        actor_id: &str,
        at: Instant,
    ) -> Result<(PayableAccount, PayableEntry)> {
        Ok(build_settlement_payable(source, amount, actor_id, at)?)
    }
    fn cost(&mut self, delta: &SettlementCostDeltaFact) -> Result<Vec<CostEntry>> {
        Ok(build_settlement_cost_delta(delta)?)
    }
}
fn execute(
    finance: &mut impl FinancePreparation,
    statement: &mut SupplierSettlementStatement,
    work_item: &mut WorkItem,
    items: &[SupplierSettlementItem],
    differences: &[SupplierSettlementDifference],
    input: ReviewInput<'_>,
) -> Result<PreparedReview> {
    let (payable, payable_entry, cost_entries, cost_delta, result_status) =
        match input.request.decision.action {
            dto::SettlementReviewAction::Confirm => {
                let cost_delta = statement.ensure_confirmable(items, differences)?;
                let payable_amount = statement.erp_amount.checked_add(cost_delta.gross);
                let source = SettlementPayableSource {
                    statement_no: statement.statement_no.clone(),
                    supplier_id: statement.supplier_id.clone(),
                    subject_hash: statement.subject_hash.clone(),
                    period_end: statement.period_end,
                };
                let (account, entry) = finance.payable(&source, payable_amount, input.actor_id, input.at)?;
                let cost_entries = finance.cost(&SettlementCostDeltaFact {
                    gross: cost_delta.gross,
                    net: cost_delta.net,
                    tax: cost_delta.tax,
                })?;
                statement.record_review(
                    SettlementReviewDecision::Confirm {
                        payable_account_id: account.base.id.clone().into(),
                        comment: input.request.decision.comment.clone(),
                    },
                    input.actor_id,
                    input.at,
                )?;
                (
                    Some(account),
                    Some(entry),
                    cost_entries,
                    Some(cost_delta.gross),
                    dto::SettlementReviewDecisionStatus::Confirmed,
                )
            }
            dto::SettlementReviewAction::Reject => {
                let return_status = if differences.is_empty() {
                    SettlementStatus::Draft
                } else {
                    SettlementStatus::HasDifference
                };
                let Some(reason) = input.reject_reason else {
                    return Err(Error::ValidationError("驳回必须携带原因代码".to_string()));
                };
                statement.record_review(
                    SettlementReviewDecision::Reject {
                        return_status,
                        reason_code: reason,
                        comment: input.request.decision.comment.clone(),
                    },
                    input.actor_id,
                    input.at,
                )?;
                (
                    None,
                    None,
                    Vec::new(),
                    None,
                    dto::SettlementReviewDecisionStatus::Rejected,
                )
            }
        };
    work_item.record_activity(input.actor_id, input.at)?;
    work_item.complete_by_domain_command(input.actor_id, input.at)?;
    Ok(PreparedReview {
        payable,
        payable_entry,
        cost_entries,
        cost_delta,
        result_status,
    })
}
pub(super) fn prepare(
    statement: &mut SupplierSettlementStatement,
    work_item: &mut WorkItem,
    items: &[SupplierSettlementItem],
    differences: &[SupplierSettlementDifference],
    input: ReviewInput<'_>,
) -> Result<PreparedReview> {
    execute(&mut Finance, statement, work_item, items, differences, input)
}

#[cfg(test)]
mod tests {
    use super::super::tests::{sample_statement, sample_work_item};
    use super::*;
    use erp_core::{
        ids::{
            SupplierFulfillmentItemId, SupplierFulfillmentOrderId, SupplierSettlementDifferenceId,
            SupplierSettlementItemId, SupplierSettlementStatementId,
        },
        money::Quantity,
    };
    use erp_supply::entity::supplier_settlement::{
        SettlementDifferenceConclusion, SettlementDifferenceConclusionKind, SettlementDifferenceStatus,
        SettlementDifferenceType, SupplierSettlementDifferenceData, SupplierSettlementItemData,
    };
    use erp_workflow::entity::work_item::WorkItemStatus;
    use std::str::FromStr;
    fn fixture(
        nonzero: bool,
    ) -> (
        SupplierSettlementStatement,
        WorkItem,
        Vec<SupplierSettlementItem>,
        Vec<SupplierSettlementDifference>,
    ) {
        let mut statement = sample_statement();
        let item = SupplierSettlementItem::new(
            SupplierSettlementItemId::new("item-1"),
            SupplierSettlementItemData {
                statement_id: SupplierSettlementStatementId::new("statement-1"),
                supplier_fulfillment_order_id: SupplierFulfillmentOrderId::new("order-1"),
                supplier_fulfillment_item_id: SupplierFulfillmentItemId::new("fulfillment-item-1"),
                quantity: Quantity::from_str("1").unwrap(),
                order_amount: Amount::from_str("100.00").unwrap(),
                freight_amount: Amount::from_str("0.00").unwrap(),
                service_fee_amount: Amount::from_str("0.00").unwrap(),
                refund_amount: Amount::from_str("0.00").unwrap(),
                erp_calculated_amount: Amount::from_str("100.00").unwrap(),
                erp_calculated_net_amount: Amount::from_str("87.00").unwrap(),
                erp_calculated_tax_amount: Amount::from_str("13.00").unwrap(),
                supplier_billed_amount: Amount::from_str("101.00").unwrap(),
                supplier_billed_net_amount: Amount::from_str("87.87").unwrap(),
                supplier_billed_tax_amount: Amount::from_str("13.13").unwrap(),
            },
        )
        .unwrap();
        let mut difference = SupplierSettlementDifference::new(
            SupplierSettlementDifferenceId::new("difference-1"),
            SupplierSettlementDifferenceData {
                statement_item_id: SupplierSettlementItemId::new("item-1"),
                difference_type: SettlementDifferenceType::Amount,
                difference_amount: Amount::from_str("1.00").unwrap(),
                status: SettlementDifferenceStatus::Pending,
                resolution: None,
                resolved_by: None,
                resolved_at: None,
            },
        )
        .unwrap();
        let conclusion = SettlementDifferenceConclusion::new(
            SettlementDifferenceConclusionKind::ErpAccepted,
            "ACCEPT_BILL",
            vec!["proof-1".to_string()],
        )
        .unwrap();
        difference
            .record_conclusion(&conclusion, "finance-1", Instant::from_unix_secs(1_700_000_100))
            .unwrap();
        let differences = if nonzero { vec![difference] } else { Vec::new() };
        statement
            .update_subject_hash(statement.review_subject_hash(&differences))
            .unwrap();
        statement.status = SettlementStatus::PendingReview;
        let work_item = sample_work_item(&statement);
        (statement, work_item, vec![item], differences)
    }

    fn request(statement: &SupplierSettlementStatement, reject: bool) -> dto::SettlementReviewCommand {
        serde_json::from_value(serde_json::json!({
            "work_item_id": "work-item-1", "expected_task_version": "1", "expected_subject_version": statement.subject_hash,
            "decision": { "statement_id": statement.base.id, "expected_lock_version": 1, "action": if reject { "REJECT" } else { "CONFIRM" }, "operation_id": "op-1", "reason_code": if reject { Some("AMOUNT_MISMATCH") } else { None }, "comment": null },
            "idempotency_key": "key-1"
        })).unwrap()
    }
    #[derive(Default)]
    struct RecordingFinance {
        calls: Vec<&'static str>,
        account_id: Option<String>,
    }
    impl FinancePreparation for RecordingFinance {
        fn payable(
            &mut self,
            source: &SettlementPayableSource,
            amount: Amount,
            actor_id: &str,
            at: Instant,
        ) -> Result<(PayableAccount, PayableEntry)> {
            self.calls.push("payable");
            let (account, entry) = build_settlement_payable(source, amount, actor_id, at)?;
            assert_eq!(entry.payable_account_id.as_ref(), account.base.id.as_str());
            assert_eq!(entry.posted_at, at);
            self.account_id = Some(account.base.id.clone());
            Ok((account, entry))
        }
        fn cost(&mut self, delta: &SettlementCostDeltaFact) -> Result<Vec<CostEntry>> {
            self.calls.push("cost");
            Ok(build_settlement_cost_delta(delta)?)
        }
    }
    #[test]
    fn nonzero_cost_fails_after_payable_construction_before_review_or_task_mutation() {
        let (mut statement, mut task, items, differences) = fixture(true);
        let statement_before = serde_json::to_value(&statement).unwrap();
        let task_before = serde_json::to_value(&task).unwrap();
        let req = request(&statement, false);
        let mut finance = RecordingFinance::default();
        let result = execute(
            &mut finance,
            &mut statement,
            &mut task,
            &items,
            &differences,
            ReviewInput {
                request: &req,
                reject_reason: None,
                actor_id: "reviewer-1",
                at: Instant::from_unix_secs(1700000100),
            },
        );
        assert!(
            matches!(result, Err(Error::BusinessLogicError(ref value)) if value == "ERP_ACCEPTED 成本差额暂缺权威原成本、税率与消费分配链，禁止伪造 CostEntry")
        );
        assert_eq!(finance.calls, ["payable", "cost"]);
        assert!(finance.account_id.is_some());
        assert_eq!(serde_json::to_value(&statement).unwrap(), statement_before);
        assert_eq!(serde_json::to_value(&task).unwrap(), task_before);
    }
    #[test]
    fn zero_cost_confirmation_uses_same_time_and_real_finance_result() {
        let (mut statement, mut task, items, differences) = fixture(false);
        let req = request(&statement, false);
        let mut finance = RecordingFinance::default();
        let at = Instant::from_unix_secs(1700000100);
        let result = execute(
            &mut finance,
            &mut statement,
            &mut task,
            &items,
            &differences,
            ReviewInput {
                request: &req,
                reject_reason: None,
                actor_id: "reviewer-1",
                at,
            },
        )
        .unwrap();
        assert_eq!(finance.calls, ["payable", "cost"]);
        assert!(result.cost_entries.is_empty());
        assert_eq!(statement.status, SettlementStatus::Confirmed);
        assert_eq!(statement.confirmed_at, Some(at));
        assert_eq!(
            statement.payable_account_id.as_ref().map(ToString::to_string),
            finance.account_id
        );
        assert_eq!(task.status, WorkItemStatus::Completed);
    }
    #[test]
    fn rejection_skips_finance_and_chooses_draft_or_difference_state() {
        for has_differences in [false, true] {
            let (mut statement, mut task, items, differences) = fixture(has_differences);
            let req = request(&statement, true);
            let mut finance = RecordingFinance::default();
            let result = execute(
                &mut finance,
                &mut statement,
                &mut task,
                &items,
                &differences,
                ReviewInput {
                    request: &req,
                    reject_reason: Some(SettlementReviewRejectReason::AmountMismatch),
                    actor_id: "reviewer-1",
                    at: Instant::from_unix_secs(1700000100),
                },
            )
            .unwrap();
            assert!(finance.calls.is_empty());
            assert!(result.payable.is_none());
            assert_eq!(
                statement.status,
                if has_differences {
                    SettlementStatus::HasDifference
                } else {
                    SettlementStatus::Draft
                }
            );
            assert_eq!(task.status, WorkItemStatus::Completed);
        }
    }
}
