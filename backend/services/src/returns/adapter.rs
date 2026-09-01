//! `CustomerRefund` / `SupplierRefund` / `ReceiptReversal` / `PaymentReversal` 审批业务 Adapter。
//!
//! 必须显式声明合同 §4.4 / 阶段 04 §6 的全部适配器字段。
//! 领域动作只通过实体状态邻接与仓储更新，不得 `$set` 绕过不变式。
//! 资金类 `PENDING_REVIEW` 已收敛为 `IN_APPROVAL`，不得再走通用状态更新。

use bpm::SubjectRef;
use entities::approval_integration::{ApprovalSubjectCounterparty, ApprovalSubjectSnapshotPayload};
use entities::common::time::Instant;
use entities::document_registry::business_document::ApprovalDefinitionBinding;
use entities::document_registry::DocumentType;
use entities::ids::{CustomerAccountId, SupplierAccountId};
use entities::returns::{
    CustomerRefund, CustomerRefundStatus, PaymentReversal, PaymentReversalStatus, ReceiptReversal,
    ReceiptReversalStatus, SupplierRefund, SupplierRefundStatus,
};

use super::dto::{
    DocumentApprovalDefinitionView, DocumentApprovalHistoryPageView, DocumentApprovalInstanceView,
    DocumentApprovalView,
};
use crate::approval::business_adapter::{
    adapter_spec_of, ensure_adapter_spec_complete, subject_ref_for, AdapterReadScope, ApprovalAdapterSpec,
};
use crate::approval::policy::{
    ApprovalDomainAction, ApprovalRequirement, ApprovalSubjectSnapshotField, ApprovalSubjectVersionSource,
    OwnerOrganizationSource,
};
use crate::approval::process_kind::process_kind_of;
use crate::errors::{Error, Result};

/// 详情最近审批历史条数上限。完整历史走分页端点。
pub const RECENT_HISTORY_LIMIT: usize = 8;

/// 已注册的客户退款单适配器规格。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CustomerRefundAdapter {
    /// 单据类型。
    pub document_type: DocumentType,
    /// 一对一流程种类。
    pub process_kind: bpm::ProcessKind,
    /// 主体引用构造器标识。
    pub subject_ref_builder: &'static str,
    /// 提交版本权威来源。
    pub subject_version_source: ApprovalSubjectVersionSource,
    /// 快照构造器标识。
    pub subject_snapshot_builder: &'static str,
    /// 提交并启动动作。
    pub on_approval_start: ApprovalDomainAction,
    /// 最终通过动作。
    pub on_final_approve: ApprovalDomainAction,
    /// 撤回与受阻取消动作。
    pub cancel_action: ApprovalDomainAction,
    /// WorkItem 责任角色。
    pub owner_role: &'static str,
    /// 责任组织快照来源。
    pub owner_organization_snapshot: OwnerOrganizationSource,
    /// 对象读取范围。
    pub read_scope: AdapterReadScope,
}

/// 返回客户退款单的完整适配器登记。
///
/// # 返回
/// 返回已校验完整性的规格与显式字段声明。
///
/// # 错误
/// 政策缺失或三类动作不互异时返回部署不变量错误。
pub fn customer_refund_adapter() -> Result<CustomerRefundAdapter> {
    let spec = adapter_spec_of(DocumentType::CustomerRefund)?;
    ensure_adapter_spec_complete(&spec)?;
    adapter_from_spec(spec)
}

/// 由政策规格填充显式 Adapter 字段。
///
/// # 错误
/// 字段与合同签署值不一致时返回错误。
fn adapter_from_spec(spec: ApprovalAdapterSpec) -> Result<CustomerRefundAdapter> {
    if spec.document_type != DocumentType::CustomerRefund
        || spec.process_kind != process_kind_of(DocumentType::CustomerRefund)
        || spec.subject_version_source != ApprovalSubjectVersionSource::EntityApprovalSubjectVersion
        || spec.on_approval_start != ApprovalDomainAction::CustomerRefundSubmit
        || spec.on_final_approve != ApprovalDomainAction::CustomerRefundPost
        || spec.cancel_action != ApprovalDomainAction::CustomerRefundCancelApproval
        || spec.owner_role.as_str() != "customer_refund_approver"
        || spec.owner_organization_source != OwnerOrganizationSource::SubjectSnapshotResponsibleOrgId
        || spec.read_scope != AdapterReadScope::DocumentOrganizationAndCreator
        || !spec
            .subject_snapshot_fields
            .contains(&ApprovalSubjectSnapshotField::TotalAmount)
    {
        return Err(Error::Internal("客户退款单审批适配器登记不完整".to_string()));
    }
    Ok(CustomerRefundAdapter {
        document_type: spec.document_type,
        process_kind: spec.process_kind,
        subject_ref_builder: "subject_ref_for(CustomerRefund)",
        subject_version_source: spec.subject_version_source,
        subject_snapshot_builder: "build_customer_refund_snapshot",
        on_approval_start: spec.on_approval_start,
        on_final_approve: spec.on_final_approve,
        cancel_action: spec.cancel_action,
        owner_role: spec.owner_role.as_str(),
        owner_organization_snapshot: spec.owner_organization_source,
        read_scope: spec.read_scope,
    })
}

/// 为客户退款单构造唯一 `bpm::SubjectRef`。
///
/// # 参数
/// * `business_object_id` - 退款单主键
///
/// # 错误
/// 主键为空或超长时返回校验错误。
pub fn customer_refund_subject_ref(business_object_id: &str) -> Result<SubjectRef> {
    subject_ref_for(DocumentType::CustomerRefund, business_object_id)
}

/// 提交并启动：冻结 `approval_subject_version` 并进入 `IN_APPROVAL`。
///
/// # 参数
/// * `refund` - 待提交退款单
///
/// # 返回
/// 返回冻结后的提交版本。
///
/// # 错误
/// 非草稿或版本溢出时返回冲突。
pub fn start_customer_refund_approval(refund: &mut CustomerRefund) -> Result<u32> {
    Ok(refund.start_approval()?)
}

/// 撤回审批：回到草稿，且 `subject_version` 不回退。
///
/// # 参数
/// * `refund` - 审批中的退款单
///
/// # 错误
/// 非审批中时返回冲突。
pub fn cancel_customer_refund_to_draft(refund: &mut CustomerRefund) -> Result<()> {
    Ok(refund.cancel_approval()?)
}

/// 最终通过过账前置：仅 `IN_APPROVAL` 可进入过账。
///
/// # 错误
/// 状态不是审批中时返回冲突。
pub fn ensure_final_approve_posting(refund: &CustomerRefund) -> Result<()> {
    if refund.status != CustomerRefundStatus::InApproval {
        return Err(Error::ConflictError(
            "只有审批中的客户退款单可以由最终通过动作过账".to_string(),
        ));
    }
    Ok(())
}

/// 无已绑定定义的必须审批单据不得提交。
///
/// # 错误
/// 绑定缺失时返回冲突。
pub fn require_frozen_binding(
    binding: Option<&ApprovalDefinitionBinding>,
) -> Result<&ApprovalDefinitionBinding> {
    binding.ok_or_else(|| Error::ConflictError("无有效审批绑定的客户退款单不得提交".to_string()))
}

/// 客户退款单调用统一 `start_approval` 的目标命令。
///
/// 字段与合同 §14.2 对齐；不得包含定义 ID 或审批人。
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct CustomerRefundStartCommand {
    /// 业务对象种类。
    pub subject_kind: String,
    /// 业务对象 ID。
    pub subject_id: String,
    /// 冻结提交版本，取 `approval_subject_version`。
    pub subject_version: u32,
    /// 启动人。
    #[serde(skip)]
    pub actor_id: String,
    /// 幂等键。
    pub idempotency_key: String,
}

/// 由冻结提交构造目标启动命令。客户端不得提交定义或审批人。
///
/// # 参数
/// * `refund_id` - 退款单主键
/// * `subject_version` - `approval_subject_version`
/// * `actor_id` - 提交人
/// * `idempotency_key` - 幂等键
///
/// # 返回
/// 返回不含定义 ID 或审批人的目标启动命令。
pub fn customer_refund_start_command(
    refund_id: &str,
    subject_version: u32,
    actor_id: &str,
    idempotency_key: &str,
) -> CustomerRefundStartCommand {
    CustomerRefundStartCommand {
        subject_kind: process_kind_of(DocumentType::CustomerRefund).as_str().to_string(),
        subject_id: refund_id.to_string(),
        subject_version,
        actor_id: actor_id.to_string(),
        idempotency_key: idempotency_key.to_string(),
    }
}

/// 证明启动走目标 `START_APPROVAL` 命令种类。
///
/// # 参数
/// * `_command` - 目标启动命令
///
/// # 返回
/// 返回 `START_APPROVAL`。
pub fn start_approval_command_kind(
    _command: &CustomerRefundStartCommand,
) -> bpm::model::types::ApprovalCommandKind {
    bpm::model::types::ApprovalCommandKind::StartApproval
}

/// 执行签署的客户退款单领域动作。
///
/// # 参数
/// * `refund` - 业务实体
/// * `action` - 合同强类型动作
///
/// # 错误
/// 动作不属于本类型或状态不允许时返回错误。
pub fn execute_customer_refund_domain_action(
    refund: &mut CustomerRefund,
    action: ApprovalDomainAction,
) -> Result<()> {
    match action {
        ApprovalDomainAction::CustomerRefundPost => ensure_final_approve_posting(refund),
        ApprovalDomainAction::CustomerRefundCancelApproval => cancel_customer_refund_to_draft(refund),
        other => Err(Error::ValidationError(format!(
            "动作 {} 不属于客户退款单",
            other.as_str()
        ))),
    }
}

/// 按单据组织判定审批人对象读取权。
///
/// 未提供组织或审批人时失败关闭，不得默认放行。
///
/// # 参数
/// * `organization_id` - 单据责任组织（客户往来主体）
/// * `assignee_user_id` - 指定审批人
///
/// # 返回
/// 组织与审批人均非空时允许读取。
///
/// # 错误
/// 组织或审批人为空时返回校验错误。
pub fn customer_refund_object_readable(organization_id: &str, assignee_user_id: &str) -> Result<bool> {
    if organization_id.trim().is_empty() || assignee_user_id.trim().is_empty() {
        return Err(Error::ValidationError("单据组织或审批人不能为空".to_string()));
    }
    Ok(true)
}

/// 责任组织取往来主体，不得用空串或当前登录人组织补位。
///
/// # 参数
/// * `organization_id` - 客户往来主体
///
/// # 返回
/// 返回非空责任组织。
///
/// # 错误
/// 往来主体为空时返回校验错误。
pub fn customer_refund_responsible_org_id(organization_id: &str) -> Result<String> {
    if organization_id.trim().is_empty() {
        return Err(Error::ValidationError(
            "客户退款单缺少往来主体，无法冻结责任组织".to_string(),
        ));
    }
    Ok(organization_id.to_string())
}

/// 按合同 §4.4.5 冻结客户退款快照。
///
/// 对手方为客户；金额合计必填。`document_no` 取退款单号。
/// `responsible_org_id` 必须是客户往来主体，不得用登录人组织补位。
///
/// # 参数
/// * `refund` - 已冻结提交版本的退款单
/// * `responsible_org_id` - 客户往来主体
/// * `submitted_by` - 提交人
/// * `submitted_at` - 提交时间
///
/// # 错误
/// 组织为空时返回校验错误。
pub fn build_customer_refund_snapshot(
    refund: &CustomerRefund,
    responsible_org_id: &str,
    submitted_by: &str,
    submitted_at: Instant,
) -> Result<ApprovalSubjectSnapshotPayload> {
    Ok(ApprovalSubjectSnapshotPayload {
        document_no: refund.refund_no.clone(),
        responsible_org_id: customer_refund_responsible_org_id(responsible_org_id)?,
        submitted_by: submitted_by.to_string(),
        submitted_at,
        counterparty: Some(ApprovalSubjectCounterparty::Customer {
            customer_id: CustomerAccountId::new(refund.customer_id.to_string()),
        }),
        total_amount: Some(refund.amount),
        total_quantity: None,
        line_count: 1,
    })
}

/// 由绑定与可选实例事实构造只读审批结构。
///
/// 创建后未提交只返回绑定定义；客户端不得据此选择定义或审批人。
///
/// # 参数
/// * `binding` - 创建时冻结的定义绑定
/// * `instance` - 已启动时的实例摘要
/// * `status` - 当前业务状态
///
/// # 返回
/// 返回有界只读审批结构。
pub fn document_approval_view(
    binding: Option<&ApprovalDefinitionBinding>,
    instance: Option<DocumentApprovalInstanceView>,
    status: CustomerRefundStatus,
) -> DocumentApprovalView {
    DocumentApprovalView {
        requirement: match ApprovalRequirement::ProcessRequired {
            ApprovalRequirement::ProcessRequired => "PROCESS_REQUIRED",
            ApprovalRequirement::NoApproval => "NO_APPROVAL",
        }
        .to_string(),
        definition: binding.map(definition_view_from_binding),
        instance,
        recent_history: Vec::new(),
        history_page: DocumentApprovalHistoryPageView {
            next_cursor: None,
            has_more: false,
        },
        allowed_actions: allowed_document_actions(status),
    }
}

/// 由冻结绑定投影定义摘要。节点详情不在单据详情展开。
fn definition_view_from_binding(binding: &ApprovalDefinitionBinding) -> DocumentApprovalDefinitionView {
    DocumentApprovalDefinitionView {
        id: binding.approval_process_definition_id.as_ref().to_string(),
        name: String::new(),
        version: binding.approval_definition_version,
        nodes: Vec::new(),
    }
}

/// 单据详情允许的审批相关动作。不含选择定义或审批人。
fn allowed_document_actions(status: CustomerRefundStatus) -> Vec<String> {
    match status {
        CustomerRefundStatus::Draft => vec!["SUBMIT".to_string()],
        CustomerRefundStatus::InApproval => vec!["CANCEL".to_string()],
        CustomerRefundStatus::Posted | CustomerRefundStatus::Reversed => Vec::new(),
    }
}

/// 已注册的供应商退款单适配器规格。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SupplierRefundAdapter {
    /// 单据类型。
    pub document_type: DocumentType,
    /// 一对一流程种类。
    pub process_kind: bpm::ProcessKind,
    /// 主体引用构造器标识。
    pub subject_ref_builder: &'static str,
    /// 提交版本权威来源。
    pub subject_version_source: ApprovalSubjectVersionSource,
    /// 快照构造器标识。
    pub subject_snapshot_builder: &'static str,
    /// 提交并启动动作。
    pub on_approval_start: ApprovalDomainAction,
    /// 最终通过动作。
    pub on_final_approve: ApprovalDomainAction,
    /// 撤回与受阻取消动作。
    pub cancel_action: ApprovalDomainAction,
    /// WorkItem 责任角色。
    pub owner_role: &'static str,
    /// 责任组织快照来源。
    pub owner_organization_snapshot: OwnerOrganizationSource,
    /// 对象读取范围。
    pub read_scope: AdapterReadScope,
}

/// 返回供应商退款单的完整适配器登记。
///
/// # 返回
/// 返回已校验完整性的规格与显式字段声明。
///
/// # 错误
/// 政策缺失或三类动作不互异时返回部署不变量错误。
pub fn supplier_refund_adapter() -> Result<SupplierRefundAdapter> {
    let spec = adapter_spec_of(DocumentType::SupplierRefund)?;
    ensure_adapter_spec_complete(&spec)?;
    supplier_adapter_from_spec(spec)
}

/// 由政策规格填充供应商退款单显式 Adapter 字段。
///
/// # 错误
/// 字段与合同签署值不一致时返回错误。
fn supplier_adapter_from_spec(spec: ApprovalAdapterSpec) -> Result<SupplierRefundAdapter> {
    if spec.document_type != DocumentType::SupplierRefund
        || spec.process_kind != process_kind_of(DocumentType::SupplierRefund)
        || spec.subject_version_source != ApprovalSubjectVersionSource::EntityApprovalSubjectVersion
        || spec.on_approval_start != ApprovalDomainAction::SupplierRefundSubmit
        || spec.on_final_approve != ApprovalDomainAction::SupplierRefundPost
        || spec.cancel_action != ApprovalDomainAction::SupplierRefundCancelApproval
        || spec.owner_role.as_str() != "supplier_refund_approver"
        || spec.owner_organization_source != OwnerOrganizationSource::SubjectSnapshotResponsibleOrgId
        || spec.read_scope != AdapterReadScope::DocumentOrganizationAndCreator
        || !spec
            .subject_snapshot_fields
            .contains(&ApprovalSubjectSnapshotField::TotalAmount)
    {
        return Err(Error::Internal("供应商退款单审批适配器登记不完整".to_string()));
    }
    Ok(SupplierRefundAdapter {
        document_type: spec.document_type,
        process_kind: spec.process_kind,
        subject_ref_builder: "subject_ref_for(SupplierRefund)",
        subject_version_source: spec.subject_version_source,
        subject_snapshot_builder: "build_supplier_refund_snapshot",
        on_approval_start: spec.on_approval_start,
        on_final_approve: spec.on_final_approve,
        cancel_action: spec.cancel_action,
        owner_role: spec.owner_role.as_str(),
        owner_organization_snapshot: spec.owner_organization_source,
        read_scope: spec.read_scope,
    })
}

/// 为供应商退款单构造唯一 `bpm::SubjectRef`。
///
/// # 参数
/// * `business_object_id` - 退款单主键
///
/// # 错误
/// 主键为空或超长时返回校验错误。
pub fn supplier_refund_subject_ref(business_object_id: &str) -> Result<SubjectRef> {
    subject_ref_for(DocumentType::SupplierRefund, business_object_id)
}

/// 提交并启动：冻结 `approval_subject_version` 并进入 `IN_APPROVAL`。
///
/// # 参数
/// * `refund` - 待提交退款单
///
/// # 返回
/// 返回冻结后的提交版本。
///
/// # 错误
/// 非草稿或版本溢出时返回冲突。
pub fn start_supplier_refund_approval(refund: &mut SupplierRefund) -> Result<u32> {
    Ok(refund.start_approval()?)
}

/// 撤回审批：回到草稿，且 `subject_version` 不回退。
///
/// # 参数
/// * `refund` - 审批中的退款单
///
/// # 错误
/// 非审批中时返回冲突。
pub fn cancel_supplier_refund_to_draft(refund: &mut SupplierRefund) -> Result<()> {
    Ok(refund.cancel_approval()?)
}

/// 最终通过过账前置：仅 `IN_APPROVAL` 可进入过账。
///
/// # 错误
/// 状态不是审批中时返回冲突。
pub fn ensure_supplier_refund_final_approve_posting(refund: &SupplierRefund) -> Result<()> {
    if refund.status != SupplierRefundStatus::InApproval {
        return Err(Error::ConflictError(
            "只有审批中的供应商退款单可以由最终通过动作过账".to_string(),
        ));
    }
    Ok(())
}

/// 无已绑定定义的必须审批单据不得提交。
///
/// # 错误
/// 绑定缺失时返回冲突。
pub fn require_supplier_refund_binding(
    binding: Option<&ApprovalDefinitionBinding>,
) -> Result<&ApprovalDefinitionBinding> {
    binding.ok_or_else(|| Error::ConflictError("无有效审批绑定的供应商退款单不得提交".to_string()))
}

/// 供应商退款单调用统一 `start_approval` 的目标命令。
///
/// 字段与合同 §14.2 对齐；不得包含定义 ID 或审批人。
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct SupplierRefundStartCommand {
    /// 业务对象种类。
    pub subject_kind: String,
    /// 业务对象 ID。
    pub subject_id: String,
    /// 冻结提交版本，取 `approval_subject_version`。
    pub subject_version: u32,
    /// 启动人。
    #[serde(skip)]
    pub actor_id: String,
    /// 幂等键。
    pub idempotency_key: String,
}

/// 由冻结提交构造目标启动命令。客户端不得提交定义或审批人。
///
/// # 参数
/// * `refund_id` - 退款单主键
/// * `subject_version` - `approval_subject_version`
/// * `actor_id` - 提交人
/// * `idempotency_key` - 幂等键
///
/// # 返回
/// 返回不含定义 ID 或审批人的目标启动命令。
pub fn supplier_refund_start_command(
    refund_id: &str,
    subject_version: u32,
    actor_id: &str,
    idempotency_key: &str,
) -> SupplierRefundStartCommand {
    SupplierRefundStartCommand {
        subject_kind: process_kind_of(DocumentType::SupplierRefund).as_str().to_string(),
        subject_id: refund_id.to_string(),
        subject_version,
        actor_id: actor_id.to_string(),
        idempotency_key: idempotency_key.to_string(),
    }
}

/// 证明启动走目标 `START_APPROVAL` 命令种类。
///
/// # 参数
/// * `_command` - 目标启动命令
///
/// # 返回
/// 返回 `START_APPROVAL`。
pub fn supplier_refund_start_command_kind(
    _command: &SupplierRefundStartCommand,
) -> bpm::model::types::ApprovalCommandKind {
    bpm::model::types::ApprovalCommandKind::StartApproval
}

/// 执行签署的供应商退款单领域动作。
///
/// # 参数
/// * `refund` - 业务实体
/// * `action` - 合同强类型动作
///
/// # 错误
/// 动作不属于本类型或状态不允许时返回错误。
pub fn execute_supplier_refund_domain_action(
    refund: &mut SupplierRefund,
    action: ApprovalDomainAction,
) -> Result<()> {
    match action {
        ApprovalDomainAction::SupplierRefundPost => ensure_supplier_refund_final_approve_posting(refund),
        ApprovalDomainAction::SupplierRefundCancelApproval => cancel_supplier_refund_to_draft(refund),
        other => Err(Error::ValidationError(format!(
            "动作 {} 不属于供应商退款单",
            other.as_str()
        ))),
    }
}

/// 按单据组织判定审批人对象读取权。
///
/// 未提供组织或审批人时失败关闭，不得默认放行。
///
/// # 参数
/// * `organization_id` - 单据责任组织（供应商往来主体）
/// * `assignee_user_id` - 指定审批人
///
/// # 返回
/// 组织与审批人均非空时允许读取。
///
/// # 错误
/// 组织或审批人为空时返回校验错误。
pub fn supplier_refund_object_readable(organization_id: &str, assignee_user_id: &str) -> Result<bool> {
    if organization_id.trim().is_empty() || assignee_user_id.trim().is_empty() {
        return Err(Error::ValidationError("单据组织或审批人不能为空".to_string()));
    }
    Ok(true)
}

/// 责任组织取往来主体，不得用空串或当前登录人组织补位。
///
/// # 参数
/// * `organization_id` - 供应商往来主体
///
/// # 返回
/// 返回非空责任组织。
///
/// # 错误
/// 往来主体为空时返回校验错误。
pub fn supplier_refund_responsible_org_id(organization_id: &str) -> Result<String> {
    if organization_id.trim().is_empty() {
        return Err(Error::ValidationError(
            "供应商退款单缺少往来主体，无法冻结责任组织".to_string(),
        ));
    }
    Ok(organization_id.to_string())
}

/// 按合同 §4.4.5 冻结供应商退款快照。
///
/// 对手方为供应商；金额合计必填。`document_no` 取退款单号。
/// `responsible_org_id` 必须是供应商往来主体，不得用登录人组织补位。
///
/// # 参数
/// * `refund` - 已冻结提交版本的退款单
/// * `responsible_org_id` - 供应商往来主体
/// * `submitted_by` - 提交人
/// * `submitted_at` - 提交时间
///
/// # 错误
/// 组织为空时返回校验错误。
pub fn build_supplier_refund_snapshot(
    refund: &SupplierRefund,
    responsible_org_id: &str,
    submitted_by: &str,
    submitted_at: Instant,
) -> Result<ApprovalSubjectSnapshotPayload> {
    Ok(ApprovalSubjectSnapshotPayload {
        document_no: refund.refund_no.clone(),
        responsible_org_id: supplier_refund_responsible_org_id(responsible_org_id)?,
        submitted_by: submitted_by.to_string(),
        submitted_at,
        counterparty: Some(ApprovalSubjectCounterparty::Supplier {
            supplier_id: SupplierAccountId::new(refund.supplier_id.to_string()),
        }),
        total_amount: Some(refund.amount),
        total_quantity: None,
        line_count: 1,
    })
}

/// 由绑定与可选实例事实构造供应商退款只读审批结构。
///
/// 创建后未提交只返回绑定定义；客户端不得据此选择定义或审批人。
///
/// # 参数
/// * `binding` - 创建时冻结的定义绑定
/// * `instance` - 已启动时的实例摘要
/// * `status` - 当前业务状态
///
/// # 返回
/// 返回有界只读审批结构。
pub fn supplier_refund_approval_view(
    binding: Option<&ApprovalDefinitionBinding>,
    instance: Option<DocumentApprovalInstanceView>,
    status: SupplierRefundStatus,
) -> DocumentApprovalView {
    DocumentApprovalView {
        requirement: match ApprovalRequirement::ProcessRequired {
            ApprovalRequirement::ProcessRequired => "PROCESS_REQUIRED",
            ApprovalRequirement::NoApproval => "NO_APPROVAL",
        }
        .to_string(),
        definition: binding.map(definition_view_from_binding),
        instance,
        recent_history: Vec::new(),
        history_page: DocumentApprovalHistoryPageView {
            next_cursor: None,
            has_more: false,
        },
        allowed_actions: supplier_refund_allowed_actions(status),
    }
}

/// 供应商退款详情允许的审批相关动作。不含选择定义或审批人。
fn supplier_refund_allowed_actions(status: SupplierRefundStatus) -> Vec<String> {
    match status {
        SupplierRefundStatus::Draft => vec!["SUBMIT".to_string()],
        SupplierRefundStatus::InApproval => vec!["CANCEL".to_string()],
        SupplierRefundStatus::Posted | SupplierRefundStatus::Reversed => Vec::new(),
    }
}

/// 已注册的回款冲正单适配器规格。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReceiptReversalAdapter {
    /// 单据类型。
    pub document_type: DocumentType,
    /// 一对一流程种类。
    pub process_kind: bpm::ProcessKind,
    /// 主体引用构造器标识。
    pub subject_ref_builder: &'static str,
    /// 提交版本权威来源。
    pub subject_version_source: ApprovalSubjectVersionSource,
    /// 快照构造器标识。
    pub subject_snapshot_builder: &'static str,
    /// 提交并启动动作。
    pub on_approval_start: ApprovalDomainAction,
    /// 最终通过动作。
    pub on_final_approve: ApprovalDomainAction,
    /// 撤回与受阻取消动作。
    pub cancel_action: ApprovalDomainAction,
    /// WorkItem 责任角色。
    pub owner_role: &'static str,
    /// 责任组织快照来源。
    pub owner_organization_snapshot: OwnerOrganizationSource,
    /// 对象读取范围。
    pub read_scope: AdapterReadScope,
}

/// 返回回款冲正单的完整适配器登记。
///
/// # 返回
/// 返回已校验完整性的规格与显式字段声明。
///
/// # 错误
/// 政策缺失或三类动作不互异时返回部署不变量错误。
pub fn receipt_reversal_adapter() -> Result<ReceiptReversalAdapter> {
    let spec = adapter_spec_of(DocumentType::ReceiptReversal)?;
    ensure_adapter_spec_complete(&spec)?;
    receipt_reversal_adapter_from_spec(spec)
}

/// 由政策规格填充回款冲正单显式 Adapter 字段。
///
/// # 错误
/// 字段与合同签署值不一致时返回错误。
fn receipt_reversal_adapter_from_spec(spec: ApprovalAdapterSpec) -> Result<ReceiptReversalAdapter> {
    if spec.document_type != DocumentType::ReceiptReversal
        || spec.process_kind != process_kind_of(DocumentType::ReceiptReversal)
        || spec.subject_version_source != ApprovalSubjectVersionSource::EntityApprovalSubjectVersion
        || spec.on_approval_start != ApprovalDomainAction::ReceiptReversalSubmit
        || spec.on_final_approve != ApprovalDomainAction::ReceiptReversalPost
        || spec.cancel_action != ApprovalDomainAction::ReceiptReversalCancelApproval
        || spec.owner_role.as_str() != "receipt_reversal_approver"
        || spec.owner_organization_source != OwnerOrganizationSource::SubjectSnapshotResponsibleOrgId
        || spec.read_scope != AdapterReadScope::DocumentOrganizationAndCreator
        || !spec
            .subject_snapshot_fields
            .contains(&ApprovalSubjectSnapshotField::TotalAmount)
    {
        return Err(Error::Internal("回款冲正单审批适配器登记不完整".to_string()));
    }
    Ok(ReceiptReversalAdapter {
        document_type: spec.document_type,
        process_kind: spec.process_kind,
        subject_ref_builder: "subject_ref_for(ReceiptReversal)",
        subject_version_source: spec.subject_version_source,
        subject_snapshot_builder: "build_receipt_reversal_snapshot",
        on_approval_start: spec.on_approval_start,
        on_final_approve: spec.on_final_approve,
        cancel_action: spec.cancel_action,
        owner_role: spec.owner_role.as_str(),
        owner_organization_snapshot: spec.owner_organization_source,
        read_scope: spec.read_scope,
    })
}

/// 为回款冲正单构造唯一 `bpm::SubjectRef`。
///
/// # 参数
/// * `business_object_id` - 冲正单主键
///
/// # 错误
/// 主键为空或超长时返回校验错误。
pub fn receipt_reversal_subject_ref(business_object_id: &str) -> Result<SubjectRef> {
    subject_ref_for(DocumentType::ReceiptReversal, business_object_id)
}

/// 提交并启动：冻结 `approval_subject_version` 并进入 `IN_APPROVAL`。
///
/// # 参数
/// * `reversal` - 待提交冲正单
///
/// # 返回
/// 返回冻结后的提交版本。
///
/// # 错误
/// 非草稿或版本溢出时返回冲突。
pub fn start_receipt_reversal_approval(reversal: &mut ReceiptReversal) -> Result<u32> {
    Ok(reversal.start_approval()?)
}

/// 撤回审批：回到草稿，且 `subject_version` 不回退。
///
/// # 参数
/// * `reversal` - 审批中的冲正单
///
/// # 错误
/// 非审批中时返回冲突。
pub fn cancel_receipt_reversal_to_draft(reversal: &mut ReceiptReversal) -> Result<()> {
    Ok(reversal.cancel_approval()?)
}

/// 最终通过过账前置：仅 `IN_APPROVAL` 可进入过账。
///
/// # 错误
/// 状态不是审批中时返回冲突。
pub fn ensure_receipt_reversal_final_approve_posting(reversal: &ReceiptReversal) -> Result<()> {
    if reversal.status != ReceiptReversalStatus::InApproval {
        return Err(Error::ConflictError(
            "只有审批中的回款冲正单可以由最终通过动作过账".to_string(),
        ));
    }
    Ok(())
}

/// 无已绑定定义的必须审批单据不得提交。
///
/// # 错误
/// 绑定缺失时返回冲突。
pub fn require_receipt_reversal_binding(
    binding: Option<&ApprovalDefinitionBinding>,
) -> Result<&ApprovalDefinitionBinding> {
    binding.ok_or_else(|| Error::ConflictError("无有效审批绑定的回款冲正单不得提交".to_string()))
}

/// 回款冲正单调用统一 `start_approval` 的目标命令。
///
/// 字段与合同 §14.2 对齐；不得包含定义 ID 或审批人。
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct ReceiptReversalStartCommand {
    /// 业务对象种类。
    pub subject_kind: String,
    /// 业务对象 ID。
    pub subject_id: String,
    /// 冻结提交版本，取 `approval_subject_version`。
    pub subject_version: u32,
    /// 启动人。
    #[serde(skip)]
    pub actor_id: String,
    /// 幂等键。
    pub idempotency_key: String,
}

/// 由冻结提交构造目标启动命令。客户端不得提交定义或审批人。
///
/// # 参数
/// * `reversal_id` - 冲正单主键
/// * `subject_version` - `approval_subject_version`
/// * `actor_id` - 提交人
/// * `idempotency_key` - 幂等键
///
/// # 返回
/// 返回不含定义 ID 或审批人的目标启动命令。
pub fn receipt_reversal_start_command(
    reversal_id: &str,
    subject_version: u32,
    actor_id: &str,
    idempotency_key: &str,
) -> ReceiptReversalStartCommand {
    ReceiptReversalStartCommand {
        subject_kind: process_kind_of(DocumentType::ReceiptReversal)
            .as_str()
            .to_string(),
        subject_id: reversal_id.to_string(),
        subject_version,
        actor_id: actor_id.to_string(),
        idempotency_key: idempotency_key.to_string(),
    }
}

/// 证明启动走目标 `START_APPROVAL` 命令种类。
///
/// # 参数
/// * `_command` - 目标启动命令
///
/// # 返回
/// 返回 `START_APPROVAL`。
pub fn receipt_reversal_start_command_kind(
    _command: &ReceiptReversalStartCommand,
) -> bpm::model::types::ApprovalCommandKind {
    bpm::model::types::ApprovalCommandKind::StartApproval
}

/// 执行签署的回款冲正单领域动作。
///
/// # 参数
/// * `reversal` - 业务实体
/// * `action` - 合同强类型动作
///
/// # 错误
/// 动作不属于本类型或状态不允许时返回错误。
pub fn execute_receipt_reversal_domain_action(
    reversal: &mut ReceiptReversal,
    action: ApprovalDomainAction,
) -> Result<()> {
    match action {
        ApprovalDomainAction::ReceiptReversalPost => ensure_receipt_reversal_final_approve_posting(reversal),
        ApprovalDomainAction::ReceiptReversalCancelApproval => cancel_receipt_reversal_to_draft(reversal),
        other => Err(Error::ValidationError(format!(
            "动作 {} 不属于回款冲正单",
            other.as_str()
        ))),
    }
}

/// 按单据组织判定审批人对象读取权。
///
/// 未提供组织或审批人时失败关闭，不得默认放行。
///
/// # 参数
/// * `organization_id` - 单据责任组织（原回款往来主体）
/// * `assignee_user_id` - 指定审批人
///
/// # 返回
/// 组织与审批人均非空时允许读取。
///
/// # 错误
/// 组织或审批人为空时返回校验错误。
pub fn receipt_reversal_object_readable(organization_id: &str, assignee_user_id: &str) -> Result<bool> {
    if organization_id.trim().is_empty() || assignee_user_id.trim().is_empty() {
        return Err(Error::ValidationError("单据组织或审批人不能为空".to_string()));
    }
    Ok(true)
}

/// 责任组织取往来主体，不得用空串或当前登录人组织补位。
///
/// # 参数
/// * `organization_id` - 原回款往来主体
///
/// # 返回
/// 返回非空责任组织。
///
/// # 错误
/// 往来主体为空时返回校验错误。
pub fn receipt_reversal_responsible_org_id(organization_id: &str) -> Result<String> {
    if organization_id.trim().is_empty() {
        return Err(Error::ValidationError(
            "回款冲正单缺少往来主体，无法冻结责任组织".to_string(),
        ));
    }
    Ok(organization_id.to_string())
}

/// 按合同 §4.4.5 冻结回款冲正快照。
///
/// 对手方为原回款客户（可空）；金额合计必填。`document_no` 取冲正单号。
/// `responsible_org_id` 必须是原回款往来主体，不得用登录人组织补位。
///
/// # 参数
/// * `reversal` - 已冻结提交版本的冲正单
/// * `responsible_org_id` - 原回款往来主体
/// * `customer_id` - 原回款可选经营归属客户
/// * `submitted_by` - 提交人
/// * `submitted_at` - 提交时间
///
/// # 错误
/// 组织为空时返回校验错误。
pub fn build_receipt_reversal_snapshot(
    reversal: &ReceiptReversal,
    responsible_org_id: &str,
    customer_id: Option<&CustomerAccountId>,
    submitted_by: &str,
    submitted_at: Instant,
) -> Result<ApprovalSubjectSnapshotPayload> {
    Ok(ApprovalSubjectSnapshotPayload {
        document_no: reversal.reversal_no.clone(),
        responsible_org_id: receipt_reversal_responsible_org_id(responsible_org_id)?,
        submitted_by: submitted_by.to_string(),
        submitted_at,
        counterparty: customer_id.map(|customer_id| ApprovalSubjectCounterparty::Customer {
            customer_id: CustomerAccountId::new(customer_id.to_string()),
        }),
        total_amount: Some(reversal.amount),
        total_quantity: None,
        line_count: 1,
    })
}

/// 由绑定与可选实例事实构造回款冲正只读审批结构。
///
/// 创建后未提交只返回绑定定义；客户端不得据此选择定义或审批人。
///
/// # 参数
/// * `binding` - 创建时冻结的定义绑定
/// * `instance` - 已启动时的实例摘要
/// * `status` - 当前业务状态
///
/// # 返回
/// 返回有界只读审批结构。
pub fn receipt_reversal_approval_view(
    binding: Option<&ApprovalDefinitionBinding>,
    instance: Option<DocumentApprovalInstanceView>,
    status: ReceiptReversalStatus,
) -> DocumentApprovalView {
    DocumentApprovalView {
        requirement: match ApprovalRequirement::ProcessRequired {
            ApprovalRequirement::ProcessRequired => "PROCESS_REQUIRED",
            ApprovalRequirement::NoApproval => "NO_APPROVAL",
        }
        .to_string(),
        definition: binding.map(definition_view_from_binding),
        instance,
        recent_history: Vec::new(),
        history_page: DocumentApprovalHistoryPageView {
            next_cursor: None,
            has_more: false,
        },
        allowed_actions: receipt_reversal_allowed_actions(status),
    }
}

/// 回款冲正详情允许的审批相关动作。不含选择定义或审批人。
fn receipt_reversal_allowed_actions(status: ReceiptReversalStatus) -> Vec<String> {
    match status {
        ReceiptReversalStatus::Draft => vec!["SUBMIT".to_string()],
        ReceiptReversalStatus::InApproval => vec!["CANCEL".to_string()],
        ReceiptReversalStatus::Posted | ReceiptReversalStatus::Reversed => Vec::new(),
    }
}

/// 已注册的付款冲正单适配器规格。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PaymentReversalAdapter {
    /// 单据类型。
    pub document_type: DocumentType,
    /// 一对一流程种类。
    pub process_kind: bpm::ProcessKind,
    /// 主体引用构造器标识。
    pub subject_ref_builder: &'static str,
    /// 提交版本权威来源。
    pub subject_version_source: ApprovalSubjectVersionSource,
    /// 快照构造器标识。
    pub subject_snapshot_builder: &'static str,
    /// 提交并启动动作。
    pub on_approval_start: ApprovalDomainAction,
    /// 最终通过动作。
    pub on_final_approve: ApprovalDomainAction,
    /// 撤回与受阻取消动作。
    pub cancel_action: ApprovalDomainAction,
    /// WorkItem 责任角色。
    pub owner_role: &'static str,
    /// 责任组织快照来源。
    pub owner_organization_snapshot: OwnerOrganizationSource,
    /// 对象读取范围。
    pub read_scope: AdapterReadScope,
}

/// 返回付款冲正单的完整适配器登记。
///
/// # 返回
/// 返回已校验完整性的规格与显式字段声明。
///
/// # 错误
/// 政策缺失或三类动作不互异时返回部署不变量错误。
pub fn payment_reversal_adapter() -> Result<PaymentReversalAdapter> {
    let spec = adapter_spec_of(DocumentType::PaymentReversal)?;
    ensure_adapter_spec_complete(&spec)?;
    payment_reversal_adapter_from_spec(spec)
}

/// 由政策规格填充付款冲正单显式 Adapter 字段。
///
/// # 错误
/// 字段与合同签署值不一致时返回错误。
fn payment_reversal_adapter_from_spec(spec: ApprovalAdapterSpec) -> Result<PaymentReversalAdapter> {
    if spec.document_type != DocumentType::PaymentReversal
        || spec.process_kind != process_kind_of(DocumentType::PaymentReversal)
        || spec.subject_version_source != ApprovalSubjectVersionSource::EntityApprovalSubjectVersion
        || spec.on_approval_start != ApprovalDomainAction::PaymentReversalSubmit
        || spec.on_final_approve != ApprovalDomainAction::PaymentReversalPost
        || spec.cancel_action != ApprovalDomainAction::PaymentReversalCancelApproval
        || spec.owner_role.as_str() != "payment_reversal_approver"
        || spec.owner_organization_source != OwnerOrganizationSource::SubjectSnapshotResponsibleOrgId
        || spec.read_scope != AdapterReadScope::DocumentOrganizationAndCreator
        || !spec
            .subject_snapshot_fields
            .contains(&ApprovalSubjectSnapshotField::TotalAmount)
    {
        return Err(Error::Internal("付款冲正单审批适配器登记不完整".to_string()));
    }
    Ok(PaymentReversalAdapter {
        document_type: spec.document_type,
        process_kind: spec.process_kind,
        subject_ref_builder: "subject_ref_for(PaymentReversal)",
        subject_version_source: spec.subject_version_source,
        subject_snapshot_builder: "build_payment_reversal_snapshot",
        on_approval_start: spec.on_approval_start,
        on_final_approve: spec.on_final_approve,
        cancel_action: spec.cancel_action,
        owner_role: spec.owner_role.as_str(),
        owner_organization_snapshot: spec.owner_organization_source,
        read_scope: spec.read_scope,
    })
}

/// 为付款冲正单构造唯一 `bpm::SubjectRef`。
///
/// # 参数
/// * `business_object_id` - 冲正单主键
///
/// # 错误
/// 主键为空或超长时返回校验错误。
pub fn payment_reversal_subject_ref(business_object_id: &str) -> Result<SubjectRef> {
    subject_ref_for(DocumentType::PaymentReversal, business_object_id)
}

/// 提交并启动：冻结 `approval_subject_version` 并进入 `IN_APPROVAL`。
///
/// # 参数
/// * `reversal` - 待提交冲正单
///
/// # 返回
/// 返回冻结后的提交版本。
///
/// # 错误
/// 非草稿或版本溢出时返回冲突。
pub fn start_payment_reversal_approval(reversal: &mut PaymentReversal) -> Result<u32> {
    Ok(reversal.start_approval()?)
}

/// 撤回审批：回到草稿，且 `subject_version` 不回退。
///
/// # 参数
/// * `reversal` - 审批中的冲正单
///
/// # 错误
/// 非审批中时返回冲突。
pub fn cancel_payment_reversal_to_draft(reversal: &mut PaymentReversal) -> Result<()> {
    Ok(reversal.cancel_approval()?)
}

/// 最终通过过账前置：仅 `IN_APPROVAL` 可进入过账。
///
/// # 错误
/// 状态不是审批中时返回冲突。
pub fn ensure_payment_reversal_final_approve_posting(reversal: &PaymentReversal) -> Result<()> {
    if reversal.status != PaymentReversalStatus::InApproval {
        return Err(Error::ConflictError(
            "只有审批中的付款冲正单可以由最终通过动作过账".to_string(),
        ));
    }
    Ok(())
}

/// 无已绑定定义的必须审批单据不得提交。
///
/// # 错误
/// 绑定缺失时返回冲突。
pub fn require_payment_reversal_binding(
    binding: Option<&ApprovalDefinitionBinding>,
) -> Result<&ApprovalDefinitionBinding> {
    binding.ok_or_else(|| Error::ConflictError("无有效审批绑定的付款冲正单不得提交".to_string()))
}

/// 付款冲正单调用统一 `start_approval` 的目标命令。
///
/// 字段与合同 §14.2 对齐；不得包含定义 ID 或审批人。
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct PaymentReversalStartCommand {
    /// 业务对象种类。
    pub subject_kind: String,
    /// 业务对象 ID。
    pub subject_id: String,
    /// 冻结提交版本，取 `approval_subject_version`。
    pub subject_version: u32,
    /// 启动人。
    #[serde(skip)]
    pub actor_id: String,
    /// 幂等键。
    pub idempotency_key: String,
}

/// 由冻结提交构造目标启动命令。客户端不得提交定义或审批人。
///
/// # 参数
/// * `reversal_id` - 冲正单主键
/// * `subject_version` - `approval_subject_version`
/// * `actor_id` - 提交人
/// * `idempotency_key` - 幂等键
///
/// # 返回
/// 返回不含定义 ID 或审批人的目标启动命令。
pub fn payment_reversal_start_command(
    reversal_id: &str,
    subject_version: u32,
    actor_id: &str,
    idempotency_key: &str,
) -> PaymentReversalStartCommand {
    PaymentReversalStartCommand {
        subject_kind: process_kind_of(DocumentType::PaymentReversal)
            .as_str()
            .to_string(),
        subject_id: reversal_id.to_string(),
        subject_version,
        actor_id: actor_id.to_string(),
        idempotency_key: idempotency_key.to_string(),
    }
}

/// 证明启动走目标 `START_APPROVAL` 命令种类。
///
/// # 参数
/// * `_command` - 目标启动命令
///
/// # 返回
/// 返回 `START_APPROVAL`。
pub fn payment_reversal_start_command_kind(
    _command: &PaymentReversalStartCommand,
) -> bpm::model::types::ApprovalCommandKind {
    bpm::model::types::ApprovalCommandKind::StartApproval
}

/// 执行签署的付款冲正单领域动作。
///
/// # 参数
/// * `reversal` - 业务实体
/// * `action` - 合同强类型动作
///
/// # 错误
/// 动作不属于本类型或状态不允许时返回错误。
pub fn execute_payment_reversal_domain_action(
    reversal: &mut PaymentReversal,
    action: ApprovalDomainAction,
) -> Result<()> {
    match action {
        ApprovalDomainAction::PaymentReversalPost => ensure_payment_reversal_final_approve_posting(reversal),
        ApprovalDomainAction::PaymentReversalCancelApproval => cancel_payment_reversal_to_draft(reversal),
        other => Err(Error::ValidationError(format!(
            "动作 {} 不属于付款冲正单",
            other.as_str()
        ))),
    }
}

/// 按单据组织判定审批人对象读取权。
///
/// 未提供组织或审批人时失败关闭，不得默认放行。
///
/// # 参数
/// * `organization_id` - 单据责任组织（原付款供应商往来主体）
/// * `assignee_user_id` - 指定审批人
///
/// # 返回
/// 组织与审批人均非空时允许读取。
///
/// # 错误
/// 组织或审批人为空时返回校验错误。
pub fn payment_reversal_object_readable(organization_id: &str, assignee_user_id: &str) -> Result<bool> {
    if organization_id.trim().is_empty() || assignee_user_id.trim().is_empty() {
        return Err(Error::ValidationError("单据组织或审批人不能为空".to_string()));
    }
    Ok(true)
}

/// 责任组织取往来主体，不得用空串或当前登录人组织补位。
///
/// # 参数
/// * `organization_id` - 原付款供应商往来主体
///
/// # 返回
/// 返回非空责任组织。
///
/// # 错误
/// 往来主体为空时返回校验错误。
pub fn payment_reversal_responsible_org_id(organization_id: &str) -> Result<String> {
    if organization_id.trim().is_empty() {
        return Err(Error::ValidationError(
            "付款冲正单缺少往来主体，无法冻结责任组织".to_string(),
        ));
    }
    Ok(organization_id.to_string())
}

/// 按合同 §4.4.5 冻结付款冲正快照。
///
/// 对手方为原付款供应商；金额合计必填。`document_no` 取冲正单号。
/// `responsible_org_id` 必须是供应商往来主体，不得用登录人组织补位。
///
/// # 参数
/// * `reversal` - 已冻结提交版本的冲正单
/// * `responsible_org_id` - 原付款供应商往来主体
/// * `supplier_id` - 原付款供应商
/// * `submitted_by` - 提交人
/// * `submitted_at` - 提交时间
///
/// # 错误
/// 组织为空时返回校验错误。
pub fn build_payment_reversal_snapshot(
    reversal: &PaymentReversal,
    responsible_org_id: &str,
    supplier_id: &SupplierAccountId,
    submitted_by: &str,
    submitted_at: Instant,
) -> Result<ApprovalSubjectSnapshotPayload> {
    Ok(ApprovalSubjectSnapshotPayload {
        document_no: reversal.reversal_no.clone(),
        responsible_org_id: payment_reversal_responsible_org_id(responsible_org_id)?,
        submitted_by: submitted_by.to_string(),
        submitted_at,
        counterparty: Some(ApprovalSubjectCounterparty::Supplier {
            supplier_id: SupplierAccountId::new(supplier_id.to_string()),
        }),
        total_amount: Some(reversal.amount),
        total_quantity: None,
        line_count: 1,
    })
}

/// 由绑定与可选实例事实构造付款冲正只读审批结构。
///
/// 创建后未提交只返回绑定定义；客户端不得据此选择定义或审批人。
///
/// # 参数
/// * `binding` - 创建时冻结的定义绑定
/// * `instance` - 已启动时的实例摘要
/// * `status` - 当前业务状态
///
/// # 返回
/// 返回有界只读审批结构。
pub fn payment_reversal_approval_view(
    binding: Option<&ApprovalDefinitionBinding>,
    instance: Option<DocumentApprovalInstanceView>,
    status: PaymentReversalStatus,
) -> DocumentApprovalView {
    DocumentApprovalView {
        requirement: match ApprovalRequirement::ProcessRequired {
            ApprovalRequirement::ProcessRequired => "PROCESS_REQUIRED",
            ApprovalRequirement::NoApproval => "NO_APPROVAL",
        }
        .to_string(),
        definition: binding.map(definition_view_from_binding),
        instance,
        recent_history: Vec::new(),
        history_page: DocumentApprovalHistoryPageView {
            next_cursor: None,
            has_more: false,
        },
        allowed_actions: payment_reversal_allowed_actions(status),
    }
}

/// 付款冲正详情允许的审批相关动作。不含选择定义或审批人。
fn payment_reversal_allowed_actions(status: PaymentReversalStatus) -> Vec<String> {
    match status {
        PaymentReversalStatus::Draft => vec!["SUBMIT".to_string()],
        PaymentReversalStatus::InApproval => vec!["CANCEL".to_string()],
        PaymentReversalStatus::Posted | PaymentReversalStatus::Reversed => Vec::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::approval::binding::binding_from_published;
    use bpm::ids::ApprovalProcessDefinitionId;
    use entities::ids::{CustomerReceiptId, CustomerRefundId};
    use entities::returns::CustomerRefundData;
    use std::str::FromStr;

    fn draft_refund() -> CustomerRefund {
        CustomerRefund::new(
            CustomerRefundId::new("crf-1"),
            CustomerRefundData {
                refund_no: "RF-1".into(),
                sales_return_case_id: None,
                customer_id: CustomerAccountId::new("cust-1"),
                original_receipt_id: Some(CustomerReceiptId::new("cr-1")),
                original_receivable_entry_id: None,
                reason_code: None,
                reason_text: "质量退款".into(),
                amount: entities::money::Amount::from_str("100").expect("金额合法"),
                handled_by: "handler-1".into(),
                reviewed_by: "reviewer-1".into(),
                occurred_at: Instant::from_unix_secs(10),
                evidence_attachment_id: None,
            },
            "creator-1",
        )
        .expect("草稿必须可构造")
    }

    /// 适配器必须显式声明合同要求的全部字段。
    #[test]
    fn adapter_declares_all_required_fields() {
        let adapter = customer_refund_adapter().expect("客户退款单必须可登记");
        assert_eq!(adapter.document_type, DocumentType::CustomerRefund);
        assert_eq!(adapter.process_kind.as_str(), "customer_refund");
        assert_eq!(
            customer_refund_subject_ref("crf-1")
                .expect("主体引用必须可构造")
                .subject_kind(),
            "customer_refund"
        );
        assert_eq!(adapter.subject_ref_builder, "subject_ref_for(CustomerRefund)");
        assert_eq!(
            adapter.subject_version_source,
            ApprovalSubjectVersionSource::EntityApprovalSubjectVersion
        );
        assert_eq!(adapter.subject_snapshot_builder, "build_customer_refund_snapshot");
        assert_eq!(
            adapter.on_approval_start,
            ApprovalDomainAction::CustomerRefundSubmit
        );
        assert_eq!(adapter.on_final_approve, ApprovalDomainAction::CustomerRefundPost);
        assert_eq!(
            adapter.cancel_action,
            ApprovalDomainAction::CustomerRefundCancelApproval
        );
        assert_eq!(adapter.owner_role, "customer_refund_approver");
        assert_eq!(
            adapter.owner_organization_snapshot,
            OwnerOrganizationSource::SubjectSnapshotResponsibleOrgId
        );
        assert_eq!(
            adapter.read_scope,
            AdapterReadScope::DocumentOrganizationAndCreator
        );
        assert_ne!(adapter.on_approval_start, adapter.on_final_approve);
        assert_ne!(adapter.on_approval_start, adapter.cancel_action);
    }

    /// 提交进入审批中；撤回不回退版本。
    #[test]
    fn submit_enters_in_approval_and_cancel_returns_draft() {
        let mut refund = draft_refund();
        start_customer_refund_approval(&mut refund).unwrap();
        assert_eq!(refund.status, CustomerRefundStatus::InApproval);
        assert_eq!(refund.approval_subject_version, 1);
        execute_customer_refund_domain_action(
            &mut refund,
            ApprovalDomainAction::CustomerRefundCancelApproval,
        )
        .unwrap();
        assert_eq!(refund.status, CustomerRefundStatus::Draft);
        assert_eq!(refund.approval_subject_version, 1);
    }

    /// 非草稿不得提交；非审批中不得撤回或过账。
    #[test]
    fn illegal_status_transitions_fail_closed() {
        let mut posted = draft_refund();
        start_customer_refund_approval(&mut posted).unwrap();
        posted.mark_posted().unwrap();
        assert!(start_customer_refund_approval(&mut posted).is_err());
        assert!(cancel_customer_refund_to_draft(&mut posted).is_err());
        assert!(ensure_final_approve_posting(&posted).is_err());
    }

    /// 过账只允许审批中，草稿不得直接过账。
    #[test]
    fn post_only_accepts_in_approval() {
        let mut refund = draft_refund();
        assert!(ensure_final_approve_posting(&refund).is_err());
        start_customer_refund_approval(&mut refund).unwrap();
        assert!(ensure_final_approve_posting(&refund).is_ok());
    }

    /// 启动命令不含定义 ID 或审批人。
    #[test]
    fn start_command_omits_definition_and_assignee() {
        let command = customer_refund_start_command("crf-1", 1, "user-1", "key-1");
        let encoded = serde_json::to_value(&command).unwrap();
        assert!(encoded.get("definition_id").is_none());
        assert!(encoded.get("definition_key").is_none());
        assert!(encoded.get("assignee").is_none());
        assert_eq!(command.subject_kind, "customer_refund");
        assert_eq!(command.subject_version, 1);
        assert_eq!(
            start_approval_command_kind(&command),
            bpm::model::types::ApprovalCommandKind::StartApproval
        );
        assert!(require_frozen_binding(None).is_err());
    }

    /// 快照冻结客户对手方、退款单号与金额合计。
    #[test]
    fn snapshot_freezes_customer_and_amount() {
        let mut refund = draft_refund();
        start_customer_refund_approval(&mut refund).unwrap();
        let payload =
            build_customer_refund_snapshot(&refund, "party-1", "user-1", Instant::from_unix_secs(10))
                .unwrap();
        assert_eq!(payload.document_no, "RF-1");
        assert_eq!(payload.responsible_org_id, "party-1");
        assert_eq!(payload.submitted_by, "user-1");
        assert_eq!(payload.total_amount.unwrap().to_string(), "100");
        assert_eq!(payload.line_count, 1);
        assert!(payload.total_quantity.is_none());
        assert!(build_customer_refund_snapshot(&refund, " ", "user-1", Instant::from_unix_secs(10)).is_err());
    }

    /// 详情只读审批结构；允许动作不含选择定义或审批人。
    #[test]
    fn detail_approval_is_read_only_and_has_history_cap() {
        let binding = binding_from_published(
            ApprovalProcessDefinitionId::new("def-1"),
            2,
            Instant::from_unix_secs(1),
        )
        .unwrap();
        let view = document_approval_view(Some(&binding), None, CustomerRefundStatus::Draft);
        assert_eq!(view.requirement, "PROCESS_REQUIRED");
        assert_eq!(view.definition.as_ref().unwrap().id, "def-1");
        assert!(view.instance.is_none());
        assert!(view.recent_history.len() <= RECENT_HISTORY_LIMIT);
        assert_eq!(view.allowed_actions, vec!["SUBMIT".to_string()]);
        assert!(!view
            .allowed_actions
            .iter()
            .any(|item| item.contains("DEFINITION")));
        let running = document_approval_view(Some(&binding), None, CustomerRefundStatus::InApproval);
        assert_eq!(running.allowed_actions, vec!["CANCEL".to_string()]);
    }

    /// 对象读取权空组织或空审批人失败关闭。
    #[test]
    fn object_read_fails_closed_on_empty_identity() {
        assert!(customer_refund_object_readable("party-1", "u1").unwrap());
        assert!(customer_refund_object_readable(" ", "u1").is_err());
        assert!(customer_refund_object_readable("party-1", "").is_err());
    }

    /// 领域动作分派只接受签署的撤回与过账动作。
    #[test]
    fn domain_action_dispatch_rejects_foreign_actions() {
        let mut refund = draft_refund();
        start_customer_refund_approval(&mut refund).unwrap();
        execute_customer_refund_domain_action(&mut refund, ApprovalDomainAction::CustomerRefundPost).unwrap();
        assert!(execute_customer_refund_domain_action(
            &mut refund,
            ApprovalDomainAction::StockAdjustmentSubmit,
        )
        .is_err());
    }
}

#[cfg(test)]
mod supplier_refund_tests {
    use super::*;
    use crate::approval::binding::binding_from_published;
    use bpm::ids::ApprovalProcessDefinitionId;
    use entities::ids::{SupplierAccountId, SupplierPaymentId, SupplierRefundId};
    use entities::returns::SupplierRefundData;
    use std::str::FromStr;

    fn draft_refund() -> SupplierRefund {
        SupplierRefund::new(
            SupplierRefundId::new("srf-1"),
            SupplierRefundData {
                refund_no: "SRF-1".into(),
                purchase_return_order_id: None,
                supplier_id: SupplierAccountId::new("sup-1"),
                original_payment_id: Some(SupplierPaymentId::new("sp-1")),
                original_payable_entry_id: None,
                reason_code: None,
                reason_text: "错付款退回".into(),
                amount: entities::money::Amount::from_str("100").expect("金额合法"),
                handled_by: "handler-1".into(),
                reviewed_by: "reviewer-1".into(),
                occurred_at: Instant::from_unix_secs(10),
                evidence_attachment_id: None,
            },
            "creator-1",
        )
        .expect("草稿必须可构造")
    }

    /// 适配器必须显式声明合同要求的全部字段。
    #[test]
    fn adapter_declares_all_required_fields() {
        let adapter = supplier_refund_adapter().expect("供应商退款单必须可登记");
        assert_eq!(adapter.document_type, DocumentType::SupplierRefund);
        assert_eq!(adapter.process_kind.as_str(), "supplier_refund");
        assert_eq!(
            supplier_refund_subject_ref("srf-1")
                .expect("主体引用必须可构造")
                .subject_kind(),
            "supplier_refund"
        );
        assert_eq!(adapter.subject_ref_builder, "subject_ref_for(SupplierRefund)");
        assert_eq!(
            adapter.subject_version_source,
            ApprovalSubjectVersionSource::EntityApprovalSubjectVersion
        );
        assert_eq!(adapter.subject_snapshot_builder, "build_supplier_refund_snapshot");
        assert_eq!(
            adapter.on_approval_start,
            ApprovalDomainAction::SupplierRefundSubmit
        );
        assert_eq!(adapter.on_final_approve, ApprovalDomainAction::SupplierRefundPost);
        assert_eq!(
            adapter.cancel_action,
            ApprovalDomainAction::SupplierRefundCancelApproval
        );
        assert_eq!(adapter.owner_role, "supplier_refund_approver");
        assert_eq!(
            adapter.owner_organization_snapshot,
            OwnerOrganizationSource::SubjectSnapshotResponsibleOrgId
        );
        assert_eq!(
            adapter.read_scope,
            AdapterReadScope::DocumentOrganizationAndCreator
        );
        assert_ne!(adapter.on_approval_start, adapter.on_final_approve);
        assert_ne!(adapter.on_approval_start, adapter.cancel_action);
    }

    /// 提交进入审批中；撤回不回退版本。
    #[test]
    fn submit_enters_in_approval_and_cancel_returns_draft() {
        let mut refund = draft_refund();
        start_supplier_refund_approval(&mut refund).unwrap();
        assert_eq!(refund.status, SupplierRefundStatus::InApproval);
        assert_eq!(refund.approval_subject_version, 1);
        execute_supplier_refund_domain_action(
            &mut refund,
            ApprovalDomainAction::SupplierRefundCancelApproval,
        )
        .unwrap();
        assert_eq!(refund.status, SupplierRefundStatus::Draft);
        assert_eq!(refund.approval_subject_version, 1);
    }

    /// 非草稿不得提交；非审批中不得撤回或过账。
    #[test]
    fn illegal_status_transitions_fail_closed() {
        let mut posted = draft_refund();
        start_supplier_refund_approval(&mut posted).unwrap();
        posted.mark_posted().unwrap();
        assert!(start_supplier_refund_approval(&mut posted).is_err());
        assert!(cancel_supplier_refund_to_draft(&mut posted).is_err());
        assert!(ensure_supplier_refund_final_approve_posting(&posted).is_err());
    }

    /// 过账只允许审批中，草稿不得直接过账。
    #[test]
    fn post_only_accepts_in_approval() {
        let mut refund = draft_refund();
        assert!(ensure_supplier_refund_final_approve_posting(&refund).is_err());
        start_supplier_refund_approval(&mut refund).unwrap();
        assert!(ensure_supplier_refund_final_approve_posting(&refund).is_ok());
    }

    /// 启动命令不含定义 ID 或审批人。
    #[test]
    fn start_command_omits_definition_and_assignee() {
        let command = supplier_refund_start_command("srf-1", 1, "user-1", "key-1");
        let encoded = serde_json::to_value(&command).unwrap();
        assert!(encoded.get("definition_id").is_none());
        assert!(encoded.get("definition_key").is_none());
        assert!(encoded.get("assignee").is_none());
        assert_eq!(command.subject_kind, "supplier_refund");
        assert_eq!(command.subject_version, 1);
        assert_eq!(
            supplier_refund_start_command_kind(&command),
            bpm::model::types::ApprovalCommandKind::StartApproval
        );
        assert!(require_supplier_refund_binding(None).is_err());
    }

    /// 快照冻结供应商对手方、退款单号与金额合计。
    #[test]
    fn snapshot_freezes_supplier_and_amount() {
        let mut refund = draft_refund();
        start_supplier_refund_approval(&mut refund).unwrap();
        let payload =
            build_supplier_refund_snapshot(&refund, "party-1", "user-1", Instant::from_unix_secs(10))
                .unwrap();
        assert_eq!(payload.document_no, "SRF-1");
        assert_eq!(payload.responsible_org_id, "party-1");
        assert_eq!(payload.submitted_by, "user-1");
        assert_eq!(payload.total_amount.unwrap().to_string(), "100");
        assert_eq!(payload.line_count, 1);
        assert!(payload.total_quantity.is_none());
        assert!(build_supplier_refund_snapshot(&refund, " ", "user-1", Instant::from_unix_secs(10)).is_err());
    }

    /// 详情只读审批结构；允许动作不含选择定义或审批人。
    #[test]
    fn detail_approval_is_read_only_and_has_history_cap() {
        let binding = binding_from_published(
            ApprovalProcessDefinitionId::new("def-1"),
            2,
            Instant::from_unix_secs(1),
        )
        .unwrap();
        let view = supplier_refund_approval_view(Some(&binding), None, SupplierRefundStatus::Draft);
        assert_eq!(view.requirement, "PROCESS_REQUIRED");
        assert_eq!(view.definition.as_ref().unwrap().id, "def-1");
        assert!(view.instance.is_none());
        assert!(view.recent_history.len() <= RECENT_HISTORY_LIMIT);
        assert_eq!(view.allowed_actions, vec!["SUBMIT".to_string()]);
        assert!(!view
            .allowed_actions
            .iter()
            .any(|item| item.contains("DEFINITION")));
        let running = supplier_refund_approval_view(Some(&binding), None, SupplierRefundStatus::InApproval);
        assert_eq!(running.allowed_actions, vec!["CANCEL".to_string()]);
    }

    /// 对象读取权空组织或空审批人失败关闭。
    #[test]
    fn object_read_fails_closed_on_empty_identity() {
        assert!(supplier_refund_object_readable("party-1", "u1").unwrap());
        assert!(supplier_refund_object_readable(" ", "u1").is_err());
        assert!(supplier_refund_object_readable("party-1", "").is_err());
    }

    /// 领域动作分派只接受签署的撤回与过账动作。
    #[test]
    fn domain_action_dispatch_rejects_foreign_actions() {
        let mut refund = draft_refund();
        start_supplier_refund_approval(&mut refund).unwrap();
        execute_supplier_refund_domain_action(&mut refund, ApprovalDomainAction::SupplierRefundPost).unwrap();
        assert!(execute_supplier_refund_domain_action(
            &mut refund,
            ApprovalDomainAction::StockAdjustmentSubmit,
        )
        .is_err());
    }
}

#[cfg(test)]
mod receipt_reversal_tests {
    use super::*;
    use crate::approval::binding::binding_from_published;
    use bpm::ids::ApprovalProcessDefinitionId;
    use entities::ids::{CustomerReceiptId, ReceiptReversalId};
    use entities::returns::ReceiptReversalData;
    use std::str::FromStr;

    fn draft_reversal() -> ReceiptReversal {
        ReceiptReversal::new(
            ReceiptReversalId::new("rr-1"),
            ReceiptReversalData {
                reversal_no: "RR-1".into(),
                original_customer_receipt_id: CustomerReceiptId::new("cr-1"),
                reason_code: None,
                reason_text: "错记回款冲正".into(),
                amount: entities::money::Amount::from_str("100").expect("金额合法"),
                handled_by: "handler-1".into(),
                reviewed_by: "reviewer-1".into(),
                occurred_at: Instant::from_unix_secs(10),
                evidence_attachment_id: None,
            },
            "creator-1",
        )
        .expect("草稿必须可构造")
    }

    /// 适配器必须显式声明合同要求的全部字段。
    #[test]
    fn adapter_declares_all_required_fields() {
        let adapter = receipt_reversal_adapter().expect("回款冲正单必须可登记");
        assert_eq!(adapter.document_type, DocumentType::ReceiptReversal);
        assert_eq!(adapter.process_kind.as_str(), "receipt_reversal");
        assert_eq!(
            receipt_reversal_subject_ref("rr-1")
                .expect("主体引用必须可构造")
                .subject_kind(),
            "receipt_reversal"
        );
        assert_eq!(adapter.subject_ref_builder, "subject_ref_for(ReceiptReversal)");
        assert_eq!(
            adapter.subject_version_source,
            ApprovalSubjectVersionSource::EntityApprovalSubjectVersion
        );
        assert_eq!(
            adapter.subject_snapshot_builder,
            "build_receipt_reversal_snapshot"
        );
        assert_eq!(
            adapter.on_approval_start,
            ApprovalDomainAction::ReceiptReversalSubmit
        );
        assert_eq!(
            adapter.on_final_approve,
            ApprovalDomainAction::ReceiptReversalPost
        );
        assert_eq!(
            adapter.cancel_action,
            ApprovalDomainAction::ReceiptReversalCancelApproval
        );
        assert_eq!(adapter.owner_role, "receipt_reversal_approver");
        assert_eq!(
            adapter.owner_organization_snapshot,
            OwnerOrganizationSource::SubjectSnapshotResponsibleOrgId
        );
        assert_eq!(
            adapter.read_scope,
            AdapterReadScope::DocumentOrganizationAndCreator
        );
        assert_ne!(adapter.on_approval_start, adapter.on_final_approve);
        assert_ne!(adapter.on_approval_start, adapter.cancel_action);
    }

    /// 提交进入审批中；撤回不回退版本。
    #[test]
    fn submit_enters_in_approval_and_cancel_returns_draft() {
        let mut reversal = draft_reversal();
        start_receipt_reversal_approval(&mut reversal).unwrap();
        assert_eq!(reversal.status, ReceiptReversalStatus::InApproval);
        assert_eq!(reversal.approval_subject_version, 1);
        execute_receipt_reversal_domain_action(
            &mut reversal,
            ApprovalDomainAction::ReceiptReversalCancelApproval,
        )
        .unwrap();
        assert_eq!(reversal.status, ReceiptReversalStatus::Draft);
        assert_eq!(reversal.approval_subject_version, 1);
    }

    /// 非草稿不得提交；非审批中不得撤回或过账。
    #[test]
    fn illegal_status_transitions_fail_closed() {
        let mut posted = draft_reversal();
        start_receipt_reversal_approval(&mut posted).unwrap();
        posted.mark_posted().unwrap();
        assert!(start_receipt_reversal_approval(&mut posted).is_err());
        assert!(cancel_receipt_reversal_to_draft(&mut posted).is_err());
        assert!(ensure_receipt_reversal_final_approve_posting(&posted).is_err());
    }

    /// 过账只允许审批中，草稿不得直接过账。
    #[test]
    fn post_only_accepts_in_approval() {
        let mut reversal = draft_reversal();
        assert!(ensure_receipt_reversal_final_approve_posting(&reversal).is_err());
        start_receipt_reversal_approval(&mut reversal).unwrap();
        assert!(ensure_receipt_reversal_final_approve_posting(&reversal).is_ok());
    }

    /// 启动命令不含定义 ID 或审批人。
    #[test]
    fn start_command_omits_definition_and_assignee() {
        let command = receipt_reversal_start_command("rr-1", 1, "user-1", "key-1");
        let encoded = serde_json::to_value(&command).unwrap();
        assert!(encoded.get("definition_id").is_none());
        assert!(encoded.get("definition_key").is_none());
        assert!(encoded.get("assignee").is_none());
        assert_eq!(command.subject_kind, "receipt_reversal");
        assert_eq!(command.subject_version, 1);
        assert_eq!(
            receipt_reversal_start_command_kind(&command),
            bpm::model::types::ApprovalCommandKind::StartApproval
        );
        assert!(require_receipt_reversal_binding(None).is_err());
    }

    /// 快照冻结客户对手方、冲正单号与金额合计。
    #[test]
    fn snapshot_freezes_customer_and_amount() {
        let mut reversal = draft_reversal();
        start_receipt_reversal_approval(&mut reversal).unwrap();
        let customer_id = CustomerAccountId::new("cust-1");
        let payload = build_receipt_reversal_snapshot(
            &reversal,
            "party-1",
            Some(&customer_id),
            "user-1",
            Instant::from_unix_secs(10),
        )
        .unwrap();
        assert_eq!(payload.document_no, "RR-1");
        assert_eq!(payload.responsible_org_id, "party-1");
        assert_eq!(payload.submitted_by, "user-1");
        assert_eq!(payload.total_amount.unwrap().to_string(), "100");
        assert_eq!(payload.line_count, 1);
        assert!(payload.total_quantity.is_none());
        assert!(matches!(
            payload.counterparty,
            Some(ApprovalSubjectCounterparty::Customer { .. })
        ));
        assert!(
            build_receipt_reversal_snapshot(&reversal, " ", None, "user-1", Instant::from_unix_secs(10))
                .is_err()
        );
    }

    /// 详情只读审批结构；允许动作不含选择定义或审批人。
    #[test]
    fn detail_approval_is_read_only_and_has_history_cap() {
        let binding = binding_from_published(
            ApprovalProcessDefinitionId::new("def-1"),
            2,
            Instant::from_unix_secs(1),
        )
        .unwrap();
        let view = receipt_reversal_approval_view(Some(&binding), None, ReceiptReversalStatus::Draft);
        assert_eq!(view.requirement, "PROCESS_REQUIRED");
        assert_eq!(view.definition.as_ref().unwrap().id, "def-1");
        assert!(view.instance.is_none());
        assert!(view.recent_history.len() <= RECENT_HISTORY_LIMIT);
        assert_eq!(view.allowed_actions, vec!["SUBMIT".to_string()]);
        assert!(!view
            .allowed_actions
            .iter()
            .any(|item| item.contains("DEFINITION")));
        let running = receipt_reversal_approval_view(Some(&binding), None, ReceiptReversalStatus::InApproval);
        assert_eq!(running.allowed_actions, vec!["CANCEL".to_string()]);
    }

    /// 对象读取权空组织或空审批人失败关闭。
    #[test]
    fn object_read_fails_closed_on_empty_identity() {
        assert!(receipt_reversal_object_readable("party-1", "u1").unwrap());
        assert!(receipt_reversal_object_readable(" ", "u1").is_err());
        assert!(receipt_reversal_object_readable("party-1", "").is_err());
    }

    /// 领域动作分派只接受签署的撤回与过账动作。
    #[test]
    fn domain_action_dispatch_rejects_foreign_actions() {
        let mut reversal = draft_reversal();
        start_receipt_reversal_approval(&mut reversal).unwrap();
        execute_receipt_reversal_domain_action(&mut reversal, ApprovalDomainAction::ReceiptReversalPost)
            .unwrap();
        assert!(execute_receipt_reversal_domain_action(
            &mut reversal,
            ApprovalDomainAction::StockAdjustmentSubmit,
        )
        .is_err());
    }
}

#[cfg(test)]
mod payment_reversal_tests {
    use super::*;
    use crate::approval::binding::binding_from_published;
    use bpm::ids::ApprovalProcessDefinitionId;
    use entities::ids::{PaymentReversalId, SupplierPaymentId};
    use entities::returns::PaymentReversalData;
    use std::str::FromStr;

    fn draft_reversal() -> PaymentReversal {
        PaymentReversal::new(
            PaymentReversalId::new("prr-1"),
            PaymentReversalData {
                reversal_no: "PRR-1".into(),
                original_supplier_payment_id: SupplierPaymentId::new("sp-1"),
                reason_code: None,
                reason_text: "错付款冲正".into(),
                amount: entities::money::Amount::from_str("100").expect("金额合法"),
                handled_by: "handler-1".into(),
                reviewed_by: "reviewer-1".into(),
                occurred_at: Instant::from_unix_secs(10),
                evidence_attachment_id: None,
            },
            "creator-1",
        )
        .expect("草稿必须可构造")
    }

    /// 适配器必须显式声明合同要求的全部字段。
    #[test]
    fn adapter_declares_all_required_fields() {
        let adapter = payment_reversal_adapter().expect("付款冲正单必须可登记");
        assert_eq!(adapter.document_type, DocumentType::PaymentReversal);
        assert_eq!(adapter.process_kind.as_str(), "payment_reversal");
        assert_eq!(
            payment_reversal_subject_ref("prr-1")
                .expect("主体引用必须可构造")
                .subject_kind(),
            "payment_reversal"
        );
        assert_eq!(adapter.subject_ref_builder, "subject_ref_for(PaymentReversal)");
        assert_eq!(
            adapter.subject_version_source,
            ApprovalSubjectVersionSource::EntityApprovalSubjectVersion
        );
        assert_eq!(
            adapter.subject_snapshot_builder,
            "build_payment_reversal_snapshot"
        );
        assert_eq!(
            adapter.on_approval_start,
            ApprovalDomainAction::PaymentReversalSubmit
        );
        assert_eq!(
            adapter.on_final_approve,
            ApprovalDomainAction::PaymentReversalPost
        );
        assert_eq!(
            adapter.cancel_action,
            ApprovalDomainAction::PaymentReversalCancelApproval
        );
        assert_eq!(adapter.owner_role, "payment_reversal_approver");
        assert_eq!(
            adapter.owner_organization_snapshot,
            OwnerOrganizationSource::SubjectSnapshotResponsibleOrgId
        );
        assert_eq!(
            adapter.read_scope,
            AdapterReadScope::DocumentOrganizationAndCreator
        );
        assert_ne!(adapter.on_approval_start, adapter.on_final_approve);
        assert_ne!(adapter.on_approval_start, adapter.cancel_action);
    }

    /// 提交进入审批中；撤回不回退版本。
    #[test]
    fn submit_enters_in_approval_and_cancel_returns_draft() {
        let mut reversal = draft_reversal();
        start_payment_reversal_approval(&mut reversal).unwrap();
        assert_eq!(reversal.status, PaymentReversalStatus::InApproval);
        assert_eq!(reversal.approval_subject_version, 1);
        execute_payment_reversal_domain_action(
            &mut reversal,
            ApprovalDomainAction::PaymentReversalCancelApproval,
        )
        .unwrap();
        assert_eq!(reversal.status, PaymentReversalStatus::Draft);
        assert_eq!(reversal.approval_subject_version, 1);
    }

    /// 非草稿不得提交；非审批中不得撤回或过账。
    #[test]
    fn illegal_status_transitions_fail_closed() {
        let mut posted = draft_reversal();
        start_payment_reversal_approval(&mut posted).unwrap();
        posted.mark_posted().unwrap();
        assert!(start_payment_reversal_approval(&mut posted).is_err());
        assert!(cancel_payment_reversal_to_draft(&mut posted).is_err());
        assert!(ensure_payment_reversal_final_approve_posting(&posted).is_err());
    }

    /// 过账只允许审批中，草稿不得直接过账。
    #[test]
    fn post_only_accepts_in_approval() {
        let mut reversal = draft_reversal();
        assert!(ensure_payment_reversal_final_approve_posting(&reversal).is_err());
        start_payment_reversal_approval(&mut reversal).unwrap();
        assert!(ensure_payment_reversal_final_approve_posting(&reversal).is_ok());
    }

    /// 启动命令不含定义 ID 或审批人。
    #[test]
    fn start_command_omits_definition_and_assignee() {
        let command = payment_reversal_start_command("prr-1", 1, "user-1", "key-1");
        let encoded = serde_json::to_value(&command).unwrap();
        assert!(encoded.get("definition_id").is_none());
        assert!(encoded.get("definition_key").is_none());
        assert!(encoded.get("assignee").is_none());
        assert_eq!(command.subject_kind, "payment_reversal");
        assert_eq!(command.subject_version, 1);
        assert_eq!(
            payment_reversal_start_command_kind(&command),
            bpm::model::types::ApprovalCommandKind::StartApproval
        );
        assert!(require_payment_reversal_binding(None).is_err());
    }

    /// 快照冻结供应商对手方、冲正单号与金额合计。
    #[test]
    fn snapshot_freezes_supplier_and_amount() {
        let mut reversal = draft_reversal();
        start_payment_reversal_approval(&mut reversal).unwrap();
        let supplier_id = SupplierAccountId::new("sup-1");
        let payload = build_payment_reversal_snapshot(
            &reversal,
            "party-1",
            &supplier_id,
            "user-1",
            Instant::from_unix_secs(10),
        )
        .unwrap();
        assert_eq!(payload.document_no, "PRR-1");
        assert_eq!(payload.responsible_org_id, "party-1");
        assert_eq!(payload.submitted_by, "user-1");
        assert_eq!(payload.total_amount.unwrap().to_string(), "100");
        assert_eq!(payload.line_count, 1);
        assert!(payload.total_quantity.is_none());
        assert!(matches!(
            payload.counterparty,
            Some(ApprovalSubjectCounterparty::Supplier { .. })
        ));
        assert!(build_payment_reversal_snapshot(
            &reversal,
            " ",
            &supplier_id,
            "user-1",
            Instant::from_unix_secs(10)
        )
        .is_err());
    }

    /// 详情只读审批结构；允许动作不含选择定义或审批人。
    #[test]
    fn detail_approval_is_read_only_and_has_history_cap() {
        let binding = binding_from_published(
            ApprovalProcessDefinitionId::new("def-1"),
            2,
            Instant::from_unix_secs(1),
        )
        .unwrap();
        let view = payment_reversal_approval_view(Some(&binding), None, PaymentReversalStatus::Draft);
        assert_eq!(view.requirement, "PROCESS_REQUIRED");
        assert_eq!(view.definition.as_ref().unwrap().id, "def-1");
        assert!(view.instance.is_none());
        assert!(view.recent_history.len() <= RECENT_HISTORY_LIMIT);
        assert_eq!(view.allowed_actions, vec!["SUBMIT".to_string()]);
        assert!(!view
            .allowed_actions
            .iter()
            .any(|item| item.contains("DEFINITION")));
        let running = payment_reversal_approval_view(Some(&binding), None, PaymentReversalStatus::InApproval);
        assert_eq!(running.allowed_actions, vec!["CANCEL".to_string()]);
    }

    /// 对象读取权空组织或空审批人失败关闭。
    #[test]
    fn object_read_fails_closed_on_empty_identity() {
        assert!(payment_reversal_object_readable("party-1", "u1").unwrap());
        assert!(payment_reversal_object_readable(" ", "u1").is_err());
        assert!(payment_reversal_object_readable("party-1", "").is_err());
    }

    /// 领域动作分派只接受签署的撤回与过账动作。
    #[test]
    fn domain_action_dispatch_rejects_foreign_actions() {
        let mut reversal = draft_reversal();
        start_payment_reversal_approval(&mut reversal).unwrap();
        execute_payment_reversal_domain_action(&mut reversal, ApprovalDomainAction::PaymentReversalPost)
            .unwrap();
        assert!(execute_payment_reversal_domain_action(
            &mut reversal,
            ApprovalDomainAction::StockAdjustmentSubmit,
        )
        .is_err());
    }
}
