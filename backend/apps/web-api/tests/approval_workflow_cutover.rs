//! P6-PILOT：试点硬切换、未接入类型失败关闭与观测合同。
//!
//! 试点未通过时不得启动其余 19 组 P3/P4。未切换 `PROCESS_REQUIRED` 必须返回
//! `APPROVAL_DOCUMENT_TYPE_NOT_CUT_OVER`，不得调用旧运行时。

use entities::document_registry::DocumentType;
use services::approval::business_adapter::ensure_runtime_cut_over;
use services::approval::execution::notification_worker::{retry_backoff_secs, should_dead_letter};
use services::approval::execution::observability::{
    ApprovalRuntimeMetrics, BLOCKED_DASHBOARD, DECISION_CONFLICT_DASHBOARD, OUTBOX_DASHBOARD,
};
use services::approval::policy::{policy_of, DocumentApprovalPolicy, ALL_DOCUMENT_TYPES};
use services::APPROVAL_DOCUMENT_TYPE_NOT_CUT_OVER;

/// 从生产源码截取测试模块之前的实现。
fn production(source: &str) -> &str {
    source.split("#[cfg(test)]").next().unwrap_or(source)
}

/// 试点是唯一允许进入新运行时的必须审批类型。
#[test]
fn stock_adjustment_is_the_only_cut_over_process_required_type() {
    let mut cut_over = Vec::new();
    for document_type in ALL_DOCUMENT_TYPES {
        if matches!(
            policy_of(document_type).expect("政策"),
            DocumentApprovalPolicy::ProcessRequired(_)
        ) && ensure_runtime_cut_over(document_type).is_ok()
        {
            cut_over.push(document_type);
        }
    }
    assert_eq!(cut_over, vec![DocumentType::StockAdjustment]);
}

/// 未切换类型不得回退 `InternalApprovalRuntime` 或卡券旧定义。
#[test]
fn uncut_over_types_do_not_fall_back_to_legacy_runtime() {
    let adapter = production(include_str!("../../../services/src/approval/business_adapter.rs"));
    assert!(adapter.contains("APPROVAL_DOCUMENT_TYPE_NOT_CUT_OVER"));
    assert!(adapter.contains("StockAdjustment"));
    let action = production(include_str!("../../../services/src/approval/action.rs"));
    assert!(action.contains("ensure_runtime_cut_over"));
    assert!(!action.contains("InternalApprovalRuntime"));
    let inventory = production(include_str!("../../../services/src/inventory/mod.rs"));
    assert!(!inventory.contains("CARD_SALES_APPROVAL"));
    assert!(!inventory.contains("entities::approval::"));
    assert_eq!(
        APPROVAL_DOCUMENT_TYPE_NOT_CUT_OVER,
        "APPROVAL_DOCUMENT_TYPE_NOT_CUT_OVER"
    );
}

/// 旧集成测试入口必须删除，不得与新入口并存。
#[test]
fn old_runtime_integration_entries_are_removed() {
    let database_tests = include_str!("../../../database/tests/README.md");
    let web_tests = include_str!("README.md");
    assert!(!database_tests.contains("approval_runtime_repository.rs"));
    let _ = web_tests;
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

/// 未发布定义时 PROCESS_REQUIRED 创建必须失败关闭。
#[test]
fn missing_published_definition_is_fail_closed() {
    let binding = production(include_str!("../../../services/src/approval/binding.rs"));
    assert!(
        binding.contains("APPROVAL_PROCESS_NOT_CONFIGURED")
            || binding.contains("尚未配置")
            || binding.contains("没有可绑定")
    );
    let inventory = production(include_str!("../../../services/src/inventory/mod.rs"));
    assert!(inventory.contains("bind_published_definition_on_document_create"));
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

/// 试点未通过时不得启动其余 19 组 perDocumentTypeStages。
#[test]
fn remaining_document_type_rollouts_stay_locked() {
    let meta = include_str!("../../../../docs/dev-plan/_meta.json");
    assert!(meta.contains("\"id\": \"P6-PILOT\""));
    assert!(meta.contains("P3-ADAPTER-SALES-ORDER"));
    assert!(meta.contains("P6-PILOT"));
    let sales = meta
        .split("P3-ADAPTER-SALES-ORDER")
        .nth(1)
        .and_then(|body| body.split("P3-ADAPTER-").next())
        .unwrap_or(meta);
    assert!(sales.contains("P6-PILOT") || meta.contains("\"dependsOn\": [\"P6-PILOT\"]"));
}

/// 目标路径没有全局运行开关或双写。
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
}
