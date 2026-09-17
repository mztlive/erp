//! 审批绑定升级所需的强业务事实与 Fresh 门禁。
//!
//! 本模块只读取强业务对象及其责任链，不读取 `business_documents` 注册投影，
//! 也不执行授权、绑定写入或事务提交。通用事实加载不得要求对象仍未提交，
//! 从而允许 Replay 在对象后来提交或启动后按当前权限回读原结果。调用方必须在
//! 同一外层事务中先加载事实并完成授权、查询收据；只有无收据的 Fresh 分支才
//! 执行初始未提交门禁、强对象版本重验和绑定 CAS。

use erp_workflow::entity::document_registry::DocumentType;
use erp_workflow::service::approval::business_adapter::BindingRevalidationContext;
use erp_workflow::service::approval::policy::require_process_required;
use mongodb::Database;
use persistence_core::Executor;

use super::upgrade_documents::{
    ensure_fresh_customer_receipt, ensure_fresh_customer_refund, ensure_fresh_payment_reversal,
    ensure_fresh_purchase_change, ensure_fresh_purchase_order, ensure_fresh_receipt_reversal,
    ensure_fresh_sales_change, ensure_fresh_sales_order, ensure_fresh_stock_adjustment,
    ensure_fresh_supplier_refund, load_customer_receipt, load_customer_refund, load_payment_reversal,
    load_purchase_change, load_purchase_order, load_receipt_reversal, load_sales_change, load_sales_order,
    load_stock_adjustment, load_supplier_refund,
};
use super::upgrade_shared::ensure_exact_document_id;
use crate::{Error, Result};

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

/// 审批升级单据的内部路由种类。
///
/// `load` 与 Fresh 门禁共用 [`classify_upgrade_subject`] 这一张分类表；
/// `NO_APPROVAL` 拒绝收敛到该表一处。新增单据类型只需扩展本枚举并在两处
/// 同形分派各加一臂，不得新增第二张分派表。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum UpgradeSubjectKind {
    /// 实物及服务销售单或卡券销售单。
    SalesOrder,
    /// 销售变更单。
    SalesChangeOrder,
    /// 采购单。
    PurchaseOrder,
    /// 采购变更单。
    PurchaseChangeOrder,
    /// 库存调整单。
    StockAdjustment,
    /// 开票申请：创建即提交，不支持升级绑定。
    InvoiceRequestRejected,
    /// 客户回款单。
    CustomerReceipt,
    /// 客户退款单。
    CustomerRefund,
    /// 供应商退款单。
    SupplierRefund,
    /// 回款冲正单。
    ReceiptReversal,
    /// 付款冲正单。
    PaymentReversal,
}

/// load 与 Fresh 门禁共用的唯一单据分类表。
///
/// # 参数
/// * `document_type` - 路由给出的精确单据类型
///
/// # 返回
/// 返回后续 load 与 Fresh 门禁共用的内部路由种类。
///
/// # 错误
/// `NO_APPROVAL` 类型或开票申请以外的政策门禁失败时返回错误；调用方不得
/// 再各自重复 `NO_APPROVAL` 穷尽分派。
fn classify_upgrade_subject(document_type: DocumentType) -> Result<UpgradeSubjectKind> {
    require_process_required(document_type)?;
    match document_type {
        DocumentType::SalesOrder | DocumentType::VoucherSalesOrder => Ok(UpgradeSubjectKind::SalesOrder),
        DocumentType::SalesChangeOrder => Ok(UpgradeSubjectKind::SalesChangeOrder),
        DocumentType::PurchaseOrder => Ok(UpgradeSubjectKind::PurchaseOrder),
        DocumentType::PurchaseChangeOrder => Ok(UpgradeSubjectKind::PurchaseChangeOrder),
        DocumentType::StockAdjustment => Ok(UpgradeSubjectKind::StockAdjustment),
        DocumentType::SalesInvoiceRequest => Ok(UpgradeSubjectKind::InvoiceRequestRejected),
        DocumentType::CustomerReceipt => Ok(UpgradeSubjectKind::CustomerReceipt),
        DocumentType::CustomerRefund => Ok(UpgradeSubjectKind::CustomerRefund),
        DocumentType::SupplierRefund => Ok(UpgradeSubjectKind::SupplierRefund),
        DocumentType::ReceiptReversal => Ok(UpgradeSubjectKind::ReceiptReversal),
        DocumentType::PaymentReversal => Ok(UpgradeSubjectKind::PaymentReversal),
        DocumentType::SupplierPayment
        | DocumentType::PurchaseReceipt
        | DocumentType::Delivery
        | DocumentType::ElectronicDelivery
        | DocumentType::ServiceFulfillment
        | DocumentType::CustomerAcceptance
        | DocumentType::Invoice
        | DocumentType::SalesReturnCase
        | DocumentType::PurchaseReturnOrder => {
            Err(Error::Internal("NO_APPROVAL 类型通过了审批绑定升级政策门禁".to_string()))
        },
    }
}

/// 开票申请升级绑定的统一拒绝。
///
/// # 参数
/// * `document_type` - 仅用于错误文案的单据类型
///
/// # 返回
/// 恒返回冲突错误。
///
/// # 错误
/// 恒返回冲突：开票申请创建即提交，不支持升级已提交申请的审批绑定。
fn reject_invoice_request_upgrade(document_type: DocumentType) -> Error {
    let _ = document_type;
    Error::ConflictError("开票申请创建即提交，不支持升级已提交申请的审批绑定".into())
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
        let kind = classify_upgrade_subject(document_type)?;
        ensure_exact_document_id(document_type, document_id)?;

        match kind {
            UpgradeSubjectKind::SalesOrder => {
                load_sales_order(db, document_type, document_id, executor).await
            },
            UpgradeSubjectKind::SalesChangeOrder => load_sales_change(db, document_id, executor).await,
            UpgradeSubjectKind::PurchaseOrder => load_purchase_order(db, document_id, executor).await,
            UpgradeSubjectKind::PurchaseChangeOrder => load_purchase_change(db, document_id, executor).await,
            UpgradeSubjectKind::StockAdjustment => load_stock_adjustment(db, document_id, executor).await,
            UpgradeSubjectKind::InvoiceRequestRejected => Err(reject_invoice_request_upgrade(document_type)),
            UpgradeSubjectKind::CustomerReceipt => load_customer_receipt(db, document_id, executor).await,
            UpgradeSubjectKind::CustomerRefund => load_customer_refund(db, document_id, executor).await,
            UpgradeSubjectKind::SupplierRefund => load_supplier_refund(db, document_id, executor).await,
            UpgradeSubjectKind::ReceiptReversal => load_receipt_reversal(db, document_id, executor).await,
            UpgradeSubjectKind::PaymentReversal => load_payment_reversal(db, document_id, executor).await,
        }
    }

    /// 构造定义图资格重验所需的强业务上下文。
    ///
    /// # 返回
    /// 返回责任组织与不可变创建人；不读取或注入当前操作人。
    pub fn binding_context(&self) -> BindingRevalidationContext {
        BindingRevalidationContext::new(self.responsible_org_id.clone(), self.creator_id.clone())
    }

    /// 校验客户端期望版本仍等于强业务对象版本。
    ///
    /// # 错误
    /// 版本不一致时返回冲突，调用方不得继续升级注册投影。
    pub fn ensure_expected_business_object_version(&self, expected: u64) -> Result<()> {
        if self.business_object_version != expected {
            return Err(Error::ConflictError("强业务对象版本已变化，请刷新后重试".to_string()));
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
    match classify_upgrade_subject(facts.document_type)? {
        UpgradeSubjectKind::SalesOrder => ensure_fresh_sales_order(db, facts, executor).await,
        UpgradeSubjectKind::SalesChangeOrder => ensure_fresh_sales_change(db, facts, executor).await,
        UpgradeSubjectKind::PurchaseOrder => ensure_fresh_purchase_order(db, facts, executor).await,
        UpgradeSubjectKind::PurchaseChangeOrder => ensure_fresh_purchase_change(db, facts, executor).await,
        UpgradeSubjectKind::StockAdjustment => ensure_fresh_stock_adjustment(db, facts, executor).await,
        UpgradeSubjectKind::InvoiceRequestRejected => {
            Err(reject_invoice_request_upgrade(facts.document_type))
        },
        UpgradeSubjectKind::CustomerReceipt => ensure_fresh_customer_receipt(db, facts, executor).await,
        UpgradeSubjectKind::CustomerRefund => ensure_fresh_customer_refund(db, facts, executor).await,
        UpgradeSubjectKind::SupplierRefund => ensure_fresh_supplier_refund(db, facts, executor).await,
        UpgradeSubjectKind::ReceiptReversal => ensure_fresh_receipt_reversal(db, facts, executor).await,
        UpgradeSubjectKind::PaymentReversal => ensure_fresh_payment_reversal(db, facts, executor).await,
    }
}

/// Process-owned upgrade-subject adapter over remaining domain aggregates.
pub struct ProcessUpgradeSubject {
    db: Database,
}

impl ProcessUpgradeSubject {
    /// Bind the adapter to a database.
    ///
    /// # 参数
    /// * `db` - MongoDB 数据库
    ///
    /// # 返回
    /// 返回可注入 workflow `UpgradeSubjectPort` 的适配器。
    ///
    /// # 错误
    /// 无。
    pub fn new(db: Database) -> Self {
        Self { db }
    }
}

fn map_upgrade_error(error: Error) -> erp_workflow::Error {
    match error {
        Error::ValidationError(message) => erp_workflow::Error::ValidationError(message),
        Error::NotFound(message) => erp_workflow::Error::NotFound(message),
        Error::BusinessLogicError(message) => erp_workflow::Error::BusinessLogicError(message),
        Error::ConflictError(message) => erp_workflow::Error::ConflictError(message),
        Error::Forbidden(message) => erp_workflow::Error::Forbidden(message),
        Error::Unauthenticated(message) => erp_workflow::Error::Unauthenticated(message),
        Error::Internal(message) => erp_workflow::Error::Internal(message),
        Error::Logic(error) => erp_workflow::Error::Logic(error),
        Error::OutcomeUnknown(error) => erp_workflow::Error::OutcomeUnknown(error),
        Error::RepositoryError(error) => erp_workflow::Error::RepositoryError(error),
        other => erp_workflow::Error::Internal(other.to_string()),
    }
}

fn to_workflow_facts(facts: ApprovalUpgradeSubjectFacts) -> erp_workflow::ApprovalUpgradeSubjectFacts {
    erp_workflow::ApprovalUpgradeSubjectFacts {
        document_type: facts.document_type,
        document_id: facts.document_id,
        business_object_version: facts.business_object_version,
        document_no: facts.document_no,
        responsible_org_id: facts.responsible_org_id,
        creator_id: facts.creator_id,
    }
}

fn to_entities_facts(facts: &erp_workflow::ApprovalUpgradeSubjectFacts) -> ApprovalUpgradeSubjectFacts {
    ApprovalUpgradeSubjectFacts {
        document_type: facts.document_type,
        document_id: facts.document_id.clone(),
        business_object_version: facts.business_object_version,
        document_no: facts.document_no.clone(),
        responsible_org_id: facts.responsible_org_id.clone(),
        creator_id: facts.creator_id.clone(),
    }
}

#[async_trait::async_trait]
impl erp_workflow::UpgradeSubjectPort for ProcessUpgradeSubject {
    async fn load(
        &self,
        document_type: erp_workflow::DocumentType,
        document_id: &str,
        executor: &mut dyn Executor,
    ) -> erp_workflow::Result<erp_workflow::ApprovalUpgradeSubjectFacts> {
        let facts = load_approval_upgrade_subject_facts(&self.db, document_type, document_id, executor)
            .await
            .map_err(map_upgrade_error)?;
        Ok(to_workflow_facts(facts))
    }

    async fn ensure_initial_unsubmitted(
        &self,
        facts: &erp_workflow::ApprovalUpgradeSubjectFacts,
        executor: &mut dyn Executor,
    ) -> erp_workflow::Result<()> {
        ensure_initial_unsubmitted_approval_upgrade_subject(&self.db, &to_entities_facts(facts), executor)
            .await
            .map_err(map_upgrade_error)
    }
}

#[cfg(test)]
mod tests {
    use erp_sales::entity::sales_order::BusinessType;

    use super::super::upgrade_shared::{
        build_facts, ensure_goods_service_source, ensure_sales_document_type,
    };
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
            BindingRevalidationContext::new("warehouse-1".to_string(), "creator-1".to_string())
        );
        facts.ensure_expected_business_object_version(7).expect("强对象版本一致");
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
        use erp_core::ids::{StockAdjustmentId, WarehouseId};
        use erp_inventory::{AdjustmentReasonType, StockAdjustment, StockAdjustmentData};

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
