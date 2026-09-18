//! 审批升级各单据的强业务事实加载与 Fresh 门禁实现。
//!
//! 本模块收拢 `load_*` 与 `ensure_fresh_*` 的按单据分派实现；分类表与对外
//! 入口仍在 [`upgrade_subject`]，共享校验见 [`upgrade_shared`]。

use erp_core::ids::{SalesChangeOrderId, SalesOrderId};
use erp_customer::CustomerExt;
use erp_finance::repository::{PayableExt, ReceivableExt};
use erp_inventory::InventoryExt;
use erp_procurement::repository::PurchaseOrderExt;
use erp_returns::repository::ReturnsExt;
use erp_sales::repository::prelude::*;
use erp_sales::repository::{SalesOrderExt, SalesReviewExt};
use erp_supplier::SupplierExt;
use erp_workflow::entity::document_registry::DocumentType;
use mongodb::Database;
use persistence_core::Executor;

use super::upgrade_shared::{
    already_submitted, build_facts, ensure_fresh_subject_identity, ensure_goods_service_source,
    ensure_initial_purchase_change_state, ensure_initial_purchase_state, ensure_initial_sales_change_state,
    ensure_initial_sales_order_state, ensure_known_sales_business_type, ensure_sales_document_type,
};
use super::upgrade_subject::ApprovalUpgradeSubjectFacts;
use crate::{Error, Result};

pub(crate) async fn load_sales_order(
    db: &Database,
    requested_type: DocumentType,
    document_id: &str,
    executor: &mut dyn Executor,
) -> Result<ApprovalUpgradeSubjectFacts> {
    let order = db
        .sales_orders()
        .find_by_id(document_id, executor)
        .await?
        .ok_or_else(|| Error::NotFound(format!("{}不存在", requested_type.label())))?;
    ensure_sales_document_type(requested_type, order.business_type)?;
    build_facts(
        requested_type,
        document_id,
        &order.base.id,
        order.base.version,
        order.order_no,
        order.settlement_party_id.as_ref(),
        &order.stable.created_by,
    )
}

pub(crate) async fn load_sales_change(
    db: &Database,
    document_id: &str,
    executor: &mut dyn Executor,
) -> Result<ApprovalUpgradeSubjectFacts> {
    let change = db
        .sales_change_orders()
        .find_by_id(document_id, executor)
        .await?
        .ok_or_else(|| Error::NotFound("销售变更单不存在".to_string()))?;
    let order = db
        .sales_orders()
        .find_by_id(change.sales_order_id.as_ref(), executor)
        .await?
        .ok_or_else(|| Error::NotFound("销售变更单来源销售单不存在".to_string()))?;
    ensure_known_sales_business_type(order.business_type)?;
    build_facts(
        DocumentType::SalesChangeOrder,
        document_id,
        &change.base.id,
        change.base.version,
        String::new(),
        order.settlement_party_id.as_ref(),
        &change.stable.created_by,
    )
}

pub(crate) async fn load_purchase_order(
    db: &Database,
    document_id: &str,
    executor: &mut dyn Executor,
) -> Result<ApprovalUpgradeSubjectFacts> {
    let purchase = db
        .purchase_orders()
        .find_by_id(document_id, executor)
        .await?
        .ok_or_else(|| Error::NotFound("采购单不存在".to_string()))?;
    let sales = db
        .sales_orders()
        .find_by_id(purchase.sales_order_id.as_ref(), executor)
        .await?
        .ok_or_else(|| Error::NotFound("采购单来源销售单不存在".to_string()))?;
    ensure_goods_service_source(sales.business_type, DocumentType::PurchaseOrder)?;
    build_facts(
        DocumentType::PurchaseOrder,
        document_id,
        &purchase.base.id,
        purchase.base.version,
        purchase.purchase_no,
        sales.settlement_party_id.as_ref(),
        &purchase.stable.created_by,
    )
}

pub(crate) async fn load_purchase_change(
    db: &Database,
    document_id: &str,
    executor: &mut dyn Executor,
) -> Result<ApprovalUpgradeSubjectFacts> {
    let change = db
        .purchase_change_orders()
        .find_by_id(document_id, executor)
        .await?
        .ok_or_else(|| Error::NotFound("采购变更单不存在".to_string()))?;
    let purchase = db
        .purchase_orders()
        .find_by_id(change.purchase_order_id.as_ref(), executor)
        .await?
        .ok_or_else(|| Error::NotFound("采购变更单来源采购单不存在".to_string()))?;
    let sales = db
        .sales_orders()
        .find_by_id(purchase.sales_order_id.as_ref(), executor)
        .await?
        .ok_or_else(|| Error::NotFound("采购变更单来源销售单不存在".to_string()))?;
    ensure_goods_service_source(sales.business_type, DocumentType::PurchaseChangeOrder)?;
    build_facts(
        DocumentType::PurchaseChangeOrder,
        document_id,
        &change.base.id,
        change.base.version,
        String::new(),
        sales.settlement_party_id.as_ref(),
        &change.stable.created_by,
    )
}

pub(crate) async fn load_stock_adjustment(
    db: &Database,
    document_id: &str,
    executor: &mut dyn Executor,
) -> Result<ApprovalUpgradeSubjectFacts> {
    let adjustment = db
        .stock_adjustments()
        .find_by_id(document_id, executor)
        .await?
        .ok_or_else(|| Error::NotFound("库存调整单不存在".to_string()))?;
    build_facts(
        DocumentType::StockAdjustment,
        document_id,
        &adjustment.base.id,
        adjustment.base.version,
        adjustment.adjustment_no,
        adjustment.warehouse_id.as_ref(),
        &adjustment.created_by,
    )
}

pub(crate) async fn load_customer_receipt(
    db: &Database,
    document_id: &str,
    executor: &mut dyn Executor,
) -> Result<ApprovalUpgradeSubjectFacts> {
    let receipt = db
        .customer_receipts()
        .find_by_id(document_id, executor)
        .await?
        .ok_or_else(|| Error::NotFound("客户回款单不存在".to_string()))?;
    build_facts(
        DocumentType::CustomerReceipt,
        document_id,
        &receipt.base.id,
        receipt.base.version,
        receipt.receipt_no,
        receipt.counterparty_party_id.as_ref(),
        &receipt.created_by,
    )
}

pub(crate) async fn load_customer_refund(
    db: &Database,
    document_id: &str,
    executor: &mut dyn Executor,
) -> Result<ApprovalUpgradeSubjectFacts> {
    let refund = db
        .customer_refunds()
        .find_by_id(document_id, executor)
        .await?
        .ok_or_else(|| Error::NotFound("客户退款单不存在".to_string()))?;
    let customer = db
        .customer_accounts()
        .find_by_id(refund.customer_id.as_ref(), executor)
        .await?
        .ok_or_else(|| Error::NotFound("客户退款单所属客户不存在".to_string()))?;
    build_facts(
        DocumentType::CustomerRefund,
        document_id,
        &refund.base.id,
        refund.base.version,
        refund.refund_no,
        customer.party_id.as_ref(),
        &refund.created_by,
    )
}

pub(crate) async fn load_supplier_refund(
    db: &Database,
    document_id: &str,
    executor: &mut dyn Executor,
) -> Result<ApprovalUpgradeSubjectFacts> {
    let refund = db
        .supplier_refunds()
        .find_by_id(document_id, executor)
        .await?
        .ok_or_else(|| Error::NotFound("供应商退款单不存在".to_string()))?;
    let supplier = db
        .supplier_accounts()
        .find_by_id(refund.supplier_id.as_ref(), executor)
        .await?
        .ok_or_else(|| Error::NotFound("供应商退款单所属供应商不存在".to_string()))?;
    build_facts(
        DocumentType::SupplierRefund,
        document_id,
        &refund.base.id,
        refund.base.version,
        refund.refund_no,
        supplier.party_id.as_ref(),
        &refund.created_by,
    )
}

pub(crate) async fn load_receipt_reversal(
    db: &Database,
    document_id: &str,
    executor: &mut dyn Executor,
) -> Result<ApprovalUpgradeSubjectFacts> {
    let reversal = db
        .receipt_reversals()
        .find_by_id(document_id, executor)
        .await?
        .ok_or_else(|| Error::NotFound("回款冲正单不存在".to_string()))?;
    let receipt = db
        .customer_receipts()
        .find_by_id(reversal.original_customer_receipt_id.as_ref(), executor)
        .await?
        .ok_or_else(|| Error::NotFound("回款冲正单原回款不存在".to_string()))?;
    build_facts(
        DocumentType::ReceiptReversal,
        document_id,
        &reversal.base.id,
        reversal.base.version,
        reversal.reversal_no,
        receipt.counterparty_party_id.as_ref(),
        &reversal.created_by,
    )
}

pub(crate) async fn load_payment_reversal(
    db: &Database,
    document_id: &str,
    executor: &mut dyn Executor,
) -> Result<ApprovalUpgradeSubjectFacts> {
    let reversal = db
        .payment_reversals()
        .find_by_id(document_id, executor)
        .await?
        .ok_or_else(|| Error::NotFound("付款冲正单不存在".to_string()))?;
    let payment = db
        .supplier_payments()
        .find_by_id(reversal.original_supplier_payment_id.as_ref(), executor)
        .await?
        .ok_or_else(|| Error::NotFound("付款冲正单原付款不存在".to_string()))?;
    let supplier = db
        .supplier_accounts()
        .find_by_id(payment.supplier_id.as_ref(), executor)
        .await?
        .ok_or_else(|| Error::NotFound("付款冲正单原付款供应商不存在".to_string()))?;
    build_facts(
        DocumentType::PaymentReversal,
        document_id,
        &reversal.base.id,
        reversal.base.version,
        reversal.reversal_no,
        supplier.party_id.as_ref(),
        &reversal.created_by,
    )
}

pub(crate) async fn ensure_fresh_sales_order(
    db: &Database,
    facts: &ApprovalUpgradeSubjectFacts,
    executor: &mut dyn Executor,
) -> Result<()> {
    let order = db
        .sales_orders()
        .find_by_id(&facts.document_id, executor)
        .await?
        .ok_or_else(|| Error::NotFound(format!("{}不存在", facts.document_type.label())))?;
    ensure_sales_document_type(facts.document_type, order.business_type)?;
    ensure_fresh_subject_identity(facts, &order.base.id, order.base.version)?;
    ensure_initial_sales_order_state(&order)?;
    let latest_submission = db
        .sales_order_submissions()
        .find_latest_by_order(&SalesOrderId::new(order.base.id.clone()), executor)
        .await?;
    if latest_submission.is_some() {
        return Err(already_submitted(facts.document_type));
    }
    Ok(())
}

pub(crate) async fn ensure_fresh_sales_change(
    db: &Database,
    facts: &ApprovalUpgradeSubjectFacts,
    executor: &mut dyn Executor,
) -> Result<()> {
    let change = db
        .sales_change_orders()
        .find_by_id(&facts.document_id, executor)
        .await?
        .ok_or_else(|| Error::NotFound("销售变更单不存在".to_string()))?;
    ensure_fresh_subject_identity(facts, &change.base.id, change.base.version)?;
    ensure_initial_sales_change_state(&change)?;
    let latest_submission_no = db
        .sales_change_submissions()
        .latest_submission_no_by_change_order(&SalesChangeOrderId::new(change.base.id.clone()), executor)
        .await?;
    if latest_submission_no != 0 {
        return Err(already_submitted(DocumentType::SalesChangeOrder));
    }
    Ok(())
}

pub(crate) async fn ensure_fresh_purchase_order(
    db: &Database,
    facts: &ApprovalUpgradeSubjectFacts,
    executor: &mut dyn Executor,
) -> Result<()> {
    let order = db
        .purchase_orders()
        .find_by_id(&facts.document_id, executor)
        .await?
        .ok_or_else(|| Error::NotFound("采购单不存在".to_string()))?;
    ensure_fresh_subject_identity(facts, &order.base.id, order.base.version)?;
    ensure_initial_purchase_state(&order)
}

pub(crate) async fn ensure_fresh_purchase_change(
    db: &Database,
    facts: &ApprovalUpgradeSubjectFacts,
    executor: &mut dyn Executor,
) -> Result<()> {
    let change = db
        .purchase_change_orders()
        .find_by_id(&facts.document_id, executor)
        .await?
        .ok_or_else(|| Error::NotFound("采购变更单不存在".to_string()))?;
    ensure_fresh_subject_identity(facts, &change.base.id, change.base.version)?;
    ensure_initial_purchase_change_state(&change)
}

pub(crate) async fn ensure_fresh_stock_adjustment(
    db: &Database,
    facts: &ApprovalUpgradeSubjectFacts,
    executor: &mut dyn Executor,
) -> Result<()> {
    let adjustment = db
        .stock_adjustments()
        .find_by_id(&facts.document_id, executor)
        .await?
        .ok_or_else(|| Error::NotFound("库存调整单不存在".to_string()))?;
    ensure_fresh_subject_identity(facts, &adjustment.base.id, adjustment.base.version)?;
    adjustment.ensure_initial_approval_state().map_err(|_| already_submitted(DocumentType::StockAdjustment))
}

pub(crate) async fn ensure_fresh_customer_receipt(
    db: &Database,
    facts: &ApprovalUpgradeSubjectFacts,
    executor: &mut dyn Executor,
) -> Result<()> {
    let receipt = db
        .customer_receipts()
        .find_by_id(&facts.document_id, executor)
        .await?
        .ok_or_else(|| Error::NotFound("客户回款单不存在".to_string()))?;
    ensure_fresh_subject_identity(facts, &receipt.base.id, receipt.base.version)?;
    receipt.ensure_initial_approval_state().map_err(|_| already_submitted(DocumentType::CustomerReceipt))
}

pub(crate) async fn ensure_fresh_customer_refund(
    db: &Database,
    facts: &ApprovalUpgradeSubjectFacts,
    executor: &mut dyn Executor,
) -> Result<()> {
    let refund = db
        .customer_refunds()
        .find_by_id(&facts.document_id, executor)
        .await?
        .ok_or_else(|| Error::NotFound("客户退款单不存在".to_string()))?;
    ensure_fresh_subject_identity(facts, &refund.base.id, refund.base.version)?;
    refund.ensure_initial_approval_state().map_err(|_| already_submitted(DocumentType::CustomerRefund))
}

pub(crate) async fn ensure_fresh_supplier_refund(
    db: &Database,
    facts: &ApprovalUpgradeSubjectFacts,
    executor: &mut dyn Executor,
) -> Result<()> {
    let refund = db
        .supplier_refunds()
        .find_by_id(&facts.document_id, executor)
        .await?
        .ok_or_else(|| Error::NotFound("供应商退款单不存在".to_string()))?;
    ensure_fresh_subject_identity(facts, &refund.base.id, refund.base.version)?;
    refund.ensure_initial_approval_state().map_err(|_| already_submitted(DocumentType::SupplierRefund))
}

pub(crate) async fn ensure_fresh_receipt_reversal(
    db: &Database,
    facts: &ApprovalUpgradeSubjectFacts,
    executor: &mut dyn Executor,
) -> Result<()> {
    let reversal = db
        .receipt_reversals()
        .find_by_id(&facts.document_id, executor)
        .await?
        .ok_or_else(|| Error::NotFound("回款冲正单不存在".to_string()))?;
    ensure_fresh_subject_identity(facts, &reversal.base.id, reversal.base.version)?;
    reversal.ensure_initial_approval_state().map_err(|_| already_submitted(DocumentType::ReceiptReversal))
}

pub(crate) async fn ensure_fresh_payment_reversal(
    db: &Database,
    facts: &ApprovalUpgradeSubjectFacts,
    executor: &mut dyn Executor,
) -> Result<()> {
    let reversal = db
        .payment_reversals()
        .find_by_id(&facts.document_id, executor)
        .await?
        .ok_or_else(|| Error::NotFound("付款冲正单不存在".to_string()))?;
    ensure_fresh_subject_identity(facts, &reversal.base.id, reversal.base.version)?;
    reversal.ensure_initial_approval_state().map_err(|_| already_submitted(DocumentType::PaymentReversal))
}
