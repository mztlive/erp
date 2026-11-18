//! `CustomerReceipt` 审批业务 Adapter。
//!
//! 必须显式声明合同 §4.4 / 阶段 04 §6 的全部适配器字段。
//! 领域动作只通过实体状态邻接与仓储更新，不得 `$set` 绕过不变式。
//! 资金类 `PENDING_REVIEW` 已收敛为 `IN_APPROVAL`，不得再走通用状态更新。

use bpm::SubjectRef;
use erp_core::common::time::Instant;
use erp_core::ids::CustomerAccountId;
use erp_finance::entity::receivable::{CustomerReceipt, CustomerReceiptStatus, PendingReceiptAllocation};
#[cfg(test)]
use erp_read_models::finance::receivable::approval_view::document_approval_view;
use erp_workflow::entity::approval_integration::{
    ApprovalSubjectCounterparty, ApprovalSubjectSnapshotPayload, document_object_readable,
};
use erp_workflow::entity::document_registry::DocumentType;
use erp_workflow::entity::document_registry::business_document::ApprovalDefinitionBinding;
use erp_workflow::service::approval::business_adapter::{
    AdapterReadScope, ApprovalAdapterSpec, adapter_spec_of, ensure_adapter_spec_complete,
};
use erp_workflow::service::approval::policy::{
    ApprovalDomainAction, ApprovalSubjectSnapshotField, ApprovalSubjectVersionSource, OwnerOrganizationSource,
};
use erp_workflow::service::approval::process_kind::process_kind_of;

use crate::{Error, Result};

/// 已注册的客户回款单适配器规格。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CustomerReceiptAdapter {
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

/// 装配客户回款单的审批适配器，启动、通过和撤回时按已登记的动作分派。政策缺失或动作重复时拒绝装配。
pub fn customer_receipt_adapter() -> Result<CustomerReceiptAdapter> {
    let spec = adapter_spec_of(DocumentType::CustomerReceipt)?;
    ensure_adapter_spec_complete(&spec)?;
    adapter_from_spec(spec)
}

// 由政策规格填充显式 Adapter 字段。字段与合同签署值不一致时返回错误。
fn adapter_from_spec(spec: ApprovalAdapterSpec) -> Result<CustomerReceiptAdapter> {
    if spec.document_type != DocumentType::CustomerReceipt
        || spec.process_kind != process_kind_of(DocumentType::CustomerReceipt)
        || spec.subject_version_source != ApprovalSubjectVersionSource::EntityApprovalSubjectVersion
        || spec.on_approval_start != ApprovalDomainAction::CustomerReceiptSubmit
        || spec.on_final_approve != ApprovalDomainAction::CustomerReceiptPost
        || spec.cancel_action != ApprovalDomainAction::CustomerReceiptCancelApproval
        || spec.owner_role.as_str() != "customer_receipt_approver"
        || spec.owner_organization_source != OwnerOrganizationSource::SubjectSnapshotResponsibleOrgId
        || spec.read_scope != AdapterReadScope::DocumentOrganizationAndCreator
        || !spec.subject_snapshot_fields.contains(&ApprovalSubjectSnapshotField::TotalAmount)
    {
        return Err(Error::Internal("客户回款单审批适配器登记不完整".to_string()));
    }
    Ok(CustomerReceiptAdapter {
        document_type: spec.document_type,
        process_kind: spec.process_kind,
        subject_ref_builder: "subject_ref_for(CustomerReceipt)",
        subject_version_source: spec.subject_version_source,
        subject_snapshot_builder: "build_customer_receipt_snapshot",
        on_approval_start: spec.on_approval_start,
        on_final_approve: spec.on_final_approve,
        cancel_action: spec.cancel_action,
        owner_role: spec.owner_role.as_str(),
        owner_organization_snapshot: spec.owner_organization_source,
        read_scope: spec.read_scope,
    })
}

/// 为客户回款单构造唯一 `bpm::SubjectRef`。
///
/// `business_object_id` 为回款单主键。返回客户回款单的 `SubjectRef`。主键为空或超长时返回校验错误。
pub fn customer_receipt_subject_ref(business_object_id: &str) -> Result<SubjectRef> {
    erp_workflow::entity::approval_integration::subject_ref_for(
        DocumentType::CustomerReceipt,
        business_object_id,
    )
    .map_err(|error| Error::ValidationError(error.to_string()))
}

/// 提交并启动：冻结 `approval_subject_version` 并进入 `IN_APPROVAL`。
///
/// `receipt` 为待提交回款单；`allocations` 为冻结的待过账核销分配。返回冻结后的提交版本。非草稿、分配非法或版本溢出时返回冲突。
pub fn start_customer_receipt_approval(
    receipt: &mut CustomerReceipt,
    allocations: Vec<PendingReceiptAllocation>,
) -> Result<u32> {
    Ok(receipt.start_approval(allocations)?)
}

/// 撤回审批：回到草稿，且 `approval_subject_version` 不回退。
///
/// `receipt` 为审批中的回款单。撤回成功时无返回值；回款单回到草稿，且 `approval_subject_version` 不回退。非审批中时返回冲突。
pub fn cancel_customer_receipt_to_draft(receipt: &mut CustomerReceipt) -> Result<()> {
    Ok(receipt.cancel_approval()?)
}

/// 最终通过动作登记核销前先检查回款状态；只接受 `IN_APPROVAL`，其他状态返回冲突。
pub fn ensure_final_approve_posting(receipt: &CustomerReceipt) -> Result<()> {
    if receipt.status != CustomerReceiptStatus::InApproval {
        return Err(Error::ConflictError("只有审批中的客户回款单可以由最终通过动作过账".to_string()));
    }
    Ok(())
}

/// 无已绑定定义的必须审批单据不得提交。
///
/// `binding` 为已查到的审批定义绑定；缺失时为 `None`。绑定存在时返回该绑定的引用。绑定缺失时返回冲突。
pub fn require_frozen_binding(
    binding: Option<&ApprovalDefinitionBinding>,
) -> Result<&ApprovalDefinitionBinding> {
    binding.ok_or_else(|| Error::ConflictError("无有效审批绑定的客户回款单不得提交".to_string()))
}

/// 审批运行时分派客户回款动作：最终通过只校验过账前状态，撤回将回款改回草稿。
/// 不属于回款的动作或当前状态不允许时返回错误。
pub fn execute_customer_receipt_domain_action(
    receipt: &mut CustomerReceipt,
    action: ApprovalDomainAction,
) -> Result<()> {
    match action {
        ApprovalDomainAction::CustomerReceiptPost => ensure_final_approve_posting(receipt),
        ApprovalDomainAction::CustomerReceiptCancelApproval => cancel_customer_receipt_to_draft(receipt),
        _ => Err(Error::ValidationError("该客户回款单不支持此审批操作，请从对应审批待办办理".into())),
    }
}

/// 按单据组织判定审批人对象读取权。
///
/// 未提供组织或审批人时失败关闭，不得默认放行。
///
/// `organization_id` 为单据责任组织（往来主体）。组织与审批人均非空时允许读取。组织或审批人为空时返回校验错误。
pub fn customer_receipt_object_readable(organization_id: &str, assignee_user_id: &str) -> Result<bool> {
    document_object_readable(organization_id, assignee_user_id).map_err(Into::into)
}

/// 责任组织取往来主体，不得用空串或当前登录人组织补位。
///
/// 唯一规则源为 `CustomerReceipt::approval_responsible_org_id`；本函数只做错误适配，不复制取值与缺省规则。
///
/// `receipt` 为回款单。返回非空责任组织。往来主体为空时返回校验错误。
pub fn customer_receipt_responsible_org_id(receipt: &CustomerReceipt) -> Result<String> {
    receipt.approval_responsible_org_id().map_err(|error| Error::ValidationError(error.to_string()))
}

/// 按合同 §4.4.5 冻结客户回款快照。
///
/// 领域事实唯一来源为 `CustomerReceipt::approval_facts`；本函数只把领域 facts 映射到 BPM action 载荷（注入提交人/提交时间），不复制取值与缺省规则。
///
/// `receipt` 为已冻结提交版本的回款单。返回注入提交人与提交时间后的审批主体快照。组织为空、分配行为空或行数溢出时返回校验错误。
pub fn build_customer_receipt_snapshot(
    receipt: &CustomerReceipt,
    submitted_by: &str,
    submitted_at: Instant,
) -> Result<ApprovalSubjectSnapshotPayload> {
    let facts = receipt.approval_facts().map_err(|error| Error::ValidationError(error.to_string()))?;
    Ok(ApprovalSubjectSnapshotPayload {
        document_no: facts.document_no,
        responsible_org_id: facts.responsible_org_id,
        submitted_by: submitted_by.to_string(),
        submitted_at,
        counterparty: facts.customer_id.map(|customer_id| ApprovalSubjectCounterparty::Customer {
            customer_id: CustomerAccountId::new(customer_id.to_string()),
        }),
        total_amount: Some(facts.total_amount),
        total_quantity: None,
        line_count: facts.line_count,
    })
}

#[cfg(test)]
mod tests {
    use std::str::FromStr;

    use bpm::ids::ApprovalProcessDefinitionId;
    use erp_core::ids::{CustomerReceiptId, PartyId, ReceivableEntryId};
    use erp_finance::entity::receivable::CustomerReceiptData;
    use erp_workflow::service::approval::binding::binding_from_published;

    use super::*;

    fn draft_receipt() -> CustomerReceipt {
        CustomerReceipt::new(
            CustomerReceiptId::new("cr-1"),
            CustomerReceiptData {
                receipt_no: "RC-1".into(),
                counterparty_party_id: PartyId::new("party-1"),
                customer_id: Some(CustomerAccountId::new("cust-1")),
                received_at: Instant::from_unix_secs(10),
                amount: erp_core::money::Amount::from_str("100").expect("金额合法"),
                bank_reference: None,
            },
            "creator-1",
        )
        .expect("草稿必须可构造")
    }

    fn one_allocation() -> PendingReceiptAllocation {
        PendingReceiptAllocation::new(
            ReceivableEntryId::new("re-1"),
            erp_core::money::Amount::from_str("40").expect("金额合法"),
        )
        .expect("分配必须可构造")
    }

    // 验证客户回款审批适配器逐项声明合同规定的字段。
    #[test]
    fn adapter_declares_all_required_fields() {
        let adapter = customer_receipt_adapter().expect("客户回款单必须可登记");
        assert_eq!(adapter.document_type, DocumentType::CustomerReceipt);
        assert_eq!(adapter.process_kind.as_str(), "customer_receipt");
        assert_eq!(
            customer_receipt_subject_ref("cr-1").expect("主体引用必须可构造").subject_kind(),
            "customer_receipt"
        );
        assert_eq!(adapter.subject_ref_builder, "subject_ref_for(CustomerReceipt)");
        assert_eq!(
            adapter.subject_version_source,
            ApprovalSubjectVersionSource::EntityApprovalSubjectVersion
        );
        assert_eq!(adapter.subject_snapshot_builder, "build_customer_receipt_snapshot");

        assert_eq!(adapter.on_approval_start, ApprovalDomainAction::CustomerReceiptSubmit);
        assert_eq!(adapter.on_final_approve, ApprovalDomainAction::CustomerReceiptPost);
        assert_eq!(adapter.cancel_action, ApprovalDomainAction::CustomerReceiptCancelApproval);

        assert_eq!(adapter.owner_role, "customer_receipt_approver");
        assert_eq!(
            adapter.owner_organization_snapshot,
            OwnerOrganizationSource::SubjectSnapshotResponsibleOrgId
        );
        assert_eq!(adapter.read_scope, AdapterReadScope::DocumentOrganizationAndCreator);
        assert_ne!(adapter.on_approval_start, adapter.on_final_approve);
        assert_ne!(adapter.on_approval_start, adapter.cancel_action);
    }

    // 验证客户回款提交后进入审批中，撤回恢复草稿但保留已推进的提交版本。
    #[test]
    fn submit_enters_in_approval_and_cancel_returns_draft() {
        let mut receipt = draft_receipt();
        start_customer_receipt_approval(&mut receipt, vec![one_allocation()]).unwrap();
        assert_eq!(receipt.status, CustomerReceiptStatus::InApproval);
        assert_eq!(receipt.approval_subject_version, 1);

        execute_customer_receipt_domain_action(
            &mut receipt,
            ApprovalDomainAction::CustomerReceiptCancelApproval,
        )
        .unwrap();
        assert_eq!(receipt.status, CustomerReceiptStatus::Draft);
        assert_eq!(receipt.approval_subject_version, 1);
    }

    // 对照客户回款的非法状态：非草稿不能提交，非审批中不能撤回或过账。
    #[test]
    fn illegal_status_transitions_fail_closed() {
        let mut posted = draft_receipt();
        start_customer_receipt_approval(&mut posted, vec![one_allocation()]).unwrap();
        posted.mark_posted().unwrap();
        assert!(start_customer_receipt_approval(&mut posted, vec![one_allocation()]).is_err());
        assert!(cancel_customer_receipt_to_draft(&mut posted).is_err());
        assert!(ensure_final_approve_posting(&posted).is_err());
    }

    // 验证客户回款只有审批中允许过账，草稿不能跳过审批。
    #[test]
    fn post_only_accepts_in_approval() {
        let mut receipt = draft_receipt();
        assert!(ensure_final_approve_posting(&receipt).is_err());
        start_customer_receipt_approval(&mut receipt, vec![one_allocation()]).unwrap();
        assert!(ensure_final_approve_posting(&receipt).is_ok());
    }

    // 验证客户回款缺少冻结审批定义时不能提交。
    #[test]
    fn missing_frozen_binding_is_rejected() {
        assert!(require_frozen_binding(None).is_err());
    }

    // 快照冻结客户对手方、回款单号与金额合计。
    #[test]
    fn snapshot_freezes_customer_and_amount() {
        let mut receipt = draft_receipt();
        start_customer_receipt_approval(&mut receipt, vec![one_allocation()]).unwrap();
        let payload =
            build_customer_receipt_snapshot(&receipt, "user-1", Instant::from_unix_secs(10)).unwrap();
        assert_eq!(payload.document_no, "RC-1");
        assert_eq!(payload.responsible_org_id, "party-1");
        assert_eq!(payload.submitted_by, "user-1");
        assert_eq!(payload.total_amount.unwrap().to_string(), "100");
        assert_eq!(payload.line_count, 1);
        assert!(payload.total_quantity.is_none());

        let mut empty = draft_receipt();
        empty.pending_allocations.clear();
        assert!(build_customer_receipt_snapshot(&empty, "user-1", Instant::from_unix_secs(10)).is_err());
    }

    // 详情只读审批结构；未加载历史时为空，允许动作不含选择定义或审批人。
    #[test]
    fn detail_approval_is_read_only_without_loaded_history() {
        let binding =
            binding_from_published(ApprovalProcessDefinitionId::new("def-1"), 2, Instant::from_unix_secs(1))
                .unwrap();
        let view = document_approval_view(Some(&binding), None, CustomerReceiptStatus::Draft);
        assert_eq!(view.requirement, "PROCESS_REQUIRED");
        assert_eq!(view.definition.as_ref().unwrap().id, "def-1");
        assert!(view.instance.is_none());
        assert!(view.recent_history.is_empty());
        assert_eq!(view.allowed_actions, vec!["SUBMIT".to_string()]);
        assert!(!view.allowed_actions.iter().any(|item| item.contains("DEFINITION")));

        let running = document_approval_view(Some(&binding), None, CustomerReceiptStatus::InApproval);
        assert_eq!(running.allowed_actions, vec!["CANCEL".to_string()]);
    }

    // 验证客户回款审批读取缺少责任组织或办理人时不放行。
    #[test]
    fn object_read_fails_closed_on_empty_identity() {
        assert!(customer_receipt_object_readable("party-1", "u1").unwrap());
        assert!(customer_receipt_object_readable(" ", "u1").is_err());
        assert!(customer_receipt_object_readable("party-1", "").is_err());
    }

    // 验证客户回款的领域分派只接受已登记的撤回和过账动作，拒绝其他单据动作。
    #[test]
    fn domain_action_dispatch_rejects_foreign_actions() {
        let mut receipt = draft_receipt();
        start_customer_receipt_approval(&mut receipt, vec![one_allocation()]).unwrap();
        execute_customer_receipt_domain_action(&mut receipt, ApprovalDomainAction::CustomerReceiptPost)
            .unwrap();
        assert!(execute_customer_receipt_domain_action(
            &mut receipt,
            ApprovalDomainAction::StockAdjustmentSubmit,
        )
        .is_err());
    }
}
