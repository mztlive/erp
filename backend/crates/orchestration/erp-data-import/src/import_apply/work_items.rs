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
