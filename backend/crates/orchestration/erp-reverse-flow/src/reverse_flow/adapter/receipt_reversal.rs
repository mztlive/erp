use bpm::SubjectRef;
use erp_core::common::time::Instant;
use erp_core::ids::CustomerAccountId;
use erp_returns::entity::returns::ReceiptReversal;
#[cfg(test)]
use erp_returns::entity::returns::ReceiptReversalStatus;
#[cfg(test)]
use erp_returns::service::approval::start_receipt_reversal_approval;
pub use erp_returns::service::approval::{
    cancel_receipt_reversal_to_draft, ensure_receipt_reversal_final_approve_posting,
};
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

/// 装配回款冲正单的审批适配器，启动、通过和撤回时按已登记的动作分派。政策缺失或动作重复时拒绝装配。
pub fn receipt_reversal_adapter() -> Result<ReceiptReversalAdapter> {
    let spec = adapter_spec_of(DocumentType::ReceiptReversal)?;
    ensure_adapter_spec_complete(&spec)?;
    receipt_reversal_adapter_from_spec(spec)
}

// 由政策规格填充回款冲正单显式 Adapter 字段。字段与合同签署值不一致时返回错误。
fn receipt_reversal_adapter_from_spec(spec: ApprovalAdapterSpec) -> Result<ReceiptReversalAdapter> {
    ensure_reverse_adapter_spec(
        &spec,
        &ExpectedReverseAdapterContracts {
            document_type: DocumentType::ReceiptReversal,
            on_approval_start: ApprovalDomainAction::ReceiptReversalSubmit,
            on_final_approve: ApprovalDomainAction::ReceiptReversalPost,
            cancel_action: ApprovalDomainAction::ReceiptReversalCancelApproval,
            owner_role: "receipt_reversal_approver",
            mismatch_message: "回款冲正单审批适配器登记不完整",
        },
    )?;

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
/// `business_object_id` 为冲正单主键。返回该冲正单的 `SubjectRef`。主键为空或超长时返回校验错误。
pub fn receipt_reversal_subject_ref(business_object_id: &str) -> Result<SubjectRef> {
    erp_workflow::entity::approval_integration::subject_ref_for(
        DocumentType::ReceiptReversal,
        business_object_id,
    )
    .map_err(|error| Error::ValidationError(error.to_string()))
}

/// 无已绑定定义的必须审批单据不得提交。
///
/// `binding` 为已保存的定义绑定；缺失表示尚未冻结。绑定存在时返回该绑定。绑定缺失时返回冲突。
pub fn require_receipt_reversal_binding(
    binding: Option<&ApprovalDefinitionBinding>,
) -> Result<&ApprovalDefinitionBinding> {
    binding.ok_or_else(|| Error::ConflictError("无有效审批绑定的回款冲正单不得提交".to_string()))
}

/// 执行签署的回款冲正单领域动作。
///
/// `reversal` 为业务实体；`action` 为合同强类型动作。成功时无返回值。过账或撤回动作已作用到 `reversal`。
///
/// 动作不属于本类型或状态不允许时返回错误。
pub fn execute_receipt_reversal_domain_action(
    reversal: &mut ReceiptReversal,
    action: ApprovalDomainAction,
) -> Result<()> {
    match action {
        ApprovalDomainAction::ReceiptReversalPost => {
            ensure_receipt_reversal_final_approve_posting(reversal).map_err(Error::from)
        },
        ApprovalDomainAction::ReceiptReversalCancelApproval => {
            cancel_receipt_reversal_to_draft(reversal).map_err(Error::from)
        },
        _ => Err(Error::ValidationError("该回款冲正单不支持此审批操作，请从对应审批待办办理".into())),
    }
}

/// 按单据组织判定审批人对象读取权。
///
/// 未提供组织或审批人时失败关闭，不得默认放行。
///
/// `organization_id` 为单据责任组织（原回款往来主体）。组织与审批人均非空时允许读取。组织或审批人为空时返回校验错误。
pub fn receipt_reversal_object_readable(organization_id: &str, assignee_user_id: &str) -> Result<bool> {
    document_object_readable(organization_id, assignee_user_id).map_err(Into::into)
}

/// 责任组织取往来主体，不得用空串或当前登录人组织补位。
///
/// `organization_id` 为原回款往来主体。返回非空责任组织。往来主体为空时返回校验错误。
pub fn receipt_reversal_responsible_org_id(organization_id: &str) -> Result<String> {
    if organization_id.trim().is_empty() {
        return Err(Error::ValidationError("回款冲正单缺少往来主体，无法冻结责任组织".to_string()));
    }
    Ok(organization_id.to_string())
}

/// 按合同 §4.4.5 冻结回款冲正快照。
///
/// 对手方为原回款客户（可空）；金额合计必填。`document_no` 取冲正单号。 `responsible_org_id` 必须是原回款往来主体，不得用登录人组织补位。
///
/// `reversal` 为已冻结提交版本的冲正单；`responsible_org_id` 为原回款往来主体；`customer_id` 为原回款可选经营归属客户。
///
/// 返回已冻结的审批主体快照。组织为空时返回校验错误。
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

#[cfg(test)]
mod receipt_reversal_tests {
    use std::str::FromStr;

    use erp_core::ids::{CustomerReceiptId, ReceiptReversalId};
    use erp_returns::entity::returns::ReceiptReversalData;

    use super::*;

    fn draft_reversal() -> ReceiptReversal {
        ReceiptReversal::new(
            ReceiptReversalId::new("rr-1"),
            ReceiptReversalData {
                reversal_no: "RR-1".into(),
                original_customer_receipt_id: CustomerReceiptId::new("cr-1"),
                reason_code: None,
                reason_text: "错记回款冲正".into(),
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

    // 验证回款冲正审批适配器逐项声明合同规定的字段。
    #[test]
    fn adapter_declares_all_required_fields() {
        let adapter = receipt_reversal_adapter().expect("回款冲正单必须可登记");
        assert_eq!(adapter.document_type, DocumentType::ReceiptReversal);
        assert_eq!(adapter.process_kind.as_str(), "receipt_reversal");

        assert_eq!(
            receipt_reversal_subject_ref("rr-1").expect("主体引用必须可构造").subject_kind(),
            "receipt_reversal"
        );
        assert_eq!(adapter.subject_ref_builder, "subject_ref_for(ReceiptReversal)");
        assert_eq!(
            adapter.subject_version_source,
            ApprovalSubjectVersionSource::EntityApprovalSubjectVersion
        );
        assert_eq!(adapter.subject_snapshot_builder, "build_receipt_reversal_snapshot");

        assert_eq!(adapter.on_approval_start, ApprovalDomainAction::ReceiptReversalSubmit);
        assert_eq!(adapter.on_final_approve, ApprovalDomainAction::ReceiptReversalPost);
        assert_eq!(adapter.cancel_action, ApprovalDomainAction::ReceiptReversalCancelApproval);

        assert_eq!(adapter.owner_role, "receipt_reversal_approver");
        assert_eq!(
            adapter.owner_organization_snapshot,
            OwnerOrganizationSource::SubjectSnapshotResponsibleOrgId
        );
        assert_eq!(adapter.read_scope, AdapterReadScope::DocumentOrganizationAndCreator);

        assert_ne!(adapter.on_approval_start, adapter.on_final_approve);
        assert_ne!(adapter.on_approval_start, adapter.cancel_action);
    }

    // 验证回款冲正提交后进入审批中，撤回恢复草稿但保留已推进的提交版本。
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

    // 对照回款冲正的非法状态：非草稿不能提交，非审批中不能撤回或过账。
    #[test]
    fn illegal_status_transitions_fail_closed() {
        let mut posted = draft_reversal();
        start_receipt_reversal_approval(&mut posted).unwrap();
        posted.mark_posted().unwrap();
        assert!(start_receipt_reversal_approval(&mut posted).is_err());
        assert!(cancel_receipt_reversal_to_draft(&mut posted).is_err());
        assert!(ensure_receipt_reversal_final_approve_posting(&posted).is_err());
    }

    // 验证回款冲正只有审批中允许过账，草稿不能跳过审批。
    #[test]
    fn post_only_accepts_in_approval() {
        let mut reversal = draft_reversal();
        assert!(ensure_receipt_reversal_final_approve_posting(&reversal).is_err());
        start_receipt_reversal_approval(&mut reversal).unwrap();
        assert!(ensure_receipt_reversal_final_approve_posting(&reversal).is_ok());
    }

    // 验证回款冲正缺少冻结审批定义时不能提交。
    #[test]
    fn missing_frozen_binding_is_rejected() {
        assert!(require_receipt_reversal_binding(None).is_err());
    }

    // 快照冻结客户对手方、冲正单号与金额合计。
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
        assert!(matches!(payload.counterparty, Some(ApprovalSubjectCounterparty::Customer { .. })));

        assert!(
            build_receipt_reversal_snapshot(&reversal, " ", None, "user-1", Instant::from_unix_secs(10))
                .is_err()
        );
    }

    // 验证回款冲正审批读取缺少责任组织或办理人时不放行。
    #[test]
    fn object_read_fails_closed_on_empty_identity() {
        assert!(receipt_reversal_object_readable("party-1", "u1").unwrap());
        assert!(receipt_reversal_object_readable(" ", "u1").is_err());
        assert!(receipt_reversal_object_readable("party-1", "").is_err());
    }

    // 验证回款冲正的领域分派只接受已登记的撤回和过账动作，拒绝其他单据动作。
    #[test]
    fn domain_action_dispatch_rejects_foreign_actions() {
        let mut reversal = draft_reversal();
        start_receipt_reversal_approval(&mut reversal).unwrap();
        execute_receipt_reversal_domain_action(&mut reversal, ApprovalDomainAction::ReceiptReversalPost)
            .unwrap();
        assert!(
            execute_receipt_reversal_domain_action(
                &mut reversal,
                ApprovalDomainAction::StockAdjustmentSubmit,
            )
            .is_err()
        );
    }
}
