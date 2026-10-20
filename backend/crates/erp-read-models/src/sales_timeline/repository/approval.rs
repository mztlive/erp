use std::collections::BTreeMap;

use erp_workflow::entity::approval_integration::subject_ref_for;
use erp_workflow::{BpmExt, DocumentType};
use mongodb::Database;
use mongodb::bson::doc;
use persistence_core::Executor;

use super::super::approval::{decision_event, instance_events};
use super::super::{TimelineCategory, TimelineDocument, TimelineEvent, TimelineQuery};
use crate::{Error, Result};

pub(in crate::sales_timeline) async fn events(
    db: &Database,
    documents: &BTreeMap<String, TimelineDocument>,
    query: &TimelineQuery,
    executor: &mut dyn Executor,
) -> Result<Vec<TimelineEvent>> {
    if query.category.is_some_and(|category| category != TimelineCategory::Approval) || documents.is_empty() {
        return Ok(Vec::new());
    }
    let mut identities = Vec::new();
    for document in documents.values() {
        let subject = subject_ref_for(document.kind, &document.id)?;
        identities.push(doc! { "subject.subject_kind": subject.subject_kind(), "subject.subject_id": subject.subject_id() });
        if document.kind == DocumentType::SalesOrder {
            let voucher = subject_ref_for(DocumentType::VoucherSalesOrder, &document.id)?;
            identities.push(doc! { "subject.subject_kind": voucher.subject_kind(), "subject.subject_id": voucher.subject_id() });
        }
    }
    let instances = db.approval_process_instances().find_many(doc! { "$or": identities }, executor).await?;
    let instances = instances
        .into_iter()
        .map(|instance| (instance.base.id.clone(), instance))
        .collect::<BTreeMap<_, _>>();
    let ids = instances.keys().cloned().collect::<Vec<_>>();
    let executions = db
        .approval_node_executions()
        .find_many(doc! { "process_instance_id": { "$in": ids } }, executor)
        .await?;
    if executions.len() > 10000 {
        return Err(Error::ValidationError("审批历史过多，请按单据查看".into()));
    }
    let mut events = Vec::new();
    for instance in instances.values() {
        if let Some(document) = documents.get(instance.subject.subject_id()) {
            events.extend(instance_events(instance, document));
        }
    }
    for execution in executions {
        let Some(instance) = instances.get(execution.process_instance_id.as_ref()) else {
            continue;
        };
        let Some(document) = documents.get(instance.subject.subject_id()) else {
            continue;
        };
        if let Some(event) = decision_event(&execution, document) {
            events.push(event);
        }
    }
    super::approval_history::enrich_cancellations(db, &instances, &mut events, executor).await?;
    events
        .extend(super::approval_history::recovery_events(db, documents, &instances, query, executor).await?);
    Ok(events)
}
