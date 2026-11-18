use bpm::SubjectRef;
use erp_core::common::time::Instant;
use erp_core::ids::CustomerAccountId;
use erp_returns::entity::returns::CustomerRefund;
#[cfg(test)]
use erp_returns::entity::returns::CustomerRefundStatus;
#[cfg(test)]
use erp_returns::service::approval::start_customer_refund_approval;
pub use erp_returns::service::approval::{cancel_customer_refund_to_draft, ensure_final_approve_posting};
use erp_workflow::entity::approval_integration::{
    ApprovalSubjectCounterparty, ApprovalSubjectSnapshotPayload, document_object_readable,
};
use erp_workflow::entity::document_registry::DocumentType;
use erp_workflow::entity::document_registry::business_document::ApprovalDefinitionBinding;
use erp_workflow::service::approval::business_adapter::{
    AdapterReadScope, ApprovalAdapterSpec, adapter_spec_of, ensure_adapter_spec_complete,
};
use erp_workflow::service::approval::policy::{
    ApprovalDomainAction, ApprovalSubjectVersionSource, OwnerOrganizationSource,
};

use super::common::{ExpectedReverseAdapterContracts, ensure_reverse_adapter_spec};
use crate::{Error, Result};

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

/// 装配客户退款单的审批适配器，启动、通过和撤回时按已登记的动作分派。政策缺失或动作重复时拒绝装配。
pub fn customer_refund_adapter() -> Result<CustomerRefundAdapter> {
    let spec = adapter_spec_of(DocumentType::CustomerRefund)?;
    ensure_adapter_spec_complete(&spec)?;
    adapter_from_spec(spec)
}

// 由政策规格填充显式 Adapter 字段。字段与合同签署值不一致时返回错误。
fn adapter_from_spec(spec: ApprovalAdapterSpec) -> Result<CustomerRefundAdapter> {
    ensure_reverse_adapter_spec(
        &spec,
        &ExpectedReverseAdapterContracts {
            document_type: DocumentType::CustomerRefund,
            on_approval_start: ApprovalDomainAction::CustomerRefundSubmit,
            on_final_approve: ApprovalDomainAction::CustomerRefundPost,
            cancel_action: ApprovalDomainAction::CustomerRefundCancelApproval,
            owner_role: "customer_refund_approver",
            mismatch_message: "客户退款单审批适配器登记不完整",
        },
    )?;

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
/// `business_object_id` 为退款单主键。返回该退款单的 `SubjectRef`。主键为空或超长时返回校验错误。
pub fn customer_refund_subject_ref(business_object_id: &str) -> Result<SubjectRef> {
    erp_workflow::entity::approval_integration::subject_ref_for(
        DocumentType::CustomerRefund,
        business_object_id,
    )
    .map_err(|error| Error::ValidationError(error.to_string()))
}

/// 无已绑定定义的必须审批单据不得提交。
///
/// `binding` 为已保存的定义绑定；缺失表示尚未冻结。绑定存在时返回该绑定。绑定缺失时返回冲突。
pub fn require_frozen_binding(
    binding: Option<&ApprovalDefinitionBinding>,
) -> Result<&ApprovalDefinitionBinding> {
    binding.ok_or_else(|| Error::ConflictError("无有效审批绑定的客户退款单不得提交".to_string()))
}

/// 执行签署的客户退款单领域动作。
///
/// `refund` 为业务实体；`action` 为合同强类型动作。成功时无返回值。过账或撤回动作已作用到 `refund`。动作不属于本类型或状态不允许时返回错误。
pub fn execute_customer_refund_domain_action(
    refund: &mut CustomerRefund,
    action: ApprovalDomainAction,
) -> Result<()> {
    match action {
        ApprovalDomainAction::CustomerRefundPost => ensure_final_approve_posting(refund).map_err(Error::from),
        ApprovalDomainAction::CustomerRefundCancelApproval => {
            cancel_customer_refund_to_draft(refund).map_err(Error::from)
        },
        _ => Err(Error::ValidationError("该客户退款单不支持此审批操作，请从对应审批待办办理".into())),
    }
}

/// 按单据组织判定审批人对象读取权。
///
/// 未提供组织或审批人时失败关闭，不得默认放行。
///
/// `organization_id` 为单据责任组织（客户往来主体）。组织与审批人均非空时允许读取。组织或审批人为空时返回校验错误。
pub fn customer_refund_object_readable(organization_id: &str, assignee_user_id: &str) -> Result<bool> {
    document_object_readable(organization_id, assignee_user_id).map_err(Into::into)
}

/// 责任组织取往来主体，不得用空串或当前登录人组织补位。
///
/// `organization_id` 为客户往来主体。返回非空责任组织。往来主体为空时返回校验错误。
pub fn customer_refund_responsible_org_id(organization_id: &str) -> Result<String> {
    if organization_id.trim().is_empty() {
        return Err(Error::ValidationError("客户退款单缺少往来主体，无法冻结责任组织".to_string()));
    }
    Ok(organization_id.to_string())
}

/// 按合同 §4.4.5 冻结客户退款快照。
///
/// 对手方为客户；金额合计必填。`document_no` 取退款单号。 `responsible_org_id` 必须是客户往来主体，不得用登录人组织补位。
///
/// `refund` 为已冻结提交版本的退款单；`responsible_org_id` 为客户往来主体。返回已冻结的审批主体快照。组织为空时返回校验错误。
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

#[cfg(test)]
mod tests {
    use std::str::FromStr;

    use erp_core::ids::{CustomerReceiptId, CustomerRefundId};
    use erp_returns::entity::returns::CustomerRefundData;

    use super::*;

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
                amount: erp_core::money::Amount::from_str("100").expect("金额合法"),
                handled_by: "handler-1".into(),
                reviewed_by: "reviewer-1".into(),
                occurred_at: Instant::from_unix_secs(10),
                evidence_attachment_id: None,
            },
            "creator-1",
        )
        .expect("草稿必须可构造")
    }

    // 验证客户退款审批适配器逐项声明合同规定的字段。
    #[test]
    fn adapter_declares_all_required_fields() {
        let adapter = customer_refund_adapter().expect("客户退款单必须可登记");
        assert_eq!(adapter.document_type, DocumentType::CustomerRefund);
        assert_eq!(adapter.process_kind.as_str(), "customer_refund");

        assert_eq!(
            customer_refund_subject_ref("crf-1").expect("主体引用必须可构造").subject_kind(),
            "customer_refund"
        );
        assert_eq!(adapter.subject_ref_builder, "subject_ref_for(CustomerRefund)");
        assert_eq!(
            adapter.subject_version_source,
            ApprovalSubjectVersionSource::EntityApprovalSubjectVersion
        );
        assert_eq!(adapter.subject_snapshot_builder, "build_customer_refund_snapshot");

        assert_eq!(adapter.on_approval_start, ApprovalDomainAction::CustomerRefundSubmit);
        assert_eq!(adapter.on_final_approve, ApprovalDomainAction::CustomerRefundPost);
        assert_eq!(adapter.cancel_action, ApprovalDomainAction::CustomerRefundCancelApproval);

        assert_eq!(adapter.owner_role, "customer_refund_approver");
        assert_eq!(
            adapter.owner_organization_snapshot,
            OwnerOrganizationSource::SubjectSnapshotResponsibleOrgId
        );
        assert_eq!(adapter.read_scope, AdapterReadScope::DocumentOrganizationAndCreator);

        assert_ne!(adapter.on_approval_start, adapter.on_final_approve);
        assert_ne!(adapter.on_approval_start, adapter.cancel_action);
    }

    // 验证客户退款提交后进入审批中，撤回恢复草稿但保留已推进的提交版本。
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

    // 对照客户退款的非法状态：非草稿不能提交，非审批中不能撤回或过账。
    #[test]
    fn illegal_status_transitions_fail_closed() {
        let mut posted = draft_refund();
        start_customer_refund_approval(&mut posted).unwrap();
        posted.mark_posted().unwrap();
        assert!(start_customer_refund_approval(&mut posted).is_err());
        assert!(cancel_customer_refund_to_draft(&mut posted).is_err());
        assert!(ensure_final_approve_posting(&posted).is_err());
    }

    // 验证客户退款只有审批中允许过账，草稿不能跳过审批。
    #[test]
    fn post_only_accepts_in_approval() {
        let mut refund = draft_refund();
        assert!(ensure_final_approve_posting(&refund).is_err());
        start_customer_refund_approval(&mut refund).unwrap();
        assert!(ensure_final_approve_posting(&refund).is_ok());
    }

    // 验证客户退款缺少冻结审批定义时不能提交。
    #[test]
    fn missing_frozen_binding_is_rejected() {
        assert!(require_frozen_binding(None).is_err());
    }

    // 快照冻结客户对手方、退款单号与金额合计。
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

    // 验证客户退款审批读取缺少责任组织或办理人时不放行。
    #[test]
    fn object_read_fails_closed_on_empty_identity() {
        assert!(customer_refund_object_readable("party-1", "u1").unwrap());
        assert!(customer_refund_object_readable(" ", "u1").is_err());
        assert!(customer_refund_object_readable("party-1", "").is_err());
    }

    // 验证客户退款的领域分派只接受已登记的撤回和过账动作，拒绝其他单据动作。
    #[test]
    fn domain_action_dispatch_rejects_foreign_actions() {
        let mut refund = draft_refund();
        start_customer_refund_approval(&mut refund).unwrap();
        execute_customer_refund_domain_action(&mut refund, ApprovalDomainAction::CustomerRefundPost).unwrap();
        assert!(
            execute_customer_refund_domain_action(&mut refund, ApprovalDomainAction::StockAdjustmentSubmit,)
                .is_err()
        );
    }
}
