use erp_integration::dto::{
    IntegrationActionOutcome, IntegrationItemType, IntegrationNonTerminalTaskAction,
    IntegrationTaskActionCommand, IntegrationTaskActionKind,
};

use super::{command_identity, next_allowed_actions};

fn command(key: &str, kind: IntegrationTaskActionKind) -> IntegrationTaskActionCommand {
    IntegrationTaskActionCommand {
        work_item_id: "wi-1".to_string(),
        expected_task_version: "1".to_string(),
        expected_subject_version: "1".to_string(),
        action: IntegrationNonTerminalTaskAction {
            item_type: IntegrationItemType::ErrorTask,
            item_id: "task-1".to_string(),
            kind,
            operation_id: "op-1".to_string(),
            reason_code: None,
            comment: None,
            evidence_refs: Vec::new(),
        },
        idempotency_key: key.to_string(),
    }
}

#[test]
fn receipt_never_contains_raw_idempotency_key() {
    let command = command("raw-secret-key", IntegrationTaskActionKind::QueryOriginalResult);
    let receipt = command_identity(
        "actor-1",
        super::TASK_ACTION_AUDIT,
        "work_item",
        "wi-1",
        &command.idempotency_key,
        &command,
    )
    .unwrap();

    assert!(!receipt.receipt_id().contains("raw-secret-key"));
    assert_eq!(receipt.resource_id(), "wi-1");
    assert_eq!(receipt.fingerprint().len(), 64);
}

#[test]
fn terminal_evidence_allows_explicit_resolve_without_completing_action() {
    let actions =
        next_allowed_actions(IntegrationItemType::ErrorTask, IntegrationActionOutcome::TerminalEvidenceFound);
    assert!(actions.iter().any(|action| action == "RESOLVE"));

    let actions = next_allowed_actions(
        IntegrationItemType::ReconciliationDifference,
        IntegrationActionOutcome::TerminalEvidenceFound,
    );
    assert!(actions.iter().any(|action| action == "RESOLVE"));
}

#[test]
fn confirmed_no_result_allows_replay_only_for_error_task() {
    let actions =
        next_allowed_actions(IntegrationItemType::ErrorTask, IntegrationActionOutcome::NoResultConfirmed);
    assert!(actions.iter().any(|action| action == "REPLAY_ORIGINAL"));

    let actions = next_allowed_actions(
        IntegrationItemType::ReconciliationDifference,
        IntegrationActionOutcome::NoResultConfirmed,
    );
    assert!(!actions.iter().any(|action| action == "REPLAY_ORIGINAL"));
}

/// 生产代码（测试模块之前部分），供分层守卫断言，避免字面量自匹配。
///
/// # 返回
/// 返回去掉测试模块后的生产代码全文。
fn production_source() -> &'static str {
    concat!(
        include_str!("mod.rs"),
        include_str!("action.rs"),
        include_str!("complete.rs"),
        include_str!("direct.rs"),
        include_str!("guard.rs"),
        include_str!("../../../../erp-integration/src/service/task_decision/mod.rs"),
        include_str!("../../../../erp-integration/src/service/task_decision/action.rs"),
        include_str!("../../../../erp-integration/src/service/task_decision/complete.rs"),
        include_str!("../../../../erp-integration/src/service/task_decision/direct.rs"),
        include_str!("../../../../erp-integration/src/service/task_decision/guard.rs"),
    )
    .split("mod tests {")
    .next()
    .expect("必须存在生产代码")
}

/// 分层守卫（INT-E22）：版本只由 Prepared DTO 单次解析，服务只做 typed 比较。
///
/// 锁定服务不再对版本字符串二次解析；typed 版本与规范化字段来自 Prepared 目标。
#[test]
fn versions_flow_typed_from_prepared_without_reparse() {
    let source = production_source();
    assert!(!source.contains("parse::<u64>()"));
    assert!(source.contains("PreparedWorkItemTarget::try_from(&command)"));
    assert!(source.contains("PreparedDirectDecisionTarget::try_from(&command)"));
    assert!(source.contains("target.task_version"));
    assert!(source.contains("prepared.difference_version"));
}

/// 分层守卫（INT-E23）：结论映射与序号追加归领域，服务只做编排与结果映射。
///
/// 锁定旧派生源（`completion_as_action`、`build_resolution`、结论三元组）
/// 已删除；动作/状态派生走领域结论映射与追加工厂。
#[test]
fn conclusion_and_append_are_owned_by_domain() {
    let source = production_source();
    assert!(!source.contains("fn completion_as_action"));
    assert!(!source.contains("fn build_resolution"));
    assert!(!source.contains("ConfirmValidDifference,\n"));
    assert!(source.contains("DirectConclusion::from("));
    assert!(source.contains("ReconciliationDifferenceResolution::append("));
    assert!(source.contains("as_non_terminal_action()"));
}
