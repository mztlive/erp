//! 退款与冲正的审批结果写入；全部读取与写入沿用运行时事务。
use application_core::AuditActor;
use erp_audit::AuditActorLogs;
use erp_audit::execution::persist_log;
use erp_returns::repository::ReturnsExt;
use erp_returns::service::ReturnsService;
use erp_workflow::entity::document_registry::DocumentType;
use erp_workflow::service::approval::policy::ApprovalDomainAction;
use mongodb::Database;
use persistence_core::Executor;

use super::adapter;
use crate::{Error, Result};

/// 绑定同一次审批结果写入的原单身份、动作、版本与审计名称。
pub(super) struct ApprovalOutcome<'a> {
    pub document_type: DocumentType,
    pub id: &'a str,
    pub action: ApprovalDomainAction,
    pub expected_subject_version: Option<u32>,
    pub audit_action: &'a str,
}

// 按真实单据类型迁移并保存审批结果，沿用调用方事务。
//
// `outcome` 为原单身份、领域动作、驳回提交版本与审计名称；`actor` 为已授权操作人；`executor` 为运行时持有的同一事务。保存状态迁移与审计。
//
// 原单不存在、版本或状态冲突、持久化失败时返回错误。
pub(super) async fn persist_approval_outcome(
    db: &Database,
    outcome: ApprovalOutcome<'_>,
    actor: &AuditActor,
    executor: &mut dyn Executor,
) -> Result<()> {
    let ApprovalOutcome { document_type, id, action, expected_subject_version, audit_action } = outcome;
    match document_type {
        DocumentType::CustomerRefund => {
            persist_customer_refund(db, id, action, expected_subject_version, executor).await?;
        },
        DocumentType::SupplierRefund => {
            persist_supplier_refund(db, id, action, expected_subject_version, executor).await?;
        },
        DocumentType::ReceiptReversal => {
            persist_receipt_reversal(db, id, action, expected_subject_version, executor).await?;
        },
        DocumentType::PaymentReversal => {
            persist_payment_reversal(db, id, action, expected_subject_version, executor).await?;
        },
        other => {
            return Err(Error::ValidationError(format!(
                "{}不适用此退货或退款审批操作，请从对应审批待办办理",
                other.label()
            )));
        },
    }

    let audit = actor.clone().resource_log(audit_action, document_type.as_str(), id.to_string())?;
    persist_log(db, &audit, executor).await?;
    Ok(())
}

// 结算审批终态时读取客户退款原单，按绑定的提交版本迁移状态并保留原 CAS 保存语义。
async fn persist_customer_refund(
    db: &Database,
    id: &str,
    action: ApprovalDomainAction,
    expected_subject_version: Option<u32>,
    executor: &mut dyn Executor,
) -> Result<()> {
    let mut refund = db
        .customer_refunds()
        .find_by_id(id, executor)
        .await?
        .ok_or_else(|| Error::NotFound("客户退款单不存在".to_string()))?;
    if let Some(version) = expected_subject_version {
        refund.reject_approval(version)?;
    } else {
        adapter::execute_customer_refund_domain_action(&mut refund, action)?;
    }
    ReturnsService::new(db.clone()).persist_customer_refund(&mut refund, executor).await?;
    Ok(())
}

// 结算审批终态时读取供应商退款原单，按绑定的提交版本迁移状态并保留原 CAS 保存语义。
async fn persist_supplier_refund(
    db: &Database,
    id: &str,
    action: ApprovalDomainAction,
    expected_subject_version: Option<u32>,
    executor: &mut dyn Executor,
) -> Result<()> {
    let mut refund = db
        .supplier_refunds()
        .find_by_id(id, executor)
        .await?
        .ok_or_else(|| Error::NotFound("供应商退款单不存在".to_string()))?;
    if let Some(version) = expected_subject_version {
        refund.reject_approval(version)?;
    } else {
        adapter::execute_supplier_refund_domain_action(&mut refund, action)?;
    }
    ReturnsService::persist_supplier_refund(db, &mut refund, executor).await?;
    Ok(())
}

// 结算审批终态时读取回款冲正原单，按绑定的提交版本迁移状态并保留原 CAS 保存语义。
async fn persist_receipt_reversal(
    db: &Database,
    id: &str,
    action: ApprovalDomainAction,
    expected_subject_version: Option<u32>,
    executor: &mut dyn Executor,
) -> Result<()> {
    let mut reversal = db
        .receipt_reversals()
        .find_by_id(id, executor)
        .await?
        .ok_or_else(|| Error::NotFound("回款冲正单不存在".to_string()))?;
    if let Some(version) = expected_subject_version {
        reversal.reject_approval(version)?;
    } else {
        adapter::execute_receipt_reversal_domain_action(&mut reversal, action)?;
    }
    ReturnsService::persist_receipt_reversal(db, &mut reversal, executor).await?;
    Ok(())
}

// 结算审批终态时读取付款冲正原单，按绑定的提交版本迁移状态并保留原 CAS 保存语义。
async fn persist_payment_reversal(
    db: &Database,
    id: &str,
    action: ApprovalDomainAction,
    expected_subject_version: Option<u32>,
    executor: &mut dyn Executor,
) -> Result<()> {
    let mut reversal = db
        .payment_reversals()
        .find_by_id(id, executor)
        .await?
        .ok_or_else(|| Error::NotFound("付款冲正单不存在".to_string()))?;
    if let Some(version) = expected_subject_version {
        reversal.reject_approval(version)?;
    } else {
        adapter::execute_payment_reversal_domain_action(&mut reversal, action)?;
    }
    ReturnsService::persist_payment_reversal(db, &mut reversal, executor).await?;
    Ok(())
}
