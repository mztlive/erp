//! 导入确认使用的工作项授权能力，由应用注入实际工作流服务。

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
    /// `id`：正式工作项身份；`executor`：导入查询持有的原快照执行器。
    ///
    /// 返回按当前责任资格生成的安全任务视图。
    ///
    /// 任务不存在、无权查看或授权读取失败时返回工作流错误。
    async fn authorize_work_item(
        &self,
        id: &str,
        actor: &AuditActor,
        executor: &mut dyn Executor,
    ) -> WorkflowResult<AuthorizedWorkItem>;

    /// 在原确认事务中重验当前决定资格。
    ///
    /// `executor`：导入查询持有的原快照执行器；`item`：已核对身份、版本和责任的正式工作项；`executor`：确认命令沿用的同一事务执行器。
    ///
    /// 当前账号、权限和对象资格仍有效时成功。
    ///
    /// 账号失效、责任或权限不满足，或事实查询失败时返回工作流错误。
    async fn ensure_domain_decision_access(
        &self,
        actor: &AuditActor,
        item: &WorkItem,
        executor: &mut dyn Executor,
    ) -> WorkflowResult<()>;

    /// 在原写事务证明正式任务仍分配给当前账号，业务状态由调用方核对。
    ///
    /// actor 为当前操作人，id 为精确任务 ID，executor 为调用方事务。
    ///
    /// 当前任务分配及详情范围均满足时成功。
    ///
    /// 账号、任务分配、范围或身份设施失效时拒绝。
    async fn require_assigned(
        &self,
        actor: &AuditActor,
        id: &str,
        executor: &mut dyn Executor,
    ) -> WorkflowResult<()>;
}

#[async_trait]
impl<A> ImportWorkItems for WorkItemService<A>
where
    A: WorkflowAuthorizationPort + Send + Sync + 'static,
{
    async fn authorize_work_item(
        &self,
        id: &str,
        actor: &AuditActor,
        executor: &mut dyn Executor,
    ) -> WorkflowResult<AuthorizedWorkItem> {
        self.authorize_work_item_with_executor(id, actor, executor).await
    }

    async fn ensure_domain_decision_access(
        &self,
        actor: &AuditActor,
        item: &WorkItem,
        executor: &mut dyn Executor,
    ) -> WorkflowResult<()> {
        self.ensure_domain_decision_access(actor, item, executor).await
    }

    async fn require_assigned(
        &self,
        actor: &AuditActor,
        id: &str,
        executor: &mut dyn Executor,
    ) -> WorkflowResult<()> {
        self.authorization.require_assigned(actor, id, executor).await.map(|_| ())
    }
}
