//! 审批管理沿真实业务来源授权；任务参与者不调用普通业务范围。

use std::sync::Arc;

use application_core::AuditActor;
use erp_core::ids::{CustomerRefundId, PaymentReversalId, ReceiptReversalId, SupplierRefundId};
use erp_finance::repository::{PayableExt, ReceivableExt};
use erp_identity::AuthorizationService;
use erp_returns::repository::ReturnsExt;
use erp_workflow::entity::document_registry::DocumentType;
use erp_workflow::ports::OrderTaskSource;
use mongodb::Database;
use persistence_core::Executor;

use super::order_access;
use crate::wiring::errors::{Error, Result};
use crate::wiring::funds_access;
use crate::wiring::inventory::inventory_service;

// 管理访问必须覆盖完整来源对象；缺失来源和未接入类型一律拒绝。
// `authorization` 为启动时完整绑定的共享授权服务，`executor` 为调用方执行器。
// 来源详情可读时返回 `true`；类型未接入、来源缺失或无权限时返回 `false`。
// 订单来源、库存或资金来源查询中的非拒绝类错误原样返回。
pub(super) async fn readable(
    db: &Database,
    authorization: &Arc<AuthorizationService>,
    actor: &AuditActor,
    kind: DocumentType,
    id: &str,
    executor: &mut dyn Executor,
) -> Result<bool> {
    if OrderTaskSource::approval_kind(kind).is_some() {
        return Ok(order_access::approval_readable(db, authorization, actor, kind, id, executor).await?);
    }
    if kind == DocumentType::StockAdjustment {
        return stock_readable(db, authorization, actor, id, executor).await;
    }
    let Some((resource, source_id)) = finance_source(db, kind, id, executor).await? else {
        return Ok(false);
    };
    match funds_access(db.clone(), authorization.clone())
        .source_document_readable(actor, resource, &source_id, executor)
        .await
        .map_err(Error::from)
    {
        Err(Error::Forbidden(_) | Error::NotFound(_)) => Ok(false),
        result => result,
    }
}

// 审批管理员查看库存调整资料时，沿真实调整单的仓库范围决定读取资格。
async fn stock_readable(
    db: &Database,
    authorization: &Arc<AuthorizationService>,
    actor: &AuditActor,
    id: &str,
    executor: &mut dyn Executor,
) -> Result<bool> {
    match inventory_service(db.clone(), authorization.clone())
        .readable_stock_adjustment(id, actor, executor)
        .await
        .map_err(Error::from)
    {
        Ok(_) => Ok(true),
        Err(Error::Forbidden(_) | Error::NotFound(_)) => Ok(false),
        Err(error) => Err(error),
    }
}

// 校验审批材料权限时按强外键获取真实财务来源，结算主体身份不能替代来源数据授权。
async fn finance_source(
    db: &Database,
    kind: DocumentType,
    id: &str,
    executor: &mut dyn Executor,
) -> Result<Option<(&'static str, String)>> {
    match kind {
        DocumentType::CustomerReceipt => Ok(Some(("customer_receipt", id.into()))),
        DocumentType::SalesInvoiceRequest => Ok(Some(("sales_invoice_request", id.into()))),
        DocumentType::SupplierPayment => Ok(Some(("supplier_payment", id.into()))),
        DocumentType::CustomerRefund => customer_refund_source(db, id, executor).await,
        DocumentType::SupplierRefund => supplier_refund_source(db, id, executor).await,
        DocumentType::ReceiptReversal => Ok(db
            .receipt_reversals()
            .find_by_id(&ReceiptReversalId::new(id), executor)
            .await?
            .map(|row| ("customer_receipt", row.original_customer_receipt_id.to_string()))),
        DocumentType::PaymentReversal => Ok(db
            .payment_reversals()
            .find_by_id(&PaymentReversalId::new(id), executor)
            .await?
            .map(|row| ("supplier_payment", row.original_supplier_payment_id.to_string()))),
        _ => Ok(None),
    }
}

// 为客户退款审批查找可授权来源时，只使用关联原回款或应收分录的真实子账。
async fn customer_refund_source(
    db: &Database,
    id: &str,
    executor: &mut dyn Executor,
) -> Result<Option<(&'static str, String)>> {
    let Some(row) = db.customer_refunds().find_by_id(&CustomerRefundId::new(id), executor).await? else {
        return Ok(None);
    };
    match (row.original_receipt_id, row.original_receivable_entry_id) {
        (Some(id), None) => Ok(Some(("customer_receipt", id.to_string()))),
        (None, Some(id)) => Ok(db
            .receivable_entries()
            .find_by_id(&id, executor)
            .await?
            .map(|entry| ("receivable_account", entry.receivable_account_id.to_string()))),
        _ => Ok(None),
    }
}

// 为供应商退款审批沿原付款或应付分录确定访问范围，由资金读取器识别采购或结算来源。
async fn supplier_refund_source(
    db: &Database,
    id: &str,
    executor: &mut dyn Executor,
) -> Result<Option<(&'static str, String)>> {
    let Some(row) = db.supplier_refunds().find_by_id(&SupplierRefundId::new(id), executor).await? else {
        return Ok(None);
    };
    match (row.original_payment_id, row.original_payable_entry_id) {
        (Some(id), None) => Ok(Some(("supplier_payment", id.to_string()))),
        (None, Some(id)) => Ok(db
            .payable_entries()
            .find_by_id(&id, executor)
            .await?
            .map(|entry| ("payable_account", entry.payable_account_id.to_string()))),
        _ => Ok(None),
    }
}
