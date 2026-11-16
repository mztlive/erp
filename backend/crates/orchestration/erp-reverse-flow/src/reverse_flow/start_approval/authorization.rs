//! 退款和冲正提交统一校验真实单据，业务层只保留岗位分离约束。
use application_core::AuditActor;
use erp_core::ids::ReceiptReversalId;
use erp_returns::repository::ReturnsExt;
use erp_returns::service::ReturnsService;
use erp_workflow::entity::document_registry::DocumentType;
use mongodb::Database;
use persistence_core::Executor;

use super::financial_access::require_submission;
use crate::{Error, Result, ReverseFlowPorts};

/// 在读取退款数据前使用真实单据身份执行固定完整提交规则。
/// # 参数
/// rbac 为共享设施，actor 为当前账号，kind、id 为真实退款或冲正单据，executor 为原事务。
/// # 返回
/// 固定提交资格、完整来源与原经办职责成立时成功。
/// # 错误
/// 类型未登记、非事务执行器、缺少资格、来源不完整或越权时拒绝。
pub(in crate::reverse_flow) async fn ensure_submit_permission(
    rbac: &ReverseFlowPorts,
    actor: &AuditActor,
    kind: DocumentType,
    id: &str,
    executor: &mut dyn Executor,
) -> Result<()> {
    require_submission(&rbac.identity, actor, kind, id, executor).await
}

/// 原事务读取真实单据，保留经办人与复核人的业务岗位分离。
async fn ensure_financial_separation(
    db: &Database,
    kind: DocumentType,
    id: &str,
    executor: &mut dyn Executor,
) -> Result<()> {
    let service = ReturnsService::new(db.clone());
    match kind {
        DocumentType::CustomerRefund => {
            service.load_customer_refund(id, executor).await?.ensure_separation()?;
        },
        DocumentType::SupplierRefund => {
            service.load_supplier_refund(id, executor).await?.ensure_separation()?;
        },
        DocumentType::ReceiptReversal => {
            db.receipt_reversals()
                .find_by_id(&ReceiptReversalId::new(id), executor)
                .await?
                .ok_or_else(|| Error::NotFound("回款冲正单不存在".into()))?
                .ensure_separation()?;
        },
        DocumentType::PaymentReversal => {
            service.load_payment_reversal(id, executor).await?.ensure_separation()?;
        },
        _ => return Err(Error::ValidationError("未登记退款或冲正提交入口".into())),
    }
    Ok(())
}

/// 同一事务使用固定完整规则重验回放单据，并保留业务岗位分离。
/// # 参数
/// db、rbac 为共享设施，actor 为当前账号，kind、id 为真实单据。
/// 提交权限由真实单据类型固定选择，executor 为原事务。
/// # 返回
/// 真实单据仍满足固定提交规则及岗位分离时成功。
/// # 错误
/// 类型未登记、缺少资格、来源无权或业务职责冲突时拒绝。
pub(in crate::reverse_flow) async fn ensure_replay_authorized(
    db: &Database,
    rbac: &ReverseFlowPorts,
    actor: &AuditActor,
    kind: DocumentType,
    id: &str,
    executor: &mut dyn Executor,
) -> Result<()> {
    ensure_submit_permission(rbac, actor, kind, id, executor).await?;
    ensure_financial_separation(db, kind, id, executor).await
}

#[cfg(test)]
mod tests {
    use persistence_core::ReadExecutor;
    use persistence_core::testing::SnapshotExecutor;

    use super::super::super::testing::{actor, denied, process};
    use super::*;

    #[tokio::test]
    async fn replay_selects_fixed_submission_from_document_type_before_business_reads() {
        let process = process().await;
        let actor = actor();
        for kind in [
            DocumentType::CustomerRefund,
            DocumentType::SupplierRefund,
            DocumentType::ReceiptReversal,
            DocumentType::PaymentReversal,
        ] {
            let error = ensure_replay_authorized(
                &process.db,
                &process.rbac,
                &actor,
                kind,
                "unread-document",
                &mut ReadExecutor::default(),
            )
            .await
            .unwrap_err();
            assert!(matches!(error, Error::ValidationError(message)
                if message == "写入授权必须使用快照事务"));
            denied(
                ensure_replay_authorized(
                    &process.db,
                    &process.rbac,
                    &actor,
                    kind,
                    "unread-document",
                    &mut SnapshotExecutor::default(),
                )
                .await,
            );
        }
    }

    #[tokio::test]
    async fn replay_rejects_other_document_types_without_loading_identity_or_business_data() {
        let process = process().await;
        let error = ensure_replay_authorized(
            &process.db,
            &process.rbac,
            &actor(),
            DocumentType::SalesOrder,
            "unread-document",
            &mut SnapshotExecutor::default(),
        )
        .await
        .unwrap_err();
        assert!(matches!(error, Error::ValidationError(message)
            if message == "未登记退款或冲正提交入口"));
    }
}
