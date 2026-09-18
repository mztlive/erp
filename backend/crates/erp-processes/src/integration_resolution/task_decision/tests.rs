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
