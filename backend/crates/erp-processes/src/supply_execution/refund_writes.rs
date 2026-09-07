//! 退款回调的原三组跨域写入，在一个调用方执行器内按序完成。
use crate::Result;
use async_trait::async_trait;
use erp_audit::{AuditExt, AuditLog};
use erp_integration::{entity::integration_ops::InboxMessage, repository::IntegrationOpsExt};
use erp_supply::entity::supplier_fulfillment::{
    SupplierFulfillmentOrder, SupplierRefundAllocation, SupplierRefundFact,
};
use erp_supply::service::supplier_fulfillment::refund_result::persist_refund_result;
use mongodb::Database;
use persistence_core::Executor;

#[async_trait]
trait RefundWrites: Send {
    async fn inbox(&mut self, executor: &mut dyn Executor) -> Result<()>;
    async fn domain(&mut self, executor: &mut dyn Executor) -> Result<()>;
    async fn audit(&mut self, executor: &mut dyn Executor) -> Result<()>;
}
async fn execute(writes: &mut impl RefundWrites, executor: &mut dyn Executor) -> Result<()> {
    writes.inbox(executor).await?;
    writes.domain(executor).await?;
    writes.audit(executor).await
}
struct MongoWrites<'a> {
    db: &'a Database,
    message: &'a InboxMessage,
    order: &'a mut SupplierFulfillmentOrder,
    fact: &'a SupplierRefundFact,
    allocations: &'a [SupplierRefundAllocation],
    audit: &'a AuditLog,
}
#[async_trait]
impl RefundWrites for MongoWrites<'_> {
    async fn inbox(&mut self, executor: &mut dyn Executor) -> Result<()> {
        self.db.inbox_messages().create(self.message, executor).await?;
        Ok(())
    }
    async fn domain(&mut self, executor: &mut dyn Executor) -> Result<()> {
        persist_refund_result(self.db, self.order, self.fact, self.allocations, executor).await?;
        Ok(())
    }
    async fn audit(&mut self, executor: &mut dyn Executor) -> Result<()> {
        self.db.audit_logs().create(self.audit, executor).await?;
        Ok(())
    }
}
/// inbox、订单/退款事实、审计均消费同一个外层执行器。
pub(super) async fn persist(
    db: &Database,
    message: &InboxMessage,
    order: &mut SupplierFulfillmentOrder,
    fact: &SupplierRefundFact,
    allocations: &[SupplierRefundAllocation],
    audit: &AuditLog,
    executor: &mut dyn Executor,
) -> Result<()> {
    execute(
        &mut MongoWrites {
            db,
            message,
            order,
            fact,
            allocations,
            audit,
        },
        executor,
    )
    .await
}
#[cfg(test)]
mod tests {
    use super::{execute, RefundWrites};
    use crate::{Error, Result};
    use async_trait::async_trait;
    use persistence_core::Executor;
    struct TestExecutor {
        _identity: u8,
    }
    impl Executor for TestExecutor {
        fn session(&mut self) -> Option<&mut mongodb::ClientSession> {
            None
        }
    }
    #[derive(Default)]
    struct Writes {
        steps: Vec<&'static str>,
        executors: Vec<usize>,
        fail: Option<&'static str>,
    }
    impl Writes {
        fn record(&mut self, step: &'static str, e: &mut dyn Executor) -> Result<()> {
            self.steps.push(step);
            self.executors.push(e as *mut dyn Executor as *mut () as usize);
            if self.fail == Some(step) {
                return Err(Error::ConflictError(step.into()));
            }
            Ok(())
        }
    }
    #[async_trait]
    impl RefundWrites for Writes {
        async fn inbox(&mut self, e: &mut dyn Executor) -> Result<()> {
            self.record("inbox", e)
        }
        async fn domain(&mut self, e: &mut dyn Executor) -> Result<()> {
            self.record("order_fact_allocations", e)
        }
        async fn audit(&mut self, e: &mut dyn Executor) -> Result<()> {
            self.record("audit", e)
        }
    }
    #[tokio::test]
    async fn refund_callback_preserves_one_executor_and_original_write_order() {
        let mut executor = TestExecutor { _identity: 1 };
        let id = &mut executor as *mut TestExecutor as usize;
        let mut writes = Writes::default();
        execute(&mut writes, &mut executor).await.unwrap();
        assert_eq!(writes.steps, ["inbox", "order_fact_allocations", "audit"]);
        assert_eq!(writes.executors, [id, id, id]);
    }
    #[tokio::test]
    async fn refund_callback_each_failure_stops_later_writes() {
        let steps = ["inbox", "order_fact_allocations", "audit"];
        for (index, fail) in steps.iter().enumerate() {
            let mut executor = TestExecutor { _identity: 1 };
            let mut writes = Writes {
                fail: Some(*fail),
                ..Default::default()
            };
            let result = execute(&mut writes, &mut executor).await;
            assert!(matches!(result,Err(Error::ConflictError(message)) if message==*fail));
            assert_eq!(writes.steps, steps[..=index]);
        }
    }
}
