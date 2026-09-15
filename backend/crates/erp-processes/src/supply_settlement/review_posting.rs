//! 正式复核事务的生产步骤与同一执行器合同。
use application_core::AuditActor;
use async_trait::async_trait;
use erp_audit::{AuditActorLogs, AuditExt};
use erp_core::money::Amount;
use erp_finance::entity::cost::CostEntry;
use erp_finance::entity::payable::{PayableAccount, PayableEntry};
use erp_finance::service::cost::supplier_settlement::persist_settlement_costs;
use erp_finance::service::payable::supplier_settlement::persist_settlement_payable;
use erp_identity::SharedRbacService;
use erp_supply::dto::supplier_settlement::SettlementReviewDecisionStatus;
use erp_supply::entity::supplier_settlement::{
    SupplierSettlementDifference, SupplierSettlementItem, SupplierSettlementStatement,
};
use erp_supply::service::supplier_settlement::SupplierSettlementService;
use erp_supply::service::supplier_settlement::review::{
    ensure_current_subject_and_resolved_differences, ensure_reviewer_separation,
};
use erp_supply::service::supplier_settlement::shared::{load_statement_differences, load_statement_items};
use erp_workflow::WorkItemExt;
use erp_workflow::entity::work_item::WorkItem;
use mongodb::Database;
use persistence_core::Executor;

use super::review::{ReviewDecisionReceipt, review_decision_receipt_message};
use crate::{Error, Result};
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Step {
    Authorize,
    Separation,
    Items,
    Differences,
    Subject,
    Statement,
    Task,
    Payable,
    Costs,
    Audit,
}
const ORDER: [Step; 10] = [
    Step::Authorize,
    Step::Separation,
    Step::Items,
    Step::Differences,
    Step::Subject,
    Step::Statement,
    Step::Task,
    Step::Payable,
    Step::Costs,
    Step::Audit,
];
#[async_trait]
trait PostingSteps: Send {
    async fn apply(&mut self, step: Step, executor: &mut dyn Executor) -> Result<()>;
}
async fn execute(steps: &mut impl PostingSteps, executor: &mut dyn Executor) -> Result<()> {
    for step in ORDER {
        steps.apply(step, executor).await?;
    }
    Ok(())
}
pub(super) struct Posting<'a> {
    pub db: &'a Database,
    pub actor: &'a AuditActor,
    pub actor_id: &'a str,
    pub rbac: &'a SharedRbacService,
    pub statement: &'a mut SupplierSettlementStatement,
    pub work_item: &'a mut WorkItem,
    pub payable: Option<&'a PayableAccount>,
    pub payable_entry: Option<&'a PayableEntry>,
    pub cost_entries: &'a [CostEntry],
    pub cost_delta: Option<Amount>,
    pub result_status: SettlementReviewDecisionStatus,
    pub operation_id: String,
    pub fingerprint: String,
    pub audit_id: String,
    pub action: String,
}
struct MongoPosting<'a> {
    input: Posting<'a>,
    items: Vec<SupplierSettlementItem>,
    differences: Vec<SupplierSettlementDifference>,
    receipt: Option<ReviewDecisionReceipt>,
}
#[async_trait]
impl PostingSteps for MongoPosting<'_> {
    async fn apply(&mut self, step: Step, ex: &mut dyn Executor) -> Result<()> {
        let input = &mut self.input;
        match step {
            Step::Authorize => {
                crate::adapters::workflow::work_item_service(input.db.clone(), input.rbac.clone())
                    .ensure_domain_decision_access(input.actor, input.work_item, ex)
                    .await?
            },
            Step::Separation => ensure_reviewer_separation(input.statement, input.actor_id)?,
            Step::Items => self.items = load_statement_items(input.db, &input.statement.base.id, ex).await?,
            Step::Differences => {
                self.differences = load_statement_differences(input.db, &self.items, ex).await?
            },
            Step::Subject => {
                ensure_current_subject_and_resolved_differences(input.statement, &self.differences)?
            },
            Step::Statement => {
                SupplierSettlementService::new(input.db.clone())
                    .persist_statement(input.statement, ex)
                    .await?
            },
            Step::Task => input.db.work_items().update(input.work_item, ex).await?,
            Step::Payable => {
                if let (Some(account), Some(entry)) = (input.payable, input.payable_entry) {
                    persist_settlement_payable(input.db, account, entry, ex).await?;
                }
            },
            Step::Costs => persist_settlement_costs(input.db, input.cost_entries, ex).await?,
            Step::Audit => {
                let receipt = ReviewDecisionReceipt {
                    operation_id: input.operation_id.clone(),
                    result_status: input.result_status,
                    statement_version: input.statement.base.version,
                    task_version: input.work_item.base.version,
                    payable_account_id: input.payable.map(|account| account.base.id.clone()),
                    cost_delta: input.cost_delta,
                };
                let audit = input.actor.clone().resource_log_with_id(
                    input.audit_id.clone(),
                    &input.action,
                    "supplier_settlement_statement",
                    input.statement.base.id.clone(),
                    Some(review_decision_receipt_message(&input.fingerprint, &receipt)),
                )?;
                input.db.audit_logs().create(&audit, ex).await?;
                self.receipt = Some(receipt);
            },
        }
        Ok(())
    }
}
pub(super) async fn post(input: Posting<'_>, ex: &mut dyn Executor) -> Result<ReviewDecisionReceipt> {
    let mut posting = MongoPosting { input, items: Vec::new(), differences: Vec::new(), receipt: None };
    execute(&mut posting, ex).await?;
    posting.receipt.ok_or_else(|| Error::Internal("结算复核审计收据未生成".to_string()))
}
#[cfg(test)]
mod tests {
    use super::*;
    struct TestExecutor {
        _identity: u8,
    }
    impl Executor for TestExecutor {
        fn session(&mut self) -> Option<&mut mongodb::ClientSession> {
            None
        }
    }
    struct Recording {
        calls: Vec<Step>,
        executor: usize,
        fail: Option<Step>,
    }
    #[async_trait]
    impl PostingSteps for Recording {
        async fn apply(&mut self, step: Step, ex: &mut dyn Executor) -> Result<()> {
            assert_eq!(ex as *mut dyn Executor as *mut () as usize, self.executor);
            self.calls.push(step);
            if Some(step) == self.fail {
                return Err(Error::ConflictError("原复核事务冲突".to_string()));
            }
            Ok(())
        }
    }
    #[tokio::test]
    async fn review_reloads_facts_before_writes_with_one_executor() {
        let mut ex = TestExecutor { _identity: 1 };
        let mut steps =
            Recording { calls: vec![], executor: &mut ex as *mut TestExecutor as usize, fail: None };
        execute(&mut steps, &mut ex).await.unwrap();
        assert_eq!(steps.calls, ORDER);
    }
    #[tokio::test]
    async fn review_propagates_every_failure_without_later_steps() {
        for (index, fail) in ORDER.into_iter().enumerate() {
            let mut ex = TestExecutor { _identity: 1 };
            let mut steps = Recording {
                calls: vec![],
                executor: &mut ex as *mut TestExecutor as usize,
                fail: Some(fail),
            };
            let error = execute(&mut steps, &mut ex).await.unwrap_err();
            assert!(matches!(error, Error::ConflictError(ref value) if value == "原复核事务冲突"));
            assert_eq!(steps.calls, ORDER[..=index]);
        }
    }
}
