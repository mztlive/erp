//! 工作台审批列表消费的运行读取接口，保留运行服务拥有的授权与快照边界。

use application_core::AuditActor;
use erp_workflow::service::approval::execution::runtime_service::{
    ApprovalRuntimeService, RuntimeInstanceListCursor, RuntimeInstanceListItem, RuntimeInstanceListPage,
    RuntimeInstanceListQuery,
};
use erp_workflow::{Result, WorkflowAuthorizationPort};
use futures_util::future::BoxFuture;
use persistence_core::Executor;

/// 仅提供已授权的审批实例页；不暴露决定、撤回或迁移能力。
pub trait ApprovalInstanceReadPort: Send + Sync {
    /// 依据原运行服务读取一页实例，保留其独立读取边界。
    ///
    /// # 参数
    /// * `query` - 原实例列表查询。
    ///
    /// # 返回
    /// 返回授权后的实例、总数及游标。
    ///
    /// # 错误
    /// 原参数、身份、范围和仓储错误原样传播。
    fn instance_list<'a>(
        &'a self,
        actor: &'a AuditActor,
        query: RuntimeInstanceListQuery,
    ) -> BoxFuture<'a, Result<RuntimeInstanceListPage>>;

    /// 读取审批实例时，在调用方事务快照中读取本人发起审批的一批授权候选。
    ///
    /// # 参数
    /// * `query` - 仅允许本人发起范围的查询。
    /// * `executor` - 调用方原执行器。
    ///
    /// # 返回
    /// 返回原顺序的候选页与续读游标。
    ///
    /// # 错误
    /// 原视图、身份、范围和仓储错误原样传播。
    fn started_instance_page<'a>(
        &'a self,
        actor: &'a AuditActor,
        query: &'a RuntimeInstanceListQuery,
        executor: &'a mut dyn Executor,
    ) -> BoxFuture<'a, Result<(Vec<RuntimeInstanceListItem>, Option<RuntimeInstanceListCursor>)>>;
}

impl<A: WorkflowAuthorizationPort> ApprovalInstanceReadPort for ApprovalRuntimeService<A> {
    fn instance_list<'a>(
        &'a self,
        actor: &'a AuditActor,
        query: RuntimeInstanceListQuery,
    ) -> BoxFuture<'a, Result<RuntimeInstanceListPage>> {
        Box::pin(ApprovalRuntimeService::instance_list(self, actor, query))
    }

    fn started_instance_page<'a>(
        &'a self,
        actor: &'a AuditActor,
        query: &'a RuntimeInstanceListQuery,
        executor: &'a mut dyn Executor,
    ) -> BoxFuture<'a, Result<(Vec<RuntimeInstanceListItem>, Option<RuntimeInstanceListCursor>)>> {
        Box::pin(ApprovalRuntimeService::started_instance_page_with(self, actor, query, executor))
    }
}
