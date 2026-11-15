//! 退款和冲正的真实单据提交入口只消费 identity 固定规则。
use application_core::AuditActor;
use erp_identity::{AccessTarget, AuthorizationService};
use erp_permissions::{Permission, returns};
use erp_workflow::entity::document_registry::DocumentType;
use persistence_core::Executor;

use crate::{Error, Result};

/// 取得真实单据类型对应的公共固定提交权限。
/// # 参数
/// kind 为退款或冲正单据类型。
/// # 返回
/// 返回公共权限目录中的固定提交权限。
/// # 错误
/// 未登记的单据类型返回校验错误。
pub(super) fn submit_permission(kind: DocumentType) -> Result<Permission> {
    match kind {
        DocumentType::CustomerRefund => Ok(returns::CUSTOMER_REFUND_CUSTOMER_REFUND_SUBMIT),
        DocumentType::SupplierRefund => Ok(returns::SUPPLIER_REFUND_SUPPLIER_REFUND_SUBMIT),
        DocumentType::ReceiptReversal => Ok(returns::RECEIPT_REVERSAL_RECEIPT_REVERSAL_SUBMIT),
        DocumentType::PaymentReversal => Ok(returns::PAYMENT_REVERSAL_PAYMENT_REVERSAL_SUBMIT),
        _ => Err(Error::ValidationError("未登记退款或冲正提交入口".into())),
    }
}

/// 使用真实单据主键执行固定完整提交规则。
/// # 参数
/// authorization 为共享授权服务，actor 为当前账号，kind、id 为真实单据身份。
/// executor 必须是调用方持有的原快照事务。
/// # 返回
/// 固定的提交资格、完整资金来源与原经办人条件均满足时成功。
/// # 错误
/// 非事务执行器、未知类型、缺少资格或越权时拒绝。
pub(super) async fn require_submission(
    authorization: &AuthorizationService,
    actor: &AuditActor,
    kind: DocumentType,
    id: &str,
    executor: &mut dyn Executor,
) -> Result<()> {
    authorization.require(actor, &submit_permission(kind)?, AccessTarget::record(id)?, executor).await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use erp_core::AccountKind;
    use erp_identity::testing::unavailable_authorization;
    use persistence_core::ReadExecutor;

    use super::*;

    /// 四种真实提交入口必须由中央规则在身份或对象取证前拒绝普通读取执行器。
    #[tokio::test]
    async fn submissions_require_snapshot_before_loading_identity_or_source() {
        let authorization = unavailable_authorization();
        let actor = AuditActor::new("actor".into(), "operator".into(), AccountKind::Admin);
        for kind in [
            DocumentType::CustomerRefund,
            DocumentType::SupplierRefund,
            DocumentType::ReceiptReversal,
            DocumentType::PaymentReversal,
        ] {
            let result = require_submission(
                &authorization,
                &actor,
                kind,
                "actual_document",
                &mut ReadExecutor::default(),
            )
            .await;
            assert!(
                matches!(result, Err(Error::ValidationError(reason)) if reason == "写入授权必须使用快照事务")
            );
        }
    }
}
