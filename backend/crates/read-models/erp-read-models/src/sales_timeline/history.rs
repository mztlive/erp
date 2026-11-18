use std::collections::BTreeMap;

use erp_audit::entity::{AuditLog, AuditValue};
use erp_audit::repository::{AuditExt, AuditTimelineRepositoryExt, TimelineTarget};
use erp_workflow::entity::document_registry::{WorkflowAction, WorkflowActionType};
use erp_workflow::repository::document_registry::WorkflowTimelineRepositoryExt;
use erp_workflow::{DocumentRegistryExt, DocumentType};
use mongodb::Database;
use persistence_core::Executor;

use super::dto::Cursor;
use super::{
    TimelineApproval, TimelineApprovalResult, TimelineCategory, TimelineDocument, TimelineEvent,
    TimelineQuery,
};
use crate::Result;

pub(super) async fn history(
    db: &Database,
    documents: &BTreeMap<String, TimelineDocument>,
    query: &TimelineQuery,
    executor: &mut dyn Executor,
) -> Result<Vec<TimelineEvent>> {
    let mut events = audit(db, documents, query, executor).await?;
    events.extend(super::repository::approval::events(db, documents, query, executor).await?);
    events.extend(approvals(db, documents, query, executor).await?);
    Ok(events)
}

async fn audit(
    db: &Database,
    documents: &BTreeMap<String, TimelineDocument>,
    query: &TimelineQuery,
    executor: &mut dyn Executor,
) -> Result<Vec<TimelineEvent>> {
    let targets = documents
        .values()
        .filter(|doc| query.category.is_none_or(|category| doc.category() == category))
        .flat_map(|doc| {
            let mut targets =
                vec![TimelineTarget { resource_type: doc.kind.as_str().into(), resource_id: doc.id.clone() }];
            if doc.kind == DocumentType::SalesOrder {
                targets.push(TimelineTarget {
                    resource_type: "voucher_sales_order".into(),
                    resource_id: doc.id.clone(),
                });
            }
            targets
        })
        .collect::<Vec<_>>();

    let mut cursor = bound(query.cursor()?.as_ref(), "audit:");
    let mut result = Vec::new();
    let limit = usize::try_from(query.limit()?).expect("分页上限可转换");

    loop {
        let rows = db
            .audit_logs()
            .timeline_events(&targets, cursor.as_ref().map(|(time, id)| (*time, id.as_str())), 100, executor)
            .await?;
        let exhausted = rows.len() < 100;
        if let Some(last) = rows.last() {
            cursor = Some((last.base.created_at, last.base.id.clone()));
        }
        result.extend(rows.into_iter().filter_map(|log| audit_event(log, documents, query.include_details)));
        if exhausted || result.len() > limit {
            break;
        }
    }
    Ok(result)
}

pub(super) fn audit_event(
    log: AuditLog,
    documents: &BTreeMap<String, TimelineDocument>,
    include_details: bool,
) -> Option<TimelineEvent> {
    let event = log.structured_event?;
    if !include_details && is_detail(&event.action_code) {
        return None;
    }
    // 工作流动作另有完整的追加式记录，避免显示同一审批的审计包装。
    if event.action_code.starts_with("workflow_action.") || event.action_code.starts_with("approval.") {
        return None;
    }

    let document = documents.get(&event.resource_id)?;
    let actor =
        event.actor_name_snapshot.filter(|name| !name.trim().is_empty()).unwrap_or(event.actor_account);
    let mut row =
        document.event(format!("audit:{}", log.base.id), log.base.created_at, event.action_label, actor);
    if let Some(number) = event.resource_number_snapshot {
        row.document_number = number;
    }

    // 跨销售单资金事件的金额只能来自本单分配，不展示整笔资金的审计金额。
    if !financial(document.kind) {
        row.details.extend(
            event.facts.into_iter().map(|fact| format!("{}：{}", fact.field_label, value(&fact.value))),
        );
        row.details.extend(event.field_changes.into_iter().map(|change| {
            format!("{}：{} → {}", change.field_label, value(&change.before), value(&change.after))
        }));
    }
    Some(row)
}

async fn approvals(
    db: &Database,
    documents: &BTreeMap<String, TimelineDocument>,
    query: &TimelineQuery,
    executor: &mut dyn Executor,
) -> Result<Vec<TimelineEvent>> {
    let ids = documents.keys().cloned().collect::<Vec<_>>();
    let mut cursor = bound(query.cursor()?.as_ref(), "workflow:");
    let mut result = Vec::new();
    let limit = usize::try_from(query.limit()?).expect("分页上限可转换");

    loop {
        let rows = db
            .workflow_actions()
            .timeline_actions(&ids, cursor.as_ref().map(|(time, id)| (*time, id.as_str())), 100, executor)
            .await?;
        let exhausted = rows.len() < 100;
        if let Some(last) = rows.last() {
            cursor = Some((last.base.created_at, last.base.id.clone()));
        }
        result.extend(
            rows.into_iter()
                .filter_map(|action| workflow_event(action, documents))
                .filter(|event| query.category.is_none_or(|category| event.category == category)),
        );
        if exhausted || result.len() > limit {
            break;
        }
    }
    Ok(result)
}

fn workflow_event(
    action: WorkflowAction,
    documents: &BTreeMap<String, TimelineDocument>,
) -> Option<TimelineEvent> {
    let document = documents.get(action.document_id.as_ref())?;
    let mut event = document.event(
        format!("workflow:{}", action.base.id),
        action.base.created_at,
        action.action_type.label().into(),
        "未记录姓名".into(),
    );
    event.category = workflow_category(action.action_type, document.category());
    event.approval = match action.action_type {
        WorkflowActionType::Approve | WorkflowActionType::ApprovalNodeApproved => {
            Some(TimelineApprovalResult::Approved)
        },
        WorkflowActionType::Reject | WorkflowActionType::ApprovalNodeRejected => {
            Some(TimelineApprovalResult::Rejected)
        },
        WorkflowActionType::ApprovalCancelled | WorkflowActionType::ApprovalBlockedCancelled => {
            Some(TimelineApprovalResult::Cancelled)
        },
        _ => None,
    }
    .map(|result| TimelineApproval {
        result,
        node_name: None,
        round_no: action.approval_context.as_ref().map(|context| context.current_round_no),
    });
    if let Some(comment) = action.comment {
        event.details.push(comment);
    }
    event.actor_id = Some(action.actor_id);
    Some(event)
}

pub(super) fn workflow_category(action: WorkflowActionType, category: TimelineCategory) -> TimelineCategory {
    match action {
        WorkflowActionType::Confirm | WorkflowActionType::Complete | WorkflowActionType::Void => category,
        _ => TimelineCategory::Approval,
    }
}

pub(super) fn financial(kind: DocumentType) -> bool {
    matches!(
        kind,
        DocumentType::CustomerReceipt
            | DocumentType::SupplierPayment
            | DocumentType::Invoice
            | DocumentType::SalesInvoiceRequest
            | DocumentType::PurchasePaymentRequest
            | DocumentType::CustomerRefund
            | DocumentType::SupplierRefund
            | DocumentType::ReceiptReversal
            | DocumentType::PaymentReversal
    )
}

fn value(value: &AuditValue) -> String {
    match value {
        AuditValue::Code { label, .. } => label.clone(),
        AuditValue::Amount { value } => value.to_string(),
        AuditValue::Quantity { value } => value.to_string(),
        AuditValue::Changed => "已变更".into(),
    }
}

pub(super) fn is_detail(action: &str) -> bool {
    [".create", ".update", ".save", ".save_draft", ".save_working_copy", ".bind_attachment"]
        .iter()
        .any(|suffix| action.ends_with(suffix))
}

pub(super) fn bound(cursor: Option<&Cursor>, prefix: &str) -> Option<(u64, String)> {
    cursor.map(|cursor| {
        let id =
            cursor.id.strip_prefix(prefix).map(str::to_owned).unwrap_or_else(|| {
                if prefix < cursor.id.as_str() { "\u{10ffff}".into() } else { String::new() }
            });
        (cursor.time, id)
    })
}

#[cfg(test)]
mod tests {
    use erp_core::ids::{BusinessDocumentId, WorkflowActionId};
    use erp_workflow::entity::document_registry::WorkflowActionData;

    use super::*;

    #[test]
    fn historical_decisions_keep_results_without_inventing_missing_rounds() {
        let document = TimelineDocument::new(DocumentType::SalesOrder, "sale".into());
        let documents = BTreeMap::from([("sale".into(), document)]);

        for (action_type, to_status, expected) in [
            (WorkflowActionType::Approve, "EFFECTIVE", Some(TimelineApprovalResult::Approved)),
            (WorkflowActionType::Reject, "DRAFT", Some(TimelineApprovalResult::Rejected)),
            (WorkflowActionType::Submit, "PENDING_REVIEW", None),
        ] {
            let action = WorkflowAction::new(
                WorkflowActionId::new("action"),
                WorkflowActionData {
                    document_id: BusinessDocumentId::new("sale"),
                    action_type,
                    from_status: "PENDING_REVIEW".into(),
                    to_status: to_status.into(),
                    actor_id: "reviewer".into(),
                    actor_role: "manager".into(),
                    comment: Some("审批意见含通过和驳回字样".into()),
                },
            )
            .unwrap();
            let event = workflow_event(action, &documents).unwrap();
            assert_eq!(event.approval.as_ref().map(|approval| approval.result), expected);
            if let Some(approval) = &event.approval {
                assert!(approval.round_no.is_none());
                assert!(approval.node_name.is_none());
            }
        }
    }
}
