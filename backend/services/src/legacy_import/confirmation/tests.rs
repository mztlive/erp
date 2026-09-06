use entities::common::time::BusinessDate;
use entities::ids::{LegacyImportBatchId, LegacyImportConfirmationId, SourceSystemId, WorkItemId};
use entities::legacy_import::{LegacyImportBatchData, LegacyImportConfirmationData};

use super::*;

fn batch() -> LegacyImportBatch {
    let mut batch = LegacyImportBatch::new(
        LegacyImportBatchId::new("batch-1"),
        LegacyImportBatchData {
            batch_no: "IMP-1".to_string(),
            source_system_id: SourceSystemId::new("source-1"),
            source_object_set: "CUSTOMER,CARD_OPENING_AR".to_string(),
            baseline_date: BusinessDate::from_ymd(2026, 8, 14).unwrap(),
            import_rule_version: "rule-1".to_string(),
            source_file_hmac: None,
            status: LegacyImportBatchStatus::PendingConfirmation,
            total_rows: 1,
            success_rows: 0,
            failed_rows: 0,
            failure_code_summary: None,
            confirmation_status_summary: None,
        },
    )
    .unwrap();
    batch.base.version = 4;
    batch
}

fn confirmation() -> LegacyImportConfirmation {
    LegacyImportConfirmation::new(
        LegacyImportConfirmationId::new("confirmation-1"),
        LegacyImportConfirmationData {
            batch_id: LegacyImportBatchId::new("batch-1"),
            confirmation_scope: "SALES".to_string(),
            owner_role: "role-sales".to_string(),
            batch_version: 1,
            trial_version: 2,
            import_rule_version: "rule-1".to_string(),
            work_item_id: WorkItemId::new("work-item-1"),
        },
    )
    .unwrap()
}

fn work_item() -> WorkItem {
    let mut item = confirmation_work_item(
        WorkItemId::new("work-item-1"),
        &LegacyImportBatchId::new("batch-1"),
        LegacyImportConfirmation::subject_version(1, 2, "rule-1"),
        "SALES",
        "user-1",
    )
    .unwrap();
    item.base.version = 3;
    item
}

fn completion_command() -> PreparedConfirmationCompletion {
    PreparedConfirmationCompletion {
        work_item_id: WorkItemId::new("work-item-1"),
        batch_id: LegacyImportBatchId::new("batch-1"),
        expected_task_version: 3,
        expected_subject_version: LegacyImportConfirmation::subject_version(1, 2, "rule-1"),
        expected_batch_version: 4,
        expected_trial_version: 2,
        confirmation_scope: "SALES".to_string(),
        decision: ConfirmationDecision::ConfirmScope,
        reason_code: None,
        comment: Some("确认".to_string()),
        idempotency_key: "request-1".to_string(),
    }
}

#[test]
fn create_command_rejects_client_owned_task_fields() {
    let payload = serde_json::json!({
        "batch_id": "batch-1",
        "confirmation_scope": "SALES",
        "owner_role": "role-root",
        "batch_version": 1,
        "trial_version": 2,
        "import_rule_version": "rule-1",
        "work_item_id": "forged-task"
    });

    assert!(serde_json::from_value::<CreateLegacyImportConfirmationRequest>(payload).is_err());
}

#[test]
fn same_batch_confirmation_scopes_use_distinct_server_responsibility_keys() {
    let batch_id = LegacyImportBatchId::new("batch-1");
    let subject_version = LegacyImportConfirmation::subject_version(1, 2, "rule-1");
    let sales = confirmation_work_item(
        WorkItemId::new("work-item-sales"),
        &batch_id,
        subject_version.clone(),
        " sales ",
        "user-sales",
    )
    .unwrap();
    let procurement = confirmation_work_item(
        WorkItemId::new("work-item-procurement"),
        &batch_id,
        subject_version,
        "PROCUREMENT",
        "user-procurement",
    )
    .unwrap();

    assert_eq!(sales.business_object_id, "batch-1");
    assert_eq!(procurement.business_object_id, "batch-1");
    assert_eq!(sales.responsibility_key(), Some("SALES"));
    assert_eq!(procurement.responsibility_key(), Some("PROCUREMENT"));
    assert_eq!(sales.owner_role, "role-sales");
    assert_eq!(procurement.owner_role, "role-procurement");
}

#[test]
fn completion_requires_exact_task_subject_batch_and_current_owner() {
    let command = completion_command();
    let item = work_item();
    let confirmation = confirmation();
    let batch = batch();

    validate_confirmation_completion(&command, &item, &confirmation, &batch, "user-1").unwrap();
    assert!(validate_confirmation_completion(&command, &item, &confirmation, &batch, "other-user").is_err());
    let mut stale = command;
    stale.expected_trial_version = 3;
    assert!(validate_confirmation_completion(&stale, &item, &confirmation, &batch, "user-1").is_err());
}

#[test]
fn domain_actions_require_pending_fact_and_process_responsibility() {
    let mut mine = vec!["VIEW".to_string(), "PROCESS".to_string()];
    append_confirmation_actions(
        &mut mine,
        ConfirmationStatus::Pending,
        &[WorkItemAllowedAction::View, WorkItemAllowedAction::Process],
    );
    assert_eq!(mine, ["VIEW", "PROCESS", "CONFIRM_SCOPE", "RETURN_FOR_FIX"]);

    let mut view_only = vec!["VIEW".to_string()];
    append_confirmation_actions(
        &mut view_only,
        ConfirmationStatus::Pending,
        &[WorkItemAllowedAction::View],
    );
    assert_eq!(view_only, ["VIEW"]);

    let mut completed = vec!["VIEW".to_string(), "PROCESS".to_string()];
    append_confirmation_actions(
        &mut completed,
        ConfirmationStatus::Confirmed,
        &[WorkItemAllowedAction::View, WorkItemAllowedAction::Process],
    );
    assert_eq!(completed, ["VIEW", "PROCESS"]);
}

#[test]
fn unauthorized_projection_masks_current_owner_and_has_no_actions() {
    let view = read_only_work_item_view(&work_item());

    assert_eq!(view.owner_user_id, None);
    assert!(view.allowed_actions.is_empty());
    assert_eq!(view.action_blockers, ["当前账号不在该责任范围，任务仅可查看。"]);
}

#[test]
fn return_for_fix_is_rejected_without_successor() {
    let next = confirmation_next_step(ConfirmationMatrixDecision::FixAndRevalidate);

    assert_eq!(next, ImportBusinessConfirmationNextStep::FixAndRevalidate);
    assert_eq!(
        confirmation_result_status(ConfirmationDecision::ReturnForFix),
        ImportBusinessConfirmationResultStatus::Rejected
    );
}

#[test]
fn last_confirmation_prepares_batch_without_starting_application() {
    let next = confirmation_next_step(ConfirmationMatrixDecision::StartApply);
    let mut import_batch = batch();
    if next == ImportBusinessConfirmationNextStep::StartApply {
        import_batch
            .advance(LegacyImportBatchStatus::ReadyToApply)
            .unwrap();
    }

    assert_eq!(next, ImportBusinessConfirmationNextStep::StartApply);
    assert_eq!(import_batch.status, LegacyImportBatchStatus::ReadyToApply);
}

#[test]
fn replaced_work_item_ids_batch_single_query_semantics() {
    fn replaced(id: &str, scope: &str, trial: u32, work_item: &str) -> LegacyImportConfirmation {
        LegacyImportConfirmation::new(
            LegacyImportConfirmationId::new(id),
            LegacyImportConfirmationData {
                batch_id: LegacyImportBatchId::new("batch-1"),
                confirmation_scope: scope.to_string(),
                owner_role: "role-sales".to_string(),
                batch_version: 1,
                trial_version: trial,
                import_rule_version: "rule-1".to_string(),
                work_item_id: WorkItemId::new(work_item),
            },
        )
        .unwrap()
    }
    let confirmations = vec![
        replaced("c-1", "SALES", 1, "work-item-1"),
        replaced("c-2", "PROCUREMENT", 1, "work-item-2"),
        replaced("c-3", "OPERATIONS", 2, "work-item-1"),
        replaced("c-4", "WAREHOUSE", 3, "work-item-4"),
    ];
    let ids = replaced_confirmation_work_item_ids(&confirmations, 3);
    assert_eq!(ids.len(), 2);
    assert_eq!(ids[0].to_string(), "work-item-1");
    assert_eq!(ids[1].to_string(), "work-item-2");
    assert!(replaced_confirmation_work_item_ids(&confirmations, 1).is_empty());
    assert!(replaced_confirmation_work_item_ids(&[], 3).is_empty());
}

#[test]
fn superseded_closable_skips_closed_and_fails_closed_on_missing() {
    use std::collections::HashMap;
    fn invalidated(id: &str, scope: &str, work_item: &str) -> LegacyImportConfirmation {
        let mut confirmation = LegacyImportConfirmation::new(
            LegacyImportConfirmationId::new(id),
            LegacyImportConfirmationData {
                batch_id: LegacyImportBatchId::new("batch-1"),
                confirmation_scope: scope.to_string(),
                owner_role: "role-sales".to_string(),
                batch_version: 1,
                trial_version: 1,
                import_rule_version: "rule-1".to_string(),
                work_item_id: WorkItemId::new(work_item),
            },
        )
        .unwrap();
        confirmation
            .invalidate(
                LegacyImportConfirmationId::new("c-new"),
                Instant::from_unix_secs(1_700_000_000),
            )
            .unwrap();
        confirmation
    }
    let batch_id = LegacyImportBatchId::new("batch-1");
    let subject = LegacyImportConfirmation::subject_version(1, 1, "rule-1");
    let open = confirmation_work_item(
        WorkItemId::new("work-item-1"),
        &batch_id,
        subject.clone(),
        "SALES",
        "user-1",
    )
    .unwrap();
    let mut closed = confirmation_work_item(
        WorkItemId::new("work-item-2"),
        &batch_id,
        subject,
        "PROCUREMENT",
        "user-2",
    )
    .unwrap();
    closed
        .close(
            "user-2",
            WorkItemCloseData {
                close_reason: "SUPERSEDED_BY_NEW_IMPORT_TRIAL".to_string(),
            },
            Instant::from_unix_secs(1_700_000_001),
        )
        .unwrap();
    let confirmations = vec![
        invalidated("c-1", "SALES", "work-item-1"),
        invalidated("c-2", "PROCUREMENT", "work-item-2"),
    ];
    let mut map = HashMap::new();
    map.insert(open.base.id.clone(), open);
    map.insert(closed.base.id.clone(), closed);
    let replacement = LegacyImportConfirmationId::new("c-new");
    let to_close = collect_superseded_closable_work_items(&confirmations, 3, &mut map, &replacement).unwrap();
    assert_eq!(to_close.len(), 1);
    assert_eq!(to_close[0].base.id, "work-item-1");
    assert!(map.is_empty());

    let confirmations = vec![invalidated("c-1", "SALES", "work-item-missing")];
    let mut empty = HashMap::new();
    assert!(collect_superseded_closable_work_items(&confirmations, 3, &mut empty, &replacement).is_err());
}

#[test]
fn idempotency_receipt_rejects_same_key_with_different_command() {
    let identity = confirmation_command_identity("user-1", "complete", &completion_command());
    let fingerprint = identity.fingerprint().to_string();
    let receipt = ConfirmationCompletionReceipt {
        result_status: ImportBusinessConfirmationResultStatus::Confirmed,
        task_version: 4,
        batch_version: 5,
        next_step: ImportBusinessConfirmationNextStep::AwaitOtherConfirmations,
    };
    let message = confirmation_completion_receipt_message(&fingerprint, receipt);

    assert_eq!(
        parse_confirmation_completion_receipt(&message, &fingerprint).unwrap(),
        receipt
    );
    assert!(parse_confirmation_completion_receipt(&message, &"0".repeat(64)).is_err());
    assert!(!identity.audit_id().contains("request-1"));
}
