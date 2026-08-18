//! 审批运行时与业务领域强类型命令之间的事务内端口。

use std::{future::Future, pin::Pin};

use database::Executor;

use crate::errors::{Error, Result};

use super::business_adapter::{document_type_from_subject_kind, ensure_runtime_cut_over};
use super::registry::ApprovalBusinessAction;

/// 强类型领域动作执行所需的冻结审批上下文。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ApprovalActionContext {
    /// 审批定义编码。
    pub definition_key: String,
    /// 审批实例 ID。
    pub approval_instance_id: String,
    /// 当前步骤实例 ID；启动前置动作允许为空。
    pub approval_step_instance_id: Option<String>,
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

/// 构造阻塞恢复业务重验使用的强类型动作上下文。
pub(crate) fn recovery_action_context(
    instance: &entities::approval::ApprovalInstance,
    step: &entities::approval::ApprovalStepInstance,
    work_item: Option<&entities::work_item::WorkItem>,
    actor_id: &str,
    reason: &str,
    idempotency_key: &str,
) -> ApprovalActionContext {
    ApprovalActionContext {
        definition_key: instance.definition_key.clone(),
        approval_instance_id: instance.base.id.clone(),
        approval_step_instance_id: Some(step.base.id.clone()),
        work_item_id: work_item.map(|item| item.base.id.clone()),
        business_object_type: instance.business_object_type.clone(),
        business_object_id: instance.business_object_id.clone(),
        subject_version: instance.subject_version.clone(),
        actor_id: actor_id.to_string(),
        reason: Some(reason.to_string()),
        idempotency_key: idempotency_key.to_string(),
    }
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
        action: ApprovalBusinessAction,
        context: &'a ApprovalActionContext,
        executor: &'a mut dyn Executor,
    ) -> ApprovalActionFuture<'a>;
}

/// 未绑定领域命令时使用的失败关闭端口。
///
/// 该实现绝不把“尚未接入”当作成功；适用于只执行定义 bootstrap、阻塞查询和恢复的运行时实例。
#[derive(Debug, Default)]
pub struct FailClosedApprovalActionPort;

impl ApprovalDomainActionPort for FailClosedApprovalActionPort {
    fn execute<'a>(
        &'a self,
        action: ApprovalBusinessAction,
        context: &'a ApprovalActionContext,
        _executor: &'a mut dyn Executor,
    ) -> ApprovalActionFuture<'a> {
        Box::pin(async move { refuse_legacy_or_uncut_action(&context.business_object_type, action) })
    }
}

/// 已知单据类型时先做 cut-over 闸门，再失败关闭；不得回退旧运行时。
///
/// # 错误
/// 未接入的必须审批类型返回 `APPROVAL_DOCUMENT_TYPE_NOT_CUT_OVER`。
fn refuse_legacy_or_uncut_action(business_object_type: &str, action: ApprovalBusinessAction) -> Result<()> {
    if let Ok(document_type) = document_type_from_subject_kind(business_object_type) {
        ensure_runtime_cut_over(document_type)?;
    }
    Err(Error::BusinessLogicError(format!(
        "审批领域动作 {action:?} 尚未绑定，已按安全策略拒绝推进"
    )))
}

#[cfg(test)]
mod tests {
    use database::NoTransaction;

    use super::*;

    #[tokio::test]
    async fn unbound_domain_action_fails_closed() {
        let context = ApprovalActionContext {
            definition_key: "CARD_SALES_APPROVAL".to_string(),
            approval_instance_id: "instance-1".to_string(),
            approval_step_instance_id: None,
            work_item_id: None,
            business_object_type: "SALES_ORDER".to_string(),
            business_object_id: "order-1".to_string(),
            subject_version: "submission-1".to_string(),
            actor_id: "user-1".to_string(),
            reason: None,
            idempotency_key: "request-1".to_string(),
        };
        let error = FailClosedApprovalActionPort
            .execute(
                ApprovalBusinessAction::SubmitCardSalesApproval,
                &context,
                &mut NoTransaction,
            )
            .await
            .unwrap_err();
        assert!(matches!(error, Error::BusinessLogicError(_)));
    }

    /// 未 cut-over 的必须审批类型不得调用旧运行时。
    #[tokio::test]
    async fn uncut_document_type_fails_closed_without_legacy_runtime() {
        let error =
            refuse_legacy_or_uncut_action("sales_order", ApprovalBusinessAction::SubmitCardSalesApproval)
                .unwrap_err();
        assert!(error
            .to_string()
            .contains(super::super::business_adapter::APPROVAL_DOCUMENT_TYPE_NOT_CUT_OVER));
        let pilot = refuse_legacy_or_uncut_action(
            "stock_adjustment",
            ApprovalBusinessAction::SubmitCardSalesApproval,
        )
        .unwrap_err();
        assert!(matches!(pilot, Error::BusinessLogicError(_)));
    }
}
