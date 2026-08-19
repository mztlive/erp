//! 审批运行时与业务领域强类型命令之间的事务内端口。

use std::{future::Future, pin::Pin};

use database::Executor;

use crate::errors::{Error, Result};

use super::policy::ApprovalDomainAction;

/// 强类型领域动作执行所需的冻结审批上下文。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ApprovalActionContext {
    /// 审批流程实例 ID。
    pub approval_process_instance_id: String,
    /// 当前节点执行 ID；启动前置动作允许为空。
    pub approval_node_execution_id: Option<String>,
    /// 当前待办 ID；启动或解析阻塞时允许为空。
    pub work_item_id: Option<String>,
    /// 业务对象类型。
    pub business_object_type: String,
    /// 业务对象 ID。
    pub business_object_id: String,
    /// 冻结业务版本。
    pub subject_version: String,
    /// 已认证操作人。
    pub actor_id: String,
    /// 正式决定或取消原因。
    pub reason: Option<String>,
    /// 调用方提供的幂等键。
    pub idempotency_key: String,
}

/// 事务内领域动作的异步返回类型。
pub type ApprovalActionFuture<'a> = Pin<Box<dyn Future<Output = Result<()>> + Send + 'a>>;

/// 审批运行时调用所属业务域的强类型命令端口。
///
/// 实现必须只通过领域 Repository 写正式事实，并复用运行时传入的执行器；
/// 禁止另开事务、调用 HTTP Handler 或根据字符串动态执行代码。
pub trait ApprovalDomainActionPort: Send + Sync {
    /// 在审批运行时的唯一外层事务内执行已注册动作。
    ///
    /// # 错误
    /// 业务版本、状态、权限或领域不变量不满足时返回稳定服务错误，外层事务整体回滚。
    fn execute<'a>(
        &'a self,
        action: ApprovalDomainAction,
        context: &'a ApprovalActionContext,
        executor: &'a mut dyn Executor,
    ) -> ApprovalActionFuture<'a>;
}

/// 未绑定领域命令时使用的失败关闭端口。
#[derive(Debug, Default)]
pub struct FailClosedApprovalActionPort;

impl ApprovalDomainActionPort for FailClosedApprovalActionPort {
    fn execute<'a>(
        &'a self,
        action: ApprovalDomainAction,
        _context: &'a ApprovalActionContext,
        _executor: &'a mut dyn Executor,
    ) -> ApprovalActionFuture<'a> {
        Box::pin(async move {
            Err(Error::BusinessLogicError(format!(
                "审批领域动作 {} 尚未绑定，已按安全策略拒绝推进",
                action.as_str()
            )))
        })
    }
}

#[cfg(test)]
mod tests {
    use database::NoTransaction;

    use super::*;

    #[tokio::test]
    async fn unbound_domain_action_fails_closed() {
        let context = ApprovalActionContext {
            approval_process_instance_id: "instance-1".to_string(),
            approval_node_execution_id: None,
            work_item_id: None,
            business_object_type: "stock_adjustment".to_string(),
            business_object_id: "adj-1".to_string(),
            subject_version: "1".to_string(),
            actor_id: "user-1".to_string(),
            reason: None,
            idempotency_key: "request-1".to_string(),
        };
        let error = FailClosedApprovalActionPort
            .execute(
                ApprovalDomainAction::StockAdjustmentSubmit,
                &context,
                &mut NoTransaction,
            )
            .await
            .unwrap_err();
        assert!(matches!(error, Error::BusinessLogicError(_)));
    }
}
