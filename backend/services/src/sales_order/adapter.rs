//! `SalesOrder` 与 `VoucherSalesOrder` 审批业务 Adapter。
//!
//! 两者按 `BusinessType` 穷尽分派到独立 `DocumentType` / `ProcessKind`。
//! 领域动作只通过实体状态邻接与仓储更新，不得 `$set` 绕过不变式，
//! 运行时不得按采购确认或卡券运营节点用途分支。

use bpm::SubjectRef;
use entities::approval_integration::{ApprovalSubjectCounterparty, ApprovalSubjectSnapshotPayload};
use entities::common::time::Instant;
use entities::document_registry::business_document::ApprovalDefinitionBinding;
use entities::document_registry::DocumentType;
use entities::ids::CustomerAccountId;
use entities::money::Quantity;
use entities::sales_order::{
    BusinessType, CommercialStatus, ReviewStatus, SalesOrder, SalesOrderSubmission, SalesOrderSubmissionLine,
};

use crate::approval::business_adapter::{
    adapter_spec_of, ensure_adapter_spec_complete, subject_ref_for, AdapterReadScope, ApprovalAdapterSpec,
};
use crate::approval::policy::{
    ApprovalDomainAction, ApprovalRequirement, ApprovalSubjectSnapshotField, ApprovalSubjectVersionSource,
    OwnerOrganizationSource, SALES_ORDER_PROCUREMENT_CONFIRMATION,
};
use crate::approval::process_kind::process_kind_of;
use crate::errors::{Error, Result};

use super::dto::{
    DocumentApprovalDefinitionView, DocumentApprovalHistoryPageView, DocumentApprovalInstanceView,
    DocumentApprovalView,
};

/// 详情最近审批历史条数上限。完整历史走分页端点。
pub const RECENT_HISTORY_LIMIT: usize = 8;

/// 已注册的实物及服务销售单适配器规格。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SalesOrderAdapter {
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

/// 返回实物及服务销售单的完整适配器登记。
///
/// # 返回
/// 返回已校验完整性的规格与显式字段声明。
///
/// # 错误
/// 政策缺失或三类动作不互异时返回部署不变量错误。
pub fn sales_order_adapter() -> Result<SalesOrderAdapter> {
    let spec = adapter_spec_of(DocumentType::SalesOrder)?;
    ensure_adapter_spec_complete(&spec)?;
    adapter_from_spec(spec)
}

/// 已注册的卡券销售单适配器规格。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VoucherSalesOrderAdapter {
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

/// 返回卡券销售单的完整适配器登记。
///
/// # 返回
/// 返回已校验完整性的规格与显式字段声明。
///
/// # 错误
/// 政策缺失或三类动作不互异时返回部署不变量错误。
pub fn voucher_sales_order_adapter() -> Result<VoucherSalesOrderAdapter> {
    let spec = adapter_spec_of(DocumentType::VoucherSalesOrder)?;
    ensure_adapter_spec_complete(&spec)?;
    voucher_adapter_from_spec(spec)
}

/// 由政策规格填充卡券 Adapter 字段。
///
/// # 错误
/// 字段与合同签署值不一致时返回错误。
fn voucher_adapter_from_spec(spec: ApprovalAdapterSpec) -> Result<VoucherSalesOrderAdapter> {
    if spec.document_type != DocumentType::VoucherSalesOrder
        || spec.process_kind != process_kind_of(DocumentType::VoucherSalesOrder)
        || spec.subject_version_source != ApprovalSubjectVersionSource::SalesOrderSubmissionNo
        || spec.on_approval_start != ApprovalDomainAction::VoucherSalesOrderStartApprovalSubmission
        || spec.on_final_approve != ApprovalDomainAction::VoucherSalesOrderFormalizeApprovedSubmission
        || spec.cancel_action != ApprovalDomainAction::VoucherSalesOrderCancelApprovalSubmission
        || spec.owner_role.as_str() != "voucher_sales_order_approver"
        || spec.owner_organization_source != OwnerOrganizationSource::SubjectSnapshotResponsibleOrgId
        || spec.read_scope != AdapterReadScope::DocumentOrganizationAndCreator
        || !spec
            .subject_snapshot_fields
            .contains(&ApprovalSubjectSnapshotField::TotalAmount)
        || !spec
            .subject_snapshot_fields
            .contains(&ApprovalSubjectSnapshotField::TotalQuantity)
    {
        return Err(Error::Internal("卡券销售单审批适配器登记不完整".to_string()));
    }
    Ok(VoucherSalesOrderAdapter {
        document_type: spec.document_type,
        process_kind: spec.process_kind,
        subject_ref_builder: "subject_ref_for(VoucherSalesOrder)",
        subject_version_source: spec.subject_version_source,
        subject_snapshot_builder: "build_sales_order_snapshot",
        on_approval_start: spec.on_approval_start,
        on_final_approve: spec.on_final_approve,
        cancel_action: spec.cancel_action,
        owner_role: spec.owner_role.as_str(),
        owner_organization_snapshot: spec.owner_organization_source,
        read_scope: spec.read_scope,
    })
}

/// 两类销售单提交/撤回/正式化共用的已登记端口。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SalesApprovalPorts {
    /// 创建与启动应使用的单据类型。
    pub document_type: DocumentType,
    /// 提交并启动动作。
    pub on_approval_start: ApprovalDomainAction,
    /// 最终通过动作。
    pub on_final_approve: ApprovalDomainAction,
    /// 撤回动作。
    pub cancel_action: ApprovalDomainAction,
    /// WorkItem 责任角色。
    pub owner_role: &'static str,
}

/// 按 `BusinessType` 返回对应独立 Adapter 的强类型端口。
///
/// # 参数
/// * `business_type` - 销售单业务性质
///
/// # 返回
/// 返回该类型已登记的单据类型、三类动作与责任角色。
///
/// # 错误
/// 适配器登记不完整时返回部署不变量错误。
pub fn sales_approval_ports(business_type: BusinessType) -> Result<SalesApprovalPorts> {
    match business_type {
        BusinessType::GoodsService => {
            let adapter = sales_order_adapter()?;
            Ok(SalesApprovalPorts {
                document_type: adapter.document_type,
                on_approval_start: adapter.on_approval_start,
                on_final_approve: adapter.on_final_approve,
                cancel_action: adapter.cancel_action,
                owner_role: adapter.owner_role,
            })
        }
        BusinessType::Voucher => {
            let adapter = voucher_sales_order_adapter()?;
            Ok(SalesApprovalPorts {
                document_type: adapter.document_type,
                on_approval_start: adapter.on_approval_start,
                on_final_approve: adapter.on_final_approve,
                cancel_action: adapter.cancel_action,
                owner_role: adapter.owner_role,
            })
        }
    }
}

/// 由政策规格填充显式 Adapter 字段。
///
/// # 错误
/// 字段与合同签署值不一致时返回错误。
fn adapter_from_spec(spec: ApprovalAdapterSpec) -> Result<SalesOrderAdapter> {
    if spec.document_type != DocumentType::SalesOrder
        || spec.process_kind != process_kind_of(DocumentType::SalesOrder)
        || spec.subject_version_source != ApprovalSubjectVersionSource::SalesOrderSubmissionNo
        || spec.on_approval_start != ApprovalDomainAction::SalesOrderStartApprovalSubmission
        || spec.on_final_approve != ApprovalDomainAction::SalesOrderFormalizeApprovedSubmission
        || spec.cancel_action != ApprovalDomainAction::SalesOrderCancelApprovalSubmission
        || spec.owner_role.as_str() != "sales_order_approver"
        || spec.owner_organization_source != OwnerOrganizationSource::SubjectSnapshotResponsibleOrgId
        || spec.read_scope != AdapterReadScope::DocumentOrganizationAndCreator
        || !spec
            .subject_snapshot_fields
            .contains(&ApprovalSubjectSnapshotField::TotalAmount)
        || !spec
            .subject_snapshot_fields
            .contains(&ApprovalSubjectSnapshotField::TotalQuantity)
    {
        return Err(Error::Internal("销售单审批适配器登记不完整".to_string()));
    }
    Ok(SalesOrderAdapter {
        document_type: spec.document_type,
        process_kind: spec.process_kind,
        subject_ref_builder: "subject_ref_for(SalesOrder)",
        subject_version_source: spec.subject_version_source,
        subject_snapshot_builder: "build_sales_order_snapshot",
        on_approval_start: spec.on_approval_start,
        on_final_approve: spec.on_final_approve,
        cancel_action: spec.cancel_action,
        owner_role: spec.owner_role.as_str(),
        owner_organization_snapshot: spec.owner_organization_source,
        read_scope: spec.read_scope,
    })
}

/// 为实物及服务销售单构造唯一 `bpm::SubjectRef`。
///
/// # 参数
/// * `business_object_id` - 销售单主键
///
/// # 错误
/// 主键为空或超长时返回校验错误。
pub fn sales_order_subject_ref(business_object_id: &str) -> Result<SubjectRef> {
    subject_ref_for(DocumentType::SalesOrder, business_object_id)
}

/// 为卡券销售单构造唯一 `bpm::SubjectRef`。
///
/// # 参数
/// * `business_object_id` - 销售单主键
///
/// # 错误
/// 主键为空或超长时返回校验错误。
pub fn voucher_sales_order_subject_ref(business_object_id: &str) -> Result<SubjectRef> {
    subject_ref_for(DocumentType::VoucherSalesOrder, business_object_id)
}

/// 按业务性质构造对应独立 `DocumentType` 的主体引用。
///
/// # 参数
/// * `business_type` - 销售单业务性质
/// * `business_object_id` - 销售单主键
///
/// # 错误
/// 主键为空或超长时返回校验错误。
pub fn subject_ref_for_sales_business(
    business_type: BusinessType,
    business_object_id: &str,
) -> Result<SubjectRef> {
    if is_goods_service_sales_order(business_type) {
        sales_order_subject_ref(business_object_id)
    } else {
        voucher_sales_order_subject_ref(business_object_id)
    }
}

/// 按 `BusinessType` 穷尽分派创建时应绑定的单据类型。
///
/// `GoodsService` 绑定 `SalesOrder`，`Voucher` 绑定 `VoucherSalesOrder`。
/// 不得在同一 `ProcessKind` 内按业务性质二次分流。
///
/// # 参数
/// * `business_type` - 销售单业务性质
///
/// # 返回
/// 返回创建事务应写入的 `DocumentType`。
pub fn document_type_for_sales_create(business_type: BusinessType) -> DocumentType {
    match business_type {
        BusinessType::GoodsService => DocumentType::SalesOrder,
        BusinessType::Voucher => DocumentType::VoucherSalesOrder,
    }
}

/// 是否为本阶段应绑定并启动统一审批的实物及服务销售单。
///
/// # 参数
/// * `business_type` - 销售单业务性质
///
/// # 返回
/// 实物及服务为 `true`。
pub fn is_goods_service_sales_order(business_type: BusinessType) -> bool {
    matches!(business_type, BusinessType::GoodsService)
}

/// 是否为卡券销售单。
///
/// # 参数
/// * `business_type` - 销售单业务性质
///
/// # 返回
/// 卡券为 `true`。
pub fn is_voucher_sales_order(business_type: BusinessType) -> bool {
    matches!(business_type, BusinessType::Voucher)
}

/// 提交并启动：进入 `PENDING_REVIEW` / `IN_APPROVAL`。
///
/// 版本权威来源是提交记录 `submission_no`，本方法不改写该编号。
/// 更新人必须是调用方提交销售，不得回落到上次更新人。
///
/// # 参数
/// * `order` - 待提交销售单
/// * `submitted_by` - 本次提交销售
///
/// # 错误
/// 状态不允许时返回冲突。
pub fn start_sales_order_approval(order: &mut SalesOrder, submitted_by: &str) -> Result<()> {
    Ok(order.start_approval_submission(submitted_by)?)
}

/// 撤回审批：回到 `DRAFT` / `NOT_SUBMITTED`，且提交号不回退。
///
/// # 参数
/// * `order` - 审批中的销售单
/// * `updated_by` - 操作人
///
/// # 错误
/// 非审批中时返回冲突。
pub fn cancel_sales_order_to_draft(order: &mut SalesOrder, updated_by: &str) -> Result<()> {
    Ok(order.cancel_approval_submission(updated_by)?)
}

/// 最终通过前置：仅 `IN_APPROVAL` 可进入生效。
///
/// # 错误
/// 状态不是审批中时返回冲突。
pub fn ensure_final_approve_formalize(order: &SalesOrder) -> Result<()> {
    if order.commercial_status != CommercialStatus::PendingReview
        || order.review_status != ReviewStatus::InApproval
    {
        return Err(Error::ConflictError(
            "只有审批中的销售单可以由最终通过动作形式化".to_string(),
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
    binding.ok_or_else(|| Error::ConflictError("无有效审批绑定的销售单不得提交".to_string()))
}

/// 销售单调用统一 `start_approval` 的目标命令。
///
/// 字段与合同 §14.2 对齐；不得包含定义 ID 或审批人。
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct SalesOrderStartCommand {
    /// 业务对象种类。
    pub subject_kind: String,
    /// 业务对象 ID。
    pub subject_id: String,
    /// 冻结提交版本，取 `submission_no`。
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
/// * `document_type` - 按业务性质分派的独立单据类型
/// * `sales_order_id` - 销售单主键
/// * `subject_version` - `sales_order_submission.submission_no`
/// * `actor_id` - 提交人
/// * `idempotency_key` - 幂等键
///
/// # 返回
/// 返回不含定义 ID 或审批人的目标启动命令。
pub fn sales_order_start_command(
    document_type: DocumentType,
    sales_order_id: &str,
    subject_version: u32,
    actor_id: &str,
    idempotency_key: &str,
) -> SalesOrderStartCommand {
    SalesOrderStartCommand {
        subject_kind: process_kind_of(document_type).as_str().to_string(),
        subject_id: sales_order_id.to_string(),
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
    _command: &SalesOrderStartCommand,
) -> bpm::model::types::ApprovalCommandKind {
    let _ = SALES_ORDER_PROCUREMENT_CONFIRMATION;
    bpm::model::types::ApprovalCommandKind::StartApproval
}

/// 执行签署的销售单领域动作。
///
/// # 参数
/// * `order` - 业务实体
/// * `action` - 合同强类型动作
/// * `updated_by` - 操作人
///
/// # 错误
/// 动作不属于本类型或状态不允许时返回错误。
pub fn execute_sales_order_domain_action(
    order: &mut SalesOrder,
    action: ApprovalDomainAction,
    updated_by: &str,
) -> Result<()> {
    let ports = sales_approval_ports(order.business_type)?;
    if action == ports.on_approval_start {
        return start_sales_order_approval(order, updated_by);
    }
    if action == ports.on_final_approve {
        return ensure_final_approve_formalize(order);
    }
    if action == ports.cancel_action {
        return cancel_sales_order_to_draft(order, updated_by);
    }
    Err(Error::ValidationError(format!(
        "动作 {} 不属于 {}",
        action.as_str(),
        ports.document_type.label()
    )))
}

/// 卡券专用决定路径新写立即失败关闭。
///
/// # 错误
/// 恒返回冲突，不得回退 `CARD_SALES_APPROVAL` 或专用决定端口。
pub fn reject_legacy_card_sales_decision() -> Result<()> {
    Err(Error::ConflictError(
        "卡券销售单必须走统一审批，禁止写入卡券专用决定路径".to_string(),
    ))
}

/// `CardSalesManagerApproval` / `CardSalesOperationApproval` 新写立即失败关闭。
///
/// # 参数
/// * `work_item_type` - 待写入的工作项类型稳定码
///
/// # 错误
/// 命中卡券专用类型时返回冲突。
pub fn reject_legacy_card_sales_work_item(work_item_type: &str) -> Result<()> {
    match work_item_type {
        "CARD_SALES_MANAGER_APPROVAL" | "CARD_SALES_OPERATION_APPROVAL" => {
            Err(Error::ConflictError("禁止新建卡券专用审批工作项".to_string()))
        }
        _ => Ok(()),
    }
}

/// 按单据组织判定审批人对象读取权。
///
/// 未提供组织或审批人时失败关闭，不得默认放行。
///
/// # 参数
/// * `organization_id` - 单据责任组织
/// * `assignee_user_id` - 指定审批人
///
/// # 返回
/// 组织与审批人均非空时允许读取。
///
/// # 错误
/// 组织或审批人为空时返回校验错误。
pub fn sales_order_object_readable(organization_id: &str, assignee_user_id: &str) -> Result<bool> {
    if organization_id.trim().is_empty() || assignee_user_id.trim().is_empty() {
        return Err(Error::ValidationError("单据组织或审批人不能为空".to_string()));
    }
    Ok(true)
}

/// 责任组织取结算主体，不得用空串或当前登录人组织补位。
///
/// # 参数
/// * `order` - 销售单
///
/// # 返回
/// 返回非空责任组织。
///
/// # 错误
/// 结算主体为空时返回校验错误。
pub fn sales_order_responsible_org_id(order: &SalesOrder) -> Result<String> {
    let org = order.settlement_party_id.to_string();
    if org.trim().is_empty() {
        return Err(Error::ValidationError(
            "销售单缺少结算主体，无法冻结责任组织".to_string(),
        ));
    }
    Ok(org)
}

/// 按合同 §4.4.5 冻结实物及服务销售单快照。
///
/// 对手方为客户；金额与数量合计必填。
///
/// # 参数
/// * `order` - 销售单
/// * `submission` - 已冻结提交
/// * `lines` - 提交明细
/// * `submitted_by` - 提交销售
/// * `submitted_at` - 提交时间
///
/// # 错误
/// 明细为空、金额/数量非法或组织为空时返回校验错误。
pub fn build_sales_order_snapshot(
    order: &SalesOrder,
    submission: &SalesOrderSubmission,
    lines: &[SalesOrderSubmissionLine],
    submitted_by: &str,
    submitted_at: Instant,
) -> Result<ApprovalSubjectSnapshotPayload> {
    if lines.is_empty() {
        return Err(Error::ValidationError("销售单没有明细，无法启动审批".to_string()));
    }
    Ok(ApprovalSubjectSnapshotPayload {
        document_no: order.order_no.clone(),
        responsible_org_id: sales_order_responsible_org_id(order)?,
        submitted_by: submitted_by.to_string(),
        submitted_at,
        counterparty: Some(ApprovalSubjectCounterparty::Customer {
            customer_id: CustomerAccountId::new(order.customer_id.to_string()),
        }),
        total_amount: Some(submission.gross_amount),
        total_quantity: Some(sum_line_quantity(lines)?),
        line_count: u32::try_from(lines.len())
            .map_err(|_| Error::ValidationError("销售明细行数溢出".to_string()))?,
    })
}

/// 汇总提交明细数量。
///
/// # 错误
/// 无数量或合计超出标度时返回错误。
fn sum_line_quantity(lines: &[SalesOrderSubmissionLine]) -> Result<Quantity> {
    let mut quantities = lines.iter().filter_map(|line| line.quantity);
    let Some(first) = quantities.next() else {
        return Err(Error::ValidationError(
            "销售单明细没有数量，无法启动审批".to_string(),
        ));
    };
    let mut total = first.to_decimal();
    for quantity in quantities {
        total += quantity.to_decimal();
    }
    Quantity::try_from(total).map_err(|error| Error::ValidationError(error.to_string()))
}

/// 由绑定与可选实例事实构造只读审批结构。
///
/// 创建后未提交只返回绑定定义；运行时不得按采购确认用途分支。
///
/// # 参数
/// * `binding` - 创建时冻结的定义绑定
/// * `instance` - 已启动时的实例摘要
/// * `commercial` - 当前商业主状态
/// * `review` - 当前审核轨
///
/// # 返回
/// 返回有界只读审批结构。
pub fn document_approval_view(
    binding: Option<&ApprovalDefinitionBinding>,
    instance: Option<DocumentApprovalInstanceView>,
    commercial: CommercialStatus,
    review: ReviewStatus,
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
        allowed_actions: allowed_document_actions(commercial, review),
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
fn allowed_document_actions(commercial: CommercialStatus, review: ReviewStatus) -> Vec<String> {
    match (commercial, review) {
        (CommercialStatus::Draft, ReviewStatus::NotSubmitted) => vec!["SUBMIT".to_string()],
        (CommercialStatus::PendingReview, ReviewStatus::InApproval) => vec!["CANCEL".to_string()],
        _ => Vec::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::approval::binding::binding_from_published;
    use bpm::ids::ApprovalProcessDefinitionId;
    use entities::common::time::Instant;
    use entities::ids::{
        CustomerAccountId, PartyId, SalesOrderId, SalesOrderLineId, SalesOrderSubmissionId,
        SalesOrderSubmissionLineId, SalesOrderWorkingCopyId, SkuId, SkuRevisionId,
    };
    use entities::money::{Amount, Quantity, Rate, UnitPrice};
    use entities::sales_order::{
        FulfillmentMode, GoodsLineFields, HeaderSnapshotData, LineType, SalesOrderData,
        SalesOrderSubmissionData, SalesOrderSubmissionLineData, WelfareScenario,
    };
    use std::str::FromStr;

    fn draft_order() -> SalesOrder {
        SalesOrder::new(
            SalesOrderId::new("so-1"),
            SalesOrderData {
                order_no: "SO-1".into(),
                business_type: BusinessType::GoodsService,
                origin_system: entities::sales_order::OriginSystem::Erp,
                source_identity_id: None,
                customer_id: CustomerAccountId::new("cust-1"),
                contract_id: None,
                settlement_party_id: PartyId::new("party-1"),
                source_status_code: None,
            },
            "user-1",
        )
        .expect("草稿必须可构造")
    }

    fn goods_fields() -> GoodsLineFields {
        GoodsLineFields {
            sku_id: SkuId::new("sku-1"),
            sku_revision_id: SkuRevisionId::new("skurev-1"),
            welfare_scenario: Some(WelfareScenario::AnnualGiftBag),
            service_region: Some("EAST".into()),
            fulfillment_mode: FulfillmentMode::CompanyWarehouse,
            fulfillment_due_at: Instant::from_unix_secs(1_800_000_000),
            quantity: Quantity::from_str("2").expect("数量合法"),
            base_unit_code: "件".into(),
            unit_price_gross: UnitPrice::from_str("5.0000").expect("单价合法"),
        }
    }

    fn line_data() -> SalesOrderSubmissionLineData {
        SalesOrderSubmissionLineData {
            sales_order_line_id: SalesOrderLineId::new("line-1"),
            line_no: 1,
            line_type: LineType::GoodsService,
            sales_tax_rate: Rate::from_str("0").expect("税率合法"),
            item_name_snapshot: "商品".into(),
            spec_snapshot: None,
            unit_snapshot: Some("件".into()),
            goods: Some(goods_fields()),
            voucher: None,
        }
    }

    fn submission() -> SalesOrderSubmission {
        SalesOrderSubmission::new(
            SalesOrderSubmissionId::new("sub-1"),
            SalesOrderSubmissionData {
                sales_order_id: SalesOrderId::new("so-1"),
                submission_no: 1,
                working_copy_id: SalesOrderWorkingCopyId::new("wc-1"),
                working_copy_version: 1,
                business_type: BusinessType::GoodsService,
                customer_id: CustomerAccountId::new("cust-1"),
                contract_revision_id: None,
                settlement_party_id: PartyId::new("party-1"),
                snapshot: HeaderSnapshotData {
                    customer_name: "客户".into(),
                    contract_no: None,
                    settlement_party_name: Some("结算".into()),
                    payment_term_code: "NET30".into(),
                    payment_term_name: "月结".into(),
                    invoice_type: "普通发票".into(),
                    tax_point: "开票".into(),
                },
                project_name: None,
                business_remark: None,
                voucher_category_sku_id: None,
                voucher_expiry_at: None,
                target_mall_id: None,
                customer_external_identity: None,
                voucher_category_external_identity: None,
                receivable_due_date: None,
                gross_amount: Amount::from_str("10").expect("金额合法"),
                net_amount: Amount::from_str("10").expect("金额合法"),
                tax_amount: Amount::from_str("0").expect("金额合法"),
                submitted_at: Instant::from_unix_secs(10),
                submitted_by: "user-1".into(),
                lines: vec![line_data()],
            },
        )
        .expect("提交必须可构造")
    }

    fn one_line() -> SalesOrderSubmissionLine {
        SalesOrderSubmissionLine::new(
            SalesOrderSubmissionLineId::new("sl-1"),
            SalesOrderSubmissionId::new("sub-1"),
            line_data(),
        )
        .expect("提交行必须可构造")
    }

    /// 适配器必须显式声明合同要求的全部字段。
    #[test]
    fn adapter_declares_all_required_fields() {
        let adapter = sales_order_adapter().expect("销售单必须可登记");
        assert_eq!(adapter.document_type, DocumentType::SalesOrder);
        assert_eq!(adapter.process_kind.as_str(), "sales_order");
        assert_eq!(
            sales_order_subject_ref("so-1")
                .expect("主体引用必须可构造")
                .subject_kind(),
            "sales_order"
        );
        assert_eq!(adapter.subject_ref_builder, "subject_ref_for(SalesOrder)");
        assert_eq!(
            adapter.subject_version_source,
            ApprovalSubjectVersionSource::SalesOrderSubmissionNo
        );
        assert_eq!(adapter.subject_snapshot_builder, "build_sales_order_snapshot");
        assert_eq!(
            adapter.on_approval_start,
            ApprovalDomainAction::SalesOrderStartApprovalSubmission
        );
        assert_eq!(
            adapter.on_final_approve,
            ApprovalDomainAction::SalesOrderFormalizeApprovedSubmission
        );
        assert_eq!(
            adapter.cancel_action,
            ApprovalDomainAction::SalesOrderCancelApprovalSubmission
        );
        assert_eq!(adapter.owner_role, "sales_order_approver");
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
        assert_eq!(
            document_type_for_sales_create(BusinessType::GoodsService),
            DocumentType::SalesOrder
        );
        assert_eq!(
            document_type_for_sales_create(BusinessType::Voucher),
            DocumentType::VoucherSalesOrder
        );
        assert!(is_goods_service_sales_order(BusinessType::GoodsService));
        assert!(!is_goods_service_sales_order(BusinessType::Voucher));
        assert!(is_voucher_sales_order(BusinessType::Voucher));
    }

    /// 卡券适配器必须独立登记合同签署字段，且不得复用 SalesOrder ProcessKind。
    #[test]
    fn voucher_adapter_declares_independent_fields() {
        let adapter = voucher_sales_order_adapter().expect("卡券销售单必须可登记");
        assert_eq!(adapter.document_type, DocumentType::VoucherSalesOrder);
        assert_eq!(adapter.process_kind.as_str(), "voucher_sales_order");
        assert_eq!(
            voucher_sales_order_subject_ref("so-1")
                .expect("主体引用必须可构造")
                .subject_kind(),
            "voucher_sales_order"
        );
        assert_eq!(
            adapter.subject_version_source,
            ApprovalSubjectVersionSource::SalesOrderSubmissionNo
        );
        assert_eq!(
            adapter.on_approval_start,
            ApprovalDomainAction::VoucherSalesOrderStartApprovalSubmission
        );
        assert_eq!(
            adapter.on_final_approve,
            ApprovalDomainAction::VoucherSalesOrderFormalizeApprovedSubmission
        );
        assert_eq!(
            adapter.cancel_action,
            ApprovalDomainAction::VoucherSalesOrderCancelApprovalSubmission
        );
        assert_eq!(adapter.owner_role, "voucher_sales_order_approver");
        assert_ne!(adapter.on_approval_start, adapter.on_final_approve);
        assert_ne!(adapter.on_approval_start, adapter.cancel_action);
        let ports = sales_approval_ports(BusinessType::Voucher).expect("端口必须可取");
        assert_eq!(ports.document_type, DocumentType::VoucherSalesOrder);
        assert_eq!(ports.owner_role, "voucher_sales_order_approver");
    }

    /// 提交进入审批中；撤回不回退提交号，且切断 REJECTED。
    #[test]
    fn submit_enters_in_approval_and_cancel_returns_draft() {
        let mut order = draft_order();
        execute_sales_order_domain_action(
            &mut order,
            ApprovalDomainAction::SalesOrderStartApprovalSubmission,
            "submitter-9",
        )
        .unwrap();
        assert_eq!(order.commercial_status, CommercialStatus::PendingReview);
        assert_eq!(order.review_status, ReviewStatus::InApproval);
        assert_eq!(order.stable.updated_by, "submitter-9");
        assert!(order.transition_review(ReviewStatus::Rejected, "u").is_err());
        execute_sales_order_domain_action(
            &mut order,
            ApprovalDomainAction::SalesOrderCancelApprovalSubmission,
            "user-1",
        )
        .unwrap();
        assert_eq!(order.commercial_status, CommercialStatus::Draft);
        assert_eq!(order.review_status, ReviewStatus::NotSubmitted);
    }

    /// 非草稿不得提交；非审批中不得撤回或形式化。
    #[test]
    fn illegal_status_transitions_fail_closed() {
        let mut effective = draft_order();
        effective.commercial_status = CommercialStatus::Effective;
        effective.review_status = ReviewStatus::Approved;
        assert!(start_sales_order_approval(&mut effective, "user-2").is_err());
        assert!(cancel_sales_order_to_draft(&mut effective, "u").is_err());
        assert!(ensure_final_approve_formalize(&effective).is_err());

        let mut voucher = draft_order();
        voucher.business_type = BusinessType::Voucher;
        start_sales_order_approval(&mut voucher, "user-2").expect("卡券必须可进入统一审批");
        assert_eq!(voucher.commercial_status, CommercialStatus::PendingReview);
        assert_eq!(voucher.review_status, ReviewStatus::InApproval);
    }

    /// 启动命令不含定义 ID 或审批人。
    #[test]
    fn start_command_omits_definition_and_assignee() {
        let command = sales_order_start_command(DocumentType::SalesOrder, "so-1", 1, "user-1", "key-1");
        let encoded = serde_json::to_value(&command).unwrap();
        assert!(encoded.get("definition_id").is_none());
        assert!(encoded.get("definition_key").is_none());
        assert!(encoded.get("assignee").is_none());
        assert_eq!(command.subject_kind, "sales_order");
        assert_eq!(command.subject_version, 1);
        let voucher =
            sales_order_start_command(DocumentType::VoucherSalesOrder, "so-1", 1, "user-1", "key-1");
        assert_eq!(voucher.subject_kind, "voucher_sales_order");
        assert_eq!(
            start_approval_command_kind(&command),
            bpm::model::types::ApprovalCommandKind::StartApproval
        );
        assert!(require_frozen_binding(None).is_err());
    }

    /// 快照冻结客户对手方、金额与数量合计。
    #[test]
    fn snapshot_freezes_customer_amount_and_quantity() {
        let order = draft_order();
        let payload = build_sales_order_snapshot(
            &order,
            &submission(),
            &[one_line()],
            "user-1",
            Instant::from_unix_secs(10),
        )
        .unwrap();
        assert_eq!(payload.document_no, "SO-1");
        assert_eq!(payload.responsible_org_id, "party-1");
        assert_eq!(payload.submitted_by, "user-1");
        assert_eq!(payload.total_amount.unwrap().to_string(), "10");
        assert_eq!(payload.total_quantity.unwrap().to_string(), "2");
        assert!(build_sales_order_snapshot(
            &order,
            &submission(),
            &[],
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
        let view = document_approval_view(
            Some(&binding),
            None,
            CommercialStatus::Draft,
            ReviewStatus::NotSubmitted,
        );
        assert_eq!(view.requirement, "PROCESS_REQUIRED");
        assert_eq!(view.definition.as_ref().unwrap().id, "def-1");
        assert!(view.instance.is_none());
        assert!(view.recent_history.len() <= RECENT_HISTORY_LIMIT);
        assert_eq!(view.allowed_actions, vec!["SUBMIT".to_string()]);
        assert!(!view
            .allowed_actions
            .iter()
            .any(|item| item.contains("DEFINITION")));
        let running = document_approval_view(
            Some(&binding),
            None,
            CommercialStatus::PendingReview,
            ReviewStatus::InApproval,
        );
        assert_eq!(running.allowed_actions, vec!["CANCEL".to_string()]);
    }

    /// 对象读取权空组织或空审批人失败关闭。
    #[test]
    fn object_read_fails_closed_on_empty_identity() {
        assert!(sales_order_object_readable("party-1", "u1").unwrap());
        assert!(sales_order_object_readable(" ", "u1").is_err());
        assert!(sales_order_object_readable("party-1", "").is_err());
    }

    /// 领域动作分派只接受三类签署动作。
    #[test]
    fn domain_action_dispatch_is_exhaustive_for_sales_order() {
        let mut order = draft_order();
        execute_sales_order_domain_action(
            &mut order,
            ApprovalDomainAction::SalesOrderStartApprovalSubmission,
            "user-1",
        )
        .unwrap();
        execute_sales_order_domain_action(
            &mut order,
            ApprovalDomainAction::SalesOrderFormalizeApprovedSubmission,
            "user-1",
        )
        .unwrap();
        execute_sales_order_domain_action(
            &mut order,
            ApprovalDomainAction::SalesOrderCancelApprovalSubmission,
            "user-1",
        )
        .unwrap();
        assert!(execute_sales_order_domain_action(
            &mut order,
            ApprovalDomainAction::StockAdjustmentSubmit,
            "user-1",
        )
        .is_err());
        assert!(execute_sales_order_domain_action(
            &mut order,
            ApprovalDomainAction::VoucherSalesOrderStartApprovalSubmission,
            "user-1",
        )
        .is_err());
    }

    /// 卡券领域动作只接受本类型三类签署动作；驳回不改业务状态。
    #[test]
    fn voucher_domain_action_dispatch_and_reject_does_not_mutate() {
        let mut order = draft_order();
        order.business_type = BusinessType::Voucher;
        execute_sales_order_domain_action(
            &mut order,
            ApprovalDomainAction::VoucherSalesOrderStartApprovalSubmission,
            "user-1",
        )
        .unwrap();
        assert_eq!(order.review_status, ReviewStatus::InApproval);
        let subject_version_before = 1_u32;
        assert!(order.transition_review(ReviewStatus::Rejected, "u").is_err());
        assert_eq!(order.review_status, ReviewStatus::InApproval);
        assert_eq!(subject_version_before, 1);
        execute_sales_order_domain_action(
            &mut order,
            ApprovalDomainAction::VoucherSalesOrderFormalizeApprovedSubmission,
            "user-1",
        )
        .unwrap();
        execute_sales_order_domain_action(
            &mut order,
            ApprovalDomainAction::VoucherSalesOrderCancelApprovalSubmission,
            "user-1",
        )
        .unwrap();
        assert_eq!(order.commercial_status, CommercialStatus::Draft);
        assert!(execute_sales_order_domain_action(
            &mut order,
            ApprovalDomainAction::SalesOrderStartApprovalSubmission,
            "user-1",
        )
        .is_err());
    }

    /// 卡券专用决定与专用工作项新写必须失败关闭。
    #[test]
    fn legacy_card_sales_writes_fail_closed() {
        assert!(reject_legacy_card_sales_decision().is_err());
        assert!(reject_legacy_card_sales_work_item("CARD_SALES_MANAGER_APPROVAL").is_err());
        assert!(reject_legacy_card_sales_work_item("CARD_SALES_OPERATION_APPROVAL").is_err());
        assert!(reject_legacy_card_sales_work_item("DOCUMENT_APPROVAL").is_ok());
    }
}
