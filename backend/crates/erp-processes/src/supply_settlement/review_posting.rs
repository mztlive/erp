//! 正式复核事务的生产步骤与同一执行器合同。
use application_core::AuditActor;
use async_trait::async_trait;
use erp_audit::AuditActorLogs;
use erp_core::money::Amount;
use erp_finance::entity::cost::CostEntry;
use erp_finance::entity::payable::{PayableAccount, PayableEntry};
use erp_finance::service::cost::supplier_settlement::persist_settlement_costs;
use erp_finance::service::payable::supplier_settlement::persist_settlement_payable;
use erp_identity::SharedRbacService;
use erp_supply::command_receipt::{ReviewDecisionReceipt, SupplyCommandResult};
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

use crate::audit::persist_log;
use crate::supply_execution::receipt::persist_supply_receipt;
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
    Receipt,
    Audit,
}
const ORDER: [Step; 11] = [
    Step::Authorize,
    Step::Separation,
    Step::Items,
    Step::Differences,
    Step::Subject,
    Step::Statement,
    Step::Task,
    Step::Payable,
    Step::Costs,
    Step::Receipt,
    Step::Audit,
];
#[async_trait]
trait PostingSteps: Send {
    async fn apply(&mut self, step: Step, executor: &mut dyn Executor) -> Result<()>;
}
/// 按 `ORDER` 逐步过账；任一步失败则不执行后续步骤。
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
    pub idempotency_key: String,
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
            Step::Authorize => authorize_posting(input, ex).await?,
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
            Step::Receipt => self.receipt = Some(persist_review_result(input, ex).await?),
            Step::Audit => {
                persist_log(input.db, &review_audit(input)?, ex).await?;
            },
        }
        Ok(())
    }
}
/// 复核权限和任务访问分别校验，保持制单人分离步骤仍在其后。
async fn authorize_posting(input: &Posting<'_>, ex: &mut dyn Executor) -> Result<()> {
    crate::adapters::workflow::work_item_service(input.db.clone(), input.rbac.clone())
        .ensure_domain_decision_access(input.actor, input.work_item, ex)
        .await?;
    let auth = crate::adapters::workflow::workflow_auth(input.db.clone(), input.rbac.clone());
    super::reviewers::ensure_reviewer(
        &auth,
        input.actor_id,
        &input.statement.prepared_by,
        &input.statement.business_org_unit_id,
        ex,
    )
    .await
}

/// 在应付、成本写入之后登记原复核结果，审计仍由后续步骤保存。
async fn persist_review_result(input: &Posting<'_>, ex: &mut dyn Executor) -> Result<ReviewDecisionReceipt> {
    let receipt = ReviewDecisionReceipt {
        operation_id: input.operation_id.clone(),
        result_status: input.result_status,
        statement_version: input.statement.base.version,
        task_version: input.work_item.base.version,
        payable_account_id: input.payable.map(|account| account.base.id.clone()),
        cost_delta: input.cost_delta,
    };
    persist_supply_receipt(
        input.db,
        &review_audit(input)?,
        &input.fingerprint,
        &input.idempotency_key,
        &input.statement.base.id,
        SupplyCommandResult::ReviewDecision(receipt.clone()),
        ex,
    )
    .await?;
    Ok(receipt)
}

/// 构造单次中文复核事件；原结果只保存在领域回执。
fn review_audit(input: &Posting<'_>) -> Result<erp_audit::AuditLog> {
    Ok(input
        .actor
        .clone()
        .resource_log_with_id(
            input.audit_id.clone(),
            &input.action,
            "supplier_settlement_statement",
            input.statement.base.id.clone(),
            Some("供应商结算财务复核决定已登记".to_string()),
        )?
        .with_command_id(Some(input.audit_id.clone()))?
        .with_resource_number(Some(input.statement.statement_no.clone()))?)
}
/// 在同一执行器上按授权、岗位分离、明细、差异、主题、结算单、任务、应付、成本、回执和审计的顺序过账。
///
/// # 参数
/// * `input` - 已准备的复核决定及财务构造结果。
/// * `ex` - 调用方事务执行器。
///
/// # 返回
/// 返回已持久化的复核决定回执。
///
/// # 错误
/// 任一步失败时返回对应错误且不执行后续步骤。回执未生成时返回 `Internal`。
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
