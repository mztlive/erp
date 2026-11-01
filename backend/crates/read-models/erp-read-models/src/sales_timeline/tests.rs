use application_core::AuditActor;
use async_trait::async_trait;
use erp_core::AccountKind;
use erp_workflow::DocumentType;
use persistence_core::{Executor, NoTransaction};

use super::dto::{Cursor, page};
use super::history::{bound, is_detail};
use super::*;
use crate::{Error, Result};

fn event(id: &str, time: u64) -> TimelineEvent {
    TimelineDocument {
        kind: DocumentType::SalesOrder,
        id: "sale".into(),
        number: "XS001".into(),
        sales_order_id: Some("sale".into()),
        purchase_order_id: None,
        category_override: None,
    }
    .event(id.into(), time, "销售单生效".into(), "销售".into())
}

#[test]
fn pagination_has_no_loss_at_equal_timestamp() {
    let events =
        vec![event("audit:b", 10), event("workflow:a", 10), event("audit:a", 10), event("allocation:x", 9)];
    let first = page(events.clone(), &TimelineQuery { page_size: Some(2), ..Default::default() }).unwrap();
    assert_eq!(
        first.items.iter().map(|event| event.id.as_str()).collect::<Vec<_>>(),
        ["workflow:a", "audit:b"]
    );
    let second =
        page(events, &TimelineQuery { cursor: first.next_cursor, page_size: Some(2), ..Default::default() })
            .unwrap();
    assert_eq!(
        second.items.iter().map(|event| event.id.as_str()).collect::<Vec<_>>(),
        ["audit:a", "allocation:x"]
    );
    assert!(second.next_cursor.is_none());
}

#[test]
fn cursor_keeps_other_streams_at_same_timestamp() {
    let cursor = Cursor { time: 10, id: "workflow:a".into() };
    assert_eq!(bound(Some(&cursor), "workflow:"), Some((10, "a".into())));
    assert_eq!(bound(Some(&cursor), "audit:"), Some((10, "\u{10ffff}".into())));
    assert_eq!(bound(Some(&cursor), "zz:"), Some((10, String::new())));
}

#[test]
fn filters_are_applied_before_page_size() {
    let mut approval = event("workflow:1", 1);
    approval.category = TimelineCategory::Approval;
    let result = page(
        vec![event("audit:1", 2), approval],
        &TimelineQuery {
            page_size: Some(1),
            category: Some(TimelineCategory::Approval),
            ..Default::default()
        },
    )
    .unwrap();
    assert_eq!(result.items[0].id, "workflow:1");
    assert!(result.next_cursor.is_none());
}

#[test]
fn rejects_invalid_cursor_and_limits() {
    for cursor in ["bad", "1:", "18446744073709551615:id"] {
        assert!(TimelineQuery { cursor: Some(cursor.into()), ..Default::default() }.cursor().is_err());
    }
    assert!(TimelineQuery { page_size: Some(0), ..Default::default() }.limit().is_err());
    assert!(TimelineQuery { page_size: Some(101), ..Default::default() }.limit().is_err());
    assert!(is_detail("sales_order.save_working_copy"));
    assert!(!is_detail("supplier_payment.post"));
}

#[test]
fn partial_allocations_and_reversals_preserve_each_original_fact() {
    let mut sources = sources::Sources::new();
    sources.add(DocumentType::SupplierPayment, "payment".into(), Some("sale"), None);
    finance::allocation(
        &mut sources,
        "payment",
        "apply",
        10,
        "30.25".into(),
        "apply",
        "付款",
        TimelineCategory::Payable,
    );
    finance::allocation(
        &mut sources,
        "payment",
        "reverse",
        20,
        "5.10".into(),
        "reverse",
        "付款",
        TimelineCategory::Payable,
    );
    let page = page(sources.facts, &TimelineQuery::default()).unwrap();
    assert_eq!(page.items.len(), 2);
    assert_eq!(page.items[0].title, "付款冲减本单分配");
    assert_eq!(page.items[0].amount.as_deref(), Some("5.10"));
    assert_eq!(page.items[1].amount.as_deref(), Some("30.25"));
    assert_eq!(page.items[1].occurred_at, 10);
}

#[test]
fn financial_audit_does_not_expose_whole_document_amount() {
    use erp_audit::{
        AuditAction, AuditFact, AuditField, AuditFieldKind, AuditValue, BusinessEventContent,
        BusinessEventContext, BusinessEventResult,
    };
    const FIELDS: &[AuditField] =
        &[AuditField { code: "amount", label: "整笔金额", kind: AuditFieldKind::Amount }];
    let log = BusinessEventContext::new(
        AuditActor::new("operator".into(), "finance".into(), AccountKind::Admin),
        AuditAction {
            code: "supplier_payment.post",
            resource_type: "supplier_payment",
            label: "付款过账",
            version: 1,
            allowed_fields: FIELDS,
        },
    )
    .unwrap()
    .with_actor_name_snapshot(Some("@出纳".into()))
    .unwrap()
    .log(BusinessEventContent {
        target_id: "payment".into(),
        target_number: Some("FK001".into()),
        result: BusinessEventResult::Succeeded,
        field_changes: Vec::new(),
        facts: vec![AuditFact {
            field: "amount".into(),
            value: AuditValue::Amount { value: "999.00".parse().unwrap() },
        }],
    })
    .unwrap();
    let mut sources = sources::Sources::new();
    sources.add(DocumentType::SupplierPayment, "payment".into(), Some("sale"), None);
    let event = history::audit_event(log, &sources.documents, true).unwrap();
    assert!(event.details.is_empty());
    assert!(event.amount.is_none());
    assert_eq!(event.actor_name, "@出纳");
    assert!(event.actor_id.is_none());
    let json = serde_json::to_value(event).unwrap();
    assert!(json.get("actor_id").is_none());
}

#[test]
fn actual_node_decision_uses_frozen_actor_round_and_decision_time() {
    use bpm::model::{
        ApprovalExecutionAssignmentSource, ApprovalNodeExecution, NewNodeExecution, ParticipantId, Timestamp,
    };
    use bpm::{ApprovalNodeExecutionId, ApprovalProcessInstanceId};
    let mut execution = ApprovalNodeExecution::new_active(NewNodeExecution {
        id: ApprovalNodeExecutionId::new("execution"),
        process_instance_id: ApprovalProcessInstanceId::new("instance"),
        node_key: "review".into(),
        node_name: "财务复核".into(),
        round_no: 2,
        execution_no: 3,
        assignment_source: ApprovalExecutionAssignmentSource::Definition,
        replaces_execution_id: None,
        assignee_participant_id: ParticipantId::new("reviewer").unwrap(),
        assignee_name_snapshot: "财务复核人".into(),
        at: Timestamp::from_unix_secs(10).unwrap(),
    })
    .unwrap();
    let mut sources = sources::Sources::new();
    sources.add(DocumentType::SalesOrder, "sale".into(), Some("sale"), None);
    let document = &sources.documents["sale"];
    assert!(approval::decision_event(&execution, document).is_none());
    execution
        .record_reject(
            ParticipantId::new("reviewer").unwrap(),
            "请补交凭证",
            Timestamp::from_unix_secs(20).unwrap(),
        )
        .unwrap();
    let decision = approval::decision_event(&execution, document).unwrap();
    assert_eq!(decision.title, "财务复核：驳回（第 2 轮）");
    assert_eq!(decision.occurred_at, 20);
    assert_eq!(decision.actor_name, "财务复核人");
    assert_eq!(decision.details, ["请补交凭证"]);
}

#[test]
fn purchase_invoices_and_fulfillment_completion_use_their_business_category() {
    use erp_workflow::entity::document_registry::WorkflowActionType;
    let mut sources = sources::Sources::new();
    sources.add(DocumentType::Invoice, "invoice".into(), Some("sale"), None);
    finance::allocation(
        &mut sources,
        "invoice",
        "row",
        10,
        "1".into(),
        "apply",
        "进项发票",
        TimelineCategory::Payable,
    );
    assert_eq!(sources.documents["invoice"].category(), TimelineCategory::Payable);
    assert!(query::href(&sources.documents["invoice"]).unwrap().contains("supplier-accounts"));
    assert_eq!(
        history::workflow_category(WorkflowActionType::Complete, TimelineCategory::Fulfillment),
        TimelineCategory::Fulfillment
    );
    assert_eq!(
        history::workflow_category(WorkflowActionType::Reject, TimelineCategory::Fulfillment),
        TimelineCategory::Approval
    );
}

struct TimelineAccess;

#[async_trait]
impl TimelineAccessPort for TimelineAccess {
    async fn readable(
        &self,
        _: &AuditActor,
        document: &TimelineDocument,
        _: &mut dyn Executor,
    ) -> Result<bool> {
        if document.id == "unavailable" {
            return Err(Error::Internal("范围服务不可用".into()));
        }
        Ok(document.id != "hidden")
    }
}

#[tokio::test]
async fn authorization_removes_hidden_documents_and_their_allocated_amounts() {
    let actor = AuditActor::new("actor".into(), "sales".into(), AccountKind::Admin);
    let mut sources = sources::Sources::new();
    sources.add(DocumentType::SalesOrder, "sale".into(), Some("sale"), None);
    sources.add(DocumentType::SupplierPayment, "hidden".into(), Some("sale"), None);
    finance::allocation(
        &mut sources,
        "hidden",
        "allocation",
        10,
        "30".into(),
        "apply",
        "付款",
        TimelineCategory::Payable,
    );
    sources.facts.push(event("submission:visible", 9));
    query::authorize(&TimelineAccess, &actor, &mut sources, &mut NoTransaction).await.unwrap();
    assert_eq!(sources.documents.keys().map(String::as_str).collect::<Vec<_>>(), ["sale"]);
    assert_eq!(
        sources.facts.iter().map(|event| event.id.as_str()).collect::<Vec<_>>(),
        ["submission:visible"]
    );
}

#[tokio::test]
async fn authorization_failure_is_not_reported_as_an_empty_timeline() {
    let actor = AuditActor::new("actor".into(), "sales".into(), AccountKind::Admin);
    let mut sources = sources::Sources::new();
    sources.add(DocumentType::SalesOrder, "unavailable".into(), None, None);
    let result = query::authorize(&TimelineAccess, &actor, &mut sources, &mut NoTransaction).await;
    assert!(matches!(result, Err(Error::Internal(message)) if message == "范围服务不可用"));
}

#[test]
fn fulfillment_sources_resolve_only_the_current_purchase() {
    for kind in
        [DocumentType::ServiceFulfillment, DocumentType::ElectronicDelivery, DocumentType::PurchaseReceipt]
    {
        let mut document = TimelineDocument::new(kind, "fulfillment".into());
        document.purchase_order_id = Some("purchase".into());
        document.resolve_purchase_source("other-purchase", "other-sale");
        assert!(document.sales_order_id.is_none());
        document.resolve_purchase_source("purchase", "sale");
        assert_eq!(document.sales_order_id.as_deref(), Some("sale"));
        assert_eq!(document.purchase_order_id.as_deref(), Some("purchase"));
    }
}

#[test]
fn missing_or_direct_sources_are_not_replaced_by_another_root() {
    let mut missing = TimelineDocument::new(DocumentType::ServiceFulfillment, "missing".into());
    missing.resolve_purchase_source("purchase", "sale");
    assert!(missing.sales_order_id.is_none());
    assert!(missing.purchase_order_id.is_none());
    let mut delivery = TimelineDocument::new(DocumentType::Delivery, "delivery".into());
    delivery.sales_order_id = Some("direct-sale".into());
    delivery.purchase_order_id = Some("purchase".into());
    delivery.resolve_purchase_source("purchase", "other-sale");
    assert_eq!(delivery.sales_order_id.as_deref(), Some("direct-sale"));
}

#[test]
fn readable_fulfillment_keeps_result_and_quantity_but_hidden_task_is_sanitized() {
    let mut document = TimelineDocument::new(DocumentType::ServiceFulfillment, "service".into());
    document.purchase_order_id = Some("purchase".into());
    document.resolve_purchase_source("purchase", "sale");
    let mut completion = document.event("audit:service".into(), 10, "确认服务".into(), "经办人".into());
    completion.details = vec!["服务结果：已完成".into(), "数量：3".into()];
    completion.href = Some("/original-document".into());
    let mut root = event("audit:sale", 9);
    root.details = vec!["销售单已生效".into()];
    let mut timeline = page(vec![completion.clone(), root.clone()], &TimelineQuery::default()).unwrap();
    task::restrict_task_details(&mut timeline, "service", true);
    assert_eq!(timeline.items[0].details, completion.details);
    assert_eq!(timeline.items[0].href, completion.href);
    task::restrict_task_details(&mut timeline, "service", false);
    assert!(timeline.items[0].details.is_empty());
    assert!(timeline.items[0].href.is_none());
    assert_eq!(timeline.items[1].details, root.details);
}
