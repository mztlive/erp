//! 财务任务生产时的原子责任分配合同，沿用调用方事务及当前轮次。

use async_trait::async_trait;
use persistence_core::Executor;

use crate::entity::work_item::{FinanceResponsibilityOperation, WorkItemType};
use crate::service::work_item::{ResolvedFinanceResponsibility, WorkItemService};
use crate::{Result, WorkflowAuthorizationPort};

/// 执行业务任务的真实负责人资格核验与责任池推进。
#[async_trait]
pub trait FinanceResponsibilityPort: Send + Sync {
    /// 在调用方事务内匹配财务负责人并推进一次轮次。
    /// 返回已经冻结的负责人及责任键。
    ///
    /// 零或多条规则、资格失效、版本冲突或持久化失败时返回原错误。
    ///
    /// `operation`、`counterparty` 与 `executor` 为原操作、往来方和执行器。
    async fn resolve_finance_responsibility(
        &self,
        operation: FinanceResponsibilityOperation,
        counterparty: &str,
        executor: &mut dyn Executor,
    ) -> Result<ResolvedFinanceResponsibility>;

    /// 按退货任务类型核验财务负责人及退货处理权限。
    /// 返回已核验退货资格的负责人和责任键。
    ///
    /// 责任解析或退货处理权限失败时返回原错误。
    ///
    /// `kind`、`counterparty` 与 `executor` 为原任务类型、往来方及执行器。
    async fn resolve_return_finance_responsibility(
        &self,
        kind: WorkItemType,
        counterparty: &str,
        executor: &mut dyn Executor,
    ) -> Result<ResolvedFinanceResponsibility>;
}

#[async_trait]
impl<A: WorkflowAuthorizationPort> FinanceResponsibilityPort for WorkItemService<A> {
    async fn resolve_finance_responsibility(
        &self,
        operation: FinanceResponsibilityOperation,
        counterparty: &str,
        executor: &mut dyn Executor,
    ) -> Result<ResolvedFinanceResponsibility> {
        WorkItemService::resolve_finance_responsibility(self, operation, counterparty, executor).await
    }

    async fn resolve_return_finance_responsibility(
        &self,
        kind: WorkItemType,
        counterparty: &str,
        executor: &mut dyn Executor,
    ) -> Result<ResolvedFinanceResponsibility> {
        WorkItemService::resolve_return_finance_responsibility(self, kind, counterparty, executor).await
    }
}
