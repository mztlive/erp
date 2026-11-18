//! 履约任务创建的真实步骤；幂等检查和责任解析完成后才创建本次授权身份。

use std::sync::Arc;

use async_trait::async_trait;
use erp_core::ids::WorkItemId;
use erp_identity::AuthorizationService;
use erp_read_models::work_item_links;
use erp_workflow::WorkItemExt;
use erp_workflow::entity::work_item::{
    AssignmentSource, WorkItem, WorkItemData, WorkItemPriority, WorkItemType,
};
use id_generator::next_id;
use mongodb::Database;
use persistence_core::Executor;

use super::{
    FulfillmentTaskObject, FulfillmentTaskOwner, ensure_fulfillment_owner_eligible,
    ensure_task_matches_frozen_identity, open_fulfillment_tasks, resolve_task_owner,
};
use crate::{Error, Result};

#[async_trait]
trait TaskCreationPort: Send + Sync {
    type Identity: Send + Sync;
    async fn open(
        &self,
        object: &FulfillmentTaskObject<'_>,
        executor: &mut dyn Executor,
    ) -> Result<Vec<WorkItem>>;
    async fn owner(
        &self,
        object: &FulfillmentTaskObject<'_>,
        executor: &mut dyn Executor,
    ) -> Result<FulfillmentTaskOwner>;
    async fn authorize(
        &self,
        identity: &Self::Identity,
        owner: &str,
        object_type: &str,
        executor: &mut dyn Executor,
    ) -> Result<()>;
    async fn create(&self, task: &WorkItem, executor: &mut dyn Executor) -> Result<()>;
}

struct MongoTaskCreation<'a>(&'a Database);

#[async_trait]
impl TaskCreationPort for MongoTaskCreation<'_> {
    type Identity = Arc<AuthorizationService>;

    async fn open(
        &self,
        object: &FulfillmentTaskObject<'_>,
        executor: &mut dyn Executor,
    ) -> Result<Vec<WorkItem>> {
        open_fulfillment_tasks(self.0, object.business_object_type(), object.business_object_id(), executor)
            .await
    }

    async fn owner(
        &self,
        object: &FulfillmentTaskObject<'_>,
        executor: &mut dyn Executor,
    ) -> Result<FulfillmentTaskOwner> {
        resolve_task_owner(self.0, object, executor).await
    }

    async fn authorize(
        &self,
        identity: &Arc<AuthorizationService>,
        owner: &str,
        object_type: &str,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        ensure_fulfillment_owner_eligible(self.0, identity, owner, object_type, executor).await
    }

    async fn create(&self, task: &WorkItem, executor: &mut dyn Executor) -> Result<()> {
        let mut task = task.clone();
        task.set_links(
            work_item_links::capture(self.0, &task.business_object_type, &task.business_object_id, executor)
                .await?,
        )?;
        self.0.work_items().create(&task, executor).await?;
        Ok(())
    }
}

/// 在调用方事务中幂等建立履约执行任务。
/// # 参数
/// `db`、`executor` 为调用方数据库与事务；`authorization` 为进程共享授权服务；
/// `object` 为已经形成的履约草稿。
/// # 返回
/// 原任务身份核验通过或新任务保存成功后返回。
/// # 错误
/// 任务重复、责任解析、授权、领域校验或写入失败时保留原错误。
pub(super) async fn ensure(
    db: &Database,
    authorization: &Arc<AuthorizationService>,
    object: FulfillmentTaskObject<'_>,
    executor: &mut dyn Executor,
) -> Result<()> {
    execute(&MongoTaskCreation(db), &|| authorization.clone(), object, executor).await
}

async fn execute<P: TaskCreationPort>(
    port: &P,
    authorization: &(dyn Fn() -> P::Identity + Send + Sync),
    object: FulfillmentTaskObject<'_>,
    executor: &mut dyn Executor,
) -> Result<()> {
    let existing = port.open(&object, executor).await?;
    match existing.as_slice() {
        [] => {},
        [task] => return ensure_task_matches_frozen_identity(task, &object),
        _ => {
            return Err(Error::BusinessLogicError(
                "履约对象存在多个开放执行任务，请联系管理员处理后重试".to_string(),
            ));
        },
    }

    let owner = port.owner(&object, executor).await?;
    let identity = authorization();
    port.authorize(&identity, &owner.owner_user_id, object.business_object_type(), executor).await?;

    let task = new_task(&object, owner)?;
    ensure_task_matches_frozen_identity(&task, &object)?;
    port.create(&task, executor).await
}

fn new_task(object: &FulfillmentTaskObject<'_>, owner: FulfillmentTaskOwner) -> Result<WorkItem> {
    Ok(WorkItem::new_with_responsibility_key(
        WorkItemId::new(next_id()),
        WorkItemData {
            work_item_type: WorkItemType::FulfillmentOperation,
            business_object_type: object.business_object_type().to_string(),
            business_object_id: object.business_object_id().to_string(),
            subject_version: object.subject_version(),
            owner_role: owner.owner_role.to_string(),
            owner_organization_id: owner.owner_organization_id,
            owner_user_id: owner.owner_user_id,
            assignment_source: AssignmentSource::SystemRule,
            priority: WorkItemPriority::Normal,
            due_at: None,
            reason_code: Some(object.reason_code().to_string()),
            impact_summary: Some(object.impact_summary()),
        },
        owner.responsibility_key,
    )?)
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use erp_workflow::entity::work_item::WorkItemStatus;

    use super::super::fixtures::{TestExecutor, delivery, task};
    use super::*;

    #[derive(Default)]
    struct Recorded {
        calls: Vec<&'static str>,
        identities: Vec<usize>,
        writes: Vec<WorkItem>,
    }

    struct Port {
        executor: usize,
        existing: Vec<WorkItem>,
        fail_at: Option<&'static str>,
        recorded: Mutex<Recorded>,
    }

    impl Port {
        fn step(&self, name: &'static str, executor: &mut dyn Executor) -> Result<()> {
            assert_eq!(executor as *mut dyn Executor as *mut () as usize, self.executor);
            self.recorded.lock().unwrap().calls.push(name);
            if self.fail_at == Some(name) {
                return Err(Error::Forbidden(format!("denied at {name}")));
            }
            Ok(())
        }

        fn identity(&self) -> usize {
            let mut recorded = self.recorded.lock().unwrap();
            recorded.calls.push("identity");
            let identity = recorded.identities.len() + 1;
            recorded.identities.push(identity);
            identity
        }
    }

    #[async_trait]
    impl TaskCreationPort for Port {
        type Identity = usize;

        async fn open(
            &self,
            object: &FulfillmentTaskObject<'_>,
            executor: &mut dyn Executor,
        ) -> Result<Vec<WorkItem>> {
            self.step("open", executor)?;
            assert_eq!(object.business_object_type(), "delivery");
            assert_eq!(object.business_object_id(), "delivery-1");
            Ok(self.existing.clone())
        }

        async fn owner(
            &self,
            _: &FulfillmentTaskObject<'_>,
            executor: &mut dyn Executor,
        ) -> Result<FulfillmentTaskOwner> {
            self.step("owner", executor)?;
            Ok(FulfillmentTaskOwner {
                owner_user_id: "buyer-1".into(),
                owner_role: "purchase_order_owner",
                owner_organization_id: "party-1".into(),
                responsibility_key: "purchase_order:po-1".into(),
            })
        }

        async fn authorize(
            &self,
            identity: &usize,
            owner: &str,
            object_type: &str,
            executor: &mut dyn Executor,
        ) -> Result<()> {
            assert_eq!(self.recorded.lock().unwrap().identities.last(), Some(identity));
            assert_eq!((owner, object_type), ("buyer-1", "delivery"));
            self.step("authorize", executor)
        }

        async fn create(&self, task: &WorkItem, executor: &mut dyn Executor) -> Result<()> {
            self.step("create", executor)?;
            self.recorded.lock().unwrap().writes.push(task.clone());
            Ok(())
        }
    }

    fn port(executor: &mut dyn Executor) -> Port {
        Port {
            executor: executor as *mut dyn Executor as *mut () as usize,
            existing: vec![],
            fail_at: None,
            recorded: Mutex::default(),
        }
    }

    async fn invoke(port: &Port, executor: &mut dyn Executor) -> Result<()> {
        let mut document = delivery();
        document.base.version = 7;
        execute(port, &|| port.identity(), FulfillmentTaskObject::Delivery(&document), executor).await
    }

    #[tokio::test]
    async fn task_creation_resolves_owner_before_fresh_identity_and_preserves_frozen_fields() {
        let mut executor = TestExecutor::default();
        let port = port(&mut executor);
        invoke(&port, &mut executor).await.unwrap();

        let recorded = port.recorded.lock().unwrap();
        assert_eq!(recorded.calls, ["open", "owner", "identity", "authorize", "create"]);
        assert_eq!(recorded.identities, [1]);

        assert_eq!(recorded.writes.len(), 1);
        let written = &recorded.writes[0];
        assert_eq!(written.work_item_type, WorkItemType::FulfillmentOperation);
        assert_eq!(written.status, WorkItemStatus::Open);
        assert_eq!(written.business_object_type, "delivery");
        assert_eq!(written.business_object_id, "delivery-1");
        assert_eq!(written.subject_version, "7");
        assert_eq!(written.owner_user_id.as_deref(), Some("buyer-1"));
        assert_eq!(written.owner_role, "purchase_order_owner");
        assert_eq!(written.owner_organization_id, "party-1");
        assert_eq!(written.responsibility_key(), Some("purchase_order:po-1"));
        assert_eq!(written.reason_code.as_deref(), Some("SUPPLIER_DIRECT_DELIVERY_READY"));
    }

    #[tokio::test]
    async fn existing_task_replay_does_not_resolve_owner_create_identity_or_write() {
        let mut executor = TestExecutor::default();
        let mut port = port(&mut executor);
        port.existing = vec![task()];
        port.fail_at = Some("owner");
        invoke(&port, &mut executor).await.unwrap();
        let recorded = port.recorded.lock().unwrap();
        assert_eq!(recorded.calls, ["open"]);
        assert!(recorded.identities.is_empty());
        assert!(recorded.writes.is_empty());
    }

    #[tokio::test]
    async fn invalid_or_duplicate_existing_tasks_stop_before_identity_and_writes() {
        let mut wrong = task();
        wrong.reason_code = Some("WAREHOUSE_DELIVERY_READY".into());
        for existing in [vec![wrong], vec![task(), task()]] {
            let mut executor = TestExecutor::default();
            let mut port = port(&mut executor);
            port.existing = existing;
            assert!(matches!(invoke(&port, &mut executor).await, Err(Error::BusinessLogicError(_))));
            let recorded = port.recorded.lock().unwrap();
            assert_eq!(recorded.calls, ["open"]);
            assert!(recorded.identities.is_empty());
            assert!(recorded.writes.is_empty());
        }
    }

    #[tokio::test]
    async fn creation_failure_stops_every_later_step_and_preserves_executor_and_error() {
        let steps = ["open", "owner", "identity", "authorize", "create"];
        for failed in [0, 1, 3, 4] {
            let mut executor = TestExecutor::default();
            let mut port = port(&mut executor);
            port.fail_at = Some(steps[failed]);
            assert!(
                matches!(invoke(&port, &mut executor).await, Err(Error::Forbidden(message)) if message == format!("denied at {}", steps[failed]))
            );
            let recorded = port.recorded.lock().unwrap();
            assert_eq!(recorded.calls, steps[..=failed]);
            assert_eq!(recorded.identities.len(), usize::from(failed > 1));
            assert!(recorded.writes.is_empty());
        }
    }

    #[tokio::test]
    async fn authorization_failure_cannot_reuse_the_identity_on_the_next_command() {
        let mut executor = TestExecutor::default();
        let mut port = port(&mut executor);
        port.fail_at = Some("authorize");
        assert!(matches!(invoke(&port, &mut executor).await, Err(Error::Forbidden(_))));

        port.fail_at = None;
        invoke(&port, &mut executor).await.unwrap();

        let recorded = port.recorded.lock().unwrap();
        assert_eq!(recorded.identities, [1, 2]);
        assert_eq!(recorded.writes.len(), 1);
        assert_eq!(
            recorded.calls,
            ["open", "owner", "identity", "authorize", "open", "owner", "identity", "authorize", "create"]
        );
    }
}
