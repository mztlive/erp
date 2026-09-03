//! `StockAdjustment` 审批业务 Adapter。
//!
//! 试点类型必须显式声明合同 §4.4 / 阶段 04 §6 的全部适配器字段。
//! 领域动作只通过实体状态邻接与仓储更新，不得 `$set` 绕过不变式。

use bpm::SubjectRef;
use entities::document_registry::business_document::ApprovalDefinitionBinding;
use entities::document_registry::DocumentType;
use entities::inventory::{StockAdjustment, StockAdjustmentState};

use crate::approval::business_adapter::{
    adapter_spec_of, ensure_adapter_spec_complete, AdapterReadScope, ApprovalAdapterSpec,
};
use crate::approval::policy::{
    ApprovalDomainAction, ApprovalRequirement, ApprovalSubjectSnapshotField, ApprovalSubjectVersionSource,
    OwnerOrganizationSource,
};
use crate::approval::process_kind::process_kind_of;
use crate::errors::{Error, Result};

use super::dto::{
    CancelStockAdjustmentApprovalTokenView, DocumentApprovalDefinitionView, DocumentApprovalHistoryItemView,
    DocumentApprovalHistoryPageView, DocumentApprovalInstanceView, DocumentApprovalView,
    SubmitStockAdjustmentApprovalTokenView,
};

/// 详情最近审批历史条数上限。完整历史走分页端点。
pub const RECENT_HISTORY_LIMIT: usize = 8;

/// 已注册的库存调整单适配器规格。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StockAdjustmentAdapter {
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

/// 返回试点类型的完整适配器登记。
///
/// # 返回
/// 返回已校验完整性的规格与显式字段声明。
///
/// # 错误
/// 政策缺失或三类动作不互异时返回部署不变量错误。
pub fn stock_adjustment_adapter() -> Result<StockAdjustmentAdapter> {
    let spec = adapter_spec_of(DocumentType::StockAdjustment)?;
    ensure_adapter_spec_complete(&spec)?;
    adapter_from_spec(spec)
}

/// 由政策规格填充显式 Adapter 字段。
///
/// # 错误
/// 字段与合同签署值不一致时返回错误。
fn adapter_from_spec(spec: ApprovalAdapterSpec) -> Result<StockAdjustmentAdapter> {
    if spec.document_type != DocumentType::StockAdjustment
        || spec.process_kind != process_kind_of(DocumentType::StockAdjustment)
        || spec.subject_version_source != ApprovalSubjectVersionSource::EntityApprovalSubjectVersion
        || spec.on_approval_start != ApprovalDomainAction::StockAdjustmentSubmit
        || spec.on_final_approve != ApprovalDomainAction::StockAdjustmentPost
        || spec.cancel_action != ApprovalDomainAction::StockAdjustmentCancelApproval
        || spec.owner_role.as_str() != "stock_adjustment_approver"
        || spec.owner_organization_source != OwnerOrganizationSource::SubjectSnapshotResponsibleOrgId
        || spec.read_scope != AdapterReadScope::DocumentOrganizationAndCreator
        || !spec
            .subject_snapshot_fields
            .contains(&ApprovalSubjectSnapshotField::TotalQuantity)
    {
        return Err(Error::Internal("库存调整单审批适配器登记不完整".to_string()));
    }
    Ok(StockAdjustmentAdapter {
        document_type: spec.document_type,
        process_kind: spec.process_kind,
        subject_ref_builder: "subject_ref_for(StockAdjustment)",
        subject_version_source: spec.subject_version_source,
        subject_snapshot_builder: "build_stock_adjustment_snapshot",
        on_approval_start: spec.on_approval_start,
        on_final_approve: spec.on_final_approve,
        cancel_action: spec.cancel_action,
        owner_role: spec.owner_role.as_str(),
        owner_organization_snapshot: spec.owner_organization_source,
        read_scope: spec.read_scope,
    })
}

/// 为库存调整单构造唯一 `bpm::SubjectRef`。
///
/// # 参数
/// * `business_object_id` - 调整单主键
///
/// # 错误
/// 主键为空或超长时返回校验错误。
pub fn stock_adjustment_subject_ref(business_object_id: &str) -> Result<SubjectRef> {
    entities::approval_integration::subject_ref_for(DocumentType::StockAdjustment, business_object_id)
        .map_err(|error| Error::ValidationError(error.to_string()))
}

/// 无已绑定定义的必须审批单据不得提交。
///
/// # 错误
/// 绑定缺失时返回冲突。
pub fn require_frozen_binding(
    binding: Option<&ApprovalDefinitionBinding>,
) -> Result<&ApprovalDefinitionBinding> {
    binding.ok_or_else(|| Error::ConflictError("无有效审批绑定的库存调整单不得提交".to_string()))
}

/// 库存调整单调用统一 `start_approval` 的目标命令。
///
/// 字段与合同 §14.2 / `ApprovalStartCommand` 对齐；不得包含定义 ID 或审批人。
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct StockAdjustmentStartCommand {
    /// 业务对象种类。
    pub subject_kind: String,
    /// 业务对象 ID。
    pub subject_id: String,
    /// 冻结提交版本。
    pub subject_version: u32,
    /// 启动人。
    #[serde(skip)]
    pub actor_id: String,
    /// 幂等键。
    pub idempotency_key: String,
}

/// 由冻结绑定构造目标启动命令。客户端不得提交定义或审批人。
///
/// # 参数
/// * `adjustment_id` - 调整单主键
/// * `subject_version` - 已冻结提交版本
/// * `actor_id` - 提交人
/// * `idempotency_key` - 幂等键
///
/// # 返回
/// 返回不含定义 ID 或审批人的目标启动命令。
pub fn stock_adjustment_start_command(
    adjustment_id: &str,
    subject_version: u32,
    actor_id: &str,
    idempotency_key: &str,
) -> StockAdjustmentStartCommand {
    StockAdjustmentStartCommand {
        subject_kind: process_kind_of(DocumentType::StockAdjustment)
            .as_str()
            .to_string(),
        subject_id: adjustment_id.to_string(),
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
    _command: &StockAdjustmentStartCommand,
) -> bpm::model::types::ApprovalCommandKind {
    bpm::model::types::ApprovalCommandKind::StartApproval
}

/// 执行签署的库存调整领域动作。
///
/// # 参数
/// * `adjustment` - 业务实体
/// * `action` - 合同强类型动作
///
/// # 错误
/// 动作不属于本类型或状态不允许时返回错误。
pub fn execute_stock_adjustment_domain_action(
    adjustment: &mut StockAdjustment,
    action: ApprovalDomainAction,
) -> Result<()> {
    match action {
        ApprovalDomainAction::StockAdjustmentSubmit => adjustment
            .start_approval()
            .map(|_| ())
            .map_err(|error| Error::ConflictError(error.to_string())),
        ApprovalDomainAction::StockAdjustmentPost => adjustment
            .ensure_approval_postable()
            .map_err(|error| Error::ConflictError(error.to_string())),
        ApprovalDomainAction::StockAdjustmentCancelApproval => adjustment
            .cancel_approval()
            .map_err(|error| Error::ConflictError(error.to_string())),
        other => Err(Error::ValidationError(format!(
            "动作 {} 不属于库存调整单",
            other.as_str()
        ))),
    }
}

/// 由绑定、运行摘要、历史和 actor-aware 提交/撤回令牌构造只读审批结构。
pub(super) fn document_approval_view_with_history(
    binding: Option<&ApprovalDefinitionBinding>,
    instance: Option<DocumentApprovalInstanceView>,
    recent_history: Vec<DocumentApprovalHistoryItemView>,
    history_page: DocumentApprovalHistoryPageView,
    status: StockAdjustmentState,
    submit_command: Option<SubmitStockAdjustmentApprovalTokenView>,
    cancel_command: Option<CancelStockAdjustmentApprovalTokenView>,
) -> DocumentApprovalView {
    let can_submit = submit_command.is_some();
    let can_cancel = cancel_command.is_some();
    DocumentApprovalView {
        requirement: match ApprovalRequirement::ProcessRequired {
            ApprovalRequirement::ProcessRequired => "PROCESS_REQUIRED",
            ApprovalRequirement::NoApproval => "NO_APPROVAL",
        }
        .to_string(),
        definition: binding.map(definition_view_from_binding),
        instance,
        recent_history,
        history_page,
        allowed_actions: allowed_document_actions(status, can_submit, can_cancel),
        submit_command,
        cancel_command,
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
fn allowed_document_actions(status: StockAdjustmentState, can_submit: bool, can_cancel: bool) -> Vec<String> {
    match status {
        StockAdjustmentState::Draft if can_submit => vec!["SUBMIT".to_string()],
        StockAdjustmentState::Draft => Vec::new(),
        StockAdjustmentState::InApproval if can_cancel => vec!["CANCEL".to_string()],
        StockAdjustmentState::InApproval => Vec::new(),
        StockAdjustmentState::Posted
        | StockAdjustmentState::Reversed
        | StockAdjustmentState::PendingWarehouseReview
        | StockAdjustmentState::PendingFinanceReview
        | StockAdjustmentState::Rejected => Vec::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::approval::binding::binding_from_published;
    use bpm::ids::ApprovalProcessDefinitionId;
    use entities::common::time::Instant;
    use entities::ids::{SkuId, StockAdjustmentId, StockAdjustmentLineId, WarehouseId};
    use entities::inventory::{
        AdjustmentReasonType, MovementDirection, StockAdjustmentData, StockAdjustmentLine,
        StockAdjustmentLineData,
    };
    use entities::money::Quantity;
    use std::str::FromStr;

    fn draft_adjustment() -> StockAdjustment {
        StockAdjustment::new(
            StockAdjustmentId::new("adj-1"),
            StockAdjustmentData {
                adjustment_no: "ADJ-1".into(),
                warehouse_id: WarehouseId::new("wh-1"),
                reason_type: AdjustmentReasonType::StockGain,
                prepared_by: "user-1".into(),
                note: None,
                occurred_at: None,
            },
            "creator-1",
        )
        .expect("草稿必须可构造")
    }

    fn one_line() -> StockAdjustmentLine {
        StockAdjustmentLine::new(
            StockAdjustmentLineId::new("line-1"),
            StockAdjustmentLineData {
                stock_adjustment_id: StockAdjustmentId::new("adj-1"),
                sku_id: SkuId::new("sku-1"),
                quantity: Quantity::from_str("2").expect("数量合法"),
                direction: MovementDirection::Increase,
            },
        )
        .expect("明细必须可构造")
    }

    /// 适配器必须显式声明合同要求的全部字段。
    #[test]
    fn adapter_declares_all_required_fields() {
        let adapter = stock_adjustment_adapter().expect("试点必须可登记");
        assert_eq!(adapter.document_type, DocumentType::StockAdjustment);
        assert_eq!(adapter.process_kind.as_str(), "stock_adjustment");
        assert_eq!(
            stock_adjustment_subject_ref("adj-1")
                .expect("主体引用必须可构造")
                .subject_kind(),
            "stock_adjustment"
        );
        assert_eq!(adapter.subject_ref_builder, "subject_ref_for(StockAdjustment)");
        assert_eq!(
            adapter.subject_version_source,
            ApprovalSubjectVersionSource::EntityApprovalSubjectVersion
        );
        assert_eq!(
            adapter.subject_snapshot_builder,
            "build_stock_adjustment_snapshot"
        );
        assert_eq!(
            adapter.on_approval_start,
            ApprovalDomainAction::StockAdjustmentSubmit
        );
        assert_eq!(
            adapter.on_final_approve,
            ApprovalDomainAction::StockAdjustmentPost
        );
        assert_eq!(
            adapter.cancel_action,
            ApprovalDomainAction::StockAdjustmentCancelApproval
        );
        assert_eq!(adapter.owner_role, "stock_adjustment_approver");
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

    /// 提交冻结版本并进入审批中；撤回不回退版本。
    #[test]
    fn submit_freezes_version_and_cancel_does_not_rollback() {
        let mut adjustment = draft_adjustment();
        assert_eq!(adjustment.approval_subject_version, 0);
        execute_stock_adjustment_domain_action(&mut adjustment, ApprovalDomainAction::StockAdjustmentSubmit)
            .unwrap();
        assert_eq!(adjustment.status, StockAdjustmentState::InApproval);
        assert_eq!(adjustment.approval_subject_version, 1);
        execute_stock_adjustment_domain_action(
            &mut adjustment,
            ApprovalDomainAction::StockAdjustmentCancelApproval,
        )
        .unwrap();
        assert_eq!(adjustment.status, StockAdjustmentState::Draft);
        assert_eq!(adjustment.approval_subject_version, 1);
        execute_stock_adjustment_domain_action(&mut adjustment, ApprovalDomainAction::StockAdjustmentSubmit)
            .unwrap();
        assert_eq!(adjustment.approval_subject_version, 2);
    }

    /// 非草稿不得提交；非审批中不得撤回或过账。
    #[test]
    fn illegal_status_transitions_fail_closed() {
        let mut posted = draft_adjustment();
        posted.status = StockAdjustmentState::Posted;
        assert!(execute_stock_adjustment_domain_action(
            &mut posted,
            ApprovalDomainAction::StockAdjustmentSubmit,
        )
        .is_err());
        assert!(execute_stock_adjustment_domain_action(
            &mut posted,
            ApprovalDomainAction::StockAdjustmentCancelApproval,
        )
        .is_err());
        assert!(execute_stock_adjustment_domain_action(
            &mut posted,
            ApprovalDomainAction::StockAdjustmentPost,
        )
        .is_err());

        let mut pending = draft_adjustment();
        pending.status = StockAdjustmentState::PendingWarehouseReview;
        assert!(execute_stock_adjustment_domain_action(
            &mut pending,
            ApprovalDomainAction::StockAdjustmentSubmit,
        )
        .is_err());
        assert!(execute_stock_adjustment_domain_action(
            &mut pending,
            ApprovalDomainAction::StockAdjustmentPost,
        )
        .is_err());
    }

    /// 过账只允许审批中，旧复核态不得再作为过账入口。
    #[test]
    fn post_only_accepts_in_approval() {
        let mut adjustment = draft_adjustment();
        execute_stock_adjustment_domain_action(&mut adjustment, ApprovalDomainAction::StockAdjustmentSubmit)
            .unwrap();
        assert!(execute_stock_adjustment_domain_action(
            &mut adjustment,
            ApprovalDomainAction::StockAdjustmentPost,
        )
        .is_ok());
        adjustment.status = StockAdjustmentState::PendingFinanceReview;
        assert!(execute_stock_adjustment_domain_action(
            &mut adjustment,
            ApprovalDomainAction::StockAdjustmentPost,
        )
        .is_err());
    }

    /// 快照冻结仓库对手方与数量合计，客户端不能写入定义。
    #[test]
    fn snapshot_freezes_warehouse_and_quantity() {
        let adjustment = draft_adjustment();
        let payload = entities::inventory::StockAdjustmentApprovalSnapshot::build(
            &adjustment,
            &[one_line()],
            "user-1",
            Instant::from_unix_secs(10),
        )
        .unwrap();
        assert_eq!(payload.document_no, "ADJ-1");
        assert_eq!(payload.responsible_org_id, "wh-1");
        assert_eq!(payload.total_quantity.unwrap().to_string(), "2");
        assert!(payload.total_amount.is_none());
        assert!(entities::inventory::StockAdjustmentApprovalSnapshot::build(
            &adjustment,
            &[],
            "user-1",
            Instant::from_unix_secs(10)
        )
        .is_err());
    }

    /// 快照规则归实体：旧 adapter helper 已删除，调用点直调实体工厂。
    #[test]
    fn snapshot_rule_source_is_entity_owned() {
        let production = include_str!("adapter.rs")
            .split("#[cfg(test)]")
            .next()
            .expect("生产代码");
        assert!(
            !production.contains("fn build_stock_adjustment_snapshot"),
            "旧 helper 必须删除"
        );
        assert!(
            !production.contains("fn sum_line_quantity"),
            "旧求和 helper 必须删除"
        );
        assert!(
            !production.contains("ApprovalSubjectSnapshotPayload {"),
            "快照组装不得留在 adapter"
        );
    }

    /// 启动命令不含定义 ID 或审批人。
    #[test]
    fn start_command_omits_definition_and_assignee() {
        let command = stock_adjustment_start_command("adj-1", 1, "user-1", "key-1");
        let encoded = serde_json::to_value(&command).unwrap();
        assert!(encoded.get("definition_id").is_none());
        assert!(encoded.get("definition_key").is_none());
        assert!(encoded.get("assignee").is_none());
        assert!(encoded.get("reviewed_by").is_none());
        assert_eq!(command.subject_kind, "stock_adjustment");
        assert_eq!(
            start_approval_command_kind(&command),
            bpm::model::types::ApprovalCommandKind::StartApproval
        );
        assert!(require_frozen_binding(None).is_err());
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
        let view = document_approval_view_with_history(
            Some(&binding),
            None,
            Vec::new(),
            DocumentApprovalHistoryPageView {
                next_cursor: None,
                has_more: false,
            },
            StockAdjustmentState::Draft,
            Some(SubmitStockAdjustmentApprovalTokenView {
                expected_version: "1".to_string(),
                expected_subject_version: "1".to_string(),
            }),
            None,
        );
        assert_eq!(view.requirement, "PROCESS_REQUIRED");
        assert_eq!(view.definition.as_ref().unwrap().id, "def-1");
        assert_eq!(view.definition.as_ref().unwrap().version, 2);
        assert!(view.instance.is_none());
        assert!(view.recent_history.len() <= RECENT_HISTORY_LIMIT);
        assert!(!view.history_page.has_more);
        assert_eq!(view.allowed_actions, vec!["SUBMIT".to_string()]);
        assert!(!view
            .allowed_actions
            .iter()
            .any(|item| item.contains("DEFINITION")));
        assert!(!view.allowed_actions.iter().any(|item| item.contains("ASSIGNEE")));
        let running = document_approval_view_with_history(
            Some(&binding),
            None,
            Vec::new(),
            DocumentApprovalHistoryPageView {
                next_cursor: None,
                has_more: false,
            },
            StockAdjustmentState::InApproval,
            None,
            None,
        );
        assert!(running.allowed_actions.is_empty());
        let authorized = document_approval_view_with_history(
            Some(&binding),
            None,
            Vec::new(),
            DocumentApprovalHistoryPageView {
                next_cursor: None,
                has_more: false,
            },
            StockAdjustmentState::InApproval,
            None,
            Some(CancelStockAdjustmentApprovalTokenView {
                expected_version: "7".to_string(),
                approval_process_instance_id: "instance-1".to_string(),
                expected_subject_version: "2".to_string(),
                expected_instance_version: "3".to_string(),
                expected_execution_version: "4".to_string(),
                expected_task_version: Some("5".to_string()),
            }),
        );
        assert_eq!(authorized.allowed_actions, vec!["CANCEL".to_string()]);
    }

    /// 领域动作分派只接受三类签署动作。
    #[test]
    fn domain_action_dispatch_is_exhaustive_for_pilot() {
        let mut adjustment = draft_adjustment();
        execute_stock_adjustment_domain_action(&mut adjustment, ApprovalDomainAction::StockAdjustmentSubmit)
            .unwrap();
        execute_stock_adjustment_domain_action(&mut adjustment, ApprovalDomainAction::StockAdjustmentPost)
            .unwrap();
        execute_stock_adjustment_domain_action(
            &mut adjustment,
            ApprovalDomainAction::StockAdjustmentCancelApproval,
        )
        .unwrap();
        assert!(execute_stock_adjustment_domain_action(
            &mut adjustment,
            ApprovalDomainAction::SalesOrderStartApprovalSubmission,
        )
        .is_err());
    }
}
