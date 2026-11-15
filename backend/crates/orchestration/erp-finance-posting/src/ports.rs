//! 财务过账消费的明确应用提供者；不自行构造身份或跨域适配器。

use std::ops::Deref;
use std::sync::Arc;

use application_core::AuditActor;
use async_trait::async_trait;
use erp_approval_runtime::ApprovalRuntimePort;
use erp_identity::{AuthorizationService, SharedRbacService};
use erp_read_models::finance::funds_scope::FundsAccess;
use erp_workflow::entity::work_item::WorkItem;
use erp_workflow::ports::FinanceResponsibilityPort;
use erp_workflow::{WorkItemService, WorkflowAuthorizationPort};
use persistence_core::Executor;

/// 财务过账消费的任务资格；责任规则解析沿用工作流领域合同。
#[async_trait]
pub trait FinanceTaskPort: FinanceResponsibilityPort {
    /// 在调用方事务验证当前责任人的正式任务决定资格。
    /// # 参数
    /// `actor`、`task` 和 `executor` 为原命令身份、任务及执行器。
    /// # 返回
    /// 当前任务决定资格成立时成功。
    /// # 错误
    /// 保留工作流的权限、冲突和存储错误。
    async fn ensure_domain_decision_access(
        &self,
        actor: &AuditActor,
        task: &WorkItem,
        executor: &mut dyn Executor,
    ) -> erp_workflow::Result<()>;

    /// 先读取当前人员任务范围，再在原事务校验具体任务访问权。
    /// # 参数
    /// `actor`、`task` 和 `executor` 为原揭示命令的身份、任务及执行器。
    /// # 返回
    /// 当前任务可读时成功。
    /// # 错误
    /// 资格撤销、范围不足及存储失败保持领域错误。
    async fn ensure_item_access(
        &self,
        actor: &AuditActor,
        task: &WorkItem,
        executor: &mut dyn Executor,
    ) -> erp_workflow::Result<()>;
}

#[async_trait]
impl<A: WorkflowAuthorizationPort> FinanceTaskPort for WorkItemService<A> {
    async fn ensure_domain_decision_access(
        &self,
        actor: &AuditActor,
        task: &WorkItem,
        executor: &mut dyn Executor,
    ) -> erp_workflow::Result<()> {
        WorkItemService::ensure_domain_decision_access(self, actor, task, executor).await
    }

    async fn ensure_item_access(
        &self,
        actor: &AuditActor,
        task: &WorkItem,
        executor: &mut dyn Executor,
    ) -> erp_workflow::Result<()> {
        self.ensure_item_access_with_executor(task, actor, executor).await
    }
}

/// 财务过账需要的身份、审批运行、来源访问和正式任务提供者。
#[derive(Clone)]
pub struct FinancePostingPorts {
    pub(crate) identity: SharedRbacService,
    pub(crate) authorization: Arc<AuthorizationService>,
    pub(crate) runtime: Arc<dyn ApprovalRuntimePort>,
    pub(crate) funds: FundsAccess,
    pub(crate) tasks: Arc<dyn FinanceTaskPort>,
}

impl FinancePostingPorts {
    /// 注入身份、统一来源授权及财务任务事实提供者。
    /// # 参数
    /// 各参数为应用共享身份、审批运行、资金来源访问及任务能力。
    /// # 返回
    /// 返回无I/O的财务能力集合。
    /// # 错误
    /// 构造时不返回错误；业务校验仍在原调用位置执行。
    pub fn new(
        identity: SharedRbacService,
        authorization: Arc<AuthorizationService>,
        runtime: Arc<dyn ApprovalRuntimePort>,
        funds: FundsAccess,
        tasks: Arc<dyn FinanceTaskPort>,
    ) -> Self {
        Self { identity, authorization, runtime, funds, tasks }
    }
}

impl Deref for FinancePostingPorts {
    type Target = SharedRbacService;

    fn deref(&self) -> &Self::Target {
        &self.identity
    }
}
