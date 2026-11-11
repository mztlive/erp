//! W01 活动与完成命令的真实生产读取、责任重验与写入边界。
use application_core::AuditActor;
use async_trait::async_trait;
use erp_core::AccountKind;
use erp_core::common::time::Instant;
use erp_identity::SharedRbacService;
use erp_read_models::work_item_links;
use erp_workflow::WorkItemExt;
use erp_workflow::entity::work_item::{WorkItem, WorkItemStatus};
use mongodb::Database;
use persistence_core::Executor;

use super::{
    FulfillmentIdentityFactory, FulfillmentTaskObject, ensure_task_matches_frozen_identity,
    load_single_open_task,
};
use crate::Result;
use crate::fulfillment_execution::task_access::{TaskPurpose, require_assignee};

#[derive(Clone, Copy)]
enum CommandKind {
    Activity,
    Complete,
}
#[async_trait]
trait TaskCommandPort: Send + Sync {
    type Identity: Send + Sync;
    async fn load_open(
        &self,
        object_type: &str,
        object_id: &str,
        executor: &mut dyn Executor,
    ) -> Result<WorkItem>;
    async fn authorize(
        &self,
        identity: &Self::Identity,
        task: &WorkItem,
        actor_id: &str,
        executor: &mut dyn Executor,
    ) -> Result<()>;
    async fn update(&self, task: &mut WorkItem, executor: &mut dyn Executor) -> Result<()>;
}
struct MongoTaskCommand<'a> {
    db: &'a Database,
}
#[async_trait]
impl TaskCommandPort for MongoTaskCommand<'_> {
    type Identity = SharedRbacService;
    async fn load_open(
        &self,
        object_type: &str,
        object_id: &str,
        executor: &mut dyn Executor,
    ) -> Result<WorkItem> {
        load_single_open_task(self.db, object_type, object_id, executor).await
    }
    async fn authorize(
        &self,
        identity: &SharedRbacService,
        task: &WorkItem,
        actor_id: &str,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        let actor = AuditActor::new(actor_id.into(), actor_id.into(), AccountKind::Admin);
        require_assignee(self.db, identity, &actor, task, TaskPurpose::Fulfillment, executor).await
    }
    async fn update(&self, task: &mut WorkItem, executor: &mut dyn Executor) -> Result<()> {
        if task.status == WorkItemStatus::Open {
            task.set_links(
                work_item_links::capture(
                    self.db,
                    &task.business_object_type,
                    &task.business_object_id,
                    executor,
                )
                .await?,
            )?;
        }
        self.db.work_items().update(task, executor).await?;
        Ok(())
    }
}
/// 同一执行器先核验当前任务指定人，再同步活动或完成事实并写回。
async fn execute<P: TaskCommandPort>(
    port: &P,
    identity_factory: &(dyn Fn() -> P::Identity + Send + Sync),
    object: FulfillmentTaskObject<'_>,
    actor_id: &str,
    kind: CommandKind,
    executor: &mut dyn Executor,
) -> Result<()> {
    let mut task =
        port.load_open(object.business_object_type(), object.business_object_id(), executor).await?;
    ensure_task_matches_frozen_identity(&task, &object)?;
    let identity = identity_factory();
    port.authorize(&identity, &task, actor_id, executor).await?;
    match kind {
        CommandKind::Activity => {
            task.record_activity(Instant::now())?;
            task.subject_version = object.subject_version();
        },
        CommandKind::Complete => task.complete_by_domain_command(actor_id, Instant::now())?,
    }
    port.update(&mut task, executor).await?;
    Ok(())
}
/// 在调用方执行器内记录履约任务活动。
///
/// 先核验当前任务指定人及完整资格，再形成活动事实并写回。
///
/// # 参数
/// * `db` - 业务数据库。
/// * `object` - 冻结责任身份的履约对象。
/// * `actor_id` - 当前操作人。
/// * `executor` - 调用方事务执行器。
///
/// # 返回
/// 活动写入成功时返回。
///
/// # 错误
/// 开放任务缺失或重复、责任身份不一致、操作人无执行权限，或领域事实与写入失败时返回错误。
pub(super) async fn record(
    db: &Database,
    identity_factory: &FulfillmentIdentityFactory,
    object: FulfillmentTaskObject<'_>,
    actor_id: &str,
    executor: &mut dyn Executor,
) -> Result<()> {
    execute(
        &MongoTaskCommand { db },
        identity_factory.as_ref(),
        object,
        actor_id,
        CommandKind::Activity,
        executor,
    )
    .await
}
/// 在调用方执行器内完成履约执行任务。
///
/// 先核验当前任务指定人及完整资格，再形成完成事实并写回。
///
/// # 参数
/// * `db` - 业务数据库。
/// * `object` - 冻结责任身份的履约对象。
/// * `actor_id` - 当前操作人。
/// * `executor` - 调用方事务执行器。
///
/// # 返回
/// 完成事实写入成功时返回。
///
/// # 错误
/// 开放任务缺失或重复、责任身份不一致、操作人无执行权限，或领域事实与写入失败时返回错误。
pub(super) async fn complete(
    db: &Database,
    identity_factory: &FulfillmentIdentityFactory,
    object: FulfillmentTaskObject<'_>,
    actor_id: &str,
    executor: &mut dyn Executor,
) -> Result<()> {
    execute(
        &MongoTaskCommand { db },
        identity_factory.as_ref(),
        object,
        actor_id,
        CommandKind::Complete,
        executor,
    )
    .await
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use erp_workflow::entity::work_item::WorkItemStatus;

    use super::super::fixtures::{TestExecutor, delivery, task};
    use super::*;
    use crate::Error;

    #[derive(Default)]
    struct Recorded {
        calls: Vec<&'static str>,
        identities: Vec<usize>,
        status_at_authorization: Option<WorkItemStatus>,
        written: Option<WorkItem>,
    }
    struct RecordingPort {
        executor: usize,
        task: WorkItem,
        fail_at: Option<usize>,
        recorded: Mutex<Recorded>,
    }
    impl RecordingPort {
        fn fresh_identity(&self) -> usize {
            let mut state = self.recorded.lock().unwrap();
            state.calls.push("identity");
            let identity = state.identities.len() + 1;
            state.identities.push(identity);
            identity
        }

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
        type Identity = usize;
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
            identity: &usize,
            task: &WorkItem,
            actor_id: &str,
            executor: &mut dyn Executor,
        ) -> Result<()> {
            self.record("authorize", executor)?;
            assert_eq!(self.recorded.lock().unwrap().identities.last(), Some(identity));
            assert_eq!(actor_id, "buyer-1");
            self.recorded.lock().unwrap().status_at_authorization = Some(task.status);
            Ok(())
        }
        async fn update(&self, task: &mut WorkItem, executor: &mut dyn Executor) -> Result<()> {
            self.record("update", executor)?;
            self.recorded.lock().unwrap().written = Some(task.clone());
            Ok(())
        }
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
            fail_at,
            recorded: Mutex::default(),
        };
        let delivery = delivery();
        let result = execute(
            &port,
            &|| port.fresh_identity(),
            FulfillmentTaskObject::Delivery(&delivery),
            actor,
            kind,
            &mut executor,
        )
        .await;
        assert_eq!(executor.marker, 79);
        (result, port.recorded.into_inner().unwrap())
    }
    /// 连续保存草稿时同步任务关联版本，保留开放状态、当前责任人与任务身份。
    #[tokio::test]
    async fn saving_draft_keeps_open_task_aligned_with_document_version() {
        let mut current = task();
        let original_id = current.base.id.clone();
        let original_owner = current.owner_user_id.clone();
        let mut started_at = None;
        for version in [2, 3] {
            let mut executor = TestExecutor { marker: 79 };
            let port = RecordingPort {
                executor: (&mut executor as *mut TestExecutor) as usize,
                task: current,
                fail_at: None,
                recorded: Mutex::default(),
            };
            let mut document = delivery();
            document.base.version = version;
            execute(
                &port,
                &|| port.fresh_identity(),
                FulfillmentTaskObject::Delivery(&document),
                "buyer-1",
                CommandKind::Activity,
                &mut executor,
            )
            .await
            .unwrap();
            let recorded = port.recorded.into_inner().unwrap();
            assert_eq!(recorded.calls, ["load", "identity", "authorize", "update"]);
            assert_eq!(recorded.identities, [1]);
            current = recorded.written.unwrap();
            assert!(current.matches_subject_version(&version.to_string()));
            assert_eq!(current.status, WorkItemStatus::Open);
            assert_eq!(current.base.id, original_id);
            assert_eq!(current.owner_user_id, original_owner);
            assert!(current.completed_at.is_none());
            assert!(current.completed_by.is_none());
            if let Some(first) = started_at {
                assert_eq!(current.started_at, Some(first));
            }
            started_at = current.started_at;
        }
    }

    /// 活动与完成均先核验当前任务指定人，再改变任务并写回同一执行器。
    #[tokio::test]
    async fn task_command_authorizes_before_mutating_and_preserves_executor() {
        for (kind, status) in [
            (CommandKind::Activity, WorkItemStatus::Open),
            (CommandKind::Complete, WorkItemStatus::Completed),
        ] {
            let (result, recorded) = invoke(kind, task(), "buyer-1", None).await;
            result.unwrap();
            assert_eq!(recorded.calls, ["load", "identity", "authorize", "update"]);
            assert_eq!(recorded.identities, [1]);
            assert_eq!(recorded.status_at_authorization, Some(WorkItemStatus::Open));
            assert_eq!(recorded.written.unwrap().status, status);
        }
    }
    /// 读取、权限重验和写入任一步失败均停止后续步骤，保留原错误。
    #[tokio::test]
    async fn task_command_stops_at_every_provider_failure() {
        let order = ["load", "identity", "authorize", "update"];
        for kind in [CommandKind::Activity, CommandKind::Complete] {
            for index in [0, 2, 3] {
                let (result, recorded) = invoke(kind, task(), "buyer-1", Some(index)).await;
                assert!(
                    matches!(result,Err(Error::ConflictError(message)) if message==format!("task failure {index}"))
                );
                assert_eq!(recorded.calls, order[..=index]);
                assert_eq!(recorded.identities.len(), usize::from(index > 0));
                assert!(recorded.written.is_none());
            }
        }
    }

    #[tokio::test]
    async fn next_activity_or_completion_creates_fresh_identity_after_authorization_failure() {
        for kind in [CommandKind::Activity, CommandKind::Complete] {
            let mut executor = TestExecutor { marker: 79 };
            let mut port = RecordingPort {
                executor: (&mut executor as *mut TestExecutor) as usize,
                task: task(),
                fail_at: Some(2),
                recorded: Mutex::default(),
            };
            let document = delivery();
            assert!(matches!(
                execute(&port, &|| port.fresh_identity(), FulfillmentTaskObject::Delivery(&document), "buyer-1", kind, &mut executor).await,
                Err(Error::ConflictError(message)) if message == "task failure 2"
            ));
            assert!(port.recorded.lock().unwrap().written.is_none());
            port.fail_at = None;
            execute(
                &port,
                &|| port.fresh_identity(),
                FulfillmentTaskObject::Delivery(&document),
                "buyer-1",
                kind,
                &mut executor,
            )
            .await
            .unwrap();
            let recorded = port.recorded.lock().unwrap();
            assert_eq!(recorded.identities, [1, 2]);
            assert_eq!(
                recorded.calls,
                ["load", "identity", "authorize", "load", "identity", "authorize", "update"]
            );
            assert!(recorded.written.is_some());
            assert_eq!(executor.marker, 79);
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
        assert!(recorded.identities.is_empty());
        assert!(recorded.written.is_none());
    }
}
