//! 审批升级各单据的强业务事实加载与 Fresh 门禁实现。
//!
//! 本模块收拢 `load_*` 与 `ensure_fresh_*` 的按单据分派实现；分类表与对外
//! 入口仍在 [`upgrade_subject`]，共享校验见 [`upgrade_shared`]。

use erp_core::ids::{SalesChangeOrderId, SalesOrderId};
use erp_customer::CustomerExt;
use erp_finance::repository::{PayableExt, ReceivableExt};
use erp_inventory::InventoryExt;
use erp_procurement::repository::PurchaseOrderExt;
use erp_returns::entity::returns::{CustomerRefund, SupplierRefund};
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

// 为审批升级主体事实取数时，读取销售单，并核验请求单据类型与业务性质一致。
//
// 调用参数：
// * `db` - 销售单所在数据库。
// * `requested_type` - 路由给出的销售单或卡券销售单类型。
// * `document_id` - 销售单主键。
//
// 返回销售单主键、版本、单号、结算主体与创建人。
//
// 销售单不存在、业务性质与请求类型不一致、主键或责任字段非法，或仓储失败时返回错误。
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
        ("sales_order", &order.base.id),
    )
}

// 为审批升级主体事实取数时，读取销售变更单，用来源销售单的结算主体作为责任组织。
//
// 调用参数：
// * `db` - 销售变更单与来源销售单所在数据库。
// * `document_id` - 销售变更单主键。
//
// 返回变更单事实；正式单号为空。
//
// 变更单或来源销售单不存在、来源业务性质无法映射、责任字段非法或仓储失败时返回错误。
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
        ("sales_order", &order.base.id),
    )
}

// 为审批升级主体事实取数时，读取采购单，并要求来源销售单是实物及服务销售。
//
// 调用参数：
// * `db` - 采购单与来源销售单所在数据库。
// * `document_id` - 采购单主键。
//
// 返回采购单主键、版本、采购单号、来源结算主体与创建人。
//
// 采购单或来源销售单不存在、来源不是实物及服务、责任字段非法或仓储失败时返回错误。
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
        ("purchase_order", &purchase.base.id),
    )
}

// 为审批升级主体事实取数时，读取采购变更单，经采购单找到来源销售单的结算主体。
//
// 调用参数：
// * `db` - 采购变更、采购单与来源销售单所在数据库。
// * `document_id` - 采购变更单主键。
//
// 返回变更单事实；正式单号为空。
//
// 变更单、来源采购单或来源销售单不存在、来源不是实物及服务、责任字段非法或仓储失败时返回错误。
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
        ("purchase_order", &purchase.base.id),
    )
}

// 为审批升级主体事实取数时，读取库存调整单，用仓库作为责任组织。
//
// 调用参数：
// * `db` - 库存调整单所在数据库。
// * `document_id` - 库存调整单主键。
//
// 返回调整单主键、版本、单号、仓库与创建人。
//
// 调整单不存在、责任字段非法或仓储失败时返回错误。
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
        ("stock_adjustment", &adjustment.base.id),
    )
}

// 为审批升级主体事实取数时，读取客户回款单，用对方主体作为责任组织。
//
// 调用参数：
// * `db` - 客户回款单所在数据库。
// * `document_id` - 客户回款单主键。
//
// 返回回款单主键、版本、单号、对方主体与创建人。
//
// 回款单不存在、责任字段非法或仓储失败时返回错误。
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
        ("customer_receipt", &receipt.base.id),
    )
}

// 为审批升级主体事实取数时，读取客户退款单，用所属客户的主体作为责任组织。
//
// 调用参数：
// * `db` - 客户退款单与客户所在数据库。
// * `document_id` - 客户退款单主键。
//
// 返回退款单主键、版本、单号、客户主体与创建人。
//
// 退款单或所属客户不存在、责任字段非法或仓储失败时返回错误。
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
    let source = customer_refund_source(db, &refund, executor).await?;

    build_facts(
        DocumentType::CustomerRefund,
        document_id,
        &refund.base.id,
        refund.base.version,
        refund.refund_no,
        customer.party_id.as_ref(),
        &refund.created_by,
        (&source.0, &source.1),
    )
}

// 为审批升级主体事实取数时，读取供应商退款单，用所属供应商的主体作为责任组织。
//
// 调用参数：
// * `db` - 供应商退款单与供应商所在数据库。
// * `document_id` - 供应商退款单主键。
//
// 返回退款单主键、版本、单号、供应商主体与创建人。
//
// 退款单或所属供应商不存在、责任字段非法或仓储失败时返回错误。
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
    let source = supplier_refund_source(db, &refund, executor).await?;

    build_facts(
        DocumentType::SupplierRefund,
        document_id,
        &refund.base.id,
        refund.base.version,
        refund.refund_no,
        supplier.party_id.as_ref(),
        &refund.created_by,
        (&source.0, &source.1),
    )
}

// 为审批升级主体事实取数时，读取回款冲正单，用原回款的对方主体作为责任组织。
//
// 调用参数：
// * `db` - 回款冲正单与原回款所在数据库。
// * `document_id` - 回款冲正单主键。
//
// 返回冲正单主键、版本、单号、原回款对方主体与创建人。
//
// 冲正单或原回款不存在、责任字段非法或仓储失败时返回错误。
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
        ("customer_receipt", &receipt.base.id),
    )
}

// 为审批升级主体事实取数时，读取付款冲正单，经原付款找到供应商主体作为责任组织。
//
// 调用参数：
// * `db` - 付款冲正、原付款与供应商所在数据库。
// * `document_id` - 付款冲正单主键。
//
// 返回冲正单主键、版本、单号、供应商主体与创建人。
//
// 冲正单、原付款或原付款供应商不存在、责任字段非法或仓储失败时返回错误。
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
        ("supplier_payment", &payment.base.id),
    )
}

// 核对审批升级主体事实边界时，证明销售单仍是从未提交的初始草稿，且身份与已加载事实一致。
//
// 调用参数：
// * `db` - 销售单与提交记录所在数据库。
// * `facts` - 同一外层事务先前加载的强业务事实。
// * `executor` - 与事实加载相同的外层事务执行器。
//
// 仍为初始未提交草稿时返回。
//
// 单据不存在、类型不匹配、主键或版本不一致、非初始草稿或已有提交时返回错误。仓储失败时返回对应错误。
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

// 核对审批升级主体事实边界时，证明销售变更单仍是从未提交的初始草稿，且身份与已加载事实一致。
//
// 调用参数：
// * `db` - 销售变更单与提交记录所在数据库。
// * `facts` - 同一外层事务先前加载的强业务事实。
// * `executor` - 与事实加载相同的外层事务执行器。
//
// 仍为初始未提交草稿且最新提交序号为 0 时返回。
//
// 变更单不存在、主键或版本不一致、非初始草稿或已有提交时返回错误。仓储失败时返回对应错误。
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

// 核对审批升级主体事实边界时，证明采购单仍是初始草稿，且身份与已加载事实一致。
//
// 调用参数：
// * `db` - 采购单所在数据库。
// * `facts` - 同一外层事务先前加载的强业务事实。
// * `executor` - 与事实加载相同的外层事务执行器。
//
// 仍为初始草稿时返回。
//
// 采购单不存在、主键或版本不一致，或初始状态门禁失败时返回错误。仓储失败时返回对应错误。
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

// 核对审批升级主体事实边界时，证明采购变更单仍是初始草稿，且身份与已加载事实一致。
//
// 调用参数：
// * `db` - 采购变更单所在数据库。
// * `facts` - 同一外层事务先前加载的强业务事实。
// * `executor` - 与事实加载相同的外层事务执行器。
//
// 仍为初始草稿时返回。
//
// 变更单不存在、主键或版本不一致，或初始状态门禁失败时返回错误。仓储失败时返回对应错误。
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

// 核对审批升级主体事实边界时，证明库存调整单仍是初始未提交状态，且身份与已加载事实一致。
//
// 调用参数：
// * `db` - 库存调整单所在数据库。
// * `facts` - 同一外层事务先前加载的强业务事实。
// * `executor` - 与事实加载相同的外层事务执行器。
//
// 仍为初始审批状态时返回。
//
// 调整单不存在、主键或版本不一致，或已离开初始审批状态时返回错误。仓储失败时返回对应错误。
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

// 核对审批升级主体事实边界时，证明客户回款单仍是初始未提交状态，且身份与已加载事实一致。
//
// 调用参数：
// * `db` - 客户回款单所在数据库。
// * `facts` - 同一外层事务先前加载的强业务事实。
// * `executor` - 与事实加载相同的外层事务执行器。
//
// 仍为初始审批状态时返回。
//
// 回款单不存在、主键或版本不一致，或已离开初始审批状态时返回错误。仓储失败时返回对应错误。
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

// 核对审批升级主体事实边界时，证明客户退款单仍是初始未提交状态，且身份与已加载事实一致。
//
// 调用参数：
// * `db` - 客户退款单所在数据库。
// * `facts` - 同一外层事务先前加载的强业务事实。
// * `executor` - 与事实加载相同的外层事务执行器。
//
// 仍为初始审批状态时返回。
//
// 退款单不存在、主键或版本不一致，或已离开初始审批状态时返回错误。仓储失败时返回对应错误。
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

// 核对审批升级主体事实边界时，证明供应商退款单仍是初始未提交状态，且身份与已加载事实一致。
//
// 调用参数：
// * `db` - 供应商退款单所在数据库。
// * `facts` - 同一外层事务先前加载的强业务事实。
// * `executor` - 与事实加载相同的外层事务执行器。
//
// 仍为初始审批状态时返回。
//
// 退款单不存在、主键或版本不一致，或已离开初始审批状态时返回错误。仓储失败时返回对应错误。
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

// 核对审批升级主体事实边界时，证明回款冲正单仍是初始未提交状态，且身份与已加载事实一致。
//
// 调用参数：
// * `db` - 回款冲正单所在数据库。
// * `facts` - 同一外层事务先前加载的强业务事实。
// * `executor` - 与事实加载相同的外层事务执行器。
//
// 仍为初始审批状态时返回。
//
// 冲正单不存在、主键或版本不一致，或已离开初始审批状态时返回错误。仓储失败时返回对应错误。
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

// 核对审批升级主体事实边界时，证明付款冲正单仍是初始未提交状态，且身份与已加载事实一致。
//
// 调用参数：
// * `db` - 付款冲正单所在数据库。
// * `facts` - 同一外层事务先前加载的强业务事实。
// * `executor` - 与事实加载相同的外层事务执行器。
//
// 仍为初始审批状态时返回。
//
// 冲正单不存在、主键或版本不一致，或已离开初始审批状态时返回错误。仓储失败时返回对应错误。
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

// 从退款的真实二选一来源取得授权根；分录来源必须先取出归属子账。
async fn customer_refund_source(
    db: &Database,
    refund: &CustomerRefund,
    executor: &mut dyn Executor,
) -> Result<(String, String)> {
    match (&refund.original_receipt_id, &refund.original_receivable_entry_id) {
        (Some(receipt), None) => Ok(("customer_receipt".into(), receipt.to_string())),
        (None, Some(entry)) => {
            let entry = db
                .receivable_entries()
                .find_by_id(entry.as_ref(), executor)
                .await?
                .ok_or_else(|| Error::NotFound("客户退款来源应收分录不存在".into()))?;
            Ok(("receivable_account".into(), entry.receivable_account_id.to_string()))
        },
        _ => Err(Error::ValidationError("客户退款必须具有唯一真实授权来源".into())),
    }
}

// 从退款的真实二选一来源取得授权根；分录来源必须先取出归属子账。
async fn supplier_refund_source(
    db: &Database,
    refund: &SupplierRefund,
    executor: &mut dyn Executor,
) -> Result<(String, String)> {
    match (&refund.original_payment_id, &refund.original_payable_entry_id) {
        (Some(payment), None) => Ok(("supplier_payment".into(), payment.to_string())),
        (None, Some(entry)) => {
            let entry = db
                .payable_entries()
                .find_by_id(entry.as_ref(), executor)
                .await?
                .ok_or_else(|| Error::NotFound("供应商退款来源应付分录不存在".into()))?;
            Ok(("payable_account".into(), entry.payable_account_id.to_string()))
        },
        _ => Err(Error::ValidationError("供应商退款必须具有唯一真实授权来源".into())),
    }
}
