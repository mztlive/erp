//! W01 活动与完成命令的真实生产读取、责任重验与写入边界。
use super::{
    ensure_current_owner_execution_access, ensure_task_matches_frozen_identity, load_single_open_task,
    FulfillmentTaskObject,
};
use crate::Result;
use async_trait::async_trait;
use erp_workflow::entity::work_item::WorkItem;
use erp_workflow::WorkItemExt;
use mongodb::Database;
use persistence_core::Executor;

#[derive(Clone, Copy)]
enum CommandKind {
    Activity,
    Complete,
}
#[async_trait]
trait TaskCommandPort: Send + Sync {
    async fn load_open(
        &self,
        object_type: &str,
        object_id: &str,
        executor: &mut dyn Executor,
    ) -> Result<WorkItem>;
    async fn authorize(&self, task: &WorkItem, actor_id: &str, executor: &mut dyn Executor) -> Result<()>;
    async fn update(&self, task: &mut WorkItem, executor: &mut dyn Executor) -> Result<()>;
}
struct MongoTaskCommand<'a> {
    db: &'a Database,
}
#[async_trait]
impl TaskCommandPort for MongoTaskCommand<'_> {
    async fn load_open(
        &self,
        object_type: &str,
        object_id: &str,
        executor: &mut dyn Executor,
    ) -> Result<WorkItem> {
        load_single_open_task(self.db, object_type, object_id, executor).await
    }
    async fn authorize(&self, task: &WorkItem, actor_id: &str, executor: &mut dyn Executor) -> Result<()> {
        ensure_current_owner_execution_access(self.db, task, actor_id, executor).await
    }
    async fn update(&self, task: &mut WorkItem, executor: &mut dyn Executor) -> Result<()> {
        self.db.work_items().update(task, executor).await?;
        Ok(())
    }
}
async fn execute(
    port: &dyn TaskCommandPort,
    object: FulfillmentTaskObject<'_>,
    actor_id: &str,
    kind: CommandKind,
    executor: &mut dyn Executor,
) -> Result<()> {
    let mut task = port
        .load_open(
            object.business_object_type(),
            object.business_object_id(),
            executor,
        )
        .await?;
    ensure_task_matches_frozen_identity(&task, &object)?;
    match kind {
        CommandKind::Activity => task.record_activity(actor_id, erp_core::common::time::Instant::now())?,
        CommandKind::Complete => {
            task.complete_by_domain_command(actor_id, erp_core::common::time::Instant::now())?
        }
    }
    port.authorize(&task, actor_id, executor).await?;
    port.update(&mut task, executor).await?;
    Ok(())
}
pub(super) async fn record(
    db: &Database,
    object: FulfillmentTaskObject<'_>,
    actor_id: &str,
    executor: &mut dyn Executor,
) -> Result<()> {
    execute(
        &MongoTaskCommand { db },
        object,
        actor_id,
        CommandKind::Activity,
        executor,
    )
    .await
}
pub(super) async fn complete(
    db: &Database,
    object: FulfillmentTaskObject<'_>,
    actor_id: &str,
    executor: &mut dyn Executor,
) -> Result<()> {
    execute(
        &MongoTaskCommand { db },
        object,
        actor_id,
        CommandKind::Complete,
        executor,
    )
    .await
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Error;
    use erp_core::ids::{DeliveryId, PurchaseOrderId, SalesOrderId, WorkItemId};
    use erp_fulfillment::entity::fulfillment::{Delivery, DeliveryData, DeliveryType};
    use erp_workflow::entity::work_item::{
        AssignmentSource, WorkItemData, WorkItemPriority, WorkItemStatus, WorkItemType,
    };
    use std::sync::Mutex;

    struct TestExecutor {
        marker: u64,
    }
    impl Executor for TestExecutor {
        fn session(&mut self) -> Option<&mut mongodb::ClientSession> {
            self.marker += 1;
            None
        }
    }
    #[derive(Default)]
    struct Recorded {
        calls: Vec<&'static str>,
        status_at_authorization: Option<WorkItemStatus>,
        written: Option<WorkItem>,
    }
    struct RecordingPort {
        executor: usize,
        task: WorkItem,
        kind: CommandKind,
        fail_at: Option<usize>,
        recorded: Mutex<Recorded>,
    }
    impl RecordingPort {
        fn record(&self, step: &'static str, executor: &mut dyn Executor) -> Result<()> {
            assert_eq!(executor as *mut dyn Executor as *mut () as usize, self.executor);
            let mut state = self.recorded.lock().unwrap();
            let index = state.calls.len();
            state.calls.push(step);
            if self.fail_at == Some(index) {
                return Err(Error::ConflictError(format!("task failure {index}")));
            }
            Ok(())
        }
    }
    #[async_trait]
    impl TaskCommandPort for RecordingPort {
        async fn load_open(
            &self,
            object_type: &str,
            object_id: &str,
            executor: &mut dyn Executor,
        ) -> Result<WorkItem> {
            self.record("load", executor)?;
            assert_eq!(object_type, "delivery");
            assert_eq!(object_id, "delivery-1");
            Ok(self.task.clone())
        }
        async fn authorize(
            &self,
            task: &WorkItem,
            actor_id: &str,
            executor: &mut dyn Executor,
        ) -> Result<()> {
            self.record("authorize", executor)?;
            assert_eq!(actor_id, "buyer-1");
            assert!(task.started_at.is_some());
            match self.kind {
                CommandKind::Activity => {
                    assert!(task.last_activity_at.is_some());
                    assert_eq!(task.completed_at, self.task.completed_at);
                    assert_eq!(task.completed_by, self.task.completed_by);
                }
                CommandKind::Complete => {
                    assert!(task.completed_at.is_some());
                    assert_eq!(task.completed_by.as_deref(), Some(actor_id));
                    assert_eq!(task.last_activity_at, self.task.last_activity_at);
                }
            }
            self.recorded.lock().unwrap().status_at_authorization = Some(task.status);
            Ok(())
        }
        async fn update(&self, task: &mut WorkItem, executor: &mut dyn Executor) -> Result<()> {
            self.record("update", executor)?;
            self.recorded.lock().unwrap().written = Some(task.clone());
            Ok(())
        }
    }
    fn task() -> WorkItem {
        WorkItem::new_with_responsibility_key(
            WorkItemId::new("task-1"),
            WorkItemData {
                work_item_type: WorkItemType::FulfillmentOperation,
                business_object_type: "delivery".to_string(),
                business_object_id: "delivery-1".to_string(),
                subject_version: "1".to_string(),
                owner_role: "purchase_order_owner".to_string(),
                owner_organization_id: "party-1".to_string(),
                owner_user_id: "buyer-1".to_string(),
                assignment_source: AssignmentSource::SystemRule,
                priority: WorkItemPriority::Normal,
                due_at: None,
                reason_code: Some("SUPPLIER_DIRECT_DELIVERY_READY".to_string()),
                impact_summary: None,
            },
            "purchase_order:po-1",
        )
        .unwrap()
    }
    fn delivery() -> Delivery {
        Delivery::new(
            DeliveryId::new("delivery-1"),
            DeliveryData {
                delivery_no: "DN20260907-000001".to_string(),
                delivery_type: DeliveryType::SupplierDirect,
                sales_order_id: SalesOrderId::new("sales-1"),
                purchase_order_id: Some(PurchaseOrderId::new("po-1")),
                warehouse_id: None,
                carrier: None,
                tracking_no: None,
                address_snapshot_encrypted: None,
                address_snapshot_fingerprint: None,
            },
        )
        .unwrap()
    }
    async fn invoke(
        kind: CommandKind,
        task: WorkItem,
        actor: &str,
        fail_at: Option<usize>,
    ) -> (Result<()>, Recorded) {
        let mut executor = TestExecutor { marker: 79 };
        let port = RecordingPort {
            executor: (&mut executor as *mut TestExecutor) as usize,
            task,
            kind,
            fail_at,
            recorded: Mutex::default(),
        };
        let delivery = delivery();
        let result = execute(
            &port,
            FulfillmentTaskObject::Delivery(&delivery),
            actor,
            kind,
            &mut executor,
        )
        .await;
        assert_eq!(executor.marker, 79);
        (result, port.recorded.into_inner().unwrap())
    }
    /// 活动与完成均先形成原任务内存事实，再重验权限并写回同一执行器。
    #[tokio::test]
    async fn task_command_preserves_executor_and_original_mutate_authorize_write_order() {
        for (kind, status) in [
            (CommandKind::Activity, WorkItemStatus::Open),
            (CommandKind::Complete, WorkItemStatus::Completed),
        ] {
            let (result, recorded) = invoke(kind, task(), "buyer-1", None).await;
            result.unwrap();
            assert_eq!(recorded.calls, ["load", "authorize", "update"]);
            assert_eq!(recorded.status_at_authorization, Some(status));
            assert_eq!(recorded.written.unwrap().status, status);
        }
    }
    /// 读取、权限重验和写入任一步失败均停止后续步骤，保留原错误。
    #[tokio::test]
    async fn task_command_stops_at_every_provider_failure() {
        let order = ["load", "authorize", "update"];
        for kind in [CommandKind::Activity, CommandKind::Complete] {
            for index in 0..order.len() {
                let (result, recorded) = invoke(kind, task(), "buyer-1", Some(index)).await;
                assert!(
                    matches!(result,Err(Error::ConflictError(message)) if message==format!("task failure {index}"))
                );
                assert_eq!(recorded.calls, order[..=index]);
                assert!(recorded.written.is_none());
            }
        }
    }
    /// 冻结责任身份不匹配在账号权限重验之前失败，不能写任务。
    #[tokio::test]
    async fn frozen_responsibility_mismatch_stops_before_authorization() {
        let mut task = task();
        task.reason_code = Some("WAREHOUSE_DELIVERY_READY".to_string());
        let (result, recorded) = invoke(CommandKind::Activity, task, "buyer-1", None).await;
        assert!(
            matches!(result,Err(Error::BusinessLogicError(message)) if message=="履约任务责任身份与业务对象不一致，请联系管理员修复后重试")
        );
        assert_eq!(recorded.calls, ["load"]);
        assert!(recorded.written.is_none());
    }
    /// 非当前责任人先被任务实体拒绝，不能绕过原顺序调用权限源或写入。
    #[tokio::test]
    async fn wrong_actor_stops_before_permission_revalidation() {
        for kind in [CommandKind::Activity, CommandKind::Complete] {
            let (result, recorded) = invoke(kind, task(), "other-user", None).await;
            assert!(matches!(result, Err(Error::Logic(_))));
            assert_eq!(recorded.calls, ["load"]);
            assert!(recorded.written.is_none());
        }
    }
}
