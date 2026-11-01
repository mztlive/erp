//! 导入确认使用的工作项授权能力，由应用注入实际工作流服务。

use std::sync::Arc;

use application_core::AuditActor;
use async_trait::async_trait;
use erp_workflow::entity::work_item::WorkItem;
use erp_workflow::service::work_item::AuthorizedWorkItem;
use erp_workflow::{Result as WorkflowResult, WorkItemService, WorkflowAuthorizationPort};
use persistence_core::Executor;

/// 查询和决定导入确认时，沿用应用已经装配的责任与对象授权规则。
#[async_trait]
pub trait ImportWorkItems: Send + Sync {
    /// 读取当前操作人可见的工作项及允许动作。
    ///
    /// # 参数
    /// * `id` - 正式工作项身份。
    /// * `actor` - 当前认证操作人。
    ///
    /// # 返回
    /// 返回按当前责任资格生成的安全任务视图。
    ///
    /// # 错误
    /// 任务不存在、无权查看或授权读取失败时返回工作流错误。
    async fn authorize_work_item(&self, id: &str, actor: &AuditActor) -> WorkflowResult<AuthorizedWorkItem>;

    /// 在原确认事务中重验当前决定资格。
    ///
    /// # 参数
    /// * `actor` - 当前认证操作人。
    /// * `item` - 已核对身份、版本和责任的正式工作项。
    /// * `executor` - 确认命令沿用的同一事务执行器。
    ///
    /// # 返回
    /// 当前账号、权限和对象资格仍有效时成功。
    ///
    /// # 错误
    /// 账号失效、责任或权限不满足，或事实查询失败时返回工作流错误。
    async fn ensure_domain_decision_access(
        &self,
        actor: &AuditActor,
        item: &WorkItem,
        executor: &mut dyn Executor,
    ) -> WorkflowResult<()>;
}

/// 确认列表沿用调用方身份；每次完成或重放资格重验使用独立身份。
pub struct ImportWorkItemAccess {
    caller: Arc<dyn ImportWorkItems>,
    decision_factory: Arc<dyn Fn() -> Arc<dyn ImportWorkItems> + Send + Sync>,
}

impl ImportWorkItemAccess {
    /// 绑定查询身份和逐次决定资格服务工厂。
    ///
    /// # 参数
    /// * `caller` - 原确认列表使用的调用方授权服务。
    /// * `decision_factory` - 每次调用创建独立身份的资格服务工厂。
    ///
    /// # 返回
    /// 返回只在原资格重验点创建独立身份的工作项能力。
    ///
    /// # 错误
    /// 构造不执行 I/O，不返回错误。
    pub fn new(
        caller: Arc<dyn ImportWorkItems>,
        decision_factory: Arc<dyn Fn() -> Arc<dyn ImportWorkItems> + Send + Sync>,
    ) -> Self {
        Self { caller, decision_factory }
    }
}

#[async_trait]
impl ImportWorkItems for ImportWorkItemAccess {
    async fn authorize_work_item(&self, id: &str, actor: &AuditActor) -> WorkflowResult<AuthorizedWorkItem> {
        self.caller.authorize_work_item(id, actor).await
    }

    async fn ensure_domain_decision_access(
        &self,
        actor: &AuditActor,
        item: &WorkItem,
        executor: &mut dyn Executor,
    ) -> WorkflowResult<()> {
        (self.decision_factory)().ensure_domain_decision_access(actor, item, executor).await
    }
}

#[async_trait]
impl<A> ImportWorkItems for WorkItemService<A>
where
    A: WorkflowAuthorizationPort + Send + Sync + 'static,
{
    async fn authorize_work_item(&self, id: &str, actor: &AuditActor) -> WorkflowResult<AuthorizedWorkItem> {
        self.authorize_work_item(id, actor).await
    }

    async fn ensure_domain_decision_access(
        &self,
        actor: &AuditActor,
        item: &WorkItem,
        executor: &mut dyn Executor,
    ) -> WorkflowResult<()> {
        self.ensure_domain_decision_access(actor, item, executor).await
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;
    use std::sync::atomic::{AtomicUsize, Ordering};

    use erp_core::AccountKind;
    use erp_core::ids::{LegacyImportBatchId, WorkItemId};
    use erp_workflow::Error;
    use mongodb::ClientSession;

    use super::*;
    use crate::import_apply::factories::confirmation_work_item;

    #[derive(Debug, PartialEq, Eq)]
    struct Call {
        identity: usize,
        actor: String,
        item: String,
    }

    struct RecordingExecutor(usize);

    impl Executor for RecordingExecutor {
        fn session(&mut self) -> Option<&mut ClientSession> {
            self.0 += 1;
            None
        }
    }

    struct Access {
        identity: usize,
        calls: Arc<Mutex<Vec<Call>>>,
    }

    #[async_trait]
    impl ImportWorkItems for Access {
        async fn authorize_work_item(
            &self,
            id: &str,
            actor: &AuditActor,
        ) -> WorkflowResult<AuthorizedWorkItem> {
            self.calls.lock().unwrap().push(Call {
                identity: self.identity,
                actor: actor.id().to_string(),
                item: id.to_string(),
            });
            Err(Error::Forbidden("caller denied".into()))
        }

        async fn ensure_domain_decision_access(
            &self,
            actor: &AuditActor,
            item: &WorkItem,
            executor: &mut dyn Executor,
        ) -> WorkflowResult<()> {
            self.calls.lock().unwrap().push(Call {
                identity: self.identity,
                actor: actor.id().to_string(),
                item: item.base.id.clone(),
            });
            assert!(executor.session().is_none());
            if self.identity == 1 {
                return Err(Error::Rbac("first decision identity unavailable".into()));
            }
            Ok(())
        }
    }

    #[tokio::test]
    async fn decision_failure_does_not_reuse_identity_or_change_caller_and_executor() {
        let calls = Arc::new(Mutex::new(Vec::new()));
        let created = Arc::new(AtomicUsize::new(0));
        let caller = Arc::new(Access { identity: 0, calls: calls.clone() });
        let factory_calls = calls.clone();
        let factory_created = created.clone();
        let access = ImportWorkItemAccess::new(
            caller,
            Arc::new(move || -> Arc<dyn ImportWorkItems> {
                let identity = factory_created.fetch_add(1, Ordering::SeqCst) + 1;
                Arc::new(Access { identity, calls: factory_calls.clone() })
            }),
        );
        let actor = AuditActor::new("actor".into(), "account".into(), AccountKind::Admin);
        let item = confirmation_work_item(
            WorkItemId::new("task"),
            &LegacyImportBatchId::new("batch"),
            "1".into(),
            "SALES",
            actor.id(),
        )
        .unwrap();
        let mut executor = RecordingExecutor(0);

        assert!(matches!(
            access.authorize_work_item("task", &actor).await,
            Err(Error::Forbidden(message)) if message == "caller denied"
        ));
        assert_eq!(created.load(Ordering::SeqCst), 0);
        assert!(matches!(
            access.ensure_domain_decision_access(&actor, &item, &mut executor).await,
            Err(Error::Rbac(message)) if message == "first decision identity unavailable"
        ));
        access.ensure_domain_decision_access(&actor, &item, &mut executor).await.unwrap();
        assert!(matches!(
            access.authorize_work_item("task", &actor).await,
            Err(Error::Forbidden(message)) if message == "caller denied"
        ));
        assert_eq!(created.load(Ordering::SeqCst), 2);
        assert_eq!(executor.0, 2);
        let expected =
            [0, 1, 2, 0].map(|identity| Call { identity, actor: "actor".into(), item: "task".into() });
        assert_eq!(*calls.lock().unwrap(), expected);
    }
}
