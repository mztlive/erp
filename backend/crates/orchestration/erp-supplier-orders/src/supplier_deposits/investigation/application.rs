//! 同一财务事务内的资金应用步骤；异常保留只在完整事务已明确回滚后使用。

use async_trait::async_trait;
use erp_finance::entity::supplier_deposit::{
    FundsAppliedResource, FundsEvidenceInbox, FundsEvidenceInboxState,
};
use persistence_core::Executor;

use super::process::FundsInvestigationRun;
use crate::{Error, Result};

pub(super) enum CashApplication {
    Applied(FundsAppliedResource),
    Review(&'static str),
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum ApplicationMode {
    Complete,
    PreserveDebit,
}

/// 已接收资金证据的领域步骤，调用方为全部步骤提供同一个事务执行器。
#[async_trait]
pub(super) trait FundsApplicationSteps: Send + Sync {
    /// 保存真实资金变化或返回需要核对的来源问题。
    /// # 参数
    /// 原收件证据及调用方事务执行器。
    /// # 返回
    /// 真实资金资源或待核对原因。
    /// # 错误
    /// 来源冲突、记账或持久化失败时拒绝。
    async fn cash(&self, inbox: &FundsEvidenceInbox, executor: &mut dyn Executor) -> Result<CashApplication>;
    /// 将原真实使用核销到已批准的采购应付。
    /// # 参数
    /// 原使用身份和同一事务执行器。
    /// # 返回
    /// 原批准范围内的核销已保存。
    /// # 错误
    /// 来源、批准或应付额度不符合时拒绝。
    async fn settle(&self, usage_id: &str, executor: &mut dyn Executor) -> Result<()>;
    /// 同步原付款任务；失败须让正常资金应用整体回滚。
    /// # 参数
    /// 原使用身份和同一事务执行器。
    /// # 返回
    /// 原付款任务已与真实核销同步。
    /// # 错误
    /// 原任务缺失、状态冲突或保存失败时拒绝。
    async fn payment_task(&self, usage_id: &str, executor: &mut dyn Executor) -> Result<()>;
    /// 在资金、核销和任务成功后保存证据应用回执。
    /// # 参数
    /// 原收件身份、真实应用资源及同一事务执行器。
    /// # 返回
    /// 来源证据已关联到真实资源。
    /// # 错误
    /// 资源不匹配、原状态冲突或保存失败时拒绝。
    async fn complete(
        &self,
        inbox_id: &str,
        resource: FundsAppliedResource,
        executor: &mut dyn Executor,
    ) -> Result<()>;
    /// 保留原资金证据并限制账户新增使用。
    /// # 参数
    /// 原收件身份、稳定脱敏原因及同一事务执行器。
    /// # 返回
    /// 待核对证据和账户限制已保存。
    /// # 错误
    /// 原记录缺失、原因无效或保存失败时拒绝。
    async fn review(&self, inbox_id: &str, reason: &str, executor: &mut dyn Executor) -> Result<()>;
}

/// 执行正常原子应用或明确失败后的资金异常保留，终态重放不再写入。
/// # 参数
/// 领域步骤、当前原证据、应用模式及调用方事务执行器。
/// # 返回
/// 返回实际应用或待核对计数；已有终态返回零计数。
/// # 错误
/// 任一步骤失败即停止，由调用方整体回滚当前事务。
pub(super) async fn apply_evidence(
    application: &impl FundsApplicationSteps,
    inbox: &FundsEvidenceInbox,
    mode: ApplicationMode,
    executor: &mut dyn Executor,
) -> Result<FundsInvestigationRun> {
    if inbox.state != FundsEvidenceInboxState::Received {
        return Ok(FundsInvestigationRun::default());
    }
    match application.cash(inbox, executor).await? {
        CashApplication::Review(reason) => {
            application.review(&inbox.base.id, reason, executor).await?;
            Ok(FundsInvestigationRun { needs_review: 1, ..Default::default() })
        },
        CashApplication::Applied(resource) => {
            finish_application(application, inbox, resource, mode, executor).await
        },
    }
}

async fn finish_application(
    application: &impl FundsApplicationSteps,
    inbox: &FundsEvidenceInbox,
    resource: FundsAppliedResource,
    mode: ApplicationMode,
    executor: &mut dyn Executor,
) -> Result<FundsInvestigationRun> {
    if let FundsAppliedResource::DebitUsage { usage_id } = &resource {
        if mode == ApplicationMode::PreserveDebit {
            application.review(&inbox.base.id, "FUNDS_SETTLEMENT_NEEDS_REVIEW", executor).await?;
            return Ok(FundsInvestigationRun {
                needs_review: 1,
                settlement_pending: 1,
                ..Default::default()
            });
        }
        application.settle(usage_id, executor).await?;
        application.payment_task(usage_id, executor).await?;
    }

    application.complete(&inbox.base.id, resource, executor).await?;
    Ok(FundsInvestigationRun { applied: 1, ..Default::default() })
}

/// 判定明确业务失败是否允许在已回滚后保存异常资金。
/// # 参数
/// 原事务返回的应用错误。
/// # 返回
/// 仅明确业务规则失败返回真，提交未知和存储错误返回假。
/// # 错误
/// 不返回错误。
pub(super) fn reviewable(error: &Error) -> bool {
    matches!(
        error,
        Error::BusinessLogicError(_)
            | Error::ValidationError(_)
            | Error::ConflictError(_)
            | Error::NotFound(_)
            | Error::Logic(_)
    )
}

#[cfg(test)]
mod tests;
