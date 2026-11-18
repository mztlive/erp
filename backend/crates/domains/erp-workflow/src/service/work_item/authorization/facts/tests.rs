#![cfg(test)]

use application_core::AuditActor;
use erp_core::AccountKind;
use erp_core::common::time::Instant;
use erp_core::ids::WorkItemId;
use erp_identity::entity::access_control::{ResolvedScope, ScopeClause, ScopedObject};
use erp_identity::entity::authorization::{AccessFact, FactKind};
use erp_identity::ports::QueryFragment;
use erp_permissions::work_item;
use mongodb::bson::{Bson, Document, serialize_to_document};

use super::*;
use crate::entity::work_item::SupplierPortalReviewTaskData;
use crate::repository::work_item::authorization::WorkItemMapping;

fn task() -> WorkItem {
    WorkItem::new_supplier_portal_review(
        WorkItemId::new("review-task"),
        SupplierPortalReviewTaskData {
            request_id: "supplier-request".into(),
            subject_version: "submission-1".into(),
            owner_user_id: "reviewer".into(),
            owner_organization_id: "procurement".into(),
            due_at: None,
            impact_summary: None,
        },
    )
    .unwrap()
}

/// 拟创建任务按完整根目标取证，无需预先存在数据库记录。
#[test]
fn task_facts_preserve_actual_identity_version_and_assignment() {
    let item = task();
    let facts = item_facts(item, Some("current-org".into()), None);
    assert_eq!(facts.resource, "work_item");
    assert_eq!(facts.subject_type, FactValue::Known("SUPPLIER_PORTAL_REVIEW".into()));
    assert_eq!(facts.subject_source_resource, FactValue::Known("supplier_portal_request".into()));
    assert_eq!(facts.id, "review-task");
    assert_eq!(facts.owner_id, FactValue::Known(Some("reviewer".into())));
    assert_eq!(facts.assignee_id, facts.owner_id);
    assert_eq!(facts.completed, FactValue::Known(false));
    assert_eq!(facts.org_unit_id, FactValue::Known(Some("current-org".into())));
}

fn context(actor: &str) -> AccessContext {
    AccessContext {
        actor: AuditActor::new(actor.into(), actor.into(), AccountKind::Admin),
        permission: work_item::LIST,
        resource: "work_item".into(),
        model: None,
        as_of: Instant::from_unix_secs(200),
    }
}

fn query_matches(filter: &Document, row: &Document) -> bool {
    if let Ok(value) = filter.get_bool("$expr") {
        return value;
    }
    if let Ok(branches) = filter.get_array("$or") {
        return branches.iter().any(|branch| query_matches(branch.as_document().unwrap(), row));
    }
    filter.iter().all(|(field, expected)| match row.get(field) {
        Some(Bson::Array(values)) => values.contains(expected),
        Some(value) => value == expected,
        None => false,
    })
}

#[test]
fn empty_collaboration_and_real_task_history_match_object_and_query_scope() {
    let mut completed = task();
    completed.reassign("replacement", Instant::from_unix_secs(100)).unwrap();
    let mut closed = completed.clone();
    completed.complete_supplier_portal_review("decider", Instant::from_unix_secs(110)).unwrap();
    closed.withdraw_supplier_portal_review("supplier", "withdrawn", Instant::from_unix_secs(110)).unwrap();
    // 终态任务允许没有当前负责人；已冻结的个人责任及结束操作事实仍保留。
    closed.owner_user_id = None;

    let mapping = WorkItemMapping::shared();
    assert!(mapping.model().facts.contains(&FactKind::Collaborators));
    assert!(mapping.model().facts.contains(&FactKind::HistoryParticipants));

    for item in [completed, closed] {
        let row = serialize_to_document(&item).unwrap();
        let facts = item_facts(item, None, None);
        assert_eq!(facts.completed, FactValue::Known(true));
        let QueryFragment::Match(records) = mapping.records(&context("reader"), None).unwrap() else {
            panic!("任务存在性不得依赖当前负责人或主属关系");
        };
        assert!(query_matches(&records, &row));

        let collaborators = facts.collaborators.get().unwrap();
        let history = facts.history_participants.get().unwrap();
        assert!(collaborators.is_empty());
        assert!(history.contains("reviewer") && history.contains("replacement"));
        assert!(history.contains("decider") || history.contains("supplier"));

        for actor in ["reviewer", "replacement", "decider", "supplier", "bpm-observer", "unrelated"] {
            let object = ScopedObject {
                owned: facts.owner_id.get().unwrap().as_deref() == Some(actor),
                collaborating: collaborators.contains(actor),
                historical_read_participant: history.contains(actor),
                org_unit_id: facts.org_unit_id.get().unwrap().as_deref(),
                settlement_party_id: None,
                warehouse_id: None,
            };
            let company = ResolvedScope {
                role_clauses: vec![ScopeClause { company: true, ..Default::default() }],
                user_limit: None,
            };
            assert_eq!(company.allows(&object, false), query_matches(&records, &row));
            for (fact, scope, allow_history) in [
                (AccessFact::Collaborating, ScopeClause { collaborative: true, ..Default::default() }, false),
                (AccessFact::HistoryParticipant, ScopeClause::default(), true),
            ] {
                let QueryFragment::Match(filter) = mapping.predicate(&fact, &context(actor), None).unwrap()
                else {
                    panic!("任务自身事实应使用原任务行");
                };
                let scope = ResolvedScope { role_clauses: vec![scope], user_limit: None };
                assert_eq!(
                    scope.allows(&object, allow_history),
                    query_matches(&records, &row) && query_matches(&filter, &row),
                );
                if allow_history {
                    assert!(!scope.allows(&object, false), "历史参与不得授予写资格");
                }
            }
        }
    }
}

#[test]
fn task_owner_organization_keeps_expiry_and_future_membership_boundaries() {
    let boundary = Instant::from_unix_secs(300);
    for org in [Some("current-org".into()), None] {
        let facts = item_facts(task(), org.clone(), Some(boundary));
        assert_eq!(facts.org_unit_id, FactValue::Known(org));
        assert_eq!(facts.valid_until, Some(boundary));
    }
    assert_eq!(item_facts(task(), None, None).valid_until, None);
}
