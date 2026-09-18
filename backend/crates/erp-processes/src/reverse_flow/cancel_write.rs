//! 审批取消的同事务写入：Apply 更新运行事实，Replay 仍执行本域 CAS 和审计。
use async_trait::async_trait;
use erp_audit::{AuditExt, AuditLog};
use erp_core::common::time::Instant;
use erp_returns::entity::returns::{CustomerRefund, PaymentReversal, ReceiptReversal, SupplierRefund};
use erp_returns::service::ReturnsService;
use erp_workflow::entity::work_item::WorkItem;
use erp_workflow::repository::prelude::*;
use erp_workflow::service::approval::execution::PreparedExecution;
use erp_workflow::{BpmExt, WorkItemExt};
use mongodb::Database;
use persistence_core::Executor;

use crate::Result;

pub(super) enum CancelledReturn<'a> {
    CustomerRefund(&'a mut CustomerRefund),
    SupplierRefund(&'a mut SupplierRefund),
    ReceiptReversal(&'a mut ReceiptReversal),
    PaymentReversal(&'a mut PaymentReversal),
}
pub(super) struct MongoCancelWrite<'a> {
    pub db: &'a Database,
    pub record: CancelledReturn<'a>,
    pub prepared: PreparedExecution,
    pub open_tasks: Vec<WorkItem>,
    pub actor_id: String,
    pub reason: String,
    pub now: Instant,
    pub audit: AuditLog,
    pub closed_tasks: Vec<WorkItem>,
}
#[async_trait]
trait CancelWritePort: Send {
    fn applies_runtime(&self) -> bool;
    async fn runtime(&mut self, executor: &mut dyn Executor) -> Result<()>;
    async fn tasks(&mut self, executor: &mut dyn Executor) -> Result<()>;
    async fn record(&mut self, executor: &mut dyn Executor) -> Result<()>;
    async fn audit(&mut self, executor: &mut dyn Executor) -> Result<()>;
}
#[async_trait]
impl CancelWritePort for MongoCancelWrite<'_> {
    fn applies_runtime(&self) -> bool {
        matches!(self.prepared, PreparedExecution::Apply(_))
    }
    async fn runtime(&mut self, executor: &mut dyn Executor) -> Result<()> {
        let PreparedExecution::Apply(writes) = &self.prepared else {
            return Ok(());
        };
        self.closed_tasks = WorkItem::close_all_for_approval_cancellation(
            std::mem::take(&mut self.open_tasks),
            &self.actor_id,
            &self.reason,
            self.now,
        )?;
        self.db
            .bpm_workflow()
            .persist_cancelled_runtime(
                &writes.instance,
                &writes.updated_executions,
                &writes.receipt,
                executor,
            )
            .await?;
        Ok(())
    }
    async fn tasks(&mut self, executor: &mut dyn Executor) -> Result<()> {
        self.db.work_items().persist_cancelled_approval_tasks(&self.closed_tasks, executor).await?;
        Ok(())
    }
    async fn record(&mut self, executor: &mut dyn Executor) -> Result<()> {
        match &mut self.record {
            CancelledReturn::CustomerRefund(record) => {
                ReturnsService::new(self.db.clone()).persist_customer_refund(record, executor).await?
            },
            CancelledReturn::SupplierRefund(record) => {
                ReturnsService::persist_supplier_refund(self.db, record, executor).await?
            },
            CancelledReturn::ReceiptReversal(record) => {
                ReturnsService::persist_receipt_reversal(self.db, record, executor).await?
            },
            CancelledReturn::PaymentReversal(record) => {
                ReturnsService::persist_payment_reversal(self.db, record, executor).await?
            },
        }
        Ok(())
    }
    async fn audit(&mut self, executor: &mut dyn Executor) -> Result<()> {
        self.db.audit_logs().create(&self.audit, executor).await?;
        Ok(())
    }
}
async fn execute<P: CancelWritePort>(port: &mut P, executor: &mut dyn Executor) -> Result<()> {
    if port.applies_runtime() {
        port.runtime(executor).await?;
        port.tasks(executor).await?;
    }
    port.record(executor).await?;
    port.audit(executor).await?;
    Ok(())
}
pub(super) async fn persist(mut port: MongoCancelWrite<'_>, executor: &mut dyn Executor) -> Result<()> {
    execute(&mut port, executor).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Error;
    struct SessionMarker(u64);
    impl Executor for SessionMarker {
        fn session(&mut self) -> Option<&mut mongodb::ClientSession> {
            self.0 += 1;
            None
        }
    }
    struct RecordingPort {
        executor: usize,
        apply: bool,
        fail: Option<usize>,
        calls: Vec<&'static str>,
    }
    impl RecordingPort {
        fn write(&mut self, step: &'static str, executor: &mut dyn Executor) -> Result<()> {
            assert_eq!(executor as *mut dyn Executor as *mut () as usize, self.executor);
            let index = self.calls.len();
            self.calls.push(step);
            if self.fail == Some(index) {
                return Err(Error::ConflictError(format!("cancel failure {index}")));
            }
            Ok(())
        }
    }
    #[async_trait]
    impl CancelWritePort for RecordingPort {
        fn applies_runtime(&self) -> bool {
            self.apply
        }
        async fn runtime(&mut self, e: &mut dyn Executor) -> Result<()> {
            self.write("runtime", e)
        }
        async fn tasks(&mut self, e: &mut dyn Executor) -> Result<()> {
            self.write("tasks", e)
        }
        async fn record(&mut self, e: &mut dyn Executor) -> Result<()> {
            self.write("record", e)
        }
        async fn audit(&mut self, e: &mut dyn Executor) -> Result<()> {
            self.write("audit", e)
        }
    }
    async fn invoke(apply: bool, fail: Option<usize>) -> (Result<()>, Vec<&'static str>) {
        let mut executor = SessionMarker(93);
        let mut port = RecordingPort {
            executor: &mut executor as *mut SessionMarker as usize,
            apply,
            fail,
            calls: Vec::new(),
        };
        let result = execute(&mut port, &mut executor).await;
        assert_eq!(executor.0, 93);
        (result, port.calls)
    }
    #[tokio::test]
    async fn apply_cancellation_keeps_runtime_tasks_record_audit_on_same_executor() {
        let (result, calls) = invoke(true, None).await;
        result.unwrap();
        assert_eq!(calls, ["runtime", "tasks", "record", "audit"]);
    }
    #[tokio::test]
    async fn replay_cancellation_still_writes_record_and_audit_without_runtime() {
        let (result, calls) = invoke(false, None).await;
        result.unwrap();
        assert_eq!(calls, ["record", "audit"]);
    }
    #[tokio::test]
    async fn cancellation_stops_at_each_original_write_failure_for_apply_and_replay() {
        for apply in [true, false] {
            let order: &[&str] =
                if apply { &["runtime", "tasks", "record", "audit"] } else { &["record", "audit"] };
            for index in 0..order.len() {
                let (result, calls) = invoke(apply, Some(index)).await;
                assert!(
                    matches!(result,Err(Error::ConflictError(message)) if message==format!("cancel failure {index}"))
                );
                assert_eq!(calls, order[..=index]);
            }
        }
    }
}
