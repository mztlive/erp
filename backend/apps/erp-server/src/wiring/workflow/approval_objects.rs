//! 绑定升级按精确主体读取订单事实，并保留非订单主体及其父链存在性检查。

use std::collections::HashSet;

use erp_customer::CustomerExt;
use erp_finance::repository::{PayableExt, PurchasePaymentRequestExt, ReceivableExt};
use erp_inventory::InventoryExt;
use erp_procurement::repository::PurchaseOrderExt;
use erp_read_models::workbench::authority::WorkItemFactsReader;
use erp_returns::repository::ReturnsExt;
use erp_sales::repository::SalesOrderExt;
use erp_supplier::SupplierExt;
use erp_workflow::entity::document_registry::DocumentType;
use erp_workflow::ports::{ApprovalBindingFacts, OrderTaskSource};
use mongodb::Database;
use persistence_core::Executor;

use crate::wiring::errors::{Error, Result};

/// 读取一个审批主体的绑定事实；不构造未被业务消费的范围维度。
/// # 参数
/// db 为业务数据库，kind/id 为精确主体，executor 沿用升级事务。
/// # 返回
/// 主体及原有必要父链存在时返回绑定事实，缺失时返回 None。
/// # 错误
/// 查询失败或订单权威来源不完整时拒绝。
pub(super) async fn load(
    db: &Database,
    kind: DocumentType,
    id: &str,
    executor: &mut dyn Executor,
) -> Result<Option<ApprovalBindingFacts>> {
    if OrderTaskSource::approval_kind(kind).is_some() {
        return order(db, kind, id, executor).await;
    }
    let exists = match kind {
        DocumentType::StockAdjustment => db.stock_adjustments().find_by_id(id, executor).await?.is_some(),
        DocumentType::SalesInvoiceRequest => {
            db.sales_invoice_requests().find_by_id(id, executor).await?.is_some()
        },
        DocumentType::PurchasePaymentRequest => {
            db.purchase_payment_requests().find_by_id(id, executor).await?.is_some()
        },
        DocumentType::CustomerReceipt => db.customer_receipts().find_by_id(id, executor).await?.is_some(),
        DocumentType::CustomerRefund | DocumentType::SupplierRefund => {
            refund_exists(db, kind, id, executor).await?
        },
        DocumentType::ReceiptReversal | DocumentType::PaymentReversal => {
            reversal_exists(db, kind, id, executor).await?
        },
        _ => false,
    };
    Ok(exists.then(ApprovalBindingFacts::default))
}

/// 订单变更仍沿强事实定位原订单，不从展示单号推断来源。
async fn order(
    db: &Database,
    kind: DocumentType,
    id: &str,
    executor: &mut dyn Executor,
) -> Result<Option<ApprovalBindingFacts>> {
    let Some(fact_kind) = OrderTaskSource::approval_kind(kind) else { return Ok(None) };
    let key = (fact_kind, id.to_owned());
    let facts = WorkItemFactsReader::new(db.clone()).load(&HashSet::from([key.clone()]), executor).await?;
    let Some(fact) = facts.get(&key) else { return Ok(None) };
    let source = fact
        .order_scope_source
        .as_ref()
        .filter(|source| source.matches_kind(fact_kind))
        .ok_or_else(|| Error::Internal("审批订单缺少权威来源".into()))?;
    let result = match source {
        OrderTaskSource::Sales(id) => {
            db.sales_orders().find_by_id(id, executor).await?.map(|row| ApprovalBindingFacts {
                order_source: Some(source.clone()),
                owner_user_id: row.sales_owner_user_id,
                business_org_unit_id: Some(row.business_org_unit_id),
            })
        },
        OrderTaskSource::Purchase(id) => {
            let Some(row) = db.purchase_orders().find_by_id(id, executor).await? else { return Ok(None) };
            Some(ApprovalBindingFacts {
                order_source: Some(source.clone()),
                owner_user_id: row.current_owner_user_id()?.to_string(),
                business_org_unit_id: Some(row.business_org_unit_id),
            })
        },
    };
    Ok(result)
}

/// 退款必须仍能定位原有客户或供应商，不再为未消费的结算主体字段创建映射。
async fn refund_exists(
    db: &Database,
    kind: DocumentType,
    id: &str,
    executor: &mut dyn Executor,
) -> Result<bool> {
    match kind {
        DocumentType::CustomerRefund => {
            let Some(row) = db.customer_refunds().find_by_id(id, executor).await? else { return Ok(false) };
            Ok(db.customer_accounts().find_by_id(row.customer_id.as_ref(), executor).await?.is_some())
        },
        DocumentType::SupplierRefund => {
            let Some(row) = db.supplier_refunds().find_by_id(id, executor).await? else { return Ok(false) };
            Ok(db.supplier_accounts().find_by_id(row.supplier_id.as_ref(), executor).await?.is_some())
        },
        _ => Ok(false),
    }
}

/// 冲销保留原收款单或原付款单及供应商的完整存在性要求。
async fn reversal_exists(
    db: &Database,
    kind: DocumentType,
    id: &str,
    executor: &mut dyn Executor,
) -> Result<bool> {
    match kind {
        DocumentType::ReceiptReversal => {
            let Some(row) = db.receipt_reversals().find_by_id(id, executor).await? else { return Ok(false) };
            Ok(db
                .customer_receipts()
                .find_by_id(row.original_customer_receipt_id.as_ref(), executor)
                .await?
                .is_some())
        },
        DocumentType::PaymentReversal => {
            let Some(row) = db.payment_reversals().find_by_id(id, executor).await? else { return Ok(false) };
            let Some(payment) = db
                .supplier_payments()
                .find_by_id(row.original_supplier_payment_id.as_ref(), executor)
                .await?
            else {
                return Ok(false);
            };
            Ok(db.supplier_accounts().find_by_id(payment.supplier_id.as_ref(), executor).await?.is_some())
        },
        _ => Ok(false),
    }
}
