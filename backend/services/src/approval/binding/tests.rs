use std::collections::HashMap;

use bpm::ids::{ApprovalCommandReceiptId, ApprovalProcessDefinitionId};
use bpm::model::{ApprovalCommandReceipt, IdempotencyKey, Timestamp};
use database::repository::bpm::DefinitionGraph;
use entities::access_control::{DataScope, DataScopeData, DataScopeSubjectType, DataScopeType};
use entities::document_registry::workflow_action::ApprovalBindingActionContext;
use entities::document_registry::{
    BusinessDocumentId, DocumentType, WorkflowAction, WorkflowActionData, WorkflowActionId,
    WorkflowActionType,
};
use entities::ids::DataScopeId;
use entities::{AccountCore, AccountCoreData, AccountKind, AccountStatus, LoginAccount, Secret};

use crate::approval::business_adapter::ensure_runtime_cut_over;
use crate::approval::execution::upgrade_binding_identity;
use crate::approval::policy::{policy_of, ApprovalRequirement, ALL_DOCUMENT_TYPES};
use crate::approval::upgrade_subject::ApprovalUpgradeSubjectFacts;
use crate::audit::AuditActor;
use crate::document_registry::new_registered_document;
use crate::errors::{Error, ErrorCode};

use super::super::business_adapter::BindingRevalidationContext;
use super::revalidate::{
    group_role_scope_facts, require_granting_role_ids, require_ready_assignee,
    require_static_decide_permission, revalidate_assignee_binding_access_by_role, revalidate_published_graph,
};
use super::upgrade::{ensure_registered_upgrade_subject, upgrade_result_from_action};
use super::*;

fn production_source() -> String {
    fn production_part(source: &str) -> &str {
        source.split("#[cfg(test)]").next().expect("必须存在生产代码")
    }
    [
        production_part(include_str!("mod.rs")),
        production_part(include_str!("types.rs")),
        production_part(include_str!("bind.rs")),
        production_part(include_str!("upgrade.rs")),
        production_part(include_str!("revalidate.rs")),
    ]
    .concat()
}

/// 构造审批人账号快照。
fn assignee_account(id: &str, status: AccountStatus) -> AccountCore {
    AccountCore::new(
        id.to_string(),
        AccountCoreData {
            secret: Secret::new(LoginAccount::new(format!("login-{id}")).unwrap(), "password123").unwrap(),
            name: id.to_string(),
            kind: AccountKind::Admin,
            status,
            email: None,
            phone: None,
            avatar: None,
        },
    )
    .unwrap()
}

/// 构造角色组织范围事实。
fn role_scope(id: &str, role_id: &str, organization_id: &str) -> DataScope {
    DataScope::new(
        DataScopeId::new(id),
        DataScopeData {
            subject_type: DataScopeSubjectType::Role,
            subject_id: role_id.to_string(),
            scope_type: DataScopeType::Organization,
            scope_targets: vec![organization_id.to_string()],
        },
    )
    .unwrap()
}

/// 构造严格回读单测命令。
fn upgrade_command() -> UpgradeUnsubmittedDefinitionCommand {
    let identity = upgrade_binding_identity(
        DocumentType::StockAdjustment.as_str(),
        "adjustment-1",
        7,
        1,
        "升级至当前发布定义",
        "admin-1",
        IdempotencyKey::parse("upgrade-key-1").unwrap(),
    )
    .unwrap();
    UpgradeUnsubmittedDefinitionCommand {
        document_type: DocumentType::StockAdjustment,
        document_id: "adjustment-1".to_string(),
        expected_business_object_version: 7,
        expected_binding_version: 1,
        reason: "升级至当前发布定义".to_string(),
        identity,
        action_id: WorkflowActionId::new("action-1"),
        receipt_id: ApprovalCommandReceiptId::new("receipt-1"),
    }
}

/// 构造收据指向的不可变升级动作。
fn upgrade_action(command: &UpgradeUnsubmittedDefinitionCommand) -> WorkflowAction {
    WorkflowAction::new_with_approval_binding_context(
        command.action_id.clone(),
        WorkflowActionData {
            document_id: BusinessDocumentId::new(command.document_id.clone()),
            action_type: WorkflowActionType::ApprovalDefinitionUpgraded,
            from_status: "DRAFT".to_string(),
            to_status: "DRAFT".to_string(),
            actor_id: "admin-1".to_string(),
            actor_role: "role-definition-admin".to_string(),
            comment: Some(command.reason.clone()),
        },
        ApprovalBindingActionContext {
            previous_definition_id: ApprovalProcessDefinitionId::new("definition-1"),
            previous_definition_version: 1,
            previous_binding_version: 1,
            current_definition_id: ApprovalProcessDefinitionId::new("definition-2"),
            current_definition_version: 2,
            current_binding_version: 2,
            business_object_version: 7,
        },
    )
    .unwrap()
}

/// 绑定政策：无审批跳过，必须审批要求发布定义。
#[test]
fn binding_policy_skips_no_approval_and_requires_published() {
    for document_type in ALL_DOCUMENT_TYPES {
        let policy = policy_of(document_type).expect("政策必须存在");
        match policy.requirement() {
            ApprovalRequirement::NoApproval => {
                assert_eq!(
                    binding_decision(policy.requirement()),
                    BindingDecision::SkipNoApproval
                );
            }
            ApprovalRequirement::ProcessRequired => {
                assert_eq!(
                    binding_decision(policy.requirement()),
                    BindingDecision::RequirePublished
                );
            }
        }
    }
}

/// 同载荷结果必须完全由收据指向的不可变动作重建。
#[test]
fn upgrade_result_is_rebuilt_from_strict_action_proof() {
    let command = upgrade_command();
    let actor = AuditActor::new("admin-1".to_string(), "admin-1".to_string(), AccountKind::Admin);
    let action = upgrade_action(&command);
    let receipt = ApprovalCommandReceipt::new(
        command.receipt_id.clone(),
        command.identity.current(),
        action.base.id.clone(),
        Timestamp::from_unix_secs(i64::try_from(action.base.created_at).unwrap()).unwrap(),
    )
    .unwrap();

    let view = upgrade_result_from_action(
        &command,
        &actor,
        &command.reason,
        &receipt,
        &action,
        UpgradeBindingOutcome::Replay,
    )
    .expect("完整动作证明必须可回读");

    assert_eq!(view.document_type, DocumentType::StockAdjustment);
    assert_eq!(view.document_id, "adjustment-1");
    assert_eq!(view.original_business_object_version, "7");
    assert_eq!(view.new_binding.approval_process_definition_id, "definition-2");
    assert_eq!(view.new_binding.approval_binding_version, "2");
    assert_eq!(view.action_id, "action-1");
    assert_eq!(view.outcome, UpgradeBindingOutcome::Replay);

    let mut corrupt = action.clone();
    corrupt.comment = Some("被篡改的原因".to_string());
    assert!(upgrade_result_from_action(
        &command,
        &actor,
        &command.reason,
        &receipt,
        &corrupt,
        UpgradeBindingOutcome::Replay,
    )
    .is_err());
}

/// 生产编排必须在当前授权后分流，Fresh 内的收据是第一物理写。
#[test]
fn upgrade_orchestration_is_authorized_replay_and_receipt_first() {
    let production = production_source();
    let upgrade = production
        .split("pub async fn upgrade_unsubmitted_document_definition")
        .nth(1)
        .expect("必须存在升级端口")
        .split("/// 在 unknown/duplicate 恢复的新事务中")
        .next()
        .unwrap();
    assert!(
        upgrade.find("load_authorized_upgrade_context").unwrap()
            < upgrade.find("find_upgrade_receipt").unwrap()
    );
    assert!(
        upgrade.find("ReceiptBranch::SamePayload").unwrap()
            < upgrade.find("ensure_expected_business_object_version").unwrap()
    );
    assert!(!upgrade.contains("NoTransaction"));

    let authorization = production
        .split("async fn load_authorized_upgrade_context")
        .nth(1)
        .expect("必须存在升级授权上下文")
        .split("/// 证明运行层传入的身份")
        .next()
        .unwrap();
    assert!(
        authorization.find("load_approval_upgrade_subject_facts").unwrap()
            < authorization.find("ensure_active_upgrade_actor").unwrap()
    );
    assert!(
        authorization.find("ensure_active_upgrade_actor").unwrap()
            < authorization
                .find("approval_binding_upgrade_authorization_with_executor")
                .unwrap()
    );

    let recovery = production
        .split("pub async fn replay_unsubmitted_document_definition_upgrade")
        .nth(1)
        .expect("必须存在只读恢复端口")
        .split("/// 得到命令签署与不可变动作共用的规范化原因")
        .next()
        .unwrap();
    assert!(
        recovery.find("load_authorized_upgrade_context").unwrap()
            < recovery.find("find_upgrade_receipt").unwrap()
    );
    assert!(recovery.contains("ReceiptBranch::Fresh => Ok(None)"));
    assert!(!recovery.contains("apply_fresh_upgrade"));
    assert!(!recovery.contains("insert_command_receipt"));

    let fresh = production
        .split("async fn apply_fresh_upgrade")
        .nth(1)
        .expect("必须存在 Fresh 编排")
        .split("/// 注册投影必须")
        .next()
        .unwrap();
    let receipt_write = fresh.find("insert_command_receipt").unwrap();
    assert!(fresh.find("ApprovalCommandReceipt::new").unwrap() < receipt_write);
    assert!(fresh.find("upgrade_result_from_action").unwrap() < receipt_write);
    assert!(receipt_write < fresh.find("business_documents().update").unwrap());
    assert!(receipt_write < fresh.find("workflow_actions().create").unwrap());
    assert!(receipt_write < fresh.find("audit_logs().create").unwrap());
    assert!(!fresh.contains("outbox"));
}

/// 单号只在注册与强实体两端均有值时作为一致性证明。
#[test]
fn upgrade_registry_identity_allows_one_sided_empty_document_number() {
    let document = new_registered_document("adjustment-1", DocumentType::StockAdjustment, "").unwrap();
    let facts = ApprovalUpgradeSubjectFacts {
        document_type: DocumentType::StockAdjustment,
        document_id: "adjustment-1".to_string(),
        business_object_version: 1,
        document_no: "ADJ-1".to_string(),
        responsible_org_id: "org-1".to_string(),
        creator_id: "creator-1".to_string(),
    };
    assert!(ensure_registered_upgrade_subject(&document, &facts).is_ok());

    let conflicting =
        new_registered_document("adjustment-1", DocumentType::StockAdjustment, "ADJ-OTHER").unwrap();
    assert!(ensure_registered_upgrade_subject(&conflicting, &facts).is_err());
}

/// 缺失发布定义失败关闭。
#[test]
fn missing_published_definition_fails_closed() {
    let error = published_definition_or_not_configured::<()>(None).unwrap_err();
    assert_eq!(
        error.to_string(),
        ErrorCode::ApprovalProcessNotConfigured.as_str()
    );
    assert!(published_definition_or_not_configured(Some(1)).is_ok());
}

/// BPM 确认已发布线性图；草稿/退役映射为未配置，损坏发布图不得被仓储过滤放过。
#[test]
fn published_graph_revalidation_uses_bpm_and_maps_configuration_errors() {
    let at = bpm::Timestamp::from_unix_secs(1).unwrap();
    let mut definition = bpm::model::ApprovalProcessDefinition::new_draft(
        bpm::ids::ApprovalProcessDefinitionId::new("def"),
        bpm::ProcessKind::StockAdjustment,
        1,
        "库存调整",
        "n1",
        bpm::ParticipantId::new("admin").unwrap(),
        at,
    )
    .unwrap();
    let graph = DefinitionGraph {
        definition: definition.clone(),
        nodes: Vec::new(),
        transitions: Vec::new(),
    };
    let draft_error = revalidate_published_graph(&graph).unwrap_err();
    assert_eq!(
        draft_error.to_string(),
        ErrorCode::ApprovalProcessNotConfigured.as_str()
    );

    definition
        .publish(bpm::ParticipantId::new("admin").unwrap(), at)
        .unwrap();
    let published_corrupt = DefinitionGraph {
        definition,
        nodes: Vec::new(),
        transitions: Vec::new(),
    };
    let corrupt = revalidate_published_graph(&published_corrupt).unwrap_err();
    assert_ne!(
        corrupt.to_string(),
        ErrorCode::ApprovalProcessNotConfigured.as_str()
    );

    let production = production_source();
    let loader = production
        .split("async fn load_published_graph")
        .nth(1)
        .and_then(|body| body.split("fn revalidate_published_graph").next())
        .expect("加载函数");
    assert!(loader.contains("load_published_definition_graph"));
    assert!(!loader.contains("load_definition_graph"));
    assert!(!loader.contains("find_published_by_process_kind"));
}

/// 全部必须审批类型进入目标运行时。
#[test]
fn process_required_types_are_cut_over() {
    assert!(ensure_runtime_cut_over(DocumentType::StockAdjustment).is_ok());
    assert!(ensure_runtime_cut_over(DocumentType::PurchaseOrder).is_ok());
}

/// 20 个 DocumentType 的 BusinessDocument 注册清点。
#[test]
fn business_document_registration_inventory() {
    const ROWS: &[(DocumentType, &str, &str)] = &[
        (
            DocumentType::SalesOrder,
            "已注册",
            "backend/services/src/sales_order/command.rs:170",
        ),
        (
            DocumentType::VoucherSalesOrder,
            "已注册(共用入口，类型分派属销售单子阶段)",
            "backend/services/src/sales_order/command.rs:170",
        ),
        (
            DocumentType::SalesChangeOrder,
            "待子阶段补齐",
            "backend/services/src/sales_review/sales_change_order.rs:139",
        ),
        (
            DocumentType::PurchaseOrder,
            "本阶段新增",
            "backend/services/src/purchase_order/draft_from_confirmation.rs",
        ),
        (
            DocumentType::PurchaseChangeOrder,
            "本阶段新增",
            "backend/services/src/purchase_order/change.rs:38",
        ),
        (
            DocumentType::StockAdjustment,
            "本阶段新增",
            "backend/services/src/inventory/mod.rs:492",
        ),
        (
            DocumentType::CustomerReceipt,
            "待子阶段补齐",
            "backend/services/src/receivable/mod.rs:735",
        ),
        (
            DocumentType::SupplierPayment,
            "本阶段新增",
            "backend/services/src/payable/mod.rs:288",
        ),
        (
            DocumentType::CustomerRefund,
            "本阶段新增",
            "backend/services/src/returns/customer_refund.rs:113",
        ),
        (
            DocumentType::SupplierRefund,
            "本阶段新增",
            "backend/services/src/returns/supplier_refund.rs:50",
        ),
        (
            DocumentType::ReceiptReversal,
            "本阶段新增",
            "backend/services/src/returns/receipt_reversal.rs:33",
        ),
        (
            DocumentType::PaymentReversal,
            "本阶段新增",
            "backend/services/src/returns/payment_reversal.rs:29",
        ),
        (
            DocumentType::PurchaseReceipt,
            "本阶段新增",
            "backend/services/src/fulfillment/purchase_receipt.rs:122",
        ),
        (
            DocumentType::Delivery,
            "本阶段新增",
            "backend/services/src/fulfillment/delivery.rs:131",
        ),
        (
            DocumentType::ElectronicDelivery,
            "本阶段新增",
            "backend/services/src/fulfillment/electronic_delivery.rs:102",
        ),
        (
            DocumentType::ServiceFulfillment,
            "本阶段新增",
            "backend/services/src/fulfillment/service_fulfillment.rs:101",
        ),
        (
            DocumentType::CustomerAcceptance,
            "本阶段新增",
            "backend/services/src/fulfillment/customer_acceptance.rs:132",
        ),
        (
            DocumentType::Invoice,
            "待子阶段补齐",
            "backend/services/src/receivable/mod.rs:995",
        ),
        (
            DocumentType::SalesReturnCase,
            "本阶段新增",
            "backend/services/src/returns/sales_return.rs:88",
        ),
        (
            DocumentType::PurchaseReturnOrder,
            "本阶段新增",
            "backend/services/src/returns/purchase_return.rs:90",
        ),
    ];
    assert_eq!(ROWS.len(), 20);
    let mut seen = std::collections::HashSet::new();
    for (document_type, status, entry) in ROWS {
        assert!(!status.is_empty());
        assert!(entry.contains("backend/services/src/"));
        assert!(seen.insert(*document_type));
        let _ = new_registered_document("id-1", *document_type, "").expect("空编号草稿可注册");
    }
}

/// bind/upgrade 生产路径必须走用户+角色范围与 Adapter 读取权闸门。
#[test]
fn bind_and_upgrade_use_shared_scope_and_read_gate() {
    let production = production_source();
    assert!(production.contains("revalidate_assignee_binding_access"));
    assert!(production.contains("load_assignee_scope_sets"));
    assert_eq!(
        production.matches(".list_by_ids(assignee_ids, executor)").count(),
        1
    );
    assert!(production.contains(".list_by_subjects("));
    assert!(!production.contains(".find_approval_assignee_by_id(user_id, executor)"));
    assert!(production.contains("DataScopeSubjectType::Role"));
    let role_loader = production
        .split("async fn load_enabled_decide_role_scopes")
        .nth(1)
        .expect("必须存在授权角色范围加载器")
        .split("/// 筛出实际授予静态审批权限的启用角色")
        .next()
        .unwrap();
    assert!(
        role_loader.find("load_enabled_role_ids").unwrap()
            < role_loader.find("permission_granting_role_ids").unwrap()
    );
    assert!(role_loader.contains("&granting_role_ids"));
    assert!(!production.contains("ensure_binding_scope(&spec, &scopes, &context.organization_id, true)"));
    let spec = crate::approval::business_adapter::adapter_spec_of(DocumentType::StockAdjustment)
        .expect("试点必须有适配器");
    let context = BindingRevalidationContext {
        organization_id: "org-1".to_string(),
        creator_id: "creator-1".to_string(),
    };
    let error = crate::approval::business_adapter::revalidate_assignee_binding_access(
        &spec,
        &[],
        &[],
        &context,
        "u1",
    )
    .unwrap_err();
    assert!(error.to_string().contains("数据范围不覆盖当前单据组织"));
}

/// 批量账号映射必须按 BPM 审批人顺序查表，不得依赖 Repository 返回顺序。
#[test]
fn assignee_account_map_preserves_bpm_validation_order() {
    let accounts = [
        assignee_account("u1", AccountStatus::Active),
        assignee_account("u2", AccountStatus::Active),
    ]
    .into_iter()
    .map(|account| (account.base.id.clone(), account))
    .collect::<HashMap<_, _>>();
    let assignee_ids = ["u2", "u1"];

    let ordered = assignee_ids
        .iter()
        .map(|user_id| {
            require_ready_assignee(accounts.get(*user_id))
                .unwrap()
                .base
                .id
                .as_str()
        })
        .collect::<Vec<_>>();

    assert_eq!(ordered, assignee_ids);
}

/// 缺失与停用审批人必须保留同一精确校验错误。
#[test]
fn missing_and_inactive_assignees_keep_exact_error() {
    let expected = "指定审批人账号不存在、已停用或任职失效";
    let missing = require_ready_assignee(None).unwrap_err();
    let inactive = assignee_account("u1", AccountStatus::Suspended);
    let inactive = require_ready_assignee(Some(&inactive)).unwrap_err();

    assert!(matches!(missing, Error::ValidationError(message) if message == expected));
    assert!(matches!(inactive, Error::ValidationError(message) if message == expected));
}

/// 逐用户 RBAC 失败必须保留绑定阶段的精确错误，不得借用定义期文案。
#[test]
fn missing_static_decide_permission_keeps_binding_error() {
    let error = require_static_decide_permission(false).unwrap_err();

    assert!(matches!(
        error,
        Error::ValidationError(message) if message == "指定审批人缺少审批权限"
    ));
    assert!(require_static_decide_permission(true).is_ok());
}

/// 权限角色与范围角色不同时必须失败关闭，禁止跨角色拼接授权。
#[test]
fn permission_and_scope_from_different_roles_cannot_be_combined() {
    let role_scope_sets = group_role_scope_facts(
        &["role-with-permission".to_string()],
        vec![role_scope("scope-b", "role-with-scope", "org-1")],
    );
    let spec = crate::approval::business_adapter::adapter_spec_of(DocumentType::StockAdjustment)
        .expect("试点必须有适配器");
    let context = BindingRevalidationContext {
        organization_id: "org-1".to_string(),
        creator_id: "creator-1".to_string(),
    };

    let error =
        revalidate_assignee_binding_access_by_role(&spec, &[], &role_scope_sets, &context, "u1").unwrap_err();

    assert!(matches!(
        error,
        Error::ValidationError(message) if message == "审批人数据范围不覆盖当前单据组织"
    ));
    assert!(role_scope_sets[0].0.is_empty());
}

/// 已停用授权角色被过滤后不得靠账号级残留权限继续放行。
#[test]
fn no_enabled_granting_role_fails_with_exact_permission_error() {
    let error = require_granting_role_ids(Vec::new()).unwrap_err();

    assert!(matches!(
        error,
        Error::ValidationError(message) if message == "指定审批人缺少审批权限"
    ));
}

/// 草稿允许空编号；正式号原样登记。
#[test]
fn draft_allows_empty_document_no() {
    let empty = new_registered_document("po-1", DocumentType::PurchaseOrder, "   ").unwrap();
    assert!(empty.document_no.is_empty());
    let numbered = new_registered_document("adj-1", DocumentType::StockAdjustment, " ADJ-1 ").unwrap();
    assert_eq!(numbered.document_no, "ADJ-1");
    assert_eq!(numbered.base.id, "adj-1");
}
