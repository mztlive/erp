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
    /// 财务命令在调用方事务内验证当前责任人能否决定原正式任务，保留工作流的权限、冲突和存储错误。
    async fn ensure_domain_decision_access(
        &self,
        actor: &AuditActor,
        task: &WorkItem,
        executor: &mut dyn Executor,
    ) -> erp_workflow::Result<()>;

    /// 财务揭示命令在原事务内先读取经办人的当前任务范围，再查具体任务访问权。
    /// 资格撤销、范围不足或存储失败时保留工作流领域错误。
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
    ///
    /// 各参数为应用共享身份、审批运行、资金来源访问及任务能力。返回无I/O的财务能力集合。构造时不返回错误；业务校验仍在原调用位置执行。
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
