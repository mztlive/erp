//! 正式复核事务的生产步骤与同一执行器合同。
use std::sync::Arc;

use application_core::AuditActor;
use async_trait::async_trait;
use erp_audit::AuditActorLogs;
use erp_audit::execution::persist_log;
use erp_core::money::Amount;
use erp_finance::entity::cost::CostEntry;
use erp_finance::entity::payable::{PayableAccount, PayableEntry};
use erp_finance::service::cost::supplier_settlement::persist_settlement_costs;
use erp_finance::service::payable::supplier_settlement::persist_settlement_payable;
use erp_fulfillment_execution::supply_execution::receipt::persist_supply_receipt;
use erp_identity::AuthorizationService;
use erp_read_models::supplier_center::settlement::require_reviewer;
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
use erp_workflow::entity::work_item::WorkItem;
use erp_workflow::ports::WorkflowAuthorizationPort;
use erp_workflow::{WorkItemExt, WorkItemService};
use mongodb::Database;
use persistence_core::Executor;

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
// 按固定 `ORDER` 执行复核写入，任一步失败立即停止，防止依赖前序校验或事实的步骤继续运行。
async fn execute(steps: &mut impl PostingSteps, executor: &mut dyn Executor) -> Result<()> {
    for step in ORDER {
        steps.apply(step, executor).await?;
    }
    Ok(())
}
pub(super) struct Posting<'a, A: WorkflowAuthorizationPort> {
    pub db: &'a Database,
    pub authorization: &'a Arc<AuthorizationService>,
    pub actor: &'a AuditActor,
    pub actor_id: &'a str,
    pub work_items: &'a WorkItemService<A>,
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
struct MongoPosting<'a, A: WorkflowAuthorizationPort> {
    input: Posting<'a, A>,
    items: Vec<SupplierSettlementItem>,
    differences: Vec<SupplierSettlementDifference>,
    receipt: Option<ReviewDecisionReceipt>,
}
#[async_trait]
impl<A: WorkflowAuthorizationPort> PostingSteps for MongoPosting<'_, A> {
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
// 分别核验正式复核权限、任务访问权和复核人资格，供过账流程在单独的制单复核分离检查前完成授权。
async fn authorize_posting<A: WorkflowAuthorizationPort>(
    input: &Posting<'_, A>,
    ex: &mut dyn Executor,
) -> Result<()> {
    require_reviewer(input.db, input.authorization, input.actor, input.work_item, ex).await?;
    input.work_items.ensure_domain_decision_access(input.actor, input.work_item, ex).await?;
    super::reviewers::ensure_reviewer(
        input.authorization,
        &input.work_items.identity,
        input.actor_id,
        &input.statement.prepared_by,
        ex,
    )
    .await
}

// 应付与成本写入后保存决定回执，记录精确结算版本和财务结果供重试恢复；展示审计由后续步骤保存。
async fn persist_review_result<A: WorkflowAuthorizationPort>(
    input: &Posting<'_, A>,
    ex: &mut dyn Executor,
) -> Result<ReviewDecisionReceipt> {
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
    .await
    .map_err(Error::from)?;
    Ok(receipt)
}

// 为本次财务复核构造单次可读审计事件；原决定及金额由领域回执负责恢复，不依赖审计文案。
fn review_audit<A: WorkflowAuthorizationPort>(input: &Posting<'_, A>) -> Result<erp_audit::AuditLog> {
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
// 沿原事务依次执行授权、岗位分离、明细和差异复验、主题复验，再写结算单、任务、应付、成本、回执和审计。
// 任一步失败即停止；成功返回正式回执，未生成回执视为内部错误。
pub(super) async fn post<A: WorkflowAuthorizationPort>(
    input: Posting<'_, A>,
    ex: &mut dyn Executor,
) -> Result<ReviewDecisionReceipt> {
    let mut posting = MongoPosting { input, items: Vec::new(), differences: Vec::new(), receipt: None };
    execute(&mut posting, ex).await?;
    posting.receipt.ok_or_else(|| Error::Internal("结算复核审计收据未生成".to_string()))
}
#[cfg(test)]
mod tests {
    use persistence_core::ReadExecutor;

    use super::*;

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
        let mut ex = ReadExecutor::default();
        let mut steps =
            Recording { calls: vec![], executor: &mut ex as *mut ReadExecutor as usize, fail: None };
        execute(&mut steps, &mut ex).await.unwrap();
        assert_eq!(steps.calls, ORDER);
    }
    #[tokio::test]
    async fn review_propagates_every_failure_without_later_steps() {
        for (index, fail) in ORDER.into_iter().enumerate() {
            let mut ex = ReadExecutor::default();
            let mut steps = Recording {
                calls: vec![],
                executor: &mut ex as *mut ReadExecutor as usize,
                fail: Some(fail),
            };

            let error = execute(&mut steps, &mut ex).await.unwrap_err();
            assert!(matches!(error, Error::ConflictError(ref value) if value == "原复核事务冲突"));
            assert_eq!(steps.calls, ORDER[..=index]);
        }
    }
}
