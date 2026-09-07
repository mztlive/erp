use super::{authorized_work_item_view, read_only_work_item_view, work_item_view};
use erp_core::common::time::Instant;
use erp_core::ids::WorkItemId;
use erp_import::ConfirmationStatus;
use erp_workflow::entity::work_item::{
    AssignmentSource, WorkItem, WorkItemData, WorkItemPriority, WorkItemStatus, WorkItemType,
};
use erp_workflow::service::work_item::{AuthorizedWorkItem, ProcessingState, WorkItemAllowedAction};

fn task(work_item_type: WorkItemType) -> WorkItem {
    WorkItem::new_at(
        WorkItemId::new("linked-task"),
        WorkItemData {
            work_item_type,
            business_object_type: if work_item_type == WorkItemType::ImportBusinessConfirmation {
                "LEGACY_IMPORT_BATCH"
            } else {
                "integration_error_task"
            }
            .to_string(),
            business_object_id: "linked-object".to_string(),
            subject_version: "subject-7".to_string(),
            owner_role: "role-sales".to_string(),
            owner_organization_id: "owner-organization".to_string(),
            owner_user_id: "owner-user".to_string(),
            assignment_source: AssignmentSource::SystemRule,
            priority: WorkItemPriority::Normal,
            due_at: None,
            reason_code: None,
            impact_summary: None,
        },
        Instant::from_unix_secs(1_700_000_000),
    )
    .unwrap()
}

#[test]
fn raw_and_read_only_views_preserve_actual_linked_task_type() {
    for (kind, wire) in [
        (
            WorkItemType::ImportBusinessConfirmation,
            "IMPORT_BUSINESS_CONFIRMATION",
        ),
        (WorkItemType::BusinessException, "BUSINESS_EXCEPTION"),
        (
            WorkItemType::IntegrationResultUnknown,
            "INTEGRATION_RESULT_UNKNOWN",
        ),
    ] {
        let item = task(kind);
        let raw = work_item_view(&item);
        assert_eq!(raw.work_item_type, kind);
        assert_eq!(serde_json::to_value(&raw).unwrap()["work_item_type"], wire);
        assert_eq!(raw.owner_user_id.as_deref(), Some("owner-user"));
        assert!(raw.allowed_actions.is_empty());
        assert_eq!(raw.handler_key, "import_business_confirmation");
        assert_eq!(raw.destination_workspace_id, "W18");

        let read_only = read_only_work_item_view(&item);
        assert_eq!(read_only.work_item_type, kind);
        assert_eq!(serde_json::to_value(&read_only).unwrap()["work_item_type"], wire);
        assert_eq!(read_only.owner_user_id, None);
        assert!(read_only.allowed_actions.is_empty());
        assert_eq!(
            read_only.action_blockers,
            ["当前账号不在该责任范围，任务仅可查看。"]
        );
    }
}

#[test]
fn authorized_projection_keeps_actual_type_status_and_existing_action_rules() {
    for (status, status_wire) in [
        (WorkItemStatus::Open, "OPEN"),
        (WorkItemStatus::Completed, "COMPLETED"),
        (WorkItemStatus::Closed, "CLOSED"),
    ] {
        let mut item = task(WorkItemType::BusinessException);
        item.status = status;
        item.base.version = 9;
        let view = authorized_work_item_view(
            AuthorizedWorkItem {
                item,
                allowed_actions: vec![WorkItemAllowedAction::View, WorkItemAllowedAction::Process],
                processing_state: ProcessingState::Ready,
                processing_blocker: None,
                action_blockers: vec!["original blocker".to_string()],
            },
            ConfirmationStatus::Pending,
        )
        .unwrap();
        assert_eq!(view.work_item_type, WorkItemType::BusinessException);
        assert_eq!(view.status, status);
        assert_eq!(view.task_version, "9");
        assert_eq!(view.subject_version, "subject-7");
        assert_eq!(view.owner_user_id.as_deref(), Some("owner-user"));
        assert_eq!(
            view.allowed_actions,
            ["VIEW", "PROCESS", "CONFIRM_SCOPE", "RETURN_FOR_FIX"]
        );
        assert_eq!(view.action_blockers, ["original blocker"]);
        let wire = serde_json::to_value(view).unwrap();
        assert_eq!(wire["work_item_type"], "BUSINESS_EXCEPTION");
        assert_eq!(wire["status"], status_wire);
        assert_eq!(wire["handler_key"], "business_exception");
        assert_eq!(wire["destination_workspace_id"], "W29");
    }
}

#[test]
fn authorized_projection_uses_registered_destinations_and_preserves_route_errors() {
    let authorized = |item| AuthorizedWorkItem {
        item,
        allowed_actions: vec![WorkItemAllowedAction::View],
        processing_state: ProcessingState::Ready,
        processing_blocker: None,
        action_blockers: Vec::new(),
    };
    for (kind, handler, workspace) in [
        (
            WorkItemType::ImportBusinessConfirmation,
            "import_business_confirmation",
            "W18",
        ),
        (
            WorkItemType::IntegrationResultUnknown,
            "integration_unknown",
            "W29",
        ),
    ] {
        let view = authorized_work_item_view(authorized(task(kind)), ConfirmationStatus::Pending).unwrap();
        assert_eq!(view.work_item_type, kind);
        assert_eq!(view.handler_key, handler);
        assert_eq!(view.destination_workspace_id, workspace);
        assert_eq!(view.allowed_actions, ["VIEW"]);
    }

    let mut unmapped = task(WorkItemType::BusinessException);
    unmapped.business_object_type = "unmapped_object".to_string();
    assert!(matches!(
        authorized_work_item_view(authorized(unmapped), ConfirmationStatus::Pending),
        Err(crate::Error::ValidationError(message)) if message == "WORK_ITEM_HANDLER_UNMAPPED"
    ));
    let mut unknown_scope = task(WorkItemType::ImportBusinessConfirmation);
    unknown_scope.owner_role = "unmapped-role".to_string();
    assert!(matches!(
        authorized_work_item_view(authorized(unknown_scope), ConfirmationStatus::Pending),
        Err(crate::Error::ValidationError(message)) if message == "IMPORT_CONFIRMATION_SCOPE_UNMAPPED"
    ));
}
