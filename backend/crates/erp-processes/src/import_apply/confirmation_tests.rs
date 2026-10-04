use erp_core::common::time::{BusinessDate, Instant};
use erp_core::ids::{LegacyImportBatchId, LegacyImportConfirmationId, SourceSystemId, WorkItemId};
use erp_import::entity::command_receipt::{
    ImportCommandReceipt, ImportCommandResult, ImportConfirmationOutcome,
};
use erp_import::{
    ConfirmationDecision, ConfirmationMatrixDecision, ConfirmationStatus,
    CreateLegacyImportConfirmationRequest, ImportBusinessConfirmationNextStep,
    ImportBusinessConfirmationResultStatus, LegacyImportBatch, LegacyImportBatchData,
    LegacyImportBatchStatus, LegacyImportConfirmation, LegacyImportConfirmationData,
    PreparedConfirmationCompletion,
};
use erp_workflow::entity::work_item::{WorkItem, WorkItemCloseData, WorkItemStatus};
use erp_workflow::service::work_item::WorkItemAllowedAction;

use super::complete::{
    ConfirmationCompletionReceipt, confirmation_command_identity, confirmation_result_status,
    validate_confirmation_completion, validate_confirmation_replay,
};
use super::confirmation_query::{append_confirmation_actions, read_only_work_item_view};
use super::create_confirmation::confirmation_next_step;
use super::supersede::{collect_superseded_closable_work_items, replaced_confirmation_work_item_ids};

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
    let mut item = super::factories::confirmation_work_item(
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
    let sales = super::factories::confirmation_work_item(
        WorkItemId::new("work-item-sales"),
        &batch_id,
        subject_version.clone(),
        " sales ",
        "user-sales",
    )
    .unwrap();
    let procurement = super::factories::confirmation_work_item(
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
    append_confirmation_actions(&mut view_only, ConfirmationStatus::Pending, &[WorkItemAllowedAction::View]);
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
        import_batch.advance(LegacyImportBatchStatus::ReadyToApply).unwrap();
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
            .invalidate(LegacyImportConfirmationId::new("c-new"), Instant::from_unix_secs(1_700_000_000))
            .unwrap();
        confirmation
    }
    let batch_id = LegacyImportBatchId::new("batch-1");
    let subject = LegacyImportConfirmation::subject_version(1, 1, "rule-1");
    let open = super::factories::confirmation_work_item(
        WorkItemId::new("work-item-1"),
        &batch_id,
        subject.clone(),
        "SALES",
        "user-1",
    )
    .unwrap();
    let mut closed = super::factories::confirmation_work_item(
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
            WorkItemCloseData { close_reason: "SUPERSEDED_BY_NEW_IMPORT_TRIAL".to_string() },
            Instant::from_unix_secs(1_700_000_001),
        )
        .unwrap();
    let confirmations =
        vec![invalidated("c-1", "SALES", "work-item-1"), invalidated("c-2", "PROCUREMENT", "work-item-2")];
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

fn confirmation_outcome() -> (ImportConfirmationOutcome, LegacyImportConfirmation, WorkItem) {
    let prepared = completion_command();
    let mut fact = confirmation();
    let mut task = work_item();
    let at = Instant::from_unix_secs(20);
    fact.decide(prepared.decision, "user-1".to_string(), at, prepared.reason_code, prepared.comment).unwrap();
    task.complete_by_domain_command("user-1", at).unwrap();
    fact.base.version += 1;
    task.base.version += 1;
    let result = ImportConfirmationOutcome {
        confirmation_id: fact.base.id.clone(),
        confirmation_version: fact.base.version,
        batch_id: fact.batch_id.to_string(),
        work_item_id: task.base.id.clone(),
        subject_version: task.subject_version.clone(),
        confirmation_scope: fact.confirmation_scope.clone(),
        decision: ConfirmationDecision::ConfirmScope,
        decided_at: at,
        receipt: ConfirmationCompletionReceipt {
            result_status: ImportBusinessConfirmationResultStatus::Confirmed,
            task_version: 4,
            batch_version: 5,
            next_step: ImportBusinessConfirmationNextStep::AwaitOtherConfirmations,
        },
    };
    (result, fact, task)
}

#[test]
fn idempotency_receipt_rejects_same_key_with_different_command() {
    let prepared = completion_command();
    let identity = confirmation_command_identity("user-1", "legacy_import_confirmation.complete", &prepared)
        .structured_receipt("legacy_import_confirmation")
        .unwrap();
    let (outcome, _, _) = confirmation_outcome();
    let fact = ImportCommandReceipt::new(
        identity.clone(),
        ImportCommandResult::Confirmation(Box::new(outcome)),
        "event-1".into(),
    )
    .unwrap();
    assert!(fact.ensure_identity(&identity).is_ok());
    let mut changed = prepared;
    changed.comment = Some("不同确认".into());
    let identity = confirmation_command_identity("user-1", "legacy_import_confirmation.complete", &changed)
        .structured_receipt("legacy_import_confirmation")
        .unwrap();
    assert!(fact.ensure_identity(&identity).is_err());
    assert!(!fact.identity.command_id.contains("request-1"));
    let decoded: ImportCommandReceipt = serde_json::from_str(&serde_json::to_string(&fact).unwrap()).unwrap();
    assert_eq!(fact, decoded);
}

#[test]
fn confirmation_replay_requires_original_decision_actor_and_task_terminal() {
    let prepared = completion_command();
    let (outcome, confirmation, task) = confirmation_outcome();
    assert!(validate_confirmation_replay(&outcome, &confirmation, &task, &prepared, "user-1").is_ok());
    assert!(validate_confirmation_replay(&outcome, &confirmation, &task, &prepared, "other").is_err());
    for case in 0..8 {
        let mut wrong = task.clone();
        match case {
            0 => wrong.base.version += 1,
            1 => wrong.completed_by = Some("other".into()),
            2 => wrong.status = WorkItemStatus::Open,
            3 => wrong.business_object_id = "other".into(),
            4 => wrong.subject_version = "foreign".into(),
            5 => wrong.owner_user_id = Some("other".into()),
            6 => wrong.owner_user_id = None,
            _ => wrong.completed_at = Some(Instant::from_unix_secs(21)),
        }
        assert!(validate_confirmation_replay(&outcome, &confirmation, &wrong, &prepared, "user-1").is_err());
    }
    for case in 0..6 {
        let mut wrong = confirmation.clone();
        match case {
            0 => wrong.decision = Some(ConfirmationDecision::ReturnForFix),
            1 => wrong.base.version += 1,
            2 => wrong.decided_by = Some("other".into()),
            3 => wrong.decided_at = Some(Instant::from_unix_secs(21)),
            4 => wrong.comment = Some("更改原决定".into()),
            _ => wrong.status = ConfirmationStatus::Pending,
        }
        assert!(validate_confirmation_replay(&outcome, &wrong, &task, &prepared, "user-1").is_err());
    }
}
