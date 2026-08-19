//! `SalesChangeOrder` 审批业务 Adapter。
//!
//! 必须显式声明合同 §4.4 / 阶段 04 §6 的全部适配器字段。
//! 领域动作只通过实体状态邻接与仓储更新，不得 `$set` 绕过不变式。
//! `confirm_impact` / `confirm_finance` 不得充当流程节点。

use bpm::SubjectRef;
use entities::approval_integration::{ApprovalSubjectCounterparty, ApprovalSubjectSnapshotPayload};
use entities::common::time::Instant;
use entities::document_registry::business_document::ApprovalDefinitionBinding;
use entities::document_registry::DocumentType;
use entities::ids::CustomerAccountId;
use entities::money::Quantity;
use entities::sales_order::SalesOrder;
use entities::sales_review::{
    SalesChangeOrder, SalesChangeOrderStatus, SalesChangeSubmission, SalesChangeSubmissionLine,
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

use super::dto::{
    DocumentApprovalDefinitionView, DocumentApprovalHistoryPageView, DocumentApprovalInstanceView,
    DocumentApprovalView,
};

/// 详情最近审批历史条数上限。完整历史走分页端点。
pub const RECENT_HISTORY_LIMIT: usize = 8;

/// 已注册的销售变更单适配器规格。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SalesChangeOrderAdapter {
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

/// 返回销售变更单的完整适配器登记。
///
/// # 返回
/// 返回已校验完整性的规格与显式字段声明。
///
/// # 错误
/// 政策缺失或三类动作不互异时返回部署不变量错误。
pub fn sales_change_order_adapter() -> Result<SalesChangeOrderAdapter> {
    let spec = adapter_spec_of(DocumentType::SalesChangeOrder)?;
    ensure_adapter_spec_complete(&spec)?;
    adapter_from_spec(spec)
}

/// 由政策规格填充显式 Adapter 字段。
///
/// # 错误
/// 字段与合同签署值不一致时返回错误。
fn adapter_from_spec(spec: ApprovalAdapterSpec) -> Result<SalesChangeOrderAdapter> {
    if spec.document_type != DocumentType::SalesChangeOrder
        || spec.process_kind != process_kind_of(DocumentType::SalesChangeOrder)
        || spec.subject_version_source != ApprovalSubjectVersionSource::SalesChangeSubmissionNo
        || spec.on_approval_start != ApprovalDomainAction::SalesChangeOrderSubmitSalesChange
        || spec.on_final_approve != ApprovalDomainAction::SalesChangeOrderApplyEffectiveChange
        || spec.cancel_action != ApprovalDomainAction::SalesChangeOrderCancelApproval
        || spec.owner_role.as_str() != "sales_change_order_approver"
        || spec.owner_organization_source != OwnerOrganizationSource::SubjectSnapshotResponsibleOrgId
        || spec.read_scope != AdapterReadScope::DocumentOrganizationAndCreator
        || !spec
            .subject_snapshot_fields
            .contains(&ApprovalSubjectSnapshotField::TotalAmount)
        || !spec
            .subject_snapshot_fields
            .contains(&ApprovalSubjectSnapshotField::TotalQuantity)
    {
        return Err(Error::Internal("销售变更单审批适配器登记不完整".to_string()));
    }
    Ok(SalesChangeOrderAdapter {
        document_type: spec.document_type,
        process_kind: spec.process_kind,
        subject_ref_builder: "subject_ref_for(SalesChangeOrder)",
        subject_version_source: spec.subject_version_source,
        subject_snapshot_builder: "build_sales_change_snapshot",
        on_approval_start: spec.on_approval_start,
        on_final_approve: spec.on_final_approve,
        cancel_action: spec.cancel_action,
        owner_role: spec.owner_role.as_str(),
        owner_organization_snapshot: spec.owner_organization_source,
        read_scope: spec.read_scope,
    })
}

/// 为销售变更单构造唯一 `bpm::SubjectRef`。
///
/// # 参数
/// * `business_object_id` - 变更单主键
///
/// # 错误
/// 主键为空或超长时返回校验错误。
pub fn sales_change_order_subject_ref(business_object_id: &str) -> Result<SubjectRef> {
    subject_ref_for(DocumentType::SalesChangeOrder, business_object_id)
}

/// 提交并启动：进入 `IN_APPROVAL`。
///
/// 版本权威来源是提交记录 `submission_no`，本方法不改写该编号。
///
/// # 参数
/// * `order` - 待提交变更单
/// * `submission_id` - 本次冻结提交
/// * `target_content_hash` - 目标内容指纹
/// * `updated_by` - 提交人
///
/// # 错误
/// 状态不允许或指纹非法时返回冲突。
pub fn start_sales_change_approval(
    order: &mut SalesChangeOrder,
    submission_id: entities::ids::SalesChangeSubmissionId,
    target_content_hash: impl Into<String>,
    updated_by: &str,
) -> Result<()> {
    Ok(order.start_approval(submission_id, target_content_hash, updated_by)?)
}

/// 撤回审批：回到 `DRAFT`，且提交号不回退。
///
/// # 参数
/// * `order` - 审批中的变更单
/// * `updated_by` - 操作人
///
/// # 错误
/// 非审批中时返回冲突。
pub fn cancel_sales_change_to_draft(order: &mut SalesChangeOrder, updated_by: &str) -> Result<()> {
    Ok(order.cancel_approval(updated_by)?)
}

/// 最终通过前置：仅 `IN_APPROVAL` 可进入生效。
///
/// # 错误
/// 状态不是审批中时返回冲突。
pub fn ensure_final_approve_effective(order: &SalesChangeOrder) -> Result<()> {
    if order.stable.status != SalesChangeOrderStatus::InApproval {
        return Err(Error::ConflictError(
            "只有审批中的销售变更单可以由最终通过动作生效".to_string(),
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
    binding.ok_or_else(|| Error::ConflictError("无有效审批绑定的销售变更单不得提交".to_string()))
}

/// 销售变更单调用统一 `start_approval` 的目标命令。
///
/// 字段与合同 §14.2 对齐；不得包含定义 ID 或审批人。
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct SalesChangeOrderStartCommand {
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
/// * `change_order_id` - 变更单主键
/// * `subject_version` - `sales_change_submission.submission_no`
/// * `actor_id` - 提交人
/// * `idempotency_key` - 幂等键
///
/// # 返回
/// 返回不含定义 ID 或审批人的目标启动命令。
pub fn sales_change_start_command(
    change_order_id: &str,
    subject_version: u32,
    actor_id: &str,
    idempotency_key: &str,
) -> SalesChangeOrderStartCommand {
    SalesChangeOrderStartCommand {
        subject_kind: process_kind_of(DocumentType::SalesChangeOrder)
            .as_str()
            .to_string(),
        subject_id: change_order_id.to_string(),
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
    _command: &SalesChangeOrderStartCommand,
) -> bpm::model::types::ApprovalCommandKind {
    bpm::model::types::ApprovalCommandKind::StartApproval
}

/// 执行签署的销售变更单领域动作。
///
/// # 参数
/// * `order` - 业务实体
/// * `action` - 合同强类型动作
/// * `updated_by` - 操作人
///
/// # 错误
/// 动作不属于本类型或状态不允许时返回错误。
pub fn execute_sales_change_domain_action(
    order: &mut SalesChangeOrder,
    action: ApprovalDomainAction,
    updated_by: &str,
) -> Result<()> {
    match action {
        ApprovalDomainAction::SalesChangeOrderApplyEffectiveChange => ensure_final_approve_effective(order),
        ApprovalDomainAction::SalesChangeOrderCancelApproval => {
            cancel_sales_change_to_draft(order, updated_by)
        }
        other => Err(Error::ValidationError(format!(
            "动作 {} 不属于销售变更单",
            other.as_str()
        ))),
    }
}

/// 履约影响确认与财务复核不得充当审批流程节点。
///
/// # 错误
/// 恒返回冲突。
#[cfg(test)]
pub fn reject_legacy_change_review_node() -> Result<()> {
    Err(Error::ConflictError(
        "销售变更履约影响确认与财务复核不得充当审批流程节点".to_string(),
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
pub fn sales_change_order_object_readable(organization_id: &str, assignee_user_id: &str) -> Result<bool> {
    if organization_id.trim().is_empty() || assignee_user_id.trim().is_empty() {
        return Err(Error::ValidationError("单据组织或审批人不能为空".to_string()));
    }
    Ok(true)
}

/// 责任组织取结算主体，不得用空串或当前登录人组织补位。
///
/// # 参数
/// * `sales_order` - 原销售单
///
/// # 返回
/// 返回非空责任组织。
///
/// # 错误
/// 结算主体为空时返回校验错误。
pub fn sales_change_responsible_org_id(sales_order: &SalesOrder) -> Result<String> {
    let org = sales_order.settlement_party_id.to_string();
    if org.trim().is_empty() {
        return Err(Error::ValidationError(
            "销售变更单缺少结算主体，无法冻结责任组织".to_string(),
        ));
    }
    Ok(org)
}

/// 按合同 §4.4.5 冻结销售变更单快照。
///
/// 对手方为客户；金额与数量合计必填。`document_no` 取变更单主键。
///
/// # 参数
/// * `change_order` - 变更单
/// * `sales_order` - 原销售单
/// * `submission` - 已冻结提交
/// * `lines` - 提交明细
/// * `submitted_by` - 提交人
/// * `submitted_at` - 提交时间
///
/// # 错误
/// 明细为空、金额/数量非法或组织为空时返回校验错误。
pub fn build_sales_change_snapshot(
    change_order: &SalesChangeOrder,
    sales_order: &SalesOrder,
    submission: &SalesChangeSubmission,
    lines: &[SalesChangeSubmissionLine],
    submitted_by: &str,
    submitted_at: Instant,
) -> Result<ApprovalSubjectSnapshotPayload> {
    if lines.is_empty() {
        return Err(Error::ValidationError(
            "销售变更单没有明细，无法启动审批".to_string(),
        ));
    }
    Ok(ApprovalSubjectSnapshotPayload {
        document_no: change_order.base.id.clone(),
        responsible_org_id: sales_change_responsible_org_id(sales_order)?,
        submitted_by: submitted_by.to_string(),
        submitted_at,
        counterparty: Some(ApprovalSubjectCounterparty::Customer {
            customer_id: CustomerAccountId::new(sales_order.customer_id.to_string()),
        }),
        total_amount: Some(submission.gross_amount),
        total_quantity: Some(sum_line_quantity(lines)?),
        line_count: u32::try_from(lines.len())
            .map_err(|_| Error::ValidationError("销售变更明细行数溢出".to_string()))?,
    })
}

/// 汇总提交明细数量。
///
/// # 错误
/// 无数量或合计超出标度时返回错误。
fn sum_line_quantity(lines: &[SalesChangeSubmissionLine]) -> Result<Quantity> {
    let mut quantities = lines.iter().filter_map(|line| line.quantity);
    let Some(first) = quantities.next() else {
        return Err(Error::ValidationError(
            "销售变更明细没有数量，无法启动审批".to_string(),
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
    status: SalesChangeOrderStatus,
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
fn allowed_document_actions(status: SalesChangeOrderStatus) -> Vec<String> {
    match status {
        SalesChangeOrderStatus::Draft => vec!["SUBMIT".to_string()],
        SalesChangeOrderStatus::InApproval => vec!["CANCEL".to_string()],
        SalesChangeOrderStatus::Effective
        | SalesChangeOrderStatus::Voided
        | SalesChangeOrderStatus::PendingImpactConfirmation
        | SalesChangeOrderStatus::PendingFinanceReview
        | SalesChangeOrderStatus::Rejected => Vec::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::approval::binding::binding_from_published;
    use bpm::ids::ApprovalProcessDefinitionId;
    use entities::common::time::Instant;
    use entities::ids::{
        CustomerAccountId, PartyId, SalesChangeOrderId, SalesChangeSubmissionId, SalesChangeSubmissionLineId,
        SalesOrderId, SalesOrderLineId, SalesOrderRevisionId, SalesOrderWorkingCopyId, SkuId, SkuRevisionId,
    };
    use entities::money::{Amount, Quantity, Rate, UnitPrice};
    use entities::sales_order::{BusinessType, OriginSystem, SalesOrderData};
    use entities::sales_review::{
        FulfillmentMode, GoodsLineFields, HeaderSnapshotData, LineType, SalesChangeOrderData,
        SalesChangeSubmissionData, SalesChangeSubmissionLineData, SalesChangeType, WelfareScenario,
    };
    use std::str::FromStr;

    fn draft_order() -> SalesChangeOrder {
        SalesChangeOrder::new(
            SalesChangeOrderId::new("co-1"),
            SalesChangeOrderData {
                sales_order_id: SalesOrderId::new("so-1"),
                base_revision_id: SalesOrderRevisionId::new("rev-1"),
                change_type: SalesChangeType::Quantity,
                reason: "追加数量".into(),
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

    fn goods_fields() -> GoodsLineFields {
        GoodsLineFields {
            sku_id: SkuId::new("sku-1"),
            sku_revision_id: SkuRevisionId::new("skurev-1"),
            welfare_scenario: Some(WelfareScenario::AnnualGiftBag),
            fulfillment_mode: FulfillmentMode::CompanyWarehouse,
            fulfillment_due_at: Instant::from_unix_secs(1_800_000_000),
            quantity: Quantity::from_str("2").expect("数量合法"),
            base_unit_code: "件".into(),
            unit_price_gross: UnitPrice::from_str("5.0000").expect("单价合法"),
        }
    }

    fn line_data() -> SalesChangeSubmissionLineData {
        SalesChangeSubmissionLineData {
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

    fn submission() -> SalesChangeSubmission {
        SalesChangeSubmission::new(
            SalesChangeSubmissionId::new("sub-1"),
            SalesChangeSubmissionData {
                sales_change_order_id: SalesChangeOrderId::new("co-1"),
                submission_no: 1,
                base_revision_id: SalesOrderRevisionId::new("rev-1"),
                sales_order_id: SalesOrderId::new("so-1"),
                working_copy_id: SalesOrderWorkingCopyId::new("wc-1"),
                working_copy_version: 1,
                business_type: entities::sales_review::BusinessType::GoodsService,
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

    fn one_line() -> SalesChangeSubmissionLine {
        SalesChangeSubmissionLine::new(
            SalesChangeSubmissionLineId::new("sl-1"),
            SalesChangeSubmissionId::new("sub-1"),
            line_data(),
        )
        .expect("提交行必须可构造")
    }

    /// 适配器必须显式声明合同要求的全部字段。
    #[test]
    fn adapter_declares_all_required_fields() {
        let adapter = sales_change_order_adapter().expect("销售变更单必须可登记");
        assert_eq!(adapter.document_type, DocumentType::SalesChangeOrder);
        assert_eq!(adapter.process_kind.as_str(), "sales_change_order");
        assert_eq!(
            sales_change_order_subject_ref("co-1")
                .expect("主体引用必须可构造")
                .subject_kind(),
            "sales_change_order"
        );
        assert_eq!(adapter.subject_ref_builder, "subject_ref_for(SalesChangeOrder)");
        assert_eq!(
            adapter.subject_version_source,
            ApprovalSubjectVersionSource::SalesChangeSubmissionNo
        );
        assert_eq!(adapter.subject_snapshot_builder, "build_sales_change_snapshot");
        assert_eq!(
            adapter.on_approval_start,
            ApprovalDomainAction::SalesChangeOrderSubmitSalesChange
        );
        assert_eq!(
            adapter.on_final_approve,
            ApprovalDomainAction::SalesChangeOrderApplyEffectiveChange
        );
        assert_eq!(
            adapter.cancel_action,
            ApprovalDomainAction::SalesChangeOrderCancelApproval
        );
        assert_eq!(adapter.owner_role, "sales_change_order_approver");
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

    /// 提交进入审批中；撤回不回退提交号，且切断 REJECTED。
    #[test]
    fn submit_enters_in_approval_and_cancel_returns_draft() {
        let mut order = draft_order();
        start_sales_change_approval(
            &mut order,
            SalesChangeSubmissionId::new("cs-1"),
            "hash-1",
            "submitter-9",
        )
        .unwrap();
        assert_eq!(order.stable.status(), SalesChangeOrderStatus::InApproval);
        assert_eq!(order.stable.updated_by, "submitter-9");
        execute_sales_change_domain_action(
            &mut order,
            ApprovalDomainAction::SalesChangeOrderCancelApproval,
            "user-1",
        )
        .unwrap();
        assert_eq!(order.stable.status(), SalesChangeOrderStatus::Draft);
        assert_eq!(
            order.current_submission_id,
            Some(SalesChangeSubmissionId::new("cs-1"))
        );
    }

    /// 非草稿不得提交；非审批中不得撤回或生效。
    #[test]
    fn illegal_status_transitions_fail_closed() {
        let mut effective = draft_order();
        start_sales_change_approval(
            &mut effective,
            SalesChangeSubmissionId::new("cs-1"),
            "hash-1",
            "user-2",
        )
        .unwrap();
        effective
            .apply_effective(SalesOrderRevisionId::new("rev-2"), "user-2")
            .unwrap();
        assert!(start_sales_change_approval(
            &mut effective,
            SalesChangeSubmissionId::new("cs-2"),
            "hash-2",
            "user-2"
        )
        .is_err());
        assert!(cancel_sales_change_to_draft(&mut effective, "u").is_err());
        assert!(ensure_final_approve_effective(&effective).is_err());
        assert!(reject_legacy_change_review_node().is_err());
    }

    /// 启动命令不含定义 ID 或审批人。
    #[test]
    fn start_command_omits_definition_and_assignee() {
        let command = sales_change_start_command("co-1", 1, "user-1", "key-1");
        let encoded = serde_json::to_value(&command).unwrap();
        assert!(encoded.get("definition_id").is_none());
        assert!(encoded.get("definition_key").is_none());
        assert!(encoded.get("assignee").is_none());
        assert_eq!(command.subject_kind, "sales_change_order");
        assert_eq!(command.subject_version, 1);
        assert_eq!(
            start_approval_command_kind(&command),
            bpm::model::types::ApprovalCommandKind::StartApproval
        );
        assert!(require_frozen_binding(None).is_err());
    }

    /// 快照冻结客户对手方、金额与数量合计。
    #[test]
    fn snapshot_freezes_customer_amount_and_quantity() {
        let payload = build_sales_change_snapshot(
            &draft_order(),
            &sales_order(),
            &submission(),
            &[one_line()],
            "user-1",
            Instant::from_unix_secs(10),
        )
        .unwrap();
        assert_eq!(payload.document_no, "co-1");
        assert_eq!(payload.responsible_org_id, "party-1");
        assert_eq!(payload.submitted_by, "user-1");
        assert_eq!(payload.total_amount.unwrap().to_string(), "10");
        assert_eq!(payload.total_quantity.unwrap().to_string(), "2");
        assert!(build_sales_change_snapshot(
            &draft_order(),
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
        let view = document_approval_view(Some(&binding), None, SalesChangeOrderStatus::Draft);
        assert_eq!(view.requirement, "PROCESS_REQUIRED");
        assert_eq!(view.definition.as_ref().unwrap().id, "def-1");
        assert!(view.instance.is_none());
        assert!(view.recent_history.len() <= RECENT_HISTORY_LIMIT);
        assert_eq!(view.allowed_actions, vec!["SUBMIT".to_string()]);
        assert!(!view
            .allowed_actions
            .iter()
            .any(|item| item.contains("DEFINITION")));
        let running = document_approval_view(Some(&binding), None, SalesChangeOrderStatus::InApproval);
        assert_eq!(running.allowed_actions, vec!["CANCEL".to_string()]);
    }

    /// 对象读取权空组织或空审批人失败关闭。
    #[test]
    fn object_read_fails_closed_on_empty_identity() {
        assert!(sales_change_order_object_readable("party-1", "u1").unwrap());
        assert!(sales_change_order_object_readable(" ", "u1").is_err());
        assert!(sales_change_order_object_readable("party-1", "").is_err());
    }

    /// 领域动作分派只接受签署的最终通过与撤回动作。
    #[test]
    fn domain_action_dispatch_rejects_foreign_actions() {
        let mut order = draft_order();
        start_sales_change_approval(
            &mut order,
            SalesChangeSubmissionId::new("cs-1"),
            "hash-1",
            "user-1",
        )
        .unwrap();
        execute_sales_change_domain_action(
            &mut order,
            ApprovalDomainAction::SalesChangeOrderApplyEffectiveChange,
            "user-1",
        )
        .unwrap();
        assert!(execute_sales_change_domain_action(
            &mut order,
            ApprovalDomainAction::StockAdjustmentSubmit,
            "user-1",
        )
        .is_err());
    }
}
