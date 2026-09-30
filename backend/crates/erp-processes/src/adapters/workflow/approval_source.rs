//! 审批管理沿真实业务来源授权；任务参与者不调用普通业务范围。

use application_core::AuditActor;
use erp_core::ids::{CustomerRefundId, PaymentReversalId, ReceiptReversalId, SupplierRefundId};
use erp_finance::repository::{PayableExt, ReceivableExt};
use erp_identity::SharedRbacService;
use erp_inventory::InventoryExt;
use erp_returns::repository::ReturnsExt;
use erp_workflow::entity::document_registry::DocumentType;
use erp_workflow::ports::{OrderTaskSource, WorkflowScopeObject};
use mongodb::Database;
use persistence_core::Executor;

use super::{approval_scope, order_access};
use crate::adapters::{funds_access_with_rbac, purchase_access};
use crate::{Error, Result};

/// 管理访问必须覆盖完整来源对象；缺失来源和未接入类型一律拒绝。
pub(super) async fn readable(
    db: &Database,
    rbac: &SharedRbacService,
    actor: &AuditActor,
    kind: DocumentType,
    id: &str,
    executor: &mut dyn Executor,
) -> Result<bool> {
    if OrderTaskSource::approval_kind(kind).is_some() {
        return Ok(order_access::approval_readable(db, rbac, actor, kind, id, executor).await?);
    }
    if kind == DocumentType::StockAdjustment {
        return stock_readable(db, rbac, actor, id, executor).await;
    }
    let Some((resource, source_id)) = finance_source(db, kind, id, executor).await? else {
        return Ok(false);
    };
    let purchase = purchase_access(db.clone(), rbac.clone());
    match funds_access_with_rbac(db.clone(), rbac.clone())
        .source_document_readable(actor, resource, &source_id, Some(&purchase), executor)
        .await
        .map_err(Error::from)
    {
        Err(Error::Forbidden(_) | Error::NotFound(_)) => Ok(false),
        result => result,
    }
}

/// 库存管理读取复用真实库存调整仓库范围。
async fn stock_readable(
    db: &Database,
    rbac: &SharedRbacService,
    actor: &AuditActor,
    id: &str,
    executor: &mut dyn Executor,
) -> Result<bool> {
    let Some(row) = db.stock_adjustments().list_active_by_ids(&[id.to_string()], executor).await?.pop()
    else {
        return Ok(false);
    };
    let scope = approval_scope::resolve(db, rbac, actor, "stock_adjustment:detail", executor).await?;
    let object = WorkflowScopeObject {
        owner_user_id: row.created_by,
        warehouse_id: Some(row.warehouse_id.to_string()),
        ..Default::default()
    };
    Ok(scope.is_some_and(|scope| scope.allows(&object)))
}

/// 按强外键取得财务源，结算主体身份不得代替来源授权。
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

/// 客户退款严格选择原回款或应收分录的真实子账。
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

/// 供应商退款沿原付款或应付分录授权，采购与结算来源由资金读取器识别。
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
