//! 派发结果在同一执行器中依次写回本域、集成信封、正式责任。
use application_core::AuditActor;
use async_trait::async_trait;
use erp_audit::{AuditActorLogs, AuditExt};
use erp_core::ids::WorkItemId;
use erp_integration::entity::integration_ops::{ErrorClass, InboxMessage, IntegrationErrorTask};
use erp_integration::repository::IntegrationOpsExt;
use erp_supply::entity::supplier_fulfillment::{SupplierFulfillmentOrder, SupplierOrderAction};
use erp_supply::service::supplier_fulfillment::{place::persist_dispatch_entities, W26_BUSINESS_OBJECT_TYPE};
use erp_workflow::{WorkItemExt, WorkItemType};
use mongodb::Database;
use persistence_core::Executor;
use services::Result;
#[async_trait]
trait DispatchWrites: Send {
    async fn domain(&mut self, executor: &mut dyn Executor) -> Result<()>;
    async fn message(&mut self, executor: &mut dyn Executor) -> Result<()>;
    async fn responsibility(&mut self, executor: &mut dyn Executor) -> Result<()>;
}
async fn execute(writes: &mut impl DispatchWrites, executor: &mut dyn Executor) -> Result<()> {
    writes.domain(executor).await?;
    writes.message(executor).await?;
    writes.responsibility(executor).await
}
pub(super) struct MongoWrites<'a> {
    pub(super) db: &'a Database,
    pub(super) order: &'a mut SupplierFulfillmentOrder,
    pub(super) action: &'a mut SupplierOrderAction,
    pub(super) message: &'a mut InboxMessage,
    pub(super) task: Option<&'a IntegrationErrorTask>,
    pub(super) work_item_id: WorkItemId,
    pub(super) actor: &'a AuditActor,
}
impl MongoWrites<'_> {
    pub(super) async fn persist(mut self, executor: &mut dyn Executor) -> Result<()> {
        execute(&mut self, executor).await
    }
}
#[async_trait]
impl DispatchWrites for MongoWrites<'_> {
    async fn domain(&mut self, executor: &mut dyn Executor) -> Result<()> {
        persist_dispatch_entities(self.db, self.order, self.action, executor).await?;
        Ok(())
    }
    async fn message(&mut self, executor: &mut dyn Executor) -> Result<()> {
        if let Some(task) = self.task {
            self.db
                .integration_ops()
                .create_error_task_with_message_failure(task, self.message, executor)
                .await?;
        } else {
            self.db.inbox_messages().update(self.message, executor).await?;
        }
        Ok(())
    }
    async fn responsibility(&mut self, executor: &mut dyn Executor) -> Result<()> {
        if let Some(task) = self.task {
            let work_item_type = if task.error_class == ErrorClass::ResultUnknown {
                WorkItemType::IntegrationResultUnknown
            } else {
                WorkItemType::BusinessException
            };
            let existing = self
                .db
                .work_items()
                .list_active_by_object(W26_BUSINESS_OBJECT_TYPE, &self.order.base.id, executor)
                .await?;
            if let Some(mut work_item) = existing
                .into_iter()
                .find(|item| item.work_item_type == work_item_type)
            {
                let current_subject_version = self.order.base.version.to_string();
                if work_item.subject_version != current_subject_version {
                    work_item.subject_version = current_subject_version;
                    self.db.work_items().update(&mut work_item, executor).await?;
                    let audit = self.actor.clone().resource_log(
                        "supplier_fulfillment.work_item.refresh_subject",
                        "work_item",
                        work_item.base.id.clone(),
                    )?;
                    self.db.audit_logs().create(&audit, executor).await?;
                }
            } else {
                let work_item = super::work_item::create(
                    self.work_item_id.clone(),
                    self.order,
                    work_item_type,
                    self.actor.id(),
                )?;
                self.db.work_items().create(&work_item, executor).await?;
                let audit = self.actor.clone().resource_log(
                    "supplier_fulfillment.work_item.create",
                    "work_item",
                    work_item.base.id.clone(),
                )?;
                self.db.audit_logs().create(&audit, executor).await?;
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::{execute, DispatchWrites};
    use async_trait::async_trait;
    use persistence_core::Executor;
    use services::{Error, Result};
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
    impl DispatchWrites for Writes {
        async fn message(&mut self, e: &mut dyn Executor) -> Result<()> {
            self.record("message_or_error", e)
        }
        async fn domain(&mut self, e: &mut dyn Executor) -> Result<()> {
            self.record("order_action", e)
        }
        async fn responsibility(&mut self, e: &mut dyn Executor) -> Result<()> {
            self.record("w26", e)
        }
    }
    #[tokio::test]
    async fn dispatch_result_preserves_one_executor_and_original_write_order() {
        let mut executor = TestExecutor { _identity: 1 };
        let id = &mut executor as *mut TestExecutor as usize;
        let mut writes = Writes::default();
        execute(&mut writes, &mut executor).await.unwrap();
        assert_eq!(writes.steps, ["order_action", "message_or_error", "w26"]);
        assert_eq!(writes.executors, [id, id, id]);
    }
    #[tokio::test]
    async fn dispatch_result_each_failure_stops_later_writes() {
        let steps = ["order_action", "message_or_error", "w26"];
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
