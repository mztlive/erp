//! 审批绑定升级所需的强业务事实与 Fresh 门禁。
//!
//! 本模块只读取强业务对象及其责任链，不读取 `business_documents` 注册投影，
//! 也不执行授权、绑定写入或事务提交。通用事实加载不得要求对象仍未提交，
//! 从而允许 Replay 在对象后来提交或启动后按当前权限回读原结果。调用方必须在
//! 同一外层事务中先加载事实并完成授权、查询收据；只有无收据的 Fresh 分支才
//! 执行初始未提交门禁、强对象版本重验和绑定 CAS。

use database::{
    CustomerExt, Executor, InventoryExt, PayableExt, PurchaseOrderExt, ReceivableExt, ReturnsExt,
    SalesOrderExt, SalesReviewExt, SupplierExt,
};
use entities::document_registry::DocumentType;
use entities::ids::{SalesChangeOrderId, SalesOrderId};
use entities::purchase_order::{PurchaseChangeOrderStatus, PurchaseOrderStatus};
use entities::sales_order::{BusinessType, CommercialStatus, ReviewStatus};
use entities::sales_review::SalesChangeOrderStatus;
use mongodb::Database;

use crate::errors::{Error, Result};

use super::business_adapter::BindingRevalidationContext;
use super::policy::require_process_required;

/// 审批绑定升级使用的强业务对象事实。
///
/// 所有字段均来自强实体或其固定父链。构造成功只证明类型、主键、创建人和
/// 责任组织完整，不证明对象仍可 Fresh 升级；调用方不得用注册表字段或当前
/// actor 覆盖。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ApprovalUpgradeSubjectFacts {
    /// 请求类型与强实体集合、业务性质核验后的单据类型。
    pub document_type: DocumentType,
    /// 强业务对象主键。
    pub document_id: String,
    /// 强业务对象 `BaseModel.version`。
    pub business_object_version: u64,
    /// 强业务对象当前正式单号；未分配正式号或无独立业务编号时为空。
    pub document_no: String,
    /// 强业务对象或固定父链给出的责任组织。
    pub responsible_org_id: String,
    /// 强业务对象不可变创建人。
    pub creator_id: String,
}

impl ApprovalUpgradeSubjectFacts {
    /// 读取并校验一张必须审批单据的强业务事实。
    ///
    /// # 参数
    /// * `db` - MongoDB 数据库
    /// * `document_type` - 路由给出的精确单据类型
    /// * `document_id` - 路由给出的精确强业务对象主键
    /// * `executor` - 调用方持有的读或事务执行器
    ///
    /// # 返回
    /// 返回强业务对象身份、版本、单号、责任组织与不可变创建人。
    ///
    /// # 错误
    /// `NO_APPROVAL`、主键非法、实体或父链缺失、销售业务性质不匹配、创建人或
    /// 责任组织缺失时失败关闭。已提交或已启动对象仍必须允许加载本事实。
    pub async fn load(
        db: &Database,
        document_type: DocumentType,
        document_id: &str,
        executor: &mut dyn Executor,
    ) -> Result<Self> {
        require_process_required(document_type)?;
        ensure_exact_document_id(document_type, document_id)?;

        match document_type {
            DocumentType::SalesOrder | DocumentType::VoucherSalesOrder => {
                load_sales_order(db, document_type, document_id, executor).await
            }
            DocumentType::SalesChangeOrder => load_sales_change(db, document_id, executor).await,
            DocumentType::PurchaseOrder => load_purchase_order(db, document_id, executor).await,
            DocumentType::PurchaseChangeOrder => load_purchase_change(db, document_id, executor).await,
            DocumentType::StockAdjustment => load_stock_adjustment(db, document_id, executor).await,
            DocumentType::CustomerReceipt => load_customer_receipt(db, document_id, executor).await,
            DocumentType::CustomerRefund => load_customer_refund(db, document_id, executor).await,
            DocumentType::SupplierRefund => load_supplier_refund(db, document_id, executor).await,
            DocumentType::ReceiptReversal => load_receipt_reversal(db, document_id, executor).await,
            DocumentType::PaymentReversal => load_payment_reversal(db, document_id, executor).await,
            DocumentType::SupplierPayment
            | DocumentType::PurchaseReceipt
            | DocumentType::Delivery
            | DocumentType::ElectronicDelivery
            | DocumentType::ServiceFulfillment
            | DocumentType::CustomerAcceptance
            | DocumentType::Invoice
            | DocumentType::SalesReturnCase
            | DocumentType::PurchaseReturnOrder => Err(Error::Internal(
                "NO_APPROVAL 类型通过了审批绑定升级政策门禁".to_string(),
            )),
        }
    }

    /// 构造定义图资格重验所需的强业务上下文。
    ///
    /// # 返回
    /// 返回责任组织与不可变创建人；不读取或注入当前操作人。
    pub fn binding_context(&self) -> BindingRevalidationContext {
        BindingRevalidationContext {
            organization_id: self.responsible_org_id.clone(),
            creator_id: self.creator_id.clone(),
        }
    }

    /// 校验客户端期望版本仍等于强业务对象版本。
    ///
    /// # 错误
    /// 版本不一致时返回冲突，调用方不得继续升级注册投影。
    pub fn ensure_expected_business_object_version(&self, expected: u64) -> Result<()> {
        if self.business_object_version != expected {
            return Err(Error::ConflictError(
                "强业务对象版本已变化，请刷新后重试".to_string(),
            ));
        }
        Ok(())
    }

    /// 仅为无收据 Fresh 分支证明对象仍是从未提交的初始草稿。
    ///
    /// # 参数
    /// * `db` - MongoDB 数据库
    /// * `executor` - 与事实加载、收据查询相同的外层事务执行器
    ///
    /// # 错误
    /// 对象已变化、非初始草稿或曾形成过不可变提交时返回冲突。Replay 分支不得
    /// 调用本方法。
    pub async fn ensure_initial_unsubmitted(&self, db: &Database, executor: &mut dyn Executor) -> Result<()> {
        ensure_initial_unsubmitted_approval_upgrade_subject(db, self, executor).await
    }
}

/// 读取并校验一张必须审批单据的强业务事实。
///
/// # 错误
/// 参见 [`ApprovalUpgradeSubjectFacts::load`]。
pub async fn load_approval_upgrade_subject_facts(
    db: &Database,
    document_type: DocumentType,
    document_id: &str,
    executor: &mut dyn Executor,
) -> Result<ApprovalUpgradeSubjectFacts> {
    ApprovalUpgradeSubjectFacts::load(db, document_type, document_id, executor).await
}

/// 仅为无收据 Fresh 分支证明对象仍是从未提交的初始草稿。
///
/// # 参数
/// * `db` - MongoDB 数据库
/// * `facts` - 同一外层事务先前加载的强业务授权事实
/// * `executor` - 与事实加载、收据查询相同的外层事务执行器
///
/// # 错误
/// 对象缺失或版本变化、非初始草稿、曾形成过不可变提交，或错误传入
/// `NO_APPROVAL` 类型时失败关闭。
pub async fn ensure_initial_unsubmitted_approval_upgrade_subject(
    db: &Database,
    facts: &ApprovalUpgradeSubjectFacts,
    executor: &mut dyn Executor,
) -> Result<()> {
    require_process_required(facts.document_type)?;
    ensure_exact_document_id(facts.document_type, &facts.document_id)?;
    match facts.document_type {
        DocumentType::SalesOrder | DocumentType::VoucherSalesOrder => {
            ensure_fresh_sales_order(db, facts, executor).await
        }
        DocumentType::SalesChangeOrder => ensure_fresh_sales_change(db, facts, executor).await,
        DocumentType::PurchaseOrder => ensure_fresh_purchase_order(db, facts, executor).await,
        DocumentType::PurchaseChangeOrder => ensure_fresh_purchase_change(db, facts, executor).await,
        DocumentType::StockAdjustment => ensure_fresh_stock_adjustment(db, facts, executor).await,
        DocumentType::CustomerReceipt => ensure_fresh_customer_receipt(db, facts, executor).await,
        DocumentType::CustomerRefund => ensure_fresh_customer_refund(db, facts, executor).await,
        DocumentType::SupplierRefund => ensure_fresh_supplier_refund(db, facts, executor).await,
        DocumentType::ReceiptReversal => ensure_fresh_receipt_reversal(db, facts, executor).await,
        DocumentType::PaymentReversal => ensure_fresh_payment_reversal(db, facts, executor).await,
        DocumentType::SupplierPayment
        | DocumentType::PurchaseReceipt
        | DocumentType::Delivery
        | DocumentType::ElectronicDelivery
        | DocumentType::ServiceFulfillment
        | DocumentType::CustomerAcceptance
        | DocumentType::Invoice
        | DocumentType::SalesReturnCase
        | DocumentType::PurchaseReturnOrder => Err(Error::Internal(
            "NO_APPROVAL 类型进入了审批绑定升级 Fresh 门禁".to_string(),
        )),
    }
}

async fn load_sales_order(
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

async fn load_sales_change(
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

async fn load_purchase_order(
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

async fn load_purchase_change(
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

async fn load_stock_adjustment(
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

async fn load_customer_receipt(
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

async fn load_customer_refund(
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

async fn load_supplier_refund(
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

async fn load_receipt_reversal(
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

async fn load_payment_reversal(
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

async fn ensure_fresh_sales_order(
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

async fn ensure_fresh_sales_change(
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

async fn ensure_fresh_purchase_order(
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

async fn ensure_fresh_purchase_change(
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

async fn ensure_fresh_stock_adjustment(
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
    adjustment
        .ensure_initial_approval_state()
        .map_err(|_| already_submitted(DocumentType::StockAdjustment))
}

async fn ensure_fresh_customer_receipt(
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
    receipt
        .ensure_initial_approval_state()
        .map_err(|_| already_submitted(DocumentType::CustomerReceipt))
}

async fn ensure_fresh_customer_refund(
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
    refund
        .ensure_initial_approval_state()
        .map_err(|_| already_submitted(DocumentType::CustomerRefund))
}

async fn ensure_fresh_supplier_refund(
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
    refund
        .ensure_initial_approval_state()
        .map_err(|_| already_submitted(DocumentType::SupplierRefund))
}

async fn ensure_fresh_receipt_reversal(
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
    reversal
        .ensure_initial_approval_state()
        .map_err(|_| already_submitted(DocumentType::ReceiptReversal))
}

async fn ensure_fresh_payment_reversal(
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
    reversal
        .ensure_initial_approval_state()
        .map_err(|_| already_submitted(DocumentType::PaymentReversal))
}

fn ensure_fresh_subject_identity(
    facts: &ApprovalUpgradeSubjectFacts,
    actual_id: &str,
    actual_version: u64,
) -> Result<()> {
    if facts.document_id != actual_id {
        return Err(Error::Internal("Fresh 强业务对象主键不一致".to_string()));
    }
    if facts.business_object_version != actual_version {
        return Err(Error::ConflictError(
            "强业务对象版本已变化，请刷新后重试".to_string(),
        ));
    }
    Ok(())
}

fn ensure_exact_document_id(document_type: DocumentType, document_id: &str) -> Result<()> {
    if document_id.is_empty() || document_id.trim() != document_id {
        return Err(Error::ValidationError("单据 ID 必须是非空精确主键".to_string()));
    }
    entities::approval_integration::subject_ref_for(document_type, document_id)
        .map_err(|error| Error::ValidationError(error.to_string()))?;
    Ok(())
}

fn ensure_sales_document_type(requested: DocumentType, actual: BusinessType) -> Result<()> {
    let actual = entities::approval_integration::document_type_of_sales_business(actual);
    if actual != requested {
        return Err(Error::ValidationError(format!(
            "请求单据类型 {} 与销售单业务性质对应类型 {} 不一致",
            requested.as_str(),
            actual.as_str()
        )));
    }
    Ok(())
}

fn ensure_known_sales_business_type(actual: BusinessType) -> Result<()> {
    match entities::approval_integration::document_type_of_sales_business(actual) {
        DocumentType::SalesOrder | DocumentType::VoucherSalesOrder => Ok(()),
        _ => Err(Error::Internal("销售单业务性质映射不完整".to_string())),
    }
}

fn ensure_goods_service_source(actual: BusinessType, target: DocumentType) -> Result<()> {
    if actual != BusinessType::GoodsService {
        return Err(Error::ValidationError(format!(
            "{} 的来源销售单必须是实物及服务销售单",
            target.label()
        )));
    }
    Ok(())
}

fn ensure_initial_sales_order_state(order: &entities::sales_order::SalesOrder) -> Result<()> {
    if order.commercial_status != CommercialStatus::Draft
        || order.review_status != ReviewStatus::NotSubmitted
        || order.stable.status != CommercialStatus::Draft
        || order.stable.current_revision_id.is_some()
    {
        return Err(already_submitted(
            entities::approval_integration::document_type_of_sales_business(order.business_type),
        ));
    }
    Ok(())
}

fn ensure_initial_sales_change_state(change: &entities::sales_review::SalesChangeOrder) -> Result<()> {
    if change.stable.status != SalesChangeOrderStatus::Draft
        || change.current_submission_id.is_some()
        || change.target_content_hash.is_some()
        || change.effective_revision_id.is_some()
    {
        return Err(already_submitted(DocumentType::SalesChangeOrder));
    }
    Ok(())
}

fn ensure_initial_purchase_state(order: &entities::purchase_order::PurchaseOrder) -> Result<()> {
    if order.stable.status != PurchaseOrderStatus::Draft
        || order.approval_subject_version != 0
        || order.current_submission_id.is_some()
        || order.stable.current_revision_id.is_some()
    {
        return Err(already_submitted(DocumentType::PurchaseOrder));
    }
    Ok(())
}

fn ensure_initial_purchase_change_state(
    change: &entities::purchase_order::PurchaseChangeOrder,
) -> Result<()> {
    if change.stable.status != PurchaseChangeOrderStatus::Draft
        || change.approval_subject_version != 0
        || change.current_submission_id.is_some()
        || change.target_content_hash.is_some()
        || change.effective_revision_id.is_some()
    {
        return Err(already_submitted(DocumentType::PurchaseChangeOrder));
    }
    Ok(())
}

fn build_facts(
    document_type: DocumentType,
    requested_id: &str,
    actual_id: &str,
    business_object_version: u64,
    document_no: String,
    responsible_org_id: &str,
    creator_id: &str,
) -> Result<ApprovalUpgradeSubjectFacts> {
    if requested_id != actual_id {
        return Err(Error::Internal("强业务对象主键与查询主键不一致".to_string()));
    }
    if business_object_version == 0 {
        return Err(Error::Internal("强业务对象版本非法".to_string()));
    }
    ensure_exact_nonempty_fact(responsible_org_id, "强业务对象责任组织缺失或非法")?;
    ensure_exact_nonempty_fact(creator_id, "强业务对象不可变创建人缺失或非法")?;
    Ok(ApprovalUpgradeSubjectFacts {
        document_type,
        document_id: actual_id.to_string(),
        business_object_version,
        document_no,
        responsible_org_id: responsible_org_id.to_string(),
        creator_id: creator_id.to_string(),
    })
}

fn ensure_exact_nonempty_fact(value: &str, message: &str) -> Result<()> {
    if value.is_empty() || value.trim() != value {
        return Err(Error::Internal(message.to_string()));
    }
    Ok(())
}

fn already_submitted(document_type: DocumentType) -> Error {
    Error::ConflictError(format!(
        "{}不是从未提交审批的初始草稿，不能升级绑定",
        document_type.label()
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strong_facts_keep_exact_identity_and_context() {
        let facts = build_facts(
            DocumentType::StockAdjustment,
            "adjustment-1",
            "adjustment-1",
            7,
            "ADJ-1".to_string(),
            "warehouse-1",
            "creator-1",
        )
        .expect("完整强事实");

        assert_eq!(facts.document_id, "adjustment-1");
        assert_eq!(facts.business_object_version, 7);
        assert_eq!(
            facts.binding_context(),
            BindingRevalidationContext {
                organization_id: "warehouse-1".to_string(),
                creator_id: "creator-1".to_string(),
            }
        );
        facts
            .ensure_expected_business_object_version(7)
            .expect("强对象版本一致");
        assert!(facts.ensure_expected_business_object_version(8).is_err());
    }

    #[test]
    fn strong_facts_fail_closed_on_identity_creator_org_or_version_corruption() {
        let make = |requested_id: &str, actual_id: &str, version: u64, org: &str, creator: &str| {
            build_facts(
                DocumentType::CustomerReceipt,
                requested_id,
                actual_id,
                version,
                "RC-1".to_string(),
                org,
                creator,
            )
        };

        assert!(make("receipt-1", "receipt-2", 1, "party-1", "creator-1").is_err());
        assert!(make("receipt-1", "receipt-1", 0, "party-1", "creator-1").is_err());
        assert!(make("receipt-1", "receipt-1", 1, "", "creator-1").is_err());
        assert!(make("receipt-1", "receipt-1", 1, "party-1", "").is_err());
        assert!(make("receipt-1", "receipt-1", 1, " party-1", "creator-1").is_err());
        assert!(make("receipt-1", "receipt-1", 1, "party-1", "creator-1 ").is_err());
    }

    #[test]
    fn sales_business_type_is_exact_and_purchase_rejects_voucher_source() {
        ensure_sales_document_type(DocumentType::SalesOrder, BusinessType::GoodsService)
            .expect("实物服务销售单");
        ensure_sales_document_type(DocumentType::VoucherSalesOrder, BusinessType::Voucher)
            .expect("卡券销售单");
        assert!(ensure_sales_document_type(DocumentType::SalesOrder, BusinessType::Voucher).is_err());
        assert!(
            ensure_sales_document_type(DocumentType::VoucherSalesOrder, BusinessType::GoodsService).is_err()
        );
        assert!(ensure_goods_service_source(BusinessType::Voucher, DocumentType::PurchaseOrder).is_err());
    }

    #[test]
    fn submitted_entity_still_builds_authorization_facts_but_fresh_guard_rejects() {
        use entities::ids::{StockAdjustmentId, WarehouseId};
        use entities::inventory::{AdjustmentReasonType, StockAdjustment, StockAdjustmentData};

        let mut adjustment = StockAdjustment::new(
            StockAdjustmentId::new("adjustment-1"),
            StockAdjustmentData {
                adjustment_no: "ADJ-1".to_string(),
                warehouse_id: WarehouseId::new("warehouse-1"),
                reason_type: AdjustmentReasonType::StockGain,
                prepared_by: "operator-1".to_string(),
                note: None,
                occurred_at: None,
            },
            "creator-1",
        )
        .expect("库存调整单");
        adjustment.start_approval().expect("形成已提交事实");

        let facts = build_facts(
            DocumentType::StockAdjustment,
            "adjustment-1",
            &adjustment.base.id,
            adjustment.base.version,
            adjustment.adjustment_no.clone(),
            adjustment.warehouse_id.as_ref(),
            &adjustment.created_by,
        )
        .expect("Replay 授权事实不得被生命周期门禁拒绝");
        assert_eq!(facts.creator_id, "creator-1");
        assert!(adjustment.ensure_initial_approval_state().is_err());
    }
}
