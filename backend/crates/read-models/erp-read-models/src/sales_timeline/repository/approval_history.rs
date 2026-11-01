//! 审批恢复与取消取成功审计或不可变取消事实；不解析自由文本消息。
use std::collections::BTreeMap;

use bpm::model::ApprovalProcessInstance;
use erp_audit::repository::{AuditExt, AuditTimelineRepositoryExt, TimelineTarget};
use erp_workflow::repository::ApprovalCancellationExt;
use mongodb::Database;
use mongodb::bson::doc;
use persistence_core::Executor;

use super::super::history::bound;
use super::super::{TimelineCategory, TimelineDocument, TimelineEvent, TimelineQuery};
use crate::Result;

pub(super) async fn enrich_cancellations(
    db: &Database,
    instances: &BTreeMap<String, ApprovalProcessInstance>,
    events: &mut [TimelineEvent],
    executor: &mut dyn Executor,
) -> Result<()> {
    let ids = instances.keys().cloned().collect::<Vec<_>>();
    for fact in db.approval_cancellation_facts().find_many(doc! { "id": { "$in": ids } }, executor).await? {
        if let Some(event) =
            events.iter_mut().find(|event| event.id == format!("instance-end:{}", fact.base.id))
        {
            event.actor_name = "未记录姓名".into();
            event.actor_id = Some(fact.actor_id);
            event.details.push(fact.reason);
        }
    }
    Ok(())
}

pub(super) async fn recovery_events(
    db: &Database,
    documents: &BTreeMap<String, TimelineDocument>,
    instances: &BTreeMap<String, ApprovalProcessInstance>,
    query: &TimelineQuery,
    executor: &mut dyn Executor,
) -> Result<Vec<TimelineEvent>> {
    let targets = instances
        .keys()
        .map(|id| TimelineTarget {
            resource_type: "approval_process_instance".into(),
            resource_id: id.clone(),
        })
        .collect::<Vec<_>>();
    let mut cursor = bound(query.cursor()?.as_ref(), "recovery:");
    let limit = usize::try_from(query.limit()?).expect("分页上限可转换");
    let mut events = Vec::new();
    loop {
        let rows = db
            .audit_logs()
            .timeline_events(&targets, cursor.as_ref().map(|(time, id)| (*time, id.as_str())), 100, executor)
            .await?;
        let exhausted = rows.len() < 100;
        if let Some(last) = rows.last() {
            cursor = Some((last.base.created_at, last.base.id.clone()));
        }
        for log in rows {
            let Some(audit) = log.structured_event else {
                continue;
            };
            if audit.action_code != "approval.resume_current_approver" {
                continue;
            }
            let Some(instance) = instances.get(&audit.resource_id) else {
                continue;
            };
            let Some(document) = documents.get(instance.subject.subject_id()) else {
                continue;
            };
            let actor = audit
                .actor_name_snapshot
                .filter(|name| !name.trim().is_empty())
                .unwrap_or(audit.actor_account);
            let mut event = document.event(
                format!("recovery:{}", log.base.id),
                log.base.created_at,
                audit.action_label,
                actor,
            );
            event.category = TimelineCategory::Approval;
            events.push(event);
        }
        if exhausted || events.len() > limit {
            break;
        }
    }
    Ok(events)
}
