//! P6-FINAL：P0-D 后硬切换、旧路径清零、reset 演练与观测合同。
//!
//! 全部 20 个固定 DocumentType 进入新运行时。`APPROVAL_DOCUMENT_TYPE_NOT_CUT_OVER`
//! 及其临时分支必须删除。真实 Mongo / 浏览器 E2E 不作为本波次执行项。

use entities::document_registry::DocumentType;
use services::approval::binding::APPROVAL_PROCESS_NOT_CONFIGURED;
use services::approval::business_adapter::{adapter_spec_of, ensure_runtime_cut_over};
use services::approval::execution::notification_worker::{retry_backoff_secs, should_dead_letter};
use services::approval::execution::observability::{
    ApprovalRuntimeMetrics, BLOCKED_DASHBOARD, DECISION_CONFLICT_DASHBOARD, OUTBOX_DASHBOARD,
};
use services::approval::policy::{policy_of, DocumentApprovalPolicy, ALL_DOCUMENT_TYPES};
use services::approval_codes;

/// 从生产源码截取测试模块之前的实现。
///
/// # 返回
/// 返回第一个 `#[cfg(test)]` 之前的文本。
fn production(source: &str) -> &str {
    source.split("#[cfg(test)]").next().unwrap_or(source)
}

/// P0-D 后全部 PROCESS_REQUIRED 类型均可进入新运行时。
#[test]
fn all_process_required_types_are_cut_over_after_p0_d() {
    let mut cut_over = Vec::new();
    for document_type in ALL_DOCUMENT_TYPES {
        if matches!(
            policy_of(document_type).expect("政策"),
            DocumentApprovalPolicy::ProcessRequired(_)
        ) {
            assert!(ensure_runtime_cut_over(document_type).is_ok());
            assert!(adapter_spec_of(document_type).is_ok());
            cut_over.push(document_type);
        }
    }
    assert_eq!(
        cut_over,
        vec![
            DocumentType::SalesOrder,
            DocumentType::VoucherSalesOrder,
            DocumentType::SalesChangeOrder,
            DocumentType::PurchaseOrder,
            DocumentType::PurchaseChangeOrder,
            DocumentType::StockAdjustment,
            DocumentType::CustomerReceipt,
            DocumentType::SupplierPayment,
            DocumentType::CustomerRefund,
            DocumentType::SupplierRefund,
            DocumentType::ReceiptReversal,
            DocumentType::PaymentReversal,
        ]
    );
}

/// 未切换失败码与旧运行时入口必须从生产代码删除。
#[test]
fn uncut_over_gate_and_legacy_runtime_are_removed() {
    let adapter = production(include_str!("../../../services/src/approval/business_adapter.rs"));
    let action = production(include_str!("../../../services/src/approval/action.rs"));
    let errors = production(include_str!("../../../services/src/errors.rs"));
    let inventory = production(include_str!("../../../services/src/inventory/mod.rs"));
    assert!(!adapter.contains("APPROVAL_DOCUMENT_TYPE_NOT_CUT_OVER"));
    assert!(!action.contains("APPROVAL_DOCUMENT_TYPE_NOT_CUT_OVER"));
    assert!(!errors.contains("APPROVAL_DOCUMENT_TYPE_NOT_CUT_OVER"));
    assert!(!approval_codes::ALL.contains(&"APPROVAL_DOCUMENT_TYPE_NOT_CUT_OVER"));
    assert!(!action.contains("InternalApprovalRuntime"));
    assert!(!inventory.contains("CARD_SALES_APPROVAL"));
    assert!(!inventory.contains("entities::approval::"));
    assert!(adapter.contains("全部固定单据类型均已切入目标运行时"));
    assert!(!adapter.contains("APPROVAL_DOCUMENT_TYPE_NOT_CUT_OVER"));
}

/// 旧集成测试入口必须删除，不得与新入口并存。
#[test]
fn old_runtime_integration_entries_are_removed() {
    let web_api_root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    let old_api = web_api_root.join("tests/approval_runtime_api.rs");
    let old_repo = web_api_root.join("../../database/tests/approval_runtime_repository.rs");
    assert!(!old_api.exists(), "旧入口 {} 必须删除", old_api.display());
    assert!(!old_repo.exists(), "旧入口 {} 必须删除", old_repo.display());
}

/// 开发重置脚本必须 drop 新旧审批集合，并删除冲突旧索引。
#[test]
fn reset_script_drops_old_and_new_approval_collections() {
    let reset = include_str!("../../../scripts/reset-dev-business-data.mongosh.js");
    for collection in [
        "approval_step_instances",
        "approval_instances",
        "approval_step_definitions",
        "approval_definitions",
        "approval_notification_outbox",
        "approval_subject_snapshots",
        "approval_command_receipts",
        "approval_instance_assignees",
        "approval_node_executions",
        "approval_process_instances",
        "approval_transition_definitions",
        "approval_node_definitions",
        "approval_process_definitions",
    ] {
        assert!(reset.contains(collection), "{collection}");
    }
    assert!(reset.contains("DOCUMENT_APPROVAL"));
    assert!(reset.contains("uk_work_items_open_approval_step"));
    assert!(reset.contains("idx_work_items_team_pool"));
    assert!(reset.contains("禁止 dropDatabase()"));
    assert!(!reset.contains(".dropDatabase("));
}

/// 硬切换演练合同：preview 集合 allowlist 与删除清单一致，必须显式确认目标。
#[test]
fn hard_cutover_reset_preview_execute_verify_contract() {
    let shell = include_str!("../../../scripts/reset-dev-business-data.sh");
    let js = include_str!("../../../scripts/reset-dev-business-data.mongosh.js");
    let md = include_str!("../../../scripts/reset-dev-business-data.md");
    assert!(shell.contains("--execute"));
    assert!(shell.contains("--verify"));
    assert!(shell.contains("--confirm-db"));
    assert!(shell.contains("--expect-summary"));
    assert!(shell.contains("preview / execute / verify"));
    assert!(js.contains("ERP_RESET_EXECUTE"));
    assert!(js.contains("ERP_RESET_VERIFY"));
    assert!(js.contains("ERP_RESET_CONFIRMED_DB"));
    assert!(js.contains("预览完成：未执行任何写入"));
    assert!(js.contains("if (execute && confirmedDb !== dbName)"));
    assert!(md.contains("preview、execute 与 verify"));
    assert!(shell.contains("不得调用 dropDatabase()"));
    assert!(js.contains("const OLD_APPROVAL_COLLECTIONS"));
    assert!(js.contains("const NEW_APPROVAL_COLLECTIONS"));
    for collection in [
        "approval_step_instances",
        "approval_instances",
        "approval_step_definitions",
        "approval_definitions",
        "approval_notification_outbox",
        "approval_subject_snapshots",
        "approval_command_receipts",
        "approval_instance_assignees",
        "approval_node_executions",
        "approval_process_instances",
        "approval_transition_definitions",
        "approval_node_definitions",
        "approval_process_definitions",
    ] {
        assert!(
            js.contains(&format!("\"{collection}\"")),
            "{collection} 必须在 preview allowlist"
        );
    }
    assert!(js.contains("work_items"));
    assert!(!js.contains(".dropDatabase("));
}

/// 未发布定义时 12 个 PROCESS_REQUIRED 创建必须失败关闭。
#[test]
fn missing_published_definition_is_fail_closed() {
    let binding = production(include_str!("../../../services/src/approval/binding.rs"));
    assert!(binding.contains("pub const APPROVAL_PROCESS_NOT_CONFIGURED"));
    assert!(binding.contains("published_definition_or_not_configured"));
    assert_eq!(APPROVAL_PROCESS_NOT_CONFIGURED, "APPROVAL_PROCESS_NOT_CONFIGURED");
    assert!(approval_codes::ALL.contains(&approval_codes::PROCESS_NOT_CONFIGURED));
    for (label, source) in [
        (
            "SalesOrder/VoucherSalesOrder",
            production(include_str!("../../../services/src/sales_order/command.rs")),
        ),
        (
            "SalesChangeOrder",
            production(include_str!(
                "../../../services/src/sales_review/sales_change_order.rs"
            )),
        ),
        (
            "PurchaseChangeOrder",
            production(include_str!("../../../services/src/purchase_order/change.rs")),
        ),
        (
            "StockAdjustment",
            production(include_str!("../../../services/src/inventory/mod.rs")),
        ),
        (
            "CustomerReceipt",
            production(include_str!("../../../services/src/receivable/mod.rs")),
        ),
        (
            "SupplierPayment",
            production(include_str!("../../../services/src/payable/mod.rs")),
        ),
        (
            "CustomerRefund",
            production(include_str!("../../../services/src/returns/customer_refund.rs")),
        ),
        (
            "SupplierRefund",
            production(include_str!("../../../services/src/returns/supplier_refund.rs")),
        ),
        (
            "ReceiptReversal",
            production(include_str!("../../../services/src/returns/receipt_reversal.rs")),
        ),
        (
            "PaymentReversal",
            production(include_str!("../../../services/src/returns/payment_reversal.rs")),
        ),
    ] {
        assert!(
            source.contains("bind_published_definition_on_document_create"),
            "{label} 创建必须走统一绑定，无发布定义失败关闭"
        );
    }
    let purchase_create = production(include_str!(
        "../../../services/src/purchase_order/creation_basis.rs"
    ));
    assert!(purchase_create.contains("采购创建依据不存在"));
    assert!(!purchase_create.contains("bind_published_definition_on_document_create"));
}

/// 低基数观测指标存在；标签不得使用实例或用户 ID。
#[test]
fn observability_metrics_are_low_cardinality() {
    let mut metrics = ApprovalRuntimeMetrics::default();
    metrics.record_decision_conflict();
    metrics.record_idempotency_conflict();
    metrics.record_decision_latency(12);
    metrics.record_outbox_retry();
    metrics.record_outbox_dead_letter();
    assert_eq!(metrics.decision_conflicts, 1);
    assert_eq!(metrics.idempotency_conflicts, 1);
    assert_eq!(metrics.decision_count, 1);
    assert_eq!(metrics.outbox_retries, 1);
    assert_eq!(metrics.outbox_dead_letters, 1);
    assert_eq!(BLOCKED_DASHBOARD, "approval.runtime.blocked");
    assert_eq!(DECISION_CONFLICT_DASHBOARD, "approval.runtime.decision_conflicts");
    assert_eq!(OUTBOX_DASHBOARD, "approval.runtime.outbox");
    let source = include_str!("../../../services/src/approval/execution/observability.rs");
    assert!(!source.contains("instance_id"));
    assert!(!source.contains("user_id"));
}

/// outbox 第 1—5 次失败退避与第 6 次死信。
#[test]
fn outbox_backoff_matches_contract_minutes() {
    assert_eq!(
        [
            retry_backoff_secs(1),
            retry_backoff_secs(2),
            retry_backoff_secs(3),
            retry_backoff_secs(4),
            retry_backoff_secs(5),
        ],
        [Some(60), Some(300), Some(900), Some(3_600), Some(21_600)]
    );
    assert!(should_dead_letter(6));
}

/// ALL_DOCUMENT_TYPE_ROLLOUTS 成员已全部登记，P6-FINAL 只依赖 P0-D。
#[test]
fn all_document_type_rollouts_are_registered_for_final() {
    let meta = include_str!("../../../../docs/dev-plan/_meta.json");
    assert!(meta.contains("\"id\": \"P6-FINAL\""));
    assert!(meta.contains("\"dependsOn\": [\"P0-D\"]"));
    assert!(meta.contains("ALL_DOCUMENT_TYPE_ROLLOUTS"));
    for stage in [
        "P3-ADAPTER-SALES-ORDER",
        "P4-SALES-ORDER",
        "P3-ADAPTER-VOUCHER-SALES-ORDER",
        "P4-VOUCHER-SALES-ORDER",
        "P3-ADAPTER-SALES-CHANGE-ORDER",
        "P4-SALES-CHANGE-ORDER",
        "P3-ADAPTER-PURCHASE-ORDER",
        "P4-PURCHASE-ORDER",
        "P3-ADAPTER-PURCHASE-CHANGE-ORDER",
        "P4-PURCHASE-CHANGE-ORDER",
        "P3-ADAPTER-CUSTOMER-RECEIPT",
        "P4-CUSTOMER-RECEIPT",
        "P3-NO-APPROVAL-INVOICE",
        "P4-INVOICE",
        "P3-ADAPTER-SUPPLIER-PAYMENT",
        "P4-SUPPLIER-PAYMENT",
        "P3-ADAPTER-CUSTOMER-REFUND",
        "P4-CUSTOMER-REFUND",
        "P3-ADAPTER-SUPPLIER-REFUND",
        "P4-SUPPLIER-REFUND",
        "P3-ADAPTER-RECEIPT-REVERSAL",
        "P4-RECEIPT-REVERSAL",
        "P3-ADAPTER-PAYMENT-REVERSAL",
        "P4-PAYMENT-REVERSAL",
        "P3-NO-APPROVAL-SALES-RETURN-CASE",
        "P4-SALES-RETURN-CASE",
        "P3-NO-APPROVAL-PURCHASE-RETURN-ORDER",
        "P4-PURCHASE-RETURN-ORDER",
        "P3-NO-APPROVAL-PURCHASE-RECEIPT",
        "P4-PURCHASE-RECEIPT",
        "P3-NO-APPROVAL-DELIVERY",
        "P4-DELIVERY",
        "P3-NO-APPROVAL-ELECTRONIC-DELIVERY",
        "P4-ELECTRONIC-DELIVERY",
        "P3-NO-APPROVAL-SERVICE-FULFILLMENT",
        "P4-SERVICE-FULFILLMENT",
        "P3-NO-APPROVAL-CUSTOMER-ACCEPTANCE",
        "P4-CUSTOMER-ACCEPTANCE",
    ] {
        assert!(meta.contains(stage), "{stage} 必须登记");
    }
}

/// 目标路径没有全局运行开关、双写或默认办理人。
#[test]
fn no_runtime_mode_switch_or_dual_write() {
    let runtime = production(include_str!(
        "../../../services/src/approval/execution/runtime_service.rs"
    ));
    assert!(!runtime.contains("legacy_runtime"));
    assert!(!runtime.contains("FEATURE_APPROVAL"));
    assert!(!runtime.contains("Noop"));
    let binding = production(include_str!("../../../services/src/approval/binding.rs"));
    assert!(!binding.contains("fallback"));
    assert!(!binding.contains("双写"));
    assert!(!binding.contains("默认办理人"));
}

/// 生产路径旧步骤字段、责任池动作与卡券旧定义必须零命中。
#[test]
fn production_legacy_fields_and_pool_actions_are_gone() {
    let work_item = production(include_str!("../../../entities/src/work_item/entity.rs"));
    let work_item_svc = production(include_str!("../../../services/src/work_item/mod.rs"));
    let routes = production(include_str!("../src/core/routes/approval_instance.rs"));
    let action_bar =
        include_str!("../../../../erp-client/features/approval-workflow/components/approval-action-bar.tsx");
    assert!(!work_item.contains("approval_step_instance_id"));
    assert!(!work_item.contains("AssignmentMode"));
    assert!(!work_item.contains("assignment_mode"));
    assert!(!work_item_svc.contains("fn claim("));
    assert!(!work_item_svc.contains("fn start_processing("));
    assert!(!work_item_svc.contains("fn release_to_team("));
    assert!(!routes.contains("RETRY_CURRENT_STEP"));
    assert!(
        action_bar.contains("hasForbiddenWorkItemActions") || action_bar.contains("START_PROCESSING"),
        "动作栏必须拒绝领取/开始处理/退回团队"
    );
    assert!(!action_bar.contains("onClaim") && !action_bar.contains("claim("));
    assert!(action_bar.contains("allowedActions") || action_bar.contains("allowed_actions"));
}

/// 不存在全局审批运行开关；回退只允许退役定义与前向重置。
#[test]
fn rollback_is_forward_only_without_legacy_runtime_restore() {
    let runbook = include_str!("../../../../docs/runbooks/approval-workflow.md");
    assert!(runbook.contains("立即退役该类型已发布定义"));
    assert!(runbook.contains("前向部署"));
    assert!(runbook.contains("回退旧运行时"));
    assert!(runbook.contains("不得新增全局审批运行开关"));
    let runtime = production(include_str!(
        "../../../services/src/approval/execution/runtime_service.rs"
    ));
    assert!(!runtime.contains("restore_legacy"));
    assert!(!runtime.contains("ENABLE_OLD_APPROVAL"));
    assert!(!runtime.contains("legacy_runtime"));
    assert!(!runtime.contains("FEATURE_APPROVAL"));
}
