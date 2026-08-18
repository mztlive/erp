//! 旧 INTERNAL 审批运行时。P3-RUNTIME 起稳定失败关闭。
//!
//! 目标编排只写 `approval::execution/**`。本文件仅保留旧公开符号，
//! 使现有 HTTP 与销售提交调用方继续编译，但不得再产生旧步骤状态机语义。

use std::{future::Future, pin::Pin, sync::Arc};

use mongodb::Database as MongoDatabase;

use crate::errors::{Error, Result};

use super::{
    action::ApprovalDomainActionPort,
    dto::{
        ApprovalRuntimeView, BlockedApprovalListParams, BlockedApprovalPage, BlockedApprovalView,
        CancelApprovalCommand, RecoverApprovalCommand, StartApprovalCommand, SubmitDecisionCommand,
    },
    resolver::ApprovalAssigneeResolver,
    scope::ApprovalManagementScope,
};

/// 稳定运行时端口的异步返回类型。
pub type ApprovalRuntimeFuture<'a> = Pin<Box<dyn Future<Output = Result<ApprovalRuntimeView>> + Send + 'a>>;

/// 审批业务只依赖的稳定运行时端口。
///
/// 四个方法的请求合同不暴露 INTERNAL/BPM 差异；当前实现一律失败关闭。
pub trait ApprovalRuntimePort: Send + Sync {
    /// 启动已注册的审批定义。
    fn start_approval<'a>(&'a self, command: StartApprovalCommand) -> ApprovalRuntimeFuture<'a>;

    /// 提交当前唯一活动步骤的正式决定。
    fn submit_decision<'a>(&'a self, command: SubmitDecisionCommand) -> ApprovalRuntimeFuture<'a>;

    /// 取消仍满足领域撤回规则的审批。
    fn cancel_approval<'a>(&'a self, command: CancelApprovalCommand) -> ApprovalRuntimeFuture<'a>;

    /// 旧 `RETRY_CURRENT_STEP` 恢复入口。已失败关闭。
    fn recover_approval<'a>(&'a self, command: RecoverApprovalCommand) -> ApprovalRuntimeFuture<'a>;
}

/// ERP 内部 MongoDB 事务审批运行时。公开命令已失败关闭。
#[derive(Clone)]
pub struct InternalApprovalRuntime {
    db: MongoDatabase,
    action_port: Arc<dyn ApprovalDomainActionPort>,
    resolver: ApprovalAssigneeResolver,
}

impl InternalApprovalRuntime {
    /// 创建绑定强类型领域动作端口的 INTERNAL 运行时。
    ///
    /// # 返回
    /// 返回仍可注入 `AppState` 的运行时实例；公开命令不会写入旧步骤状态机。
    pub fn new(db: MongoDatabase, action_port: Arc<dyn ApprovalDomainActionPort>) -> Self {
        Self {
            resolver: ApprovalAssigneeResolver::new(db.clone()),
            db,
            action_port,
        }
    }

    /// 旧事务内启动入口。已失败关闭。
    ///
    /// # 错误
    /// 始终返回业务逻辑错误。
    pub async fn start_approval_in_transaction(
        &self,
        command: StartApprovalCommand,
        _executor: &mut dyn database::Executor,
    ) -> Result<ApprovalRuntimeView> {
        let _ = (&command, &self.db, &self.action_port, &self.resolver);
        Err(closed("start_approval"))
    }

    /// 旧阻塞列表。已失败关闭。
    ///
    /// # 错误
    /// 始终返回业务逻辑错误。
    pub async fn blocked_approvals(
        &self,
        _params: &BlockedApprovalListParams,
        _owner_organization_ids: Option<&[String]>,
        _recovery_scope: Option<&ApprovalManagementScope>,
    ) -> Result<BlockedApprovalPage> {
        Err(closed("blocked_approvals"))
    }

    /// 旧单条阻塞查询。已失败关闭。
    ///
    /// # 错误
    /// 始终返回业务逻辑错误。
    pub async fn blocked_approval(
        &self,
        _approval_instance_id: &str,
        _scope: &ApprovalManagementScope,
        _can_recover: bool,
    ) -> Result<BlockedApprovalView> {
        Err(closed("blocked_approval"))
    }

    /// 旧管理范围重验。已失败关闭。
    ///
    /// # 错误
    /// 始终返回业务逻辑错误。
    pub async fn ensure_approval_in_management_scope(
        &self,
        _approval_instance_id: &str,
        _scope: &ApprovalManagementScope,
    ) -> Result<()> {
        Err(closed("ensure_approval_in_management_scope"))
    }
}

impl ApprovalRuntimePort for InternalApprovalRuntime {
    fn start_approval<'a>(&'a self, command: StartApprovalCommand) -> ApprovalRuntimeFuture<'a> {
        Box::pin(async move {
            self.start_approval_in_transaction(command, &mut database::NoTransaction)
                .await
        })
    }

    fn submit_decision<'a>(&'a self, _command: SubmitDecisionCommand) -> ApprovalRuntimeFuture<'a> {
        Box::pin(async move { Err(closed("submit_decision")) })
    }

    fn cancel_approval<'a>(&'a self, _command: CancelApprovalCommand) -> ApprovalRuntimeFuture<'a> {
        Box::pin(async move { Err(closed("cancel_approval")) })
    }

    fn recover_approval<'a>(&'a self, _command: RecoverApprovalCommand) -> ApprovalRuntimeFuture<'a> {
        Box::pin(async move { Err(closed("recover_approval")) })
    }
}

/// 旧运行时失败关闭错误。
fn closed(command: &str) -> Error {
    let _ = super::action::recovery_action_context
        as fn(
            &entities::approval::ApprovalInstance,
            &entities::approval::ApprovalStepInstance,
            Option<&entities::work_item::WorkItem>,
            &str,
            &str,
            &str,
        ) -> super::action::ApprovalActionContext;
    let _ = super::scope::ensure_recovery_authorization;
    let _ = super::registry::definition;
    let _ = super::registry::start_action;
    let _ = super::registry::cancel_action;
    let _ = super::registry::recovery_validation_action;
    let _ = super::registry::decision_action;
    Error::BusinessLogicError(format!(
        "旧审批运行时已失败关闭，命令 {command} 只能经 approval::execution 编排"
    ))
}

#[cfg(test)]
mod tests {
    use super::{closed, InternalApprovalRuntime};
    use crate::approval::FailClosedApprovalActionPort;
    use mongodb::Database;

    /// 旧公开命令必须失败关闭，且文案不含 RETRY_CURRENT_STEP 作为可执行动作。
    #[test]
    fn runtime_old_commands_fail_closed() {
        let message = closed("recover_approval").to_string();
        assert!(message.contains("旧审批运行时已失败关闭"));
        assert!(!message.contains("RETRY_CURRENT_STEP"));
        let _ = std::any::type_name::<InternalApprovalRuntime>();
        let _ = std::any::type_name::<FailClosedApprovalActionPort>();
        let _ = std::any::type_name::<Database>();
    }
}
