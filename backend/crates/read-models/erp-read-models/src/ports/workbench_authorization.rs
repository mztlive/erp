//! 工作台只消费任务 DataScope、权限代码和授权版本，不代行关联材料授权。
use application_core::AuditActor;
use erp_core::AccountKind;
use erp_workflow::{Result, WorkflowAuthorizationPort};
use futures_util::future::BoxFuture;
use persistence_core::Executor;

/// 工作台统一任务授权读取合同。
pub trait WorkbenchAuthorizationReadPort: Send + Sync {
    /// 读取任务动作经 DataScope 解析的负责人范围。
    /// # 参数
    /// `actor` 为真实身份，`action` 为 list、detail 或 manage，`executor` 为当前执行器。
    /// # 返回
    /// 公司范围为 None，空集合保持无访问权。
    /// # 错误
    /// 权限拒绝、未装配或范围读取失败时返回错误。
    fn task_scope_owners<'a>(
        &'a self,
        actor: &'a AuditActor,
        action: &'a str,
        executor: &'a mut dyn Executor,
    ) -> BoxFuture<'a, Result<Option<Vec<String>>>>;

    /// 获取包含策略、组织和关系有效期的身份授权版本。
    /// # 参数
    /// `actor` 为真实身份，`executor` 为调用方执行器。
    /// # 返回
    /// 可用于首末复验和跨页校验的版本。
    /// # 错误
    /// 账号失效、未装配或读取失败时拒绝。
    fn queue_scope_version<'a>(
        &'a self,
        actor: &'a AuditActor,
        executor: &'a mut dyn Executor,
    ) -> BoxFuture<'a, Result<String>>;

    /// 返回当前账号的有效权限，用于显示允许动作。
    /// # 参数
    /// `account_kind` 和 `account_id` 为服务端认证身份。
    /// # 返回
    /// 已授予的权限代码。
    /// # 错误
    /// 权限读取失败时返回错误。
    fn permission_codes<'a>(
        &'a self,
        account_kind: AccountKind,
        account_id: &'a str,
    ) -> BoxFuture<'a, Result<Vec<String>>>;
}

impl<A: WorkflowAuthorizationPort> WorkbenchAuthorizationReadPort for A {
    fn task_scope_owners<'a>(
        &'a self,
        actor: &'a AuditActor,
        action: &'a str,
        executor: &'a mut dyn Executor,
    ) -> BoxFuture<'a, Result<Option<Vec<String>>>> {
        Box::pin(WorkflowAuthorizationPort::task_scope_owners(self, actor, action, executor))
    }

    fn queue_scope_version<'a>(
        &'a self,
        actor: &'a AuditActor,
        executor: &'a mut dyn Executor,
    ) -> BoxFuture<'a, Result<String>> {
        Box::pin(WorkflowAuthorizationPort::queue_scope_version(self, actor, executor))
    }

    fn permission_codes<'a>(
        &'a self,
        account_kind: AccountKind,
        account_id: &'a str,
    ) -> BoxFuture<'a, Result<Vec<String>>> {
        Box::pin(WorkflowAuthorizationPort::permission_codes(self, account_kind, account_id))
    }
}
