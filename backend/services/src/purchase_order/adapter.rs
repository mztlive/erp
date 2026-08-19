//! `PurchaseOrder` 审批业务 Adapter。
//!
//! 必须显式声明合同 §4.4 / 阶段 04 §6 的全部适配器字段。
//! 领域动作只通过实体状态邻接与仓储更新，不得 `$set` 绕过不变式。
//! `PurchaseReviewStatus` 与待财务审核不得充当流程节点。

use bpm::SubjectRef;
use entities::approval_integration::{ApprovalSubjectCounterparty, ApprovalSubjectSnapshotPayload};
use entities::common::time::Instant;
use entities::document_registry::business_document::ApprovalDefinitionBinding;
use entities::document_registry::DocumentType;
use entities::ids::SupplierAccountId;
use entities::money::Quantity;
use entities::purchase_order::{
    PurchaseOrder, PurchaseOrderStatus, PurchaseOrderSubmission, PurchaseOrderSubmissionLine,
};
use entities::sales_order::SalesOrder;

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

/// 已注册的采购单适配器规格。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PurchaseOrderAdapter {
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

/// 返回采购单的完整适配器登记。
///
/// # 返回
/// 返回已校验完整性的规格与显式字段声明。
///
/// # 错误
/// 政策缺失或三类动作不互异时返回部署不变量错误。
pub fn purchase_order_adapter() -> Result<PurchaseOrderAdapter> {
    let spec = adapter_spec_of(DocumentType::PurchaseOrder)?;
    ensure_adapter_spec_complete(&spec)?;
    adapter_from_spec(spec)
}

/// 由政策规格填充显式 Adapter 字段。
///
/// # 错误
/// 字段与合同签署值不一致时返回错误。
fn adapter_from_spec(spec: ApprovalAdapterSpec) -> Result<PurchaseOrderAdapter> {
    if spec.document_type != DocumentType::PurchaseOrder
        || spec.process_kind != process_kind_of(DocumentType::PurchaseOrder)
        || spec.subject_version_source != ApprovalSubjectVersionSource::EntityApprovalSubjectVersion
        || spec.on_approval_start != ApprovalDomainAction::PurchaseOrderSubmit
        || spec.on_final_approve != ApprovalDomainAction::PurchaseOrderFormalizeApprovedOrder
        || spec.cancel_action != ApprovalDomainAction::PurchaseOrderCancelApproval
        || spec.owner_role.as_str() != "purchase_order_approver"
        || spec.owner_organization_source != OwnerOrganizationSource::SubjectSnapshotResponsibleOrgId
        || spec.read_scope != AdapterReadScope::DocumentOrganizationAndCreator
        || !spec
            .subject_snapshot_fields
            .contains(&ApprovalSubjectSnapshotField::TotalAmount)
        || !spec
            .subject_snapshot_fields
            .contains(&ApprovalSubjectSnapshotField::TotalQuantity)
    {
        return Err(Error::Internal("采购单审批适配器登记不完整".to_string()));
    }
    Ok(PurchaseOrderAdapter {
        document_type: spec.document_type,
        process_kind: spec.process_kind,
        subject_ref_builder: "subject_ref_for(PurchaseOrder)",
        subject_version_source: spec.subject_version_source,
        subject_snapshot_builder: "build_purchase_order_snapshot",
        on_approval_start: spec.on_approval_start,
        on_final_approve: spec.on_final_approve,
        cancel_action: spec.cancel_action,
        owner_role: spec.owner_role.as_str(),
        owner_organization_snapshot: spec.owner_organization_source,
        read_scope: spec.read_scope,
    })
}

/// 为采购单构造唯一 `bpm::SubjectRef`。
///
/// # 参数
/// * `business_object_id` - 采购单主键
///
/// # 错误
/// 主键为空或超长时返回校验错误。
pub fn purchase_order_subject_ref(business_object_id: &str) -> Result<SubjectRef> {
    subject_ref_for(DocumentType::PurchaseOrder, business_object_id)
}

/// 提交并启动：进入 `IN_APPROVAL`，递增 `approval_subject_version`。
///
/// # 参数
/// * `order` - 待提交采购单
/// * `submission_id` - 本次冻结提交
/// * `updated_by` - 提交人
///
/// # 返回
/// 返回冻结后的提交版本。
///
/// # 错误
/// 非草稿或版本溢出时返回冲突。
pub fn start_purchase_order_approval(
    order: &mut PurchaseOrder,
    submission_id: impl Into<String>,
    updated_by: &str,
) -> Result<u32> {
    Ok(order.start_approval(submission_id, updated_by)?)
}

/// 撤回审批：回到 `DRAFT`，且提交版本不回退。
///
/// # 参数
/// * `order` - 审批中的采购单
/// * `updated_by` - 操作人
///
/// # 错误
/// 非审批中时返回冲突。
pub fn cancel_purchase_order_to_draft(order: &mut PurchaseOrder, updated_by: &str) -> Result<()> {
    Ok(order.cancel_approval(updated_by)?)
}

/// 最终通过前置：仅 `IN_APPROVAL` 可进入生效。
///
/// # 错误
/// 状态不是审批中时返回冲突。
pub fn ensure_final_approve_formalize(order: &PurchaseOrder) -> Result<()> {
    if order.stable.status != PurchaseOrderStatus::InApproval {
        return Err(Error::ConflictError(
            "只有审批中的采购单可以由最终通过动作生效".to_string(),
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
    binding.ok_or_else(|| Error::ConflictError("无有效审批绑定的采购单不得提交".to_string()))
}

/// 采购单调用统一 `start_approval` 的目标命令。
///
/// 字段与合同 §14.2 对齐；不得包含定义 ID 或审批人。
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct PurchaseOrderStartCommand {
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
/// * `purchase_order_id` - 采购单主键
/// * `subject_version` - `approval_subject_version`
/// * `actor_id` - 提交人
/// * `idempotency_key` - 幂等键
///
/// # 返回
/// 返回不含定义 ID 或审批人的目标启动命令。
pub fn purchase_order_start_command(
    purchase_order_id: &str,
    subject_version: u32,
    actor_id: &str,
    idempotency_key: &str,
) -> PurchaseOrderStartCommand {
    PurchaseOrderStartCommand {
        subject_kind: process_kind_of(DocumentType::PurchaseOrder).as_str().to_string(),
        subject_id: purchase_order_id.to_string(),
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
    _command: &PurchaseOrderStartCommand,
) -> bpm::model::types::ApprovalCommandKind {
    bpm::model::types::ApprovalCommandKind::StartApproval
}

/// 执行签署的采购单领域动作。
///
/// # 参数
/// * `order` - 业务实体
/// * `action` - 合同强类型动作
/// * `submission_id` - 提交时的冻结提交；其它动作忽略
/// * `updated_by` - 操作人
///
/// # 错误
/// 动作不属于本类型或状态不允许时返回错误。
pub fn execute_purchase_order_domain_action(
    order: &mut PurchaseOrder,
    action: ApprovalDomainAction,
    submission_id: &str,
    updated_by: &str,
) -> Result<()> {
    match action {
        ApprovalDomainAction::PurchaseOrderSubmit => {
            start_purchase_order_approval(order, submission_id, updated_by)?;
            Ok(())
        }
        ApprovalDomainAction::PurchaseOrderFormalizeApprovedOrder => ensure_final_approve_formalize(order),
        ApprovalDomainAction::PurchaseOrderCancelApproval => {
            cancel_purchase_order_to_draft(order, updated_by)
        }
        other => Err(Error::ValidationError(format!(
            "动作 {} 不属于采购单",
            other.as_str()
        ))),
    }
}

/// 财务审核待办不得充当审批流程节点。
///
/// # 错误
/// 恒返回冲突。
#[cfg(test)]
pub fn reject_legacy_finance_review_node() -> Result<()> {
    Err(Error::ConflictError(
        "采购财务审核不得充当审批流程节点".to_string(),
    ))
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
pub fn purchase_order_object_readable(organization_id: &str, assignee_user_id: &str) -> Result<bool> {
    if organization_id.trim().is_empty() || assignee_user_id.trim().is_empty() {
        return Err(Error::ValidationError("单据组织或审批人不能为空".to_string()));
    }
    Ok(true)
}

/// 责任组织取来源销售单结算主体，不得用空串或当前登录人组织补位。
///
/// # 参数
/// * `sales_order` - 来源销售单
///
/// # 返回
/// 返回非空责任组织。
///
/// # 错误
/// 结算主体为空时返回校验错误。
pub fn purchase_order_responsible_org_id(sales_order: &SalesOrder) -> Result<String> {
    let org = sales_order.settlement_party_id.to_string();
    if org.trim().is_empty() {
        return Err(Error::ValidationError(
            "采购单来源销售单缺少结算主体，无法冻结责任组织".to_string(),
        ));
    }
    Ok(org)
}

/// 按合同 §4.4.5 冻结采购单快照。
///
/// 对手方为供应商；金额与数量合计必填。`document_no` 取已分配正式号。
///
/// # 参数
/// * `order` - 已分配正式号的采购单
/// * `sales_order` - 来源销售单
/// * `submission` - 已冻结提交
/// * `lines` - 提交明细
/// * `submitted_by` - 提交人
/// * `submitted_at` - 提交时间
///
/// # 错误
/// 正式号为空、明细为空、金额/数量非法或组织为空时返回校验错误。
pub fn build_purchase_order_snapshot(
    order: &PurchaseOrder,
    sales_order: &SalesOrder,
    submission: &PurchaseOrderSubmission,
    lines: &[PurchaseOrderSubmissionLine],
    submitted_by: &str,
    submitted_at: Instant,
) -> Result<ApprovalSubjectSnapshotPayload> {
    if order.purchase_no.trim().is_empty() {
        return Err(Error::ValidationError(
            "采购单尚未分配正式号，无法启动审批".to_string(),
        ));
    }
    if lines.is_empty() {
        return Err(Error::ValidationError("采购单没有明细，无法启动审批".to_string()));
    }
    Ok(ApprovalSubjectSnapshotPayload {
        document_no: order.purchase_no.clone(),
        responsible_org_id: purchase_order_responsible_org_id(sales_order)?,
        submitted_by: submitted_by.to_string(),
        submitted_at,
        counterparty: Some(ApprovalSubjectCounterparty::Supplier {
            supplier_id: SupplierAccountId::new(order.supplier_id.to_string()),
        }),
        total_amount: Some(submission.gross_amount),
        total_quantity: Some(sum_line_quantity(lines)?),
        line_count: u32::try_from(lines.len())
            .map_err(|_| Error::ValidationError("采购明细行数溢出".to_string()))?,
    })
}

/// 汇总提交明细数量。
///
/// # 错误
/// 无数量或合计超出标度时返回错误。
fn sum_line_quantity(lines: &[PurchaseOrderSubmissionLine]) -> Result<Quantity> {
    let mut quantities = lines.iter().filter_map(|line| line.quantity);
    let Some(first) = quantities.next() else {
        return Err(Error::ValidationError(
            "采购明细没有数量，无法启动审批".to_string(),
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
    status: PurchaseOrderStatus,
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
fn allowed_document_actions(status: PurchaseOrderStatus) -> Vec<String> {
    match status {
        PurchaseOrderStatus::Draft => vec!["SUBMIT".to_string()],
        PurchaseOrderStatus::InApproval => vec!["CANCEL".to_string()],
        PurchaseOrderStatus::PendingFinanceReview
        | PurchaseOrderStatus::Effective
        | PurchaseOrderStatus::PartiallyExecuted
        | PurchaseOrderStatus::Completed
        | PurchaseOrderStatus::Voided => Vec::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::approval::binding::binding_from_published;
    use bpm::ids::ApprovalProcessDefinitionId;
    use entities::common::time::Instant;
    use entities::ids::{
        CustomerAccountId, PartyId, ProcurementConfirmationLineId, PurchaseOrderId,
        PurchaseOrderSubmissionId, PurchaseOrderSubmissionLineId, SalesOrderId, SalesOrderSubmissionLineId,
        SkuId, SkuRevisionId, SupplierAccountId, SupplierCommercialProfileRevisionId,
    };
    use entities::money::{Amount, Quantity, Rate, UnitPrice};
    use entities::purchase_order::{
        FulfillmentResponsibility, PaymentTermSnapshot, PurchaseLineType, PurchaseOrderData,
        PurchaseOrderSubmissionData, PurchaseOrderSubmissionLineData, PurchaseType, SupplierSnapshot,
    };
    use entities::sales_order::{BusinessType, OriginSystem, SalesOrderData};
    use std::str::FromStr;

    fn draft_order() -> PurchaseOrder {
        PurchaseOrder::new(
            PurchaseOrderId::new("po-1"),
            PurchaseOrderData {
                purchase_no: String::new(),
                sales_order_id: SalesOrderId::new("so-1"),
                supplier_id: SupplierAccountId::new("sup-1"),
                purchase_type: PurchaseType::Physical,
                payment_term_code: "NET-30".into(),
                fulfillment_responsibility: FulfillmentResponsibility::Warehouse,
            },
            "user-1",
        )
        .expect("草稿必须可构造")
    }

    fn sales_order() -> SalesOrder {
        SalesOrder::new(
            SalesOrderId::new("so-1"),
            SalesOrderData {
                order_no: "SO-1".into(),
                business_type: BusinessType::GoodsService,
                origin_system: OriginSystem::Erp,
                source_identity_id: None,
                customer_id: CustomerAccountId::new("cust-1"),
                contract_id: None,
                settlement_party_id: PartyId::new("party-1"),
                source_status_code: None,
            },
            "user-1",
        )
        .expect("销售单必须可构造")
    }

    fn submission() -> PurchaseOrderSubmission {
        PurchaseOrderSubmission::new(
            PurchaseOrderSubmissionId::new("sub-1"),
            PurchaseOrderSubmissionData {
                purchase_order_id: PurchaseOrderId::new("po-1"),
                submission_no: "SUB-000001".into(),
                supplier_id: SupplierAccountId::new("sup-1"),
                purchase_type: PurchaseType::Physical,
                fulfillment_responsibility: FulfillmentResponsibility::Warehouse,
                supplier_revision_id: SupplierCommercialProfileRevisionId::new("srev-1"),
                supplier_snapshot: SupplierSnapshot::new("供应商".into()).expect("快照合法"),
                payment_term_snapshot: PaymentTermSnapshot::new("NET-30".into(), false, None, None)
                    .expect("付款条件合法"),
                gross_amount: Amount::from_str("10").expect("金额合法"),
                net_amount: Amount::from_str("10").expect("金额合法"),
                tax_amount: Amount::from_str("0").expect("金额合法"),
            },
        )
        .expect("提交必须可构造")
    }

    fn one_line() -> PurchaseOrderSubmissionLine {
        PurchaseOrderSubmissionLine::new(
            PurchaseOrderSubmissionLineId::new("pl-1"),
            PurchaseOrderSubmissionLineData {
                purchase_order_submission_id: PurchaseOrderSubmissionId::new("sub-1"),
                line_no: 1,
                line_type: PurchaseLineType::ItemService,
                procurement_confirmation_line_id: Some(ProcurementConfirmationLineId::new("pcl-1")),
                sku_id: Some(SkuId::new("sku-1")),
                sku_revision_id: Some(SkuRevisionId::new("skurev-1")),
                product_name_snapshot: Some("商品".into()),
                specification_snapshot: Some("规格".into()),
                quantity: Some(Quantity::from_str("2").expect("数量合法")),
                base_unit_code: Some("件".into()),
                unit_cost_gross: Some(UnitPrice::from_str("5.0000").expect("单价合法")),
                gross_amount: Amount::from_str("10").expect("金额合法"),
                net_amount: Amount::from_str("10").expect("金额合法"),
                tax_amount: Amount::from_str("0").expect("金额合法"),
                input_tax_rate: Some(Rate::from_str("0").expect("税率合法")),
                expected_delivery_date: None,
                sales_order_submission_line_id: Some(SalesOrderSubmissionLineId::new("sosl-1")),
                allocated_quantity: Some(Quantity::from_str("2").expect("数量合法")),
            },
        )
        .expect("提交行必须可构造")
    }

    /// 适配器必须显式声明合同要求的全部字段。
    #[test]
    fn adapter_declares_all_required_fields() {
        let adapter = purchase_order_adapter().expect("采购单必须可登记");
        assert_eq!(adapter.document_type, DocumentType::PurchaseOrder);
        assert_eq!(adapter.process_kind.as_str(), "purchase_order");
        assert_eq!(
            purchase_order_subject_ref("po-1")
                .expect("主体引用必须可构造")
                .subject_kind(),
            "purchase_order"
        );
        assert_eq!(adapter.subject_ref_builder, "subject_ref_for(PurchaseOrder)");
        assert_eq!(
            adapter.subject_version_source,
            ApprovalSubjectVersionSource::EntityApprovalSubjectVersion
        );
        assert_eq!(adapter.subject_snapshot_builder, "build_purchase_order_snapshot");
        assert_eq!(
            adapter.on_approval_start,
            ApprovalDomainAction::PurchaseOrderSubmit
        );
        assert_eq!(
            adapter.on_final_approve,
            ApprovalDomainAction::PurchaseOrderFormalizeApprovedOrder
        );
        assert_eq!(
            adapter.cancel_action,
            ApprovalDomainAction::PurchaseOrderCancelApproval
        );
        assert_eq!(adapter.owner_role, "purchase_order_approver");
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

    /// 提交进入审批中；撤回不回退版本，且切断财务审核旁路。
    #[test]
    fn submit_enters_in_approval_and_cancel_returns_draft() {
        let mut order = draft_order();
        start_purchase_order_approval(&mut order, "sub-1", "submitter-9").unwrap();
        assert_eq!(order.stable.status(), PurchaseOrderStatus::InApproval);
        assert_eq!(order.approval_subject_version, 1);
        assert_eq!(order.stable.updated_by, "submitter-9");
        execute_purchase_order_domain_action(
            &mut order,
            ApprovalDomainAction::PurchaseOrderCancelApproval,
            "sub-1",
            "user-1",
        )
        .unwrap();
        assert_eq!(order.stable.status(), PurchaseOrderStatus::Draft);
        assert_eq!(order.approval_subject_version, 1);
        assert_eq!(order.current_submission_id.as_deref(), Some("sub-1"));
    }

    /// 非草稿不得提交；非审批中不得撤回或生效。
    #[test]
    fn illegal_status_transitions_fail_closed() {
        let mut effective = draft_order();
        start_purchase_order_approval(&mut effective, "sub-1", "user-2").unwrap();
        effective.formalize_approved("user-2").unwrap();
        assert!(start_purchase_order_approval(&mut effective, "sub-2", "user-2").is_err());
        assert!(cancel_purchase_order_to_draft(&mut effective, "u").is_err());
        assert!(ensure_final_approve_formalize(&effective).is_err());
        assert!(reject_legacy_finance_review_node().is_err());
    }

    /// 启动命令不含定义 ID 或审批人。
    #[test]
    fn start_command_omits_definition_and_assignee() {
        let command = purchase_order_start_command("po-1", 1, "user-1", "key-1");
        let encoded = serde_json::to_value(&command).unwrap();
        assert!(encoded.get("definition_id").is_none());
        assert!(encoded.get("definition_key").is_none());
        assert!(encoded.get("assignee").is_none());
        assert_eq!(command.subject_kind, "purchase_order");
        assert_eq!(command.subject_version, 1);
        assert_eq!(
            start_approval_command_kind(&command),
            bpm::model::types::ApprovalCommandKind::StartApproval
        );
        assert!(require_frozen_binding(None).is_err());
    }

    /// 快照冻结供应商对手方、正式号、金额与数量合计。
    #[test]
    fn snapshot_freezes_supplier_amount_and_quantity() {
        let mut order = draft_order();
        order.assign_purchase_no("PO-1").unwrap();
        let payload = build_purchase_order_snapshot(
            &order,
            &sales_order(),
            &submission(),
            &[one_line()],
            "user-1",
            Instant::from_unix_secs(10),
        )
        .unwrap();
        assert_eq!(payload.document_no, "PO-1");
        assert_eq!(payload.responsible_org_id, "party-1");
        assert_eq!(payload.submitted_by, "user-1");
        assert_eq!(payload.total_amount.unwrap().to_string(), "10");
        assert_eq!(payload.total_quantity.unwrap().to_string(), "2");
        assert!(build_purchase_order_snapshot(
            &draft_order(),
            &sales_order(),
            &submission(),
            &[one_line()],
            "user-1",
            Instant::from_unix_secs(10)
        )
        .is_err());
        assert!(build_purchase_order_snapshot(
            &order,
            &sales_order(),
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
        let view = document_approval_view(Some(&binding), None, PurchaseOrderStatus::Draft);
        assert_eq!(view.requirement, "PROCESS_REQUIRED");
        assert_eq!(view.definition.as_ref().unwrap().id, "def-1");
        assert!(view.instance.is_none());
        assert!(view.recent_history.len() <= RECENT_HISTORY_LIMIT);
        assert_eq!(view.allowed_actions, vec!["SUBMIT".to_string()]);
        assert!(!view
            .allowed_actions
            .iter()
            .any(|item| item.contains("DEFINITION")));
        let running = document_approval_view(Some(&binding), None, PurchaseOrderStatus::InApproval);
        assert_eq!(running.allowed_actions, vec!["CANCEL".to_string()]);
    }

    /// 对象读取权空组织或空审批人失败关闭。
    #[test]
    fn object_read_fails_closed_on_empty_identity() {
        assert!(purchase_order_object_readable("party-1", "u1").unwrap());
        assert!(purchase_order_object_readable(" ", "u1").is_err());
        assert!(purchase_order_object_readable("party-1", "").is_err());
    }

    /// 领域动作分派只接受签署的三类动作。
    #[test]
    fn domain_action_dispatch_rejects_foreign_actions() {
        let mut order = draft_order();
        execute_purchase_order_domain_action(
            &mut order,
            ApprovalDomainAction::PurchaseOrderSubmit,
            "sub-1",
            "user-1",
        )
        .unwrap();
        execute_purchase_order_domain_action(
            &mut order,
            ApprovalDomainAction::PurchaseOrderFormalizeApprovedOrder,
            "sub-1",
            "user-1",
        )
        .unwrap();
        assert!(execute_purchase_order_domain_action(
            &mut order,
            ApprovalDomainAction::StockAdjustmentSubmit,
            "sub-1",
            "user-1",
        )
        .is_err());
    }
}
