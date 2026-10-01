//! Cross-domain invoice posting steps sharing the root transaction executor.

use application_core::{AuditActor, CommandReceipt};
use async_trait::async_trait;
use erp_audit::{AuditActorLogs, AuditExt, CommandReceiptServiceExt};
use erp_core::ids::{ReceivableAccountId, SalesOrderId, WorkItemId};
use erp_finance::entity::receivable::sales_invoice_allocation_plan::SalesInvoiceAllocationLine;
use erp_finance::entity::receivable::{Invoice, ReceivableAccount};
use erp_finance::service::receivable::invoice_posting::persist_sales_invoice_allocations;
use mongodb::Database;
use persistence_core::Executor;

use super::invoice_task::{self, SalesInvoiceTaskChange};
use crate::Result;

/// 扇出分片大小：同事务内按片顺序推进，首错短路。
pub(crate) const FANOUT_SHARD: usize = 50;

/// 扇出分片大小的对外别名，供红冲路径复用同一分片口径。
pub(crate) const FANOUT_CHUNK: usize = FANOUT_SHARD;

/// 去重并稳定排序，保持原失败短路顺序。
///
/// # 参数
/// * `ids` - 待分组的身份集合
///
/// # 返回
/// 返回排序去重后的身份集合。
fn dedup_sorted(mut ids: Vec<String>) -> Vec<String> {
    ids.sort();
    ids.dedup();
    ids
}

/// Immutable posting intent validated by the root invoice command.
pub(super) struct InvoicePostingInput<'a> {
    pub work_item_id: &'a WorkItemId,
    pub expected_task_version: u64,
    pub plan_lines: &'a [SalesInvoiceAllocationLine],
    pub actor: &'a AuditActor,
    pub action: &'static str,
    pub command_receipt: Option<&'a CommandReceipt>,
}

/// Post a validated invoice through finance, workflow, sales and audit in the existing transaction.
///
/// The root owns authorization, duplicate checks and commit recovery. This operation
/// preserves the prior step order and returns the first error without further writes.
pub(super) async fn post_invoice_apply(
    db: &Database,
    invoice: &mut Invoice,
    input: InvoicePostingInput<'_>,
    executor: &mut dyn Executor,
) -> Result<()> {
    let mut steps = MongoInvoicePosting { db, invoice, input, accounts: Vec::new(), account_ids: Vec::new() };
    execute_posting(&mut steps, executor).await
}

/// Invoice posting capabilities; each operation receives the unchanged root executor.
#[async_trait]
trait InvoicePostingSteps: Send {
    fn has_command_receipt(&self) -> bool;
    async fn record_execution(&mut self, executor: &mut dyn Executor) -> Result<()>;
    async fn persist_finance(&mut self, executor: &mut dyn Executor) -> Result<()>;
    async fn write_posting_audit(&mut self, executor: &mut dyn Executor) -> Result<()>;
    async fn synchronize_tasks(&mut self, executor: &mut dyn Executor) -> Result<()>;
    async fn update_sales_progress(&mut self, executor: &mut dyn Executor) -> Result<()>;
    async fn write_command_receipt(&mut self, executor: &mut dyn Executor) -> Result<()>;
}

/// Preserve execution activity, finance facts, audit, tasks, sales and root receipt order.
async fn execute_posting(steps: &mut impl InvoicePostingSteps, executor: &mut dyn Executor) -> Result<()> {
    steps.record_execution(executor).await?;
    steps.persist_finance(executor).await?;
    steps.write_posting_audit(executor).await?;
    steps.synchronize_tasks(executor).await?;
    steps.update_sales_progress(executor).await?;
    if steps.has_command_receipt() {
        steps.write_command_receipt(executor).await?;
    }
    Ok(())
}

/// Repository and service adapters for the actual posting path.
struct MongoInvoicePosting<'a> {
    db: &'a Database,
    invoice: &'a mut Invoice,
    input: InvoicePostingInput<'a>,
    accounts: Vec<ReceivableAccount>,
    account_ids: Vec<String>,
}

#[async_trait]
impl InvoicePostingSteps for MongoInvoicePosting<'_> {
    fn has_command_receipt(&self) -> bool {
        self.input.command_receipt.is_some()
    }

    async fn record_execution(&mut self, executor: &mut dyn Executor) -> Result<()> {
        let account_ids =
            self.input.plan_lines.iter().map(|line| line.receivable_account_id.clone()).collect::<Vec<_>>();
        let request_id = invoice_task::record_invoice_execution(
            self.db,
            invoice_task::InvoiceExecutionInput {
                work_item_id: self.input.work_item_id,
                expected_task_version: self.input.expected_task_version,
                party_id: &self.invoice.party_id,
                account_ids: &account_ids,
                invoice_amount: self.invoice.gross_amount,
            },
            self.input.actor,
            executor,
        )
        .await?;
        self.invoice.sales_invoice_request_id = Some(request_id);
        Ok(())
    }

    async fn persist_finance(&mut self, executor: &mut dyn Executor) -> Result<()> {
        let (accounts, account_ids) = persist_sales_invoice_allocations(
            self.db,
            self.invoice,
            self.input.plan_lines,
            self.input.actor.id(),
            executor,
        )
        .await?;
        self.accounts = accounts;
        self.account_ids = account_ids;
        Ok(())
    }

    async fn write_posting_audit(&mut self, executor: &mut dyn Executor) -> Result<()> {
        let audit = self.input.actor.clone().resource_log(
            self.input.action,
            "invoice",
            self.invoice.base.id.clone(),
        )?;
        self.db.audit_logs().create(&audit, executor).await?;
        Ok(())
    }

    async fn synchronize_tasks(&mut self, executor: &mut dyn Executor) -> Result<()> {
        // 同一事务执行器不可并发共享（`&mut Executor` 与会话排他），
        // 按片顺序短路即分组后的可靠形态，保持原失败语义。
        for chunk in dedup_sorted(self.account_ids.clone()).chunks(FANOUT_SHARD) {
            for account_id in chunk {
                invoice_task::sync_sales_invoice_task(
                    self.db,
                    &ReceivableAccountId::new(account_id.clone()),
                    SalesInvoiceTaskChange::InvoicePosted,
                    executor,
                )
                .await?;
            }
        }
        Ok(())
    }

    async fn update_sales_progress(&mut self, executor: &mut dyn Executor) -> Result<()> {
        let ids =
            dedup_sorted(self.accounts.iter().map(|account| account.sales_order_id.to_string()).collect());
        for chunk in ids.chunks(FANOUT_SHARD) {
            for id in chunk {
                crate::order_to_cash::progress::update_sales_order_money_progress(
                    self.db,
                    executor,
                    &SalesOrderId::new(id.clone()),
                    self.input.actor.id().to_string(),
                    None,
                )
                .await?;
            }
        }
        Ok(())
    }

    async fn write_command_receipt(&mut self, executor: &mut dyn Executor) -> Result<()> {
        if let Some(receipt) = self.input.command_receipt {
            let audit = receipt.audit(self.input.actor.clone(), self.invoice.base.id.clone())?;
            self.db.audit_logs().create(&audit, executor).await?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Error;

    // 非零大小保证执行器地址能够区分不同实例。
    struct TestExecutor {
        _identity: u8,
    }

    impl Executor for TestExecutor {
        fn session(&mut self) -> Option<&mut mongodb::ClientSession> {
            None
        }
    }

    struct RecordedPosting {
        calls: Vec<(&'static str, usize)>,
        fail_at: Option<&'static str>,
        command_receipt: bool,
    }

    impl RecordedPosting {
        fn record(&mut self, step: &'static str, executor: &mut dyn Executor) -> Result<()> {
            self.calls.push((step, executor as *mut dyn Executor as *mut () as usize));
            if self.fail_at == Some(step) {
                return Err(Error::ConflictError(step.to_string()));
            }
            Ok(())
        }
    }

    #[async_trait]
    impl InvoicePostingSteps for RecordedPosting {
        fn has_command_receipt(&self) -> bool {
            self.command_receipt
        }
        async fn record_execution(&mut self, e: &mut dyn Executor) -> Result<()> {
            self.record("execution", e)
        }
        async fn persist_finance(&mut self, e: &mut dyn Executor) -> Result<()> {
            self.record("finance", e)
        }
        async fn write_posting_audit(&mut self, e: &mut dyn Executor) -> Result<()> {
            self.record("audit", e)
        }
        async fn synchronize_tasks(&mut self, e: &mut dyn Executor) -> Result<()> {
            self.record("tasks", e)
        }
        async fn update_sales_progress(&mut self, e: &mut dyn Executor) -> Result<()> {
            self.record("sales", e)
        }
        async fn write_command_receipt(&mut self, e: &mut dyn Executor) -> Result<()> {
            self.record("receipt", e)
        }
    }

    #[tokio::test]
    async fn invoice_post_and_commit_keep_original_order_and_the_same_executor() {
        for command_receipt in [false, true] {
            let mut executor = TestExecutor { _identity: 1 };
            let expected_executor = &mut executor as *mut TestExecutor as usize;
            let mut steps = RecordedPosting { calls: Vec::new(), fail_at: None, command_receipt };
            execute_posting(&mut steps, &mut executor).await.unwrap();
            let mut expected = vec!["execution", "finance", "audit", "tasks", "sales"];
            if command_receipt {
                expected.push("receipt");
            }
            assert_eq!(
                steps.calls,
                expected.into_iter().map(|step| (step, expected_executor)).collect::<Vec<_>>()
            );
        }
    }

    #[tokio::test]
    async fn invoice_posting_stops_at_each_failure_and_preserves_the_original_error() {
        let sequence = ["execution", "finance", "audit", "tasks", "sales", "receipt"];
        for command_receipt in [false, true] {
            let count = if command_receipt { sequence.len() } else { sequence.len() - 1 };
            for (index, fail_at) in sequence[..count].iter().enumerate() {
                let mut steps =
                    RecordedPosting { calls: Vec::new(), fail_at: Some(fail_at), command_receipt };
                let error =
                    execute_posting(&mut steps, &mut TestExecutor { _identity: 1 }).await.unwrap_err();
                assert!(matches!(error, Error::ConflictError(message) if message == *fail_at));
                assert_eq!(steps.calls.iter().map(|(step, _)| *step).collect::<Vec<_>>(), sequence[..=index]);
            }
        }
    }
}
