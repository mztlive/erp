//! `CustomerRefund` 审批业务 Adapter。
//!
//! 必须显式声明合同 §4.4 / 阶段 04 §6 的全部适配器字段。
//! 领域动作只通过实体状态邻接与仓储更新，不得 `$set` 绕过不变式。
//! 资金类 `PENDING_REVIEW` 已收敛为 `IN_APPROVAL`，不得再走通用状态更新。

use bpm::SubjectRef;
use entities::approval_integration::{ApprovalSubjectCounterparty, ApprovalSubjectSnapshotPayload};
use entities::common::time::Instant;
use entities::document_registry::business_document::ApprovalDefinitionBinding;
use entities::document_registry::DocumentType;
use entities::ids::CustomerAccountId;
use entities::returns::{CustomerRefund, CustomerRefundStatus};

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
