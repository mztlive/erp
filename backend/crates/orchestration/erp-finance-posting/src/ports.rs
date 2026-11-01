//! 财务过账消费的明确应用提供者；不自行构造身份或跨域适配器。

use std::ops::Deref;
use std::sync::Arc;

use application_core::AuditActor;
use async_trait::async_trait;
use erp_approval_runtime::ApprovalRuntimePort;
use erp_identity::SharedRbacService;
use erp_procurement::PurchaseAccess;
use erp_read_models::finance::funds_scope::FundsAccess;
use erp_read_models::finance::payment_request::PurchasePaymentRequestReadService;
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
        let access = self.actor_access(actor).await?;
        self.ensure_item_access_with_executor(task, &access, executor).await
    }
}

type ReceivableFactory = Arc<dyn Fn() -> FinancePostingPorts + Send + Sync>;
type FreshTaskFactory = Arc<dyn Fn() -> Arc<dyn FinanceTaskPort> + Send + Sync>;

/// 财务过账需要的身份、审批运行、来源访问和正式任务提供者。
#[derive(Clone)]
pub struct FinancePostingPorts {
    pub(crate) identity: SharedRbacService,
    pub(crate) transaction_identity: SharedRbacService,
    pub(crate) runtime: Arc<dyn ApprovalRuntimePort>,
    pub(crate) purchase: PurchaseAccess,
    pub(crate) funds: FundsAccess,
    pub(crate) payment_request_read: PurchasePaymentRequestReadService,
    pub(crate) tasks: Arc<dyn FinanceTaskPort>,
    receivable_factory: Option<ReceivableFactory>,
    fresh_tasks_factory: Option<FreshTaskFactory>,
}

impl FinancePostingPorts {
    /// 注入所有实际提供者，并保持付款局部事务权限锁独立。
    /// # 参数
    /// 各参数为应用共享身份、付款局部事务身份、审批运行、采购/资金访问、付款申请读取和任务能力。
    /// # 返回
    /// 返回无I/O的财务能力集合。
    /// # 错误
    /// 构造时不返回错误；业务校验仍在原调用位置执行。
    pub fn new(
        identity: SharedRbacService,
        transaction_identity: SharedRbacService,
        runtime: Arc<dyn ApprovalRuntimePort>,
        purchase: PurchaseAccess,
        funds: FundsAccess,
        payment_request_read: PurchasePaymentRequestReadService,
        tasks: Arc<dyn FinanceTaskPort>,
    ) -> Self {
        Self {
            identity,
            transaction_identity,
            runtime,
            purchase,
            funds,
            payment_request_read,
            tasks,
            receivable_factory: None,
            fresh_tasks_factory: None,
        }
    }

    /// 注入客户往来内部新命令所需的独立能力构造器。
    /// # 参数
    /// `factory` 每次必须创建新的局部身份实例，不能捕获共享应用身份。
    /// # 返回
    /// 返回已具备局部命令再创建能力的提供者集合。
    /// # 错误
    /// 不返回错误，构造器不执行数据库或外部服务I/O。
    pub fn with_receivable_factory(mut self, factory: ReceivableFactory) -> Self {
        self.receivable_factory = Some(factory);
        self
    }

    /// 为原客户往来内部新命令重新创建独立身份和审批提供者。
    /// # 参数
    /// 无。
    /// # 返回
    /// 返回与当前实例和应用全局权限状态隔离的新提供者集合。
    /// # 错误
    /// 应用未装配局部构造器时失败关闭，不以共享身份代替。
    pub fn fresh_receivable(&self) -> crate::Result<Self> {
        self.receivable_factory
            .as_ref()
            .map(|factory| factory())
            .ok_or_else(|| crate::Error::Internal("客户往来局部身份构造器未装配".into()))
    }

    /// 注入需要独立身份的原任务资格检查构造器。
    /// # 参数
    /// `factory` 每次创建独立的局部身份和正式任务资格提供者。
    /// # 返回
    /// 返回已装配独立任务资格能力的财务提供者集合。
    /// # 错误
    /// 不返回错误，构造器不得执行I/O。
    pub fn with_fresh_tasks_factory(mut self, factory: FreshTaskFactory) -> Self {
        self.fresh_tasks_factory = Some(factory);
        self
    }

    /// 在原检查位置创建独立身份的任务资格提供者。
    /// # 参数
    /// 无。
    /// # 返回
    /// 返回与调用者和应用全局权限状态隔离的任务能力。
    /// # 错误
    /// 未装配独立构造器时失败关闭，不以调用者身份替代。
    pub(crate) fn fresh_tasks(&self) -> crate::Result<Arc<dyn FinanceTaskPort>> {
        self.fresh_tasks_factory
            .as_ref()
            .map(|factory| factory())
            .ok_or_else(|| crate::Error::Internal("财务局部任务资格构造器未装配".into()))
    }
}

impl Deref for FinancePostingPorts {
    type Target = SharedRbacService;

    fn deref(&self) -> &Self::Target {
        &self.identity
    }
}
