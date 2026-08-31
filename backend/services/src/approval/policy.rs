//! 合同 §4.3 的 20 行穷尽审批政策。
//!
//! 政策由代码注册，禁止默认政策、Noop 动作或复制 `process_kind.rs` 映射。

use bpm::ProcessKind;
use entities::document_registry::DocumentType;
use entities::Permission;

use crate::errors::{Error, ErrorCode, Result};

use super::process_kind::process_kind_of;

/// 历史销售单草稿可能仍带有的采购确认用途键。发布不再要求，保存时清除。
pub const SALES_ORDER_PROCUREMENT_CONFIRMATION: &str = "SALES_ORDER_PROCUREMENT_CONFIRMATION";

/// 定义期校验指定审批人的静态审批权限。
pub const STATIC_APPROVE_PERMISSION: &str = "approval_instance:decide";

/// 合同 §4.3 固定 20 个单据类型，复用实体层权威穷尽集合。
pub const ALL_DOCUMENT_TYPES: [DocumentType; 20] = DocumentType::ALL;

const NO_PURPOSES: &[ApprovalNodePurpose] = &[];

const SALES_PURCHASE_SNAPSHOT: &[ApprovalSubjectSnapshotField] = &[
    ApprovalSubjectSnapshotField::DocumentNo,
    ApprovalSubjectSnapshotField::ResponsibleOrgId,
    ApprovalSubjectSnapshotField::SubmittedBy,
    ApprovalSubjectSnapshotField::SubmittedAt,
    ApprovalSubjectSnapshotField::CounterpartyOptional,
    ApprovalSubjectSnapshotField::TotalAmount,
    ApprovalSubjectSnapshotField::TotalQuantity,
    ApprovalSubjectSnapshotField::LineCount,
];
const FINANCE_SNAPSHOT: &[ApprovalSubjectSnapshotField] = &[
    ApprovalSubjectSnapshotField::DocumentNo,
    ApprovalSubjectSnapshotField::ResponsibleOrgId,
    ApprovalSubjectSnapshotField::SubmittedBy,
    ApprovalSubjectSnapshotField::SubmittedAt,
    ApprovalSubjectSnapshotField::CounterpartyOptional,
    ApprovalSubjectSnapshotField::TotalAmount,
    ApprovalSubjectSnapshotField::LineCount,
];
const STOCK_SNAPSHOT: &[ApprovalSubjectSnapshotField] = &[
    ApprovalSubjectSnapshotField::DocumentNo,
    ApprovalSubjectSnapshotField::ResponsibleOrgId,
    ApprovalSubjectSnapshotField::SubmittedBy,
    ApprovalSubjectSnapshotField::SubmittedAt,
    ApprovalSubjectSnapshotField::CounterpartyOptional,
    ApprovalSubjectSnapshotField::TotalQuantity,
    ApprovalSubjectSnapshotField::LineCount,
];

/// 单据类型审批要求。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ApprovalRequirement {
    /// 该类型不启动审批。
    NoApproval,
    /// 创建单据前必须存在可绑定的已发布定义。
    ProcessRequired,
}

/// 发布时必须满足的 ERP 节点用途。当前政策均不要求用途。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ApprovalNodePurpose {
    /// 历史 `SalesOrder` 采购确认用途，仅用于识别遗留数据。
    SalesOrderProcurementConfirmation,
}

impl ApprovalNodePurpose {
    /// 返回稳定用途键。
    ///
    /// # 返回
    /// 返回合同签署的用途代码。
    pub fn as_str(self) -> &'static str {
        match self {
            Self::SalesOrderProcurementConfirmation => SALES_ORDER_PROCUREMENT_CONFIRMATION,
        }
    }
}

/// 冻结提交版本的权威来源。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ApprovalSubjectVersionSource {
    /// `sales_order_submission.submission_no`。
    SalesOrderSubmissionNo,
    /// `sales_change_submission.submission_no`。
    SalesChangeSubmissionNo,
    /// 业务实体 `approval_subject_version`。
    EntityApprovalSubjectVersion,
}

/// 启动时冻结的有界快照字段。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ApprovalSubjectSnapshotField {
    /// 单据业务编号。
    DocumentNo,
    /// 责任组织。
    ResponsibleOrgId,
    /// 提交人。
    SubmittedBy,
    /// 提交时间。
    SubmittedAt,
    /// 可空对手方。
    CounterpartyOptional,
    /// 金额合计。
    TotalAmount,
    /// 数量合计。
    TotalQuantity,
    /// 行数。
    LineCount,
}

/// 审批任务 `owner_role` 的稳定语义标签。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WorkItemOwnerRole {
    value: &'static str,
}

impl WorkItemOwnerRole {
    /// 返回稳定责任角色标签。
    ///
    /// # 返回
    /// 返回 `<prefix>_approver`。
    pub fn as_str(self) -> &'static str {
        self.value
    }
}

/// 审批任务责任组织来源。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OwnerOrganizationSource {
    /// 取 `subject_snapshot.responsible_org_id`。
    SubjectSnapshotResponsibleOrgId,
}

/// 定义期与运行期的审批人资格策略。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ApproverEligibilityPolicy {
    /// 后台有效账号且具备静态 `approval_instance:decide`。
    ActiveBackofficeWithDecidePermission,
}

/// 提交人、经办人与审批人之间的岗位分离。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SeparationOfDutiesPolicy {
    /// 禁止提交人审批自己的提交；节点间允许同一审批人。
    ForbidSubmitterAsApprover,
}

/// 合同 §4.4.4 签署的强类型领域动作。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ApprovalDomainAction {
    /// 销售单提交并启动审批。
    SalesOrderStartApprovalSubmission,
    /// 销售单最终通过并形式化提交。
    SalesOrderFormalizeApprovedSubmission,
    /// 销售单撤回审批提交。
    SalesOrderCancelApprovalSubmission,
    /// 卡券销售单提交并启动审批。
    VoucherSalesOrderStartApprovalSubmission,
    /// 卡券销售单最终通过并形式化提交。
    VoucherSalesOrderFormalizeApprovedSubmission,
    /// 卡券销售单撤回审批提交。
    VoucherSalesOrderCancelApprovalSubmission,
    /// 销售变更单提交。
    SalesChangeOrderSubmitSalesChange,
    /// 销售变更单最终生效。
    SalesChangeOrderApplyEffectiveChange,
    /// 销售变更单撤回审批。
    SalesChangeOrderCancelApproval,
    /// 采购单提交。
    PurchaseOrderSubmit,
    /// 采购单最终形式化。
    PurchaseOrderFormalizeApprovedOrder,
    /// 采购单撤回审批。
    PurchaseOrderCancelApproval,
    /// 采购变更单提交。
    PurchaseChangeOrderSubmitChange,
    /// 采购变更单最终生效。
    PurchaseChangeOrderApplyEffectiveChange,
    /// 采购变更单撤回审批。
    PurchaseChangeOrderCancelApproval,
    /// 库存调整单提交。
    StockAdjustmentSubmit,
    /// 库存调整单过账。
    StockAdjustmentPost,
    /// 库存调整单撤回审批。
    StockAdjustmentCancelApproval,
    /// 客户回款单提交。
    CustomerReceiptSubmit,
    /// 客户回款单过账。
    CustomerReceiptPost,
    /// 客户回款单撤回审批。
    CustomerReceiptCancelApproval,
    /// 客户退款单提交。
    CustomerRefundSubmit,
    /// 客户退款单过账。
    CustomerRefundPost,
    /// 客户退款单撤回审批。
    CustomerRefundCancelApproval,
    /// 供应商退款单提交。
    SupplierRefundSubmit,
    /// 供应商退款单过账。
    SupplierRefundPost,
    /// 供应商退款单撤回审批。
    SupplierRefundCancelApproval,
    /// 回款冲正单提交。
    ReceiptReversalSubmit,
    /// 回款冲正单过账。
    ReceiptReversalPost,
    /// 回款冲正单撤回审批。
    ReceiptReversalCancelApproval,
    /// 付款冲正单提交。
    PaymentReversalSubmit,
    /// 付款冲正单过账。
    PaymentReversalPost,
    /// 付款冲正单撤回审批。
    PaymentReversalCancelApproval,
}

impl ApprovalDomainAction {
    /// 返回稳定动作代码。
    ///
    /// # 返回
    /// 返回合同端口名对应的稳定字符串。
    pub fn as_str(self) -> &'static str {
        match self {
            Self::SalesOrderStartApprovalSubmission => "SalesOrderService::start_approval_submission",
            Self::SalesOrderFormalizeApprovedSubmission => "SalesOrderService::formalize_approved_submission",
            Self::SalesOrderCancelApprovalSubmission => "SalesOrderService::cancel_approval_submission",
            Self::VoucherSalesOrderStartApprovalSubmission => "SalesOrderService::start_approval_submission",
            Self::VoucherSalesOrderFormalizeApprovedSubmission => {
                "SalesOrderService::formalize_approved_submission"
            }
            Self::VoucherSalesOrderCancelApprovalSubmission => {
                "SalesOrderService::cancel_approval_submission"
            }
            Self::SalesChangeOrderSubmitSalesChange => "SalesChangeOrderService::submit_sales_change",
            Self::SalesChangeOrderApplyEffectiveChange => "SalesChangeOrderService::apply_effective_change",
            Self::SalesChangeOrderCancelApproval => "SalesChangeOrderService::cancel_approval",
            Self::PurchaseOrderSubmit => "PurchaseOrderService::submit",
            Self::PurchaseOrderFormalizeApprovedOrder => "PurchaseOrderService::formalize_approved_order",
            Self::PurchaseOrderCancelApproval => "PurchaseOrderService::cancel_approval",
            Self::PurchaseChangeOrderSubmitChange => "PurchaseChangeService::submit_change",
            Self::PurchaseChangeOrderApplyEffectiveChange => "PurchaseChangeService::apply_effective_change",
            Self::PurchaseChangeOrderCancelApproval => "PurchaseChangeService::cancel_approval",
            Self::StockAdjustmentSubmit => "InventoryService::submit_stock_adjustment",
            Self::StockAdjustmentPost => "InventoryService::post_stock_adjustment",
            Self::StockAdjustmentCancelApproval => "InventoryService::cancel_stock_adjustment_approval",
            Self::CustomerReceiptSubmit => "ReceivableService::submit_customer_receipt",
            Self::CustomerReceiptPost => "ReceivableService::post_customer_receipt",
            Self::CustomerReceiptCancelApproval => "ReceivableService::cancel_customer_receipt_approval",
            Self::CustomerRefundSubmit => "ReturnsService::submit_customer_refund",
            Self::CustomerRefundPost => "ReturnsService::post_customer_refund",
            Self::CustomerRefundCancelApproval => "ReturnsService::cancel_customer_refund_approval",
            Self::SupplierRefundSubmit => "ReturnsService::submit_supplier_refund",
            Self::SupplierRefundPost => "ReturnsService::post_supplier_refund",
            Self::SupplierRefundCancelApproval => "ReturnsService::cancel_supplier_refund_approval",
            Self::ReceiptReversalSubmit => "ReturnsService::submit_receipt_reversal",
            Self::ReceiptReversalPost => "ReturnsService::post_receipt_reversal",
            Self::ReceiptReversalCancelApproval => "ReturnsService::cancel_receipt_reversal_approval",
            Self::PaymentReversalSubmit => "ReturnsService::submit_payment_reversal",
            Self::PaymentReversalPost => "ReturnsService::post_payment_reversal",
            Self::PaymentReversalCancelApproval => "ReturnsService::cancel_payment_reversal_approval",
        }
    }
}

/// 无审批政策。只含单据类型、要求与流程种类。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NoApprovalPolicy {
    /// 固定单据类型。
    pub document_type: DocumentType,
    /// 对应 BPM 流程种类。
    pub process_kind: ProcessKind,
}

/// 必须审批的完整运行政策。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProcessRequiredApprovalPolicy {
    /// 固定单据类型。
    pub document_type: DocumentType,
    /// 对应 BPM 流程种类。
    pub process_kind: ProcessKind,
    /// 类型级定义管理权限。
    pub definition_admin_permission: Permission,
    /// 类型级运行管理权限。
    pub runtime_admin_permission: Permission,
    /// 审批人资格策略。
    pub approver_eligibility_policy: ApproverEligibilityPolicy,
    /// 岗位分离策略。
    pub separation_of_duties_policy: SeparationOfDutiesPolicy,
    /// 发布时必须恰好满足的用途集合。销售单与其它必须审批类型均为空。
    pub required_node_purposes: &'static [ApprovalNodePurpose],
    /// 提交版本权威来源。
    pub subject_version_source: ApprovalSubjectVersionSource,
    /// 启动快照字段。
    pub subject_snapshot_fields: &'static [ApprovalSubjectSnapshotField],
    /// WorkItem 责任角色。
    pub work_item_owner_role: WorkItemOwnerRole,
    /// WorkItem 责任组织来源。
    pub owner_organization_source: OwnerOrganizationSource,
    /// 提交并启动动作。
    pub start_action: ApprovalDomainAction,
    /// 最终通过动作。
    pub final_approve_action: ApprovalDomainAction,
    /// 撤回与受阻取消动作。
    pub cancel_action: ApprovalDomainAction,
}

/// 合同 §4.3 的穷尽政策。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DocumentApprovalPolicy {
    /// 无需审批。
    NoApproval(NoApprovalPolicy),
    /// 必须审批。
    ProcessRequired(ProcessRequiredApprovalPolicy),
}

impl DocumentApprovalPolicy {
    /// 返回审批要求。
    ///
    /// # 返回
    /// 返回 `NO_APPROVAL` 或 `PROCESS_REQUIRED`。
    pub fn requirement(&self) -> ApprovalRequirement {
        match self {
            Self::NoApproval(_) => ApprovalRequirement::NoApproval,
            Self::ProcessRequired(_) => ApprovalRequirement::ProcessRequired,
        }
    }

    /// 返回对应流程种类。
    ///
    /// # 返回
    /// 返回已冻结映射的 `ProcessKind`。
    pub fn process_kind(&self) -> ProcessKind {
        match self {
            Self::NoApproval(policy) => policy.process_kind,
            Self::ProcessRequired(policy) => policy.process_kind,
        }
    }

    /// 返回单据类型。
    ///
    /// # 返回
    /// 返回政策绑定的 `DocumentType`。
    pub fn document_type(&self) -> DocumentType {
        match self {
            Self::NoApproval(policy) => policy.document_type,
            Self::ProcessRequired(policy) => policy.document_type,
        }
    }
}

/// 按合同 §4.3 返回该单据类型的唯一政策。
///
/// # 参数
/// * `document_type` - 固定单据类型
///
/// # 返回
/// 返回穷尽注册的政策。
///
/// # 错误
/// 类型级权限字符串无法解析时返回部署不变量错误。
pub fn policy_of(document_type: DocumentType) -> Result<DocumentApprovalPolicy> {
    match document_type {
        DocumentType::SalesOrder => process_required(
            document_type,
            NO_PURPOSES,
            ApprovalSubjectVersionSource::SalesOrderSubmissionNo,
            SALES_PURCHASE_SNAPSHOT,
            ApprovalDomainAction::SalesOrderStartApprovalSubmission,
            ApprovalDomainAction::SalesOrderFormalizeApprovedSubmission,
            ApprovalDomainAction::SalesOrderCancelApprovalSubmission,
        ),
        DocumentType::VoucherSalesOrder => process_required(
            document_type,
            NO_PURPOSES,
            ApprovalSubjectVersionSource::SalesOrderSubmissionNo,
            SALES_PURCHASE_SNAPSHOT,
            ApprovalDomainAction::VoucherSalesOrderStartApprovalSubmission,
            ApprovalDomainAction::VoucherSalesOrderFormalizeApprovedSubmission,
            ApprovalDomainAction::VoucherSalesOrderCancelApprovalSubmission,
        ),
        DocumentType::SalesChangeOrder => process_required(
            document_type,
            NO_PURPOSES,
            ApprovalSubjectVersionSource::SalesChangeSubmissionNo,
            SALES_PURCHASE_SNAPSHOT,
            ApprovalDomainAction::SalesChangeOrderSubmitSalesChange,
            ApprovalDomainAction::SalesChangeOrderApplyEffectiveChange,
            ApprovalDomainAction::SalesChangeOrderCancelApproval,
        ),
        DocumentType::PurchaseOrder => process_required(
            document_type,
            NO_PURPOSES,
            ApprovalSubjectVersionSource::EntityApprovalSubjectVersion,
            SALES_PURCHASE_SNAPSHOT,
            ApprovalDomainAction::PurchaseOrderSubmit,
            ApprovalDomainAction::PurchaseOrderFormalizeApprovedOrder,
            ApprovalDomainAction::PurchaseOrderCancelApproval,
        ),
        DocumentType::PurchaseChangeOrder => process_required(
            document_type,
            NO_PURPOSES,
            ApprovalSubjectVersionSource::EntityApprovalSubjectVersion,
            SALES_PURCHASE_SNAPSHOT,
            ApprovalDomainAction::PurchaseChangeOrderSubmitChange,
            ApprovalDomainAction::PurchaseChangeOrderApplyEffectiveChange,
            ApprovalDomainAction::PurchaseChangeOrderCancelApproval,
        ),
        DocumentType::StockAdjustment => process_required(
            document_type,
            NO_PURPOSES,
            ApprovalSubjectVersionSource::EntityApprovalSubjectVersion,
            STOCK_SNAPSHOT,
            ApprovalDomainAction::StockAdjustmentSubmit,
            ApprovalDomainAction::StockAdjustmentPost,
            ApprovalDomainAction::StockAdjustmentCancelApproval,
        ),
        DocumentType::CustomerReceipt => finance_required(
            document_type,
            ApprovalDomainAction::CustomerReceiptSubmit,
            ApprovalDomainAction::CustomerReceiptPost,
            ApprovalDomainAction::CustomerReceiptCancelApproval,
        ),
        DocumentType::SupplierPayment => Ok(no_approval(document_type)),
        DocumentType::CustomerRefund => finance_required(
            document_type,
            ApprovalDomainAction::CustomerRefundSubmit,
            ApprovalDomainAction::CustomerRefundPost,
            ApprovalDomainAction::CustomerRefundCancelApproval,
        ),
        DocumentType::SupplierRefund => finance_required(
            document_type,
            ApprovalDomainAction::SupplierRefundSubmit,
            ApprovalDomainAction::SupplierRefundPost,
            ApprovalDomainAction::SupplierRefundCancelApproval,
        ),
        DocumentType::ReceiptReversal => finance_required(
            document_type,
            ApprovalDomainAction::ReceiptReversalSubmit,
            ApprovalDomainAction::ReceiptReversalPost,
            ApprovalDomainAction::ReceiptReversalCancelApproval,
        ),
        DocumentType::PaymentReversal => finance_required(
            document_type,
            ApprovalDomainAction::PaymentReversalSubmit,
            ApprovalDomainAction::PaymentReversalPost,
            ApprovalDomainAction::PaymentReversalCancelApproval,
        ),
        DocumentType::PurchaseReceipt
        | DocumentType::Delivery
        | DocumentType::ElectronicDelivery
        | DocumentType::ServiceFulfillment
        | DocumentType::CustomerAcceptance
        | DocumentType::Invoice
        | DocumentType::SalesReturnCase
        | DocumentType::PurchaseReturnOrder => Ok(no_approval(document_type)),
    }
}

/// 要求该类型政策为必须审批并返回完整政策。
///
/// # 错误
/// 类型为 `NO_APPROVAL` 或政策读取失败时返回错误。
pub fn require_process_required(document_type: DocumentType) -> Result<ProcessRequiredApprovalPolicy> {
    match policy_of(document_type)? {
        DocumentApprovalPolicy::ProcessRequired(policy) => Ok(policy),
        DocumentApprovalPolicy::NoApproval(_) => Err(Error::BusinessLogicError(format!(
            "{} 无需审批，不能管理流程定义",
            document_type.label()
        ))),
    }
}

/// 校验三类强类型动作均已注册且互不相同。
///
/// # 错误
/// 动作相同或无法穷尽识别时返回部署不变量错误。
pub fn ensure_actions_registered(policy: &ProcessRequiredApprovalPolicy) -> Result<()> {
    ensure_real_action(policy.start_action)?;
    ensure_real_action(policy.final_approve_action)?;
    ensure_real_action(policy.cancel_action)?;
    if policy.start_action == policy.final_approve_action
        || policy.start_action == policy.cancel_action
        || policy.final_approve_action == policy.cancel_action
    {
        return Err(Error::from_approval_code(ErrorCode::ApprovalPolicyNotRegistered));
    }
    Ok(())
}

/// 按政策校验节点用途完整性。
///
/// # 参数
/// * `policy` - 必须审批政策
/// * `node_purposes` - 当前定义各节点用途
///
/// # 返回
/// 校验通过时返回 `()`。
///
/// # 错误
/// 非销售单出现任何用途，或销售单出现未知用途时返回错误。销售单允许没有用途，也允许遗留采购确认用途。
pub fn validate_required_purposes(
    policy: &ProcessRequiredApprovalPolicy,
    node_purposes: &[Option<&str>],
) -> Result<()> {
    let actual = node_purposes
        .iter()
        .filter_map(|purpose| *purpose)
        .collect::<Vec<_>>();
    match policy.required_node_purposes {
        [] => {
            if policy.document_type == DocumentType::SalesOrder {
                return ensure_sales_order_purposes_optional(&actual);
            }
            if actual.is_empty() {
                return Ok(());
            }
            Err(Error::ValidationError(format!(
                "{} 不得包含节点用途",
                policy.document_type.label()
            )))
        }
        _ => Err(Error::from_approval_code(ErrorCode::ApprovalPolicyNotRegistered)),
    }
}

/// 构造资金类必须审批政策。
///
/// # 错误
/// 类型级权限无法解析时返回部署不变量错误。
fn finance_required(
    document_type: DocumentType,
    start_action: ApprovalDomainAction,
    final_approve_action: ApprovalDomainAction,
    cancel_action: ApprovalDomainAction,
) -> Result<DocumentApprovalPolicy> {
    process_required(
        document_type,
        NO_PURPOSES,
        ApprovalSubjectVersionSource::EntityApprovalSubjectVersion,
        FINANCE_SNAPSHOT,
        start_action,
        final_approve_action,
        cancel_action,
    )
}

/// 构造必须审批政策。
///
/// # 错误
/// 类型级权限无法解析或该类型不应注册运行政策时返回错误。
fn process_required(
    document_type: DocumentType,
    required_node_purposes: &'static [ApprovalNodePurpose],
    subject_version_source: ApprovalSubjectVersionSource,
    subject_snapshot_fields: &'static [ApprovalSubjectSnapshotField],
    start_action: ApprovalDomainAction,
    final_approve_action: ApprovalDomainAction,
    cancel_action: ApprovalDomainAction,
) -> Result<DocumentApprovalPolicy> {
    Ok(DocumentApprovalPolicy::ProcessRequired(
        ProcessRequiredApprovalPolicy {
            document_type,
            process_kind: process_kind_of(document_type),
            definition_admin_permission: type_permission(document_type, "approval_definition_admin")?,
            runtime_admin_permission: type_permission(document_type, "approval_runtime_admin")?,
            approver_eligibility_policy: ApproverEligibilityPolicy::ActiveBackofficeWithDecidePermission,
            separation_of_duties_policy: SeparationOfDutiesPolicy::ForbidSubmitterAsApprover,
            required_node_purposes,
            subject_version_source,
            subject_snapshot_fields,
            work_item_owner_role: owner_role(document_type)?,
            owner_organization_source: OwnerOrganizationSource::SubjectSnapshotResponsibleOrgId,
            start_action,
            final_approve_action,
            cancel_action,
        },
    ))
}

/// 构造无审批政策。
fn no_approval(document_type: DocumentType) -> DocumentApprovalPolicy {
    DocumentApprovalPolicy::NoApproval(NoApprovalPolicy {
        document_type,
        process_kind: process_kind_of(document_type),
    })
}

/// 解析类型级管理权限。
///
/// # 错误
/// 权限字符串不符合 `resource:action` 时返回部署不变量错误。
fn type_permission(document_type: DocumentType, action: &str) -> Result<Permission> {
    Permission::parse(format!("{}:{action}", document_type.as_str()))
        .map_err(|_| Error::from_approval_code(ErrorCode::ApprovalPolicyNotRegistered))
}

/// 返回该必须审批类型的稳定责任角色。
///
/// # 错误
/// 无审批类型误入时返回部署不变量错误。
fn owner_role(document_type: DocumentType) -> Result<WorkItemOwnerRole> {
    let value = match document_type {
        DocumentType::SalesOrder => "sales_order_approver",
        DocumentType::VoucherSalesOrder => "voucher_sales_order_approver",
        DocumentType::SalesChangeOrder => "sales_change_order_approver",
        DocumentType::PurchaseOrder => "purchase_order_approver",
        DocumentType::PurchaseChangeOrder => "purchase_change_order_approver",
        DocumentType::StockAdjustment => "stock_adjustment_approver",
        DocumentType::CustomerReceipt => "customer_receipt_approver",
        DocumentType::CustomerRefund => "customer_refund_approver",
        DocumentType::SupplierRefund => "supplier_refund_approver",
        DocumentType::ReceiptReversal => "receipt_reversal_approver",
        DocumentType::PaymentReversal => "payment_reversal_approver",
        _ => return Err(Error::Internal("无审批类型不得注册责任角色".to_string())),
    };
    Ok(WorkItemOwnerRole { value })
}

/// 穷尽识别已注册领域动作。
///
/// # 错误
/// 编译器将拒绝未覆盖变体；运行期不会落到失败分支。
fn ensure_real_action(action: ApprovalDomainAction) -> Result<()> {
    match action {
        ApprovalDomainAction::SalesOrderStartApprovalSubmission
        | ApprovalDomainAction::SalesOrderFormalizeApprovedSubmission
        | ApprovalDomainAction::SalesOrderCancelApprovalSubmission
        | ApprovalDomainAction::VoucherSalesOrderStartApprovalSubmission
        | ApprovalDomainAction::VoucherSalesOrderFormalizeApprovedSubmission
        | ApprovalDomainAction::VoucherSalesOrderCancelApprovalSubmission
        | ApprovalDomainAction::SalesChangeOrderSubmitSalesChange
        | ApprovalDomainAction::SalesChangeOrderApplyEffectiveChange
        | ApprovalDomainAction::SalesChangeOrderCancelApproval
        | ApprovalDomainAction::PurchaseOrderSubmit
        | ApprovalDomainAction::PurchaseOrderFormalizeApprovedOrder
        | ApprovalDomainAction::PurchaseOrderCancelApproval
        | ApprovalDomainAction::PurchaseChangeOrderSubmitChange
        | ApprovalDomainAction::PurchaseChangeOrderApplyEffectiveChange
        | ApprovalDomainAction::PurchaseChangeOrderCancelApproval
        | ApprovalDomainAction::StockAdjustmentSubmit
        | ApprovalDomainAction::StockAdjustmentPost
        | ApprovalDomainAction::StockAdjustmentCancelApproval
        | ApprovalDomainAction::CustomerReceiptSubmit
        | ApprovalDomainAction::CustomerReceiptPost
        | ApprovalDomainAction::CustomerReceiptCancelApproval
        | ApprovalDomainAction::CustomerRefundSubmit
        | ApprovalDomainAction::CustomerRefundPost
        | ApprovalDomainAction::CustomerRefundCancelApproval
        | ApprovalDomainAction::SupplierRefundSubmit
        | ApprovalDomainAction::SupplierRefundPost
        | ApprovalDomainAction::SupplierRefundCancelApproval
        | ApprovalDomainAction::ReceiptReversalSubmit
        | ApprovalDomainAction::ReceiptReversalPost
        | ApprovalDomainAction::ReceiptReversalCancelApproval
        | ApprovalDomainAction::PaymentReversalSubmit
        | ApprovalDomainAction::PaymentReversalPost
        | ApprovalDomainAction::PaymentReversalCancelApproval => Ok(()),
    }
}

/// 销售单不再强制采购确认用途；仅拒绝未知用途字符串。
///
/// # 参数
/// * `actual` - 节点上已出现的用途值
///
/// # 返回
/// 全为空或均为遗留采购确认用途时返回 `()`。
///
/// # 错误
/// 出现其它用途字符串时返回校验错误。
fn ensure_sales_order_purposes_optional(actual: &[&str]) -> Result<()> {
    if actual
        .iter()
        .all(|purpose| *purpose == ApprovalNodePurpose::SalesOrderProcurementConfirmation.as_str())
    {
        return Ok(());
    }
    Err(Error::ValidationError("销售单不得包含未知节点用途".to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::approval::process_kind::{document_type_of, process_kind_of};

    /// 20 行政策穷尽：11 个 PROCESS_REQUIRED 逐行断言矩阵，9 个 NO_APPROVAL 仅身份字段。
    #[test]
    fn policies_are_exhaustive_and_match_process_kind() {
        assert_eq!(ALL_DOCUMENT_TYPES.len(), 20);
        let mut required = 0;
        let mut no_approval = 0;
        for document_type in ALL_DOCUMENT_TYPES {
            let policy = policy_of(document_type).expect("政策必须可构造");
            assert_eq!(policy.document_type(), document_type);
            assert_eq!(policy.process_kind(), process_kind_of(document_type));
            assert_eq!(document_type_of(policy.process_kind()), document_type);
            match expected_process_required(document_type) {
                Some(expected) => {
                    required += 1;
                    assert_required_policy(&policy, document_type, expected);
                }
                None => {
                    no_approval += 1;
                    assert_no_approval_policy(&policy, document_type);
                }
            }
        }
        assert_eq!(required, 11);
        assert_eq!(no_approval, 9);
    }

    /// 无审批政策不得进入定义管理。
    #[test]
    fn no_approval_policy_has_only_identity_fields() {
        for document_type in ALL_DOCUMENT_TYPES {
            if expected_process_required(document_type).is_some() {
                continue;
            }
            let policy = policy_of(document_type).expect("无审批政策必须存在");
            assert_no_approval_policy(&policy, document_type);
        }
    }

    /// 销售单政策不含强制用途、提交版本与三类真实动作。
    #[test]
    fn sales_order_policy_matches_contract_matrix() {
        let policy = require_process_required(DocumentType::SalesOrder).expect("销售单必须审批");
        assert_eq!(policy.required_node_purposes, NO_PURPOSES);
        assert_eq!(
            policy.subject_version_source,
            ApprovalSubjectVersionSource::SalesOrderSubmissionNo
        );
        assert_eq!(policy.subject_snapshot_fields, SALES_PURCHASE_SNAPSHOT);
        assert_eq!(policy.work_item_owner_role.as_str(), "sales_order_approver");
        assert_eq!(
            policy.owner_organization_source,
            OwnerOrganizationSource::SubjectSnapshotResponsibleOrgId
        );
        ensure_actions_registered(&policy).expect("三类动作必须已注册");
        assert_eq!(
            policy.start_action,
            ApprovalDomainAction::SalesOrderStartApprovalSubmission
        );
        assert_ne!(policy.start_action, policy.cancel_action);
    }

    /// 库存调整使用实体版本与数量快照，不含金额必填。
    #[test]
    fn stock_adjustment_uses_entity_version_and_quantity_snapshot() {
        let policy = require_process_required(DocumentType::StockAdjustment).expect("库存调整必须审批");
        assert!(policy.required_node_purposes.is_empty());
        assert_eq!(
            policy.subject_version_source,
            ApprovalSubjectVersionSource::EntityApprovalSubjectVersion
        );
        assert_eq!(policy.subject_snapshot_fields, STOCK_SNAPSHOT);
        assert!(policy
            .subject_snapshot_fields
            .contains(&ApprovalSubjectSnapshotField::TotalQuantity));
        assert!(!policy
            .subject_snapshot_fields
            .contains(&ApprovalSubjectSnapshotField::TotalAmount));
    }

    /// 资金类快照必填金额、不要求数量。
    #[test]
    fn finance_policies_require_amount_not_quantity() {
        for document_type in [
            DocumentType::CustomerReceipt,
            DocumentType::CustomerRefund,
            DocumentType::SupplierRefund,
            DocumentType::ReceiptReversal,
            DocumentType::PaymentReversal,
        ] {
            let policy = require_process_required(document_type).expect("资金类必须审批");
            assert_eq!(policy.subject_snapshot_fields, FINANCE_SNAPSHOT);
            assert!(policy.required_node_purposes.is_empty());
        }
    }

    struct ExpectedRequiredPolicy {
        purposes: &'static [ApprovalNodePurpose],
        version: ApprovalSubjectVersionSource,
        snapshot: &'static [ApprovalSubjectSnapshotField],
        owner_role: &'static str,
        start: ApprovalDomainAction,
        approve: ApprovalDomainAction,
        cancel: ApprovalDomainAction,
    }

    fn expected_process_required(document_type: DocumentType) -> Option<ExpectedRequiredPolicy> {
        match document_type {
            DocumentType::SalesOrder => Some(ExpectedRequiredPolicy {
                purposes: NO_PURPOSES,
                version: ApprovalSubjectVersionSource::SalesOrderSubmissionNo,
                snapshot: SALES_PURCHASE_SNAPSHOT,
                owner_role: "sales_order_approver",
                start: ApprovalDomainAction::SalesOrderStartApprovalSubmission,
                approve: ApprovalDomainAction::SalesOrderFormalizeApprovedSubmission,
                cancel: ApprovalDomainAction::SalesOrderCancelApprovalSubmission,
            }),
            DocumentType::VoucherSalesOrder => Some(ExpectedRequiredPolicy {
                purposes: NO_PURPOSES,
                version: ApprovalSubjectVersionSource::SalesOrderSubmissionNo,
                snapshot: SALES_PURCHASE_SNAPSHOT,
                owner_role: "voucher_sales_order_approver",
                start: ApprovalDomainAction::VoucherSalesOrderStartApprovalSubmission,
                approve: ApprovalDomainAction::VoucherSalesOrderFormalizeApprovedSubmission,
                cancel: ApprovalDomainAction::VoucherSalesOrderCancelApprovalSubmission,
            }),
            DocumentType::SalesChangeOrder => Some(ExpectedRequiredPolicy {
                purposes: NO_PURPOSES,
                version: ApprovalSubjectVersionSource::SalesChangeSubmissionNo,
                snapshot: SALES_PURCHASE_SNAPSHOT,
                owner_role: "sales_change_order_approver",
                start: ApprovalDomainAction::SalesChangeOrderSubmitSalesChange,
                approve: ApprovalDomainAction::SalesChangeOrderApplyEffectiveChange,
                cancel: ApprovalDomainAction::SalesChangeOrderCancelApproval,
            }),
            DocumentType::PurchaseOrder => Some(ExpectedRequiredPolicy {
                purposes: NO_PURPOSES,
                version: ApprovalSubjectVersionSource::EntityApprovalSubjectVersion,
                snapshot: SALES_PURCHASE_SNAPSHOT,
                owner_role: "purchase_order_approver",
                start: ApprovalDomainAction::PurchaseOrderSubmit,
                approve: ApprovalDomainAction::PurchaseOrderFormalizeApprovedOrder,
                cancel: ApprovalDomainAction::PurchaseOrderCancelApproval,
            }),
            DocumentType::PurchaseChangeOrder => Some(ExpectedRequiredPolicy {
                purposes: NO_PURPOSES,
                version: ApprovalSubjectVersionSource::EntityApprovalSubjectVersion,
                snapshot: SALES_PURCHASE_SNAPSHOT,
                owner_role: "purchase_change_order_approver",
                start: ApprovalDomainAction::PurchaseChangeOrderSubmitChange,
                approve: ApprovalDomainAction::PurchaseChangeOrderApplyEffectiveChange,
                cancel: ApprovalDomainAction::PurchaseChangeOrderCancelApproval,
            }),
            DocumentType::StockAdjustment => Some(ExpectedRequiredPolicy {
                purposes: NO_PURPOSES,
                version: ApprovalSubjectVersionSource::EntityApprovalSubjectVersion,
                snapshot: STOCK_SNAPSHOT,
                owner_role: "stock_adjustment_approver",
                start: ApprovalDomainAction::StockAdjustmentSubmit,
                approve: ApprovalDomainAction::StockAdjustmentPost,
                cancel: ApprovalDomainAction::StockAdjustmentCancelApproval,
            }),
            DocumentType::CustomerReceipt => Some(ExpectedRequiredPolicy {
                purposes: NO_PURPOSES,
                version: ApprovalSubjectVersionSource::EntityApprovalSubjectVersion,
                snapshot: FINANCE_SNAPSHOT,
                owner_role: "customer_receipt_approver",
                start: ApprovalDomainAction::CustomerReceiptSubmit,
                approve: ApprovalDomainAction::CustomerReceiptPost,
                cancel: ApprovalDomainAction::CustomerReceiptCancelApproval,
            }),
            DocumentType::CustomerRefund => Some(ExpectedRequiredPolicy {
                purposes: NO_PURPOSES,
                version: ApprovalSubjectVersionSource::EntityApprovalSubjectVersion,
                snapshot: FINANCE_SNAPSHOT,
                owner_role: "customer_refund_approver",
                start: ApprovalDomainAction::CustomerRefundSubmit,
                approve: ApprovalDomainAction::CustomerRefundPost,
                cancel: ApprovalDomainAction::CustomerRefundCancelApproval,
            }),
            DocumentType::SupplierRefund => Some(ExpectedRequiredPolicy {
                purposes: NO_PURPOSES,
                version: ApprovalSubjectVersionSource::EntityApprovalSubjectVersion,
                snapshot: FINANCE_SNAPSHOT,
                owner_role: "supplier_refund_approver",
                start: ApprovalDomainAction::SupplierRefundSubmit,
                approve: ApprovalDomainAction::SupplierRefundPost,
                cancel: ApprovalDomainAction::SupplierRefundCancelApproval,
            }),
            DocumentType::ReceiptReversal => Some(ExpectedRequiredPolicy {
                purposes: NO_PURPOSES,
                version: ApprovalSubjectVersionSource::EntityApprovalSubjectVersion,
                snapshot: FINANCE_SNAPSHOT,
                owner_role: "receipt_reversal_approver",
                start: ApprovalDomainAction::ReceiptReversalSubmit,
                approve: ApprovalDomainAction::ReceiptReversalPost,
                cancel: ApprovalDomainAction::ReceiptReversalCancelApproval,
            }),
            DocumentType::PaymentReversal => Some(ExpectedRequiredPolicy {
                purposes: NO_PURPOSES,
                version: ApprovalSubjectVersionSource::EntityApprovalSubjectVersion,
                snapshot: FINANCE_SNAPSHOT,
                owner_role: "payment_reversal_approver",
                start: ApprovalDomainAction::PaymentReversalSubmit,
                approve: ApprovalDomainAction::PaymentReversalPost,
                cancel: ApprovalDomainAction::PaymentReversalCancelApproval,
            }),
            DocumentType::SupplierPayment
            | DocumentType::PurchaseReceipt
            | DocumentType::Delivery
            | DocumentType::ElectronicDelivery
            | DocumentType::ServiceFulfillment
            | DocumentType::CustomerAcceptance
            | DocumentType::Invoice
            | DocumentType::SalesReturnCase
            | DocumentType::PurchaseReturnOrder => None,
        }
    }

    fn assert_required_policy(
        policy: &DocumentApprovalPolicy,
        document_type: DocumentType,
        expected: ExpectedRequiredPolicy,
    ) {
        let DocumentApprovalPolicy::ProcessRequired(policy) = policy else {
            panic!("{} 必须是 PROCESS_REQUIRED", document_type.as_str());
        };
        assert_eq!(policy.document_type, document_type);
        assert_eq!(policy.process_kind, process_kind_of(document_type));
        assert_eq!(
            policy.definition_admin_permission,
            Permission::parse(format!("{}:approval_definition_admin", document_type.as_str()))
                .expect("定义管理权限可解析")
        );
        assert_eq!(
            policy.runtime_admin_permission,
            Permission::parse(format!("{}:approval_runtime_admin", document_type.as_str()))
                .expect("运行管理权限可解析")
        );
        assert_eq!(
            policy.approver_eligibility_policy,
            ApproverEligibilityPolicy::ActiveBackofficeWithDecidePermission
        );
        assert_eq!(
            policy.separation_of_duties_policy,
            SeparationOfDutiesPolicy::ForbidSubmitterAsApprover
        );
        assert_eq!(policy.required_node_purposes, expected.purposes);
        assert_eq!(policy.subject_version_source, expected.version);
        assert_eq!(policy.subject_snapshot_fields, expected.snapshot);
        assert_eq!(policy.work_item_owner_role.as_str(), expected.owner_role);
        assert_eq!(
            policy.owner_organization_source,
            OwnerOrganizationSource::SubjectSnapshotResponsibleOrgId
        );
        assert_eq!(policy.start_action, expected.start);
        assert_eq!(policy.final_approve_action, expected.approve);
        assert_eq!(policy.cancel_action, expected.cancel);
        ensure_actions_registered(policy).expect("三类动作必须已注册且互异");
        assert_ne!(policy.start_action, policy.final_approve_action);
        assert_ne!(policy.start_action, policy.cancel_action);
        assert_ne!(policy.final_approve_action, policy.cancel_action);
    }

    fn assert_no_approval_policy(policy: &DocumentApprovalPolicy, document_type: DocumentType) {
        let DocumentApprovalPolicy::NoApproval(policy) = policy else {
            panic!("{} 必须是 NO_APPROVAL", document_type.as_str());
        };
        assert_eq!(policy.document_type, document_type);
        assert_eq!(policy.process_kind, process_kind_of(document_type));
        assert_eq!(
            policy_of(document_type).expect("政策必须存在").requirement(),
            ApprovalRequirement::NoApproval
        );
        assert!(require_process_required(document_type).is_err());
    }

    /// 销售单不强制采购确认用途；其它必须审批类型不得带用途。
    #[test]
    fn sales_order_does_not_require_procurement_purpose() {
        for document_type in ALL_DOCUMENT_TYPES {
            let policy = policy_of(document_type).expect("政策必须存在");
            let purposes = match &policy {
                DocumentApprovalPolicy::ProcessRequired(policy) => policy.required_node_purposes,
                DocumentApprovalPolicy::NoApproval(_) => continue,
            };
            assert!(purposes.is_empty());
        }
        let sales = require_process_required(DocumentType::SalesOrder).expect("销售单");
        validate_required_purposes(&sales, &[]).expect("销售单无用途应通过");
        validate_required_purposes(&sales, &[None, None]).expect("销售单空用途应通过");
        validate_required_purposes(&sales, &[Some(SALES_ORDER_PROCUREMENT_CONFIRMATION)])
            .expect("销售单遗留采购确认用途应通过");
        validate_required_purposes(
            &sales,
            &[
                Some(SALES_ORDER_PROCUREMENT_CONFIRMATION),
                Some(SALES_ORDER_PROCUREMENT_CONFIRMATION),
            ],
        )
        .expect("销售单多个遗留采购确认用途应通过");
        assert!(validate_required_purposes(&sales, &[Some("WRONG_PURPOSE")]).is_err());
        assert!(validate_required_purposes(
            &require_process_required(DocumentType::StockAdjustment).expect("库存调整"),
            &[Some(SALES_ORDER_PROCUREMENT_CONFIRMATION)],
        )
        .is_err());
    }

    /// 无审批类型不能进入定义管理。
    #[test]
    fn no_approval_cannot_enter_definition_management() {
        let error = require_process_required(DocumentType::Invoice).expect_err("发票不得管理定义");
        assert!(matches!(error, Error::BusinessLogicError(_)));
    }
}
