//! Fixed first-formalization posting order within the caller's transaction.

use super::formalize::{persist_procurement_work_items, FormalizedSubmissionWrite};
#[cfg(test)]
use crate::Error;
use crate::Result;
use async_trait::async_trait;
use erp_audit::{AuditExt, AuditLog};
use erp_core::ids::SalesOrderId;
use erp_finance::entity::receivable::SalesBusinessTypeFact;
use erp_finance::service::receivable::initial_account::{create_initial_receivable, InitialReceivableInput};
use erp_workflow::DocumentRegistryExt;
use persistence_core::Executor;

/// Each variant is one existing side-effect boundary, in the original order below.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum PostingStep {
    RevalidateProcurement,
    ProcurementTasks,
    RegisterDocument,
    SalesRevision,
    SynchronizeProcurement,
    SalesSubmission,
    Receivable,
    Audit,
}

#[async_trait]
trait PostingSteps: Send {
    /// Apply one existing step using exactly the caller's executor; stop on its original error.
    async fn apply(&mut self, step: PostingStep, executor: &mut dyn Executor) -> Result<()>;
}

/// Execute the production sequence; it is also exercised by failure-injecting pure test doubles.
async fn execute(steps: &mut impl PostingSteps, executor: &mut dyn Executor) -> Result<()> {
    use PostingStep::*;
    for step in [
        RevalidateProcurement,
        ProcurementTasks,
        RegisterDocument,
        SalesRevision,
        SynchronizeProcurement,
        SalesSubmission,
        Receivable,
        Audit,
    ] {
        steps.apply(step, executor).await?;
    }
    Ok(())
}

struct MongoPosting<'a> {
    write: FormalizedSubmissionWrite,
    audit: &'a AuditLog,
}

#[async_trait]
impl PostingSteps for MongoPosting<'_> {
    async fn apply(&mut self, step: PostingStep, executor: &mut dyn Executor) -> Result<()> {
        use PostingStep::*;
        let write = &mut self.write;
        match step {
            RevalidateProcurement => {
                if let Some(plan) = write.procurement.as_ref() {
                    crate::procure_to_pay::responsibility::ProcurementResponsibilityProcess::new(
                        write.db.clone(),
                        write.rbac.clone(),
                    )
                    .revalidate_plan(&plan.inputs, &plan.resolution, executor)
                    .await?;
                }
            }
            ProcurementTasks => {
                if write.procurement.is_some() {
                    persist_procurement_work_items(&write.db, &write.procurement_items, executor).await?;
                }
            }
            RegisterDocument => {
                if let Some(mut document) = write
                    .db
                    .business_documents()
                    .find_by_id(&write.order_id, executor)
                    .await?
                {
                    document.formalize(write.now);
                    write
                        .db
                        .business_documents()
                        .update(&mut document, executor)
                        .await?;
                }
            }
            SalesRevision => {
                crate::business_ownership::ensure_attribution(&write.db, &write.order, executor).await?;
                erp_sales::service::sales_order::formalize::persist_revision(
                    &write.db,
                    &mut write.order,
                    &write.aggregate,
                    executor,
                )
                .await?;
            }
            SynchronizeProcurement => {
                if write.procurement.is_some() {
                    crate::procure_to_pay::sync_procurement_tasks_for_sales_order(
                        &write.db,
                        &SalesOrderId::new(write.order_id.clone()),
                        executor,
                    )
                    .await?;
                }
            }
            SalesSubmission => {
                erp_sales::service::sales_order::formalize::persist_submission(
                    &write.db,
                    &mut write.submission,
                    executor,
                )
                .await?;
            }
            Receivable => {
                let order = &write.order;
                create_initial_receivable(
                    &write.db,
                    InitialReceivableInput {
                        business_type: match order.business_type {
                            erp_sales::entity::sales_order::BusinessType::GoodsService => {
                                SalesBusinessTypeFact::GoodsService
                            }
                            erp_sales::entity::sales_order::BusinessType::Voucher => {
                                SalesBusinessTypeFact::Voucher
                            }
                        },
                        sales_order_id: order.base.id.clone().into(),
                        customer_id: order.customer_id.clone(),
                        counterparty_party_id: order.settlement_party_id.clone(),
                        source_sales_order_revision_id: write.aggregate.revision.base.id.clone().into(),
                        gross_total: write.aggregate.revision.gross_amount,
                        posted_at: write.now,
                    },
                    executor,
                )
                .await?;
            }
            Audit => {
                write.db.audit_logs().create(self.audit, executor).await?;
            }
        }
        Ok(())
    }
}

/// 在调用方选定的事务边界内写入销售形式化及既有供给事实，并继续写入财务、任务和审计。
///
/// # 参数
/// * `write` - 已完成领域计算的销售形式化写入上下文
/// * `audit` - 本次形式化审计记录
/// * `executor` - 调用方根事务执行器；全部步骤复用，不开启其他事务
///
/// # 返回
/// 全部写入完成后返回；销售单、正式版本与冻结时间在同一写入上下文中传递。
///
/// # 错误
/// 采购责任重验、正式版本、提交或后续财务、任务、审计任一步骤失败时，保留原错误并停止执行。
pub(super) async fn post(
    write: FormalizedSubmissionWrite,
    audit: &AuditLog,
    executor: &mut dyn Executor,
) -> Result<()> {
    execute(&mut MongoPosting { write, audit }, executor).await
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

    struct RecordingSteps {
        calls: Vec<PostingStep>,
        executor: usize,
        fail_at: Option<PostingStep>,
    }
    #[async_trait]
    impl PostingSteps for RecordingSteps {
        async fn apply(&mut self, step: PostingStep, executor: &mut dyn Executor) -> Result<()> {
            assert_eq!(executor as *mut dyn Executor as *mut () as usize, self.executor);
            self.calls.push(step);
            if Some(step) == self.fail_at {
                return Err(Error::ConflictError("original posting conflict".into()));
            }
            Ok(())
        }
    }
    fn expected() -> Vec<PostingStep> {
        use PostingStep::*;
        vec![
            RevalidateProcurement,
            ProcurementTasks,
            RegisterDocument,
            SalesRevision,
            SynchronizeProcurement,
            SalesSubmission,
            Receivable,
            Audit,
        ]
    }

    #[tokio::test]
    async fn formalization_preserves_all_domain_steps_and_executor() {
        let mut executor = TestExecutor { _identity: 1 };
        let identity = &mut executor as *mut TestExecutor as usize;
        let mut steps = RecordingSteps {
            calls: vec![],
            executor: identity,
            fail_at: None,
        };
        execute(&mut steps, &mut executor).await.unwrap();
        assert_eq!(steps.calls, expected());
    }

    #[tokio::test]
    async fn every_failure_stops_later_domains_and_preserves_original_error() {
        for (index, step) in expected().into_iter().enumerate() {
            let mut executor = TestExecutor { _identity: 1 };
            let identity = &mut executor as *mut TestExecutor as usize;
            let mut steps = RecordingSteps {
                calls: vec![],
                executor: identity,
                fail_at: Some(step),
            };
            let error = execute(&mut steps, &mut executor).await.unwrap_err();
            assert!(matches!(error,Error::ConflictError(ref text) if text=="original posting conflict"));
            assert_eq!(steps.calls, expected()[..=index]);
        }
    }
}
