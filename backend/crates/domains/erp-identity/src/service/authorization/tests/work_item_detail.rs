#![cfg(test)]

//! 工作项详情用真实公共query/require/validate验证来源条件与交付期限。
use std::collections::BTreeMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use async_trait::async_trait;
use erp_core::common::time::Instant;
use erp_permissions::{Permission, approval_admin, approval_instance, purchase_order, supplier, work_item};
use mongodb::bson::{Document, doc};
use persistence_core::testing::SnapshotExecutor;
use persistence_core::{Executor, ReadExecutor};

use super::fixtures::{Facts, Identity, actor, complete, mapping, object};
use crate::entity::access_control::{ResolvedScope, ScopeClause};
use crate::entity::authorization::{
    AccessContext, AccessTarget, FactValue, ObjectFacts, RelatedObjects, RelationKey, ResourceTargets,
};
use crate::ports::AccessFactsPort;
use crate::repository::authorization::fixtures::execute;
use crate::{AuthorizationService, CompiledAccess, Error, Result};

fn service(identity: Arc<Identity>, facts: Arc<Facts>) -> AuthorizationService {
    let mut builder = complete();
    builder.facilities = identity;
    builder.facts.insert("work_item".into(), facts);
    builder.build().unwrap()
}

fn grant(identity: &Identity, roles: &[(&str, &[Permission])]) {
    identity.state.lock().unwrap().grants =
        roles.iter().map(|(role, required)| ((*role).into(), required.to_vec())).collect();
}

fn order_task(resource: &str, owner: &str, history: bool) -> ObjectFacts {
    let mut source = object(resource, "order", owner);
    if history {
        source.history_participants = FactValue::Known([actor().id().into()].into());
    }
    ObjectFacts {
        subject_type: FactValue::Known("APPROVAL_REWORK".into()),
        subject_source_resource: FactValue::Known(resource.into()),
        relations: [(
            RelationKey::parse("order_source").unwrap(),
            RelatedObjects { expected_ids: ["order".into()].into(), objects: vec![source] },
        )]
        .into(),
        ..object("work_item", "task", "user")
    }
}

fn rows(
    access: &CompiledAccess,
    tasks: Vec<Document>,
    sources: BTreeMap<String, Vec<Document>>,
) -> Vec<String> {
    execute(tasks, access.stages(), &sources, &doc! {})
        .into_iter()
        .map(|row| row.get_str("id").unwrap().into())
        .collect()
}

#[tokio::test]
async fn detail_order_scope_matches_require_without_adding_order_menu_qualification() {
    for (resource, key, history_allowed) in
        [("sales_order", "detail_sales", false), ("purchase_order", "detail_purchase", true)]
    {
        for (owner, history) in [("user", false), ("other", false), ("other", true)] {
            let identity = Arc::new(Identity::default());
            grant(&identity, &[("task-reader", &[work_item::DETAIL])]);
            let facts = Arc::new(Facts::new(vec![order_task(resource, owner, history)]));
            let service = service(identity, facts);
            let mut executor = SnapshotExecutor::default();
            let access = service.query(&actor(), &work_item::DETAIL, &mut executor).await.unwrap();
            let result = rows(&access, vec![doc! {"id":"task","owner":"user","deleted_at":0,key:["order"]}], [(
                format!("{resource}_rows"), vec![doc! {"id":"order","owner":owner,"history":if history {"user"} else {""},"deleted_at":0}],
            )].into());
            let allowed = owner == "user" || (history && history_allowed);
            assert_eq!(result, if allowed { vec!["task"] } else { vec![] });
            service.validate(&access, &mut executor).await.unwrap();
            assert_eq!(
                service
                    .require(
                        &actor(),
                        &work_item::DETAIL,
                        AccessTarget::record("task").unwrap(),
                        &mut executor
                    )
                    .await
                    .is_ok(),
                allowed
            );
        }
    }
}

#[tokio::test]
async fn company_task_scope_keeps_restricted_purchase_history_and_personal_limit() {
    for (company_source, owner, history, allowed) in [
        (false, "other", true, true),
        (false, "other", false, false),
        (true, "other", true, false),
        (true, "user", false, true),
    ] {
        let identity = Arc::new(Identity::default());
        grant(&identity, &[("task-reader", &[work_item::DETAIL])]);
        identity.scopes.lock().unwrap().extend([
            (
                (work_item::DETAIL, work_item::DETAIL),
                ResolvedScope {
                    role_clauses: vec![ScopeClause { company: true, ..Default::default() }],
                    user_limit: None,
                },
            ),
            (
                (work_item::DETAIL, purchase_order::DETAIL),
                ResolvedScope {
                    role_clauses: vec![ScopeClause {
                        company: company_source,
                        self_owned: !company_source,
                        ..Default::default()
                    }],
                    user_limit: company_source
                        .then(|| ScopeClause { self_owned: true, ..Default::default() }),
                },
            ),
        ]);
        let mut task = order_task("purchase_order", owner, history);
        task.owner_id = FactValue::Known(Some("other-task-owner".into()));
        let service = service(identity, Arc::new(Facts::new(vec![task])));
        let mut executor = SnapshotExecutor::default();
        let access = service.query(&actor(), &work_item::DETAIL, &mut executor).await.unwrap();
        let result = rows(
            &access,
            vec![doc! {"id":"task","owner":"other-task-owner","deleted_at":0,"detail_purchase":["order"]}],
            [(
                "purchase_order_rows".into(),
                vec![doc! {
                    "id":"order","owner":owner,"history":if history {"user"} else {""},"deleted_at":0
                }],
            )]
            .into(),
        );
        assert_eq!(result, if allowed { vec!["task"] } else { vec![] });
        service.validate(&access, &mut executor).await.unwrap();
        let required = service
            .require(&actor(), &work_item::DETAIL, AccessTarget::record("task").unwrap(), &mut executor)
            .await;
        if allowed {
            required.unwrap();
        } else {
            assert!(matches!(required, Err(Error::Forbidden(_))), "{required:?}");
        }
    }
}

#[tokio::test]
async fn missing_order_source_rejects_detail_without_filtering_the_original_list() {
    let identity = Arc::new(Identity::default());
    grant(&identity, &[("reader", &[work_item::LIST, work_item::DETAIL])]);
    let service = service(identity, Arc::new(Facts::new(vec![order_task("purchase_order", "user", false)])));
    let mut executor = SnapshotExecutor::default();
    let task = doc! {"id":"task","owner":"user","deleted_at":0,"detail_purchase":["missing"]};
    let list = service.query(&actor(), &work_item::LIST, &mut executor).await.unwrap();
    assert_eq!(rows(&list, vec![task.clone()], BTreeMap::new()), ["task"]);
    service.validate(&list, &mut executor).await.unwrap();
    let detail = service.query(&actor(), &work_item::DETAIL, &mut executor).await.unwrap();
    assert!(rows(&detail, vec![task], BTreeMap::new()).is_empty());
    service.validate(&detail, &mut executor).await.unwrap();
}

#[tokio::test]
async fn portal_detail_requires_original_supplier_read_without_adding_review_permissions() {
    let identity = Arc::new(Identity::default());
    grant(&identity, &[("task-reader", &[work_item::DETAIL]), ("supplier-reader", &[supplier::DETAIL])]);
    let service = service(identity, Arc::new(Facts::default()));
    let mut executor = SnapshotExecutor::default();
    let access = service.query(&actor(), &work_item::DETAIL, &mut executor).await.unwrap();
    let sources = [
        (
            "supplier_portal_material_rows".into(),
            vec![doc! {"id":"request","deleted_at":0,"supplier":["supplier"]}],
        ),
        ("supplier_rows".into(), vec![doc! {"id":"supplier","owner":"user","deleted_at":0}]),
    ]
    .into();
    let result = rows(
        &access,
        vec![
            doc! {"id":"read","owner":"user","deleted_at":0,"detail_portal_application":["request"]},
            doc! {"id":"update","owner":"user","deleted_at":0,"detail_portal_offering":["request"]},
        ],
        sources,
    );
    assert_eq!(result, ["read"]);
    service.validate(&access, &mut executor).await.unwrap();
}

#[tokio::test]
async fn approval_detail_preserves_owner_terminal_and_complete_decision_qualification() {
    for can_decide in [false, true] {
        let identity = Arc::new(Identity::default());
        let decision = if can_decide {
            vec![approval_instance::COPIES_HISTORY, approval_instance::SUBMIT_DECISION]
        } else {
            vec![approval_instance::COPIES_HISTORY]
        };
        grant(&identity, &[("task-reader", &[work_item::DETAIL]), ("approval-reader", &decision)]);
        let service = service(identity, Arc::new(Facts::default()));
        let mut executor = SnapshotExecutor::default();
        let access = service.query(&actor(), &work_item::DETAIL, &mut executor).await.unwrap();
        let tasks = vec![
            doc! {"id":"open","owner":"user","completed":false,"deleted_at":0,"detail_approval_owner":["open"]},
            doc! {"id":"terminal","owner":"user","completed":true,"deleted_at":0,"detail_approval_owner":["terminal"]},
        ];
        let collections = [("work_item_rows".into(), tasks.clone())].into();
        assert_eq!(
            rows(&access, tasks, collections),
            if can_decide { vec!["open", "terminal"] } else { vec!["terminal"] }
        );
        service.validate(&access, &mut executor).await.unwrap();
    }
}

#[tokio::test]
async fn approval_manager_detail_requires_the_real_type_and_complete_source_range() {
    for (manager, source_owner) in [(true, "user"), (false, "user"), (true, "other")] {
        let identity = Arc::new(Identity::default());
        let management = if manager {
            vec![approval_instance::INSTANCE_LIST, approval_admin::SALES_ORDER_RUNTIME]
        } else {
            vec![approval_instance::INSTANCE_LIST]
        };
        grant(
            &identity,
            &[
                ("task-reader", &[work_item::DETAIL]),
                ("approval-reader", &[approval_instance::COPIES_HISTORY]),
                ("manager", &management),
            ],
        );
        identity.scopes.lock().unwrap().insert(
            (work_item::DETAIL, work_item::DETAIL),
            ResolvedScope {
                role_clauses: vec![ScopeClause { company: true, ..Default::default() }],
                user_limit: None,
            },
        );
        let service = service(identity, Arc::new(Facts::default()));
        let mut executor = SnapshotExecutor::default();
        let access = service.query(&actor(), &work_item::DETAIL, &mut executor).await.unwrap();
        let sources = [
            (
                "approval_instance_rows".into(),
                vec![doc! {"id":"instance","deleted_at":0,"management_sales_order_sales_order":["sale"]}],
            ),
            ("sales_order_rows".into(), vec![doc! {"id":"sale","owner":source_owner,"deleted_at":0}]),
        ]
        .into();
        let result = rows(
            &access,
            vec![doc! {"id":"task","owner":"other","deleted_at":0,"detail_approval_management":["instance"]}],
            sources,
        );
        assert_eq!(result, if manager && source_owner == "user" { vec!["task"] } else { vec![] });
        service.validate(&access, &mut executor).await.unwrap();
    }
}

#[tokio::test]
async fn detail_source_preparation_expiry_is_part_of_the_original_delivery_proof() {
    let mut builder = complete();
    let mut task = mapping(&work_item::DETAIL);
    let mut purchase = mapping(&purchase_order::DETAIL);
    let deadline = Instant::from_unix_secs(Instant::now().unix_secs() + 100);
    Arc::get_mut(&mut purchase).unwrap().dynamic = true;
    *purchase.expires.lock().unwrap() = Some(deadline);
    Arc::get_mut(&mut task)
        .unwrap()
        .relations
        .insert(RelationKey::parse("detail_purchase").unwrap(), purchase);
    builder.mappings.insert(work_item::DETAIL, task);
    let service = builder.build().unwrap();
    let mut executor = SnapshotExecutor::default();
    let mut access = service.query(&actor(), &work_item::DETAIL, &mut executor).await.unwrap();
    assert_eq!(access.proof.valid_until, Some(deadline));
    service.validate(&access, &mut executor).await.unwrap();
    access.proof.boundary = executor.boundary().unwrap();
    access.proof.valid_until = Some(Instant::from_unix_secs(1));
    assert!(matches!(service.validate(&access, &mut executor).await, Err(Error::ConflictError(_))));
}

struct ApprovalFacts {
    original: ObjectFacts,
    current: ObjectFacts,
    calls: AtomicU64,
}

#[async_trait]
impl AccessFactsPort for ApprovalFacts {
    async fn load(
        &self,
        _: &AccessContext,
        _: &ResourceTargets,
        _: &mut dyn Executor,
    ) -> Result<Vec<ObjectFacts>> {
        let first = self.calls.fetch_add(1, Ordering::SeqCst) == 0;
        Ok(vec![if first { self.original.clone() } else { self.current.clone() }])
    }
}

fn approval_manager_service(current: ObjectFacts) -> (AuthorizationService, Arc<ApprovalFacts>) {
    let identity = Arc::new(Identity::default());
    grant(
        &identity,
        &[(
            "manager",
            &[
                work_item::DETAIL,
                approval_instance::COPIES_HISTORY,
                approval_instance::INSTANCE_LIST,
                approval_admin::SALES_ORDER_RUNTIME,
            ],
        )],
    );
    identity.scopes.lock().unwrap().insert(
        (work_item::DETAIL, work_item::DETAIL),
        ResolvedScope {
            role_clauses: vec![ScopeClause { company: true, ..Default::default() }],
            user_limit: None,
        },
    );
    let task = ObjectFacts {
        subject_type: FactValue::Known("DOCUMENT_APPROVAL".into()),
        subject_source_resource: FactValue::Known("sales_order".into()),
        completed: FactValue::Known(false),
        relations: [(
            RelationKey::parse("approval_instance").unwrap(),
            RelatedObjects {
                expected_ids: ["instance".into()].into(),
                objects: vec![object("approval_instance", "instance", "other")],
            },
        )]
        .into(),
        ..object("work_item", "task", "other")
    };
    let sources =
        Arc::new(ApprovalFacts { original: management_instance(), current, calls: AtomicU64::new(0) });
    let mut builder = complete();
    builder.facilities = identity;
    builder.facts.insert("work_item".into(), Arc::new(Facts::new(vec![task])));
    builder.facts.insert("approval_instance".into(), sources.clone());
    (builder.build().unwrap(), sources)
}

fn management_instance() -> ObjectFacts {
    ObjectFacts {
        subject_type: FactValue::Known("sales_order".into()),
        relations: [(
            RelationKey::parse("subject_source").unwrap(),
            RelatedObjects {
                expected_ids: ["order".into()].into(),
                objects: vec![object("sales_order", "order", "user")],
            },
        )]
        .into(),
        ..object("approval_instance", "instance", "other")
    }
}

#[tokio::test]
async fn approval_detail_revalidates_full_management_source_before_nontransaction_success() {
    for change in ["unchanged", "owner", "version", "missing"] {
        let mut current = management_instance();
        let related = current.relations.get_mut(&RelationKey::parse("subject_source").unwrap()).unwrap();
        match change {
            "owner" => related.objects[0].owner_id = FactValue::Known(Some("other".into())),
            "version" => related.objects[0].version = "v2".into(),
            "missing" => related.objects.clear(),
            _ => {},
        }
        let (service, facts) = approval_manager_service(current);
        let result = service
            .require(
                &actor(),
                &work_item::DETAIL,
                AccessTarget::record("task").unwrap(),
                &mut ReadExecutor::default(),
            )
            .await;
        if change == "unchanged" {
            result.unwrap();
        } else {
            assert!(
                matches!(result, Err(Error::ConflictError(message)) if message.starts_with("DATA_SCOPE_CHANGED")),
                "{change}"
            );
        }
        assert_eq!(facts.calls.load(Ordering::SeqCst), 2);
    }
}

#[tokio::test]
async fn approval_detail_keeps_management_facts_in_the_original_snapshot_without_rereading() {
    let (service, facts) = approval_manager_service(management_instance());
    service
        .require(
            &actor(),
            &work_item::DETAIL,
            AccessTarget::record("task").unwrap(),
            &mut SnapshotExecutor::default(),
        )
        .await
        .unwrap();
    assert_eq!(facts.calls.load(Ordering::SeqCst), 1);
}
