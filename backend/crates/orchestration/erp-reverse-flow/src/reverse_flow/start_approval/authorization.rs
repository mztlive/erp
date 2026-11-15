//! 退款和冲正提交统一校验真实单据，业务层只保留岗位分离约束。
use application_core::AuditActor;
use erp_core::ids::ReceiptReversalId;
use erp_returns::repository::ReturnsExt;
use erp_returns::service::ReturnsService;
use erp_workflow::entity::document_registry::DocumentType;
use mongodb::Database;
use persistence_core::{Executor, Transactional};

use super::super::ReturnsProcess;
use super::financial_access::{require_submission, submit_permission};
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
/// permission 为既有命令保存的固定入口码，必须与 kind 一致；executor 为原事务。
/// # 返回
/// 真实单据仍满足固定提交规则及岗位分离时成功。
/// # 错误
/// 类型与入口码不符、缺少资格、来源无权或业务职责冲突时拒绝。
pub(in crate::reverse_flow) async fn ensure_replay_authorized(
    db: &Database,
    rbac: &ReverseFlowPorts,
    actor: &AuditActor,
    kind: DocumentType,
    permission: &str,
    id: &str,
    executor: &mut dyn Executor,
) -> Result<()> {
    if permission != submit_permission(kind)?.to_string() {
        return Err(Error::ValidationError("退款或冲正类型与固定提交入口不一致".into()));
    }
    ensure_submit_permission(rbac, actor, kind, id, executor).await?;
    ensure_financial_separation(db, kind, id, executor).await
}

impl ReturnsProcess {
    /// 已提交命令回放在新事务内重验真实单据，保持零写入。
    /// # 参数
    /// kind、id 为真实单据，permission 必须是该类型的固定提交码，actor 为回放账号。
    /// # 返回
    /// 固定完整授权与业务岗位分离通过时成功。
    /// # 错误
    /// 入口码不符、对象无权、岗位冲突或事务失败时返回对应错误。
    pub(in crate::reverse_flow) async fn authorize_refund_replay(
        &self,
        kind: DocumentType,
        permission: &str,
        id: &str,
        actor: &AuditActor,
    ) -> Result<()> {
        let db = self.db.clone();
        let rbac = self.rbac.clone();
        let actor = actor.clone();
        let id = id.to_string();
        let permission = permission.to_string();
        self.db
            .client()
            .with_transaction(move |executor| {
                Box::pin(async move {
                    ensure_replay_authorized(&db, &rbac, &actor, kind, &permission, &id, executor).await
                })
            })
            .await
    }
}
