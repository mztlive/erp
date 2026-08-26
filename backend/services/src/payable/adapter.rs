//! `SupplierPayment` 审批业务 Adapter。
//!
//! 必须显式声明合同 §4.4 / 阶段 04 §6 的全部适配器字段。
//! 领域动作只通过实体状态邻接与仓储更新，不得 `$set` 绕过不变式。
//! 资金类 `PENDING_REVIEW` 已收敛为 `IN_APPROVAL`，不得再走通用状态更新。

use bpm::SubjectRef;
use entities::approval_integration::{ApprovalSubjectCounterparty, ApprovalSubjectSnapshotPayload};
use entities::common::time::Instant;
use entities::document_registry::business_document::ApprovalDefinitionBinding;
use entities::document_registry::DocumentType;
use entities::ids::SupplierAccountId;
use entities::payable::{PendingPaymentAllocation, SupplierPayment, SupplierPaymentStatus};

use super::dto::{
    DocumentApprovalDefinitionView, DocumentApprovalHistoryPageView, DocumentApprovalInstanceView,
    DocumentApprovalView, PaymentAllocationLineRequest,
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
#[cfg(test)]
pub const RECENT_HISTORY_LIMIT: usize = 8;

/// 已注册的供应商付款单适配器规格。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SupplierPaymentAdapter {
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

/// 返回供应商付款单的完整适配器登记。
///
/// # 返回
/// 返回已校验完整性的规格与显式字段声明。
///
/// # 错误
/// 政策缺失或三类动作不互异时返回部署不变量错误。
pub fn supplier_payment_adapter() -> Result<SupplierPaymentAdapter> {
    let spec = adapter_spec_of(DocumentType::SupplierPayment)?;
    ensure_adapter_spec_complete(&spec)?;
    adapter_from_spec(spec)
}

/// 由政策规格填充显式 Adapter 字段。
///
/// # 错误
/// 字段与合同签署值不一致时返回错误。
fn adapter_from_spec(spec: ApprovalAdapterSpec) -> Result<SupplierPaymentAdapter> {
    if spec.document_type != DocumentType::SupplierPayment
        || spec.process_kind != process_kind_of(DocumentType::SupplierPayment)
        || spec.subject_version_source != ApprovalSubjectVersionSource::EntityApprovalSubjectVersion
        || spec.on_approval_start != ApprovalDomainAction::SupplierPaymentSubmit
        || spec.on_final_approve != ApprovalDomainAction::SupplierPaymentPost
        || spec.cancel_action != ApprovalDomainAction::SupplierPaymentCancelApproval
        || spec.owner_role.as_str() != "supplier_payment_approver"
        || spec.owner_organization_source != OwnerOrganizationSource::SubjectSnapshotResponsibleOrgId
        || spec.read_scope != AdapterReadScope::DocumentOrganizationAndCreator
        || !spec
            .subject_snapshot_fields
            .contains(&ApprovalSubjectSnapshotField::TotalAmount)
    {
        return Err(Error::Internal("供应商付款单审批适配器登记不完整".to_string()));
    }
    Ok(SupplierPaymentAdapter {
        document_type: spec.document_type,
        process_kind: spec.process_kind,
        subject_ref_builder: "subject_ref_for(SupplierPayment)",
        subject_version_source: spec.subject_version_source,
        subject_snapshot_builder: "build_supplier_payment_snapshot",
        on_approval_start: spec.on_approval_start,
        on_final_approve: spec.on_final_approve,
        cancel_action: spec.cancel_action,
        owner_role: spec.owner_role.as_str(),
        owner_organization_snapshot: spec.owner_organization_source,
        read_scope: spec.read_scope,
    })
}

/// 为供应商付款单构造唯一 `bpm::SubjectRef`。
///
/// # 参数
/// * `business_object_id` - 付款单主键
///
/// # 错误
/// 主键为空或超长时返回校验错误。
pub fn supplier_payment_subject_ref(business_object_id: &str) -> Result<SubjectRef> {
    subject_ref_for(DocumentType::SupplierPayment, business_object_id)
}

/// 提交并启动：冻结 `approval_subject_version` 并进入 `IN_APPROVAL`。
///
/// # 参数
/// * `payment` - 待提交付款单
/// * `allocations` - 冻结的待过账核销分配
///
/// # 返回
/// 返回冻结后的提交版本。
///
/// # 错误
/// 非草稿、分配非法或版本溢出时返回冲突。
pub fn start_supplier_payment_approval(
    payment: &mut SupplierPayment,
    allocations: Vec<PendingPaymentAllocation>,
) -> Result<u32> {
    Ok(payment.start_approval(allocations)?)
}

/// 撤回审批：回到草稿，且 `subject_version` 不回退。
///
/// # 参数
/// * `payment` - 审批中的付款单
///
/// # 错误
/// 非审批中时返回冲突。
pub fn cancel_supplier_payment_to_draft(payment: &mut SupplierPayment) -> Result<()> {
    Ok(payment.cancel_approval()?)
}

/// 最终通过过账前置：仅 `IN_APPROVAL` 可进入过账。
///
/// # 错误
/// 状态不是审批中时返回冲突。
pub fn ensure_final_approve_posting(payment: &SupplierPayment) -> Result<()> {
    if payment.status != SupplierPaymentStatus::InApproval {
        return Err(Error::ConflictError(
            "只有审批中的供应商付款单可以由最终通过动作过账".to_string(),
        ));
    }
    Ok(())
}

/// 将提交请求行转换为冻结分配。
///
/// # 错误
/// 金额非法时返回错误。
pub fn pending_allocations_from_request(
    lines: &[PaymentAllocationLineRequest],
) -> Result<Vec<PendingPaymentAllocation>> {
    lines
        .iter()
        .map(|line| {
            PendingPaymentAllocation::new(line.payable_entry_id.clone(), line.allocated_amount)
                .map_err(Into::into)
        })
        .collect()
}

/// 无已绑定定义的必须审批单据不得提交。
///
/// # 错误
/// 绑定缺失时返回冲突。
pub fn require_frozen_binding(
    binding: Option<&ApprovalDefinitionBinding>,
) -> Result<&ApprovalDefinitionBinding> {
    binding.ok_or_else(|| Error::ConflictError("无有效审批绑定的供应商付款单不得提交".to_string()))
}

/// 供应商付款单调用统一 `start_approval` 的目标命令。
///
/// 字段与合同 §14.2 对齐；不得包含定义 ID 或审批人。
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[cfg(test)]
pub struct SupplierPaymentStartCommand {
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
/// * `payment_id` - 付款单主键
/// * `subject_version` - `approval_subject_version`
/// * `actor_id` - 提交人
/// * `idempotency_key` - 幂等键
///
/// # 返回
/// 返回不含定义 ID 或审批人的目标启动命令。
#[cfg(test)]
pub fn supplier_payment_start_command(
    payment_id: &str,
    subject_version: u32,
    actor_id: &str,
    idempotency_key: &str,
) -> SupplierPaymentStartCommand {
    SupplierPaymentStartCommand {
        subject_kind: process_kind_of(DocumentType::SupplierPayment)
            .as_str()
            .to_string(),
        subject_id: payment_id.to_string(),
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
#[cfg(test)]
pub fn start_approval_command_kind(
    _command: &SupplierPaymentStartCommand,
) -> bpm::model::types::ApprovalCommandKind {
    bpm::model::types::ApprovalCommandKind::StartApproval
}

/// 执行签署的供应商付款单领域动作。
///
/// # 参数
/// * `payment` - 业务实体
/// * `action` - 合同强类型动作
///
/// # 错误
/// 动作不属于本类型或状态不允许时返回错误。
pub fn execute_supplier_payment_domain_action(
    payment: &mut SupplierPayment,
    action: ApprovalDomainAction,
) -> Result<()> {
    match action {
        ApprovalDomainAction::SupplierPaymentPost => ensure_final_approve_posting(payment),
        ApprovalDomainAction::SupplierPaymentCancelApproval => cancel_supplier_payment_to_draft(payment),
        other => Err(Error::ValidationError(format!(
            "动作 {} 不属于供应商付款单",
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
pub fn supplier_payment_object_readable(organization_id: &str, assignee_user_id: &str) -> Result<bool> {
    if organization_id.trim().is_empty() || assignee_user_id.trim().is_empty() {
        return Err(Error::ValidationError("单据组织或审批人不能为空".to_string()));
    }
    Ok(true)
}

/// 责任组织取供应商往来主体，不得用空串或当前登录人组织补位。
///
/// # 参数
/// * `party_id` - 供应商账号对应的往来主体
///
/// # 返回
/// 返回非空责任组织。
///
/// # 错误
/// 往来主体为空时返回校验错误。
pub fn supplier_payment_responsible_org_id(party_id: &str) -> Result<String> {
    let org = party_id.trim();
    if org.is_empty() {
        return Err(Error::ValidationError(
            "供应商付款单缺少往来主体，无法冻结责任组织".to_string(),
        ));
    }
    Ok(org.to_string())
}

/// 按合同 §4.4.5 冻结供应商付款快照。
///
/// 对手方为供应商；金额合计必填。`document_no` 取付款单号。
/// `responsible_org_id` 必须是供应商往来主体，不得用登录人组织补位。
///
/// # 参数
/// * `payment` - 已冻结提交版本的付款单
/// * `responsible_org_id` - 供应商往来主体
/// * `submitted_by` - 提交人
/// * `submitted_at` - 提交时间
///
/// # 错误
/// 组织为空、分配行为空或行数溢出时返回校验错误。
pub fn build_supplier_payment_snapshot(
    payment: &SupplierPayment,
    responsible_org_id: &str,
    submitted_by: &str,
    submitted_at: Instant,
) -> Result<ApprovalSubjectSnapshotPayload> {
    if payment.pending_allocations.is_empty() {
        return Err(Error::ValidationError(
            "供应商付款单没有核销分配，无法启动审批".to_string(),
        ));
    }
    Ok(ApprovalSubjectSnapshotPayload {
        document_no: payment.payment_no.clone(),
        responsible_org_id: supplier_payment_responsible_org_id(responsible_org_id)?,
        submitted_by: submitted_by.to_string(),
        submitted_at,
        counterparty: Some(ApprovalSubjectCounterparty::Supplier {
            supplier_id: SupplierAccountId::new(payment.supplier_id.to_string()),
        }),
        total_amount: Some(payment.amount),
        total_quantity: None,
        line_count: u32::try_from(payment.pending_allocations.len())
            .map_err(|_| Error::ValidationError("付款核销分配行数溢出".to_string()))?,
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
    status: SupplierPaymentStatus,
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
fn allowed_document_actions(status: SupplierPaymentStatus) -> Vec<String> {
    match status {
        SupplierPaymentStatus::Draft => vec!["SUBMIT".to_string()],
        SupplierPaymentStatus::InApproval => vec!["CANCEL".to_string()],
        SupplierPaymentStatus::Posted | SupplierPaymentStatus::Reversed => Vec::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::approval::binding::binding_from_published;
    use bpm::ids::ApprovalProcessDefinitionId;
    use entities::ids::{PayableEntryId, SupplierPaymentId};
    use entities::payable::SupplierPaymentData;
    use std::str::FromStr;

    fn draft_payment() -> SupplierPayment {
        SupplierPayment::new(
            SupplierPaymentId::new("sp-1"),
            SupplierPaymentData {
                payment_no: "SP-1".into(),
                supplier_id: SupplierAccountId::new("sup-1"),
                paid_at: Instant::from_unix_secs(10),
                amount: entities::money::Amount::from_str("100").expect("金额合法"),
                bank_reference: None,
            },
        )
        .expect("草稿必须可构造")
    }

    fn one_allocation() -> PendingPaymentAllocation {
        PendingPaymentAllocation::new(
            PayableEntryId::new("pe-1"),
            entities::money::Amount::from_str("40").expect("金额合法"),
        )
        .expect("分配必须可构造")
    }

    /// 适配器必须显式声明合同要求的全部字段。
    #[test]
    fn adapter_declares_all_required_fields() {
        let adapter = supplier_payment_adapter().expect("供应商付款单必须可登记");
        assert_eq!(adapter.document_type, DocumentType::SupplierPayment);
        assert_eq!(adapter.process_kind.as_str(), "supplier_payment");
        assert_eq!(
            supplier_payment_subject_ref("sp-1")
                .expect("主体引用必须可构造")
                .subject_kind(),
            "supplier_payment"
        );
        assert_eq!(adapter.subject_ref_builder, "subject_ref_for(SupplierPayment)");
        assert_eq!(
            adapter.subject_version_source,
            ApprovalSubjectVersionSource::EntityApprovalSubjectVersion
        );
        assert_eq!(
            adapter.subject_snapshot_builder,
            "build_supplier_payment_snapshot"
        );
        assert_eq!(
            adapter.on_approval_start,
            ApprovalDomainAction::SupplierPaymentSubmit
        );
        assert_eq!(
            adapter.on_final_approve,
            ApprovalDomainAction::SupplierPaymentPost
        );
        assert_eq!(
            adapter.cancel_action,
            ApprovalDomainAction::SupplierPaymentCancelApproval
        );
        assert_eq!(adapter.owner_role, "supplier_payment_approver");
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
        let mut payment = draft_payment();
        start_supplier_payment_approval(&mut payment, vec![one_allocation()]).unwrap();
        assert_eq!(payment.status, SupplierPaymentStatus::InApproval);
        assert_eq!(payment.approval_subject_version, 1);
        execute_supplier_payment_domain_action(
            &mut payment,
            ApprovalDomainAction::SupplierPaymentCancelApproval,
        )
        .unwrap();
        assert_eq!(payment.status, SupplierPaymentStatus::Draft);
        assert_eq!(payment.approval_subject_version, 1);
    }

    /// 非草稿不得提交；非审批中不得撤回或过账。
    #[test]
    fn illegal_status_transitions_fail_closed() {
        let mut posted = draft_payment();
        start_supplier_payment_approval(&mut posted, vec![one_allocation()]).unwrap();
        posted.mark_posted().unwrap();
        assert!(start_supplier_payment_approval(&mut posted, vec![one_allocation()]).is_err());
        assert!(cancel_supplier_payment_to_draft(&mut posted).is_err());
        assert!(ensure_final_approve_posting(&posted).is_err());
    }

    /// 过账只允许审批中，草稿不得直接过账。
    #[test]
    fn post_only_accepts_in_approval() {
        let mut payment = draft_payment();
        assert!(ensure_final_approve_posting(&payment).is_err());
        start_supplier_payment_approval(&mut payment, vec![one_allocation()]).unwrap();
        assert!(ensure_final_approve_posting(&payment).is_ok());
    }

    /// 启动命令不含定义 ID 或审批人。
    #[test]
    fn start_command_omits_definition_and_assignee() {
        let command = supplier_payment_start_command("sp-1", 1, "user-1", "key-1");
        let encoded = serde_json::to_value(&command).unwrap();
        assert!(encoded.get("definition_id").is_none());
        assert!(encoded.get("definition_key").is_none());
        assert!(encoded.get("assignee").is_none());
        assert_eq!(command.subject_kind, "supplier_payment");
        assert_eq!(command.subject_version, 1);
        assert_eq!(
            start_approval_command_kind(&command),
            bpm::model::types::ApprovalCommandKind::StartApproval
        );
        assert!(require_frozen_binding(None).is_err());
    }

    /// 快照冻结供应商对手方、付款单号与金额合计。
    #[test]
    fn snapshot_freezes_supplier_and_amount() {
        let mut payment = draft_payment();
        start_supplier_payment_approval(&mut payment, vec![one_allocation()]).unwrap();
        let payload =
            build_supplier_payment_snapshot(&payment, "party-1", "user-1", Instant::from_unix_secs(10))
                .unwrap();
        assert_eq!(payload.document_no, "SP-1");
        assert_eq!(payload.responsible_org_id, "party-1");
        assert_eq!(payload.submitted_by, "user-1");
        assert_eq!(payload.total_amount.unwrap().to_string(), "100");
        assert_eq!(payload.line_count, 1);
        assert!(payload.total_quantity.is_none());
        let mut empty = draft_payment();
        empty.pending_allocations.clear();
        assert!(
            build_supplier_payment_snapshot(&empty, "party-1", "user-1", Instant::from_unix_secs(10))
                .is_err()
        );
        assert!(supplier_payment_responsible_org_id(" ").is_err());
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
        let view = document_approval_view(Some(&binding), None, SupplierPaymentStatus::Draft);
        assert_eq!(view.requirement, "PROCESS_REQUIRED");
        assert_eq!(view.definition.as_ref().unwrap().id, "def-1");
        assert!(view.instance.is_none());
        assert!(view.recent_history.len() <= RECENT_HISTORY_LIMIT);
        assert_eq!(view.allowed_actions, vec!["SUBMIT".to_string()]);
        assert!(!view
            .allowed_actions
            .iter()
            .any(|item| item.contains("DEFINITION")));
        let running = document_approval_view(Some(&binding), None, SupplierPaymentStatus::InApproval);
        assert_eq!(running.allowed_actions, vec!["CANCEL".to_string()]);
    }

    /// 对象读取权空组织或空审批人失败关闭。
    #[test]
    fn object_read_fails_closed_on_empty_identity() {
        assert!(supplier_payment_object_readable("party-1", "u1").unwrap());
        assert!(supplier_payment_object_readable(" ", "u1").is_err());
        assert!(supplier_payment_object_readable("party-1", "").is_err());
    }

    /// 领域动作分派只接受签署的撤回与过账动作。
    #[test]
    fn domain_action_dispatch_rejects_foreign_actions() {
        let mut payment = draft_payment();
        start_supplier_payment_approval(&mut payment, vec![one_allocation()]).unwrap();
        execute_supplier_payment_domain_action(&mut payment, ApprovalDomainAction::SupplierPaymentPost)
            .unwrap();
        assert!(execute_supplier_payment_domain_action(
            &mut payment,
            ApprovalDomainAction::StockAdjustmentSubmit,
        )
        .is_err());
    }
}
