//! 审批管理经真实公共 require 执行资格、冻结类型和来源范围校验。
use std::sync::Arc;
use std::sync::atomic::Ordering;

use erp_permissions::{approval_admin, approval_instance, internal, policy_permission, sales_order};
use persistence_core::ReadExecutor;
use persistence_core::testing::SnapshotExecutor;

use super::super::AuthorizationService;
use super::fixtures::{Facts, Identity, actor, complete, object};
use crate::entity::access_control::ResolvedScope;
use crate::entity::authorization::{AccessTarget, FactValue, ObjectFacts, RelatedObjects, RelationKey};
use crate::{Error, Permission};

fn instance() -> ObjectFacts {
    ObjectFacts {
        resource: "approval_instance".into(),
        id: "instance".into(),
        version: "1".into(),
        subject_type: FactValue::Known("sales_order".into()),
        assignee_id: FactValue::Known(Some("another-approver".into())),
        participants: FactValue::Known(Default::default()),
        completed: FactValue::Known(false),
        relations: [(
            RelationKey::parse("subject_source").unwrap(),
            RelatedObjects {
                expected_ids: ["order".into()].into(),
                objects: vec![object("sales_order", "order", "user")],
            },
        )]
        .into(),
        ..Default::default()
    }
}
fn setup(grants: Vec<(String, Vec<Permission>)>, root: ObjectFacts) -> (AuthorizationService, Arc<Identity>) {
    let identity = Arc::new(Identity::default());
    identity.state.lock().unwrap().grants = grants;
    let mut builder = complete();
    builder.facilities = identity.clone();
    builder.facts.insert("approval_instance".into(), Arc::new(Facts::new(vec![root])));
    builder
        .facts
        .insert("sales_order".into(), Arc::new(Facts::new(vec![object("sales_order", "order", "user")])));
    (builder.build().unwrap(), identity)
}
fn grants() -> Vec<Permission> {
    vec![
        approval_instance::CANCEL_BLOCKED,
        approval_instance::INSTANCE_LIST,
        approval_admin::SALES_ORDER_RUNTIME,
        sales_order::DETAIL,
    ]
}

#[tokio::test]
async fn non_assigned_type_administrator_can_manage_a_fully_readable_real_source() {
    let (service, identity) = setup(vec![("manager".into(), grants())], instance());
    service
        .require(
            &actor(),
            &approval_instance::CANCEL_BLOCKED,
            AccessTarget::record("instance").unwrap(),
            &mut SnapshotExecutor::default(),
        )
        .await
        .unwrap();
    assert_eq!(identity.permissions_calls.load(Ordering::SeqCst), 1);
}

#[tokio::test]
async fn management_rejects_split_role_admin_and_a_different_subject_type_admin() {
    let all = grants();
    let split = vec![("entry".into(), all[..2].to_vec()), ("admin".into(), all[2..].to_vec())];
    let (service, _) = setup(split, instance());
    assert!(matches!(
        service
            .require(
                &actor(),
                &approval_instance::CANCEL_BLOCKED,
                AccessTarget::record("instance").unwrap(),
                &mut SnapshotExecutor::default()
            )
            .await,
        Err(Error::Forbidden(_))
    ));
    let mut wrong = grants();
    wrong[2] = approval_admin::PURCHASE_ORDER_RUNTIME;
    let (service, _) = setup(vec![("wrong".into(), wrong)], instance());
    assert!(matches!(
        service
            .require(
                &actor(),
                &approval_instance::CANCEL_BLOCKED,
                AccessTarget::record("instance").unwrap(),
                &mut SnapshotExecutor::default()
            )
            .await,
        Err(Error::Forbidden(_))
    ));
}

#[tokio::test]
async fn management_rejects_an_incomplete_real_source_even_with_type_admin() {
    let mut root = instance();
    root.relations.values_mut().next().unwrap().objects.clear();
    let (service, _) = setup(vec![("manager".into(), grants())], root);
    assert!(matches!(
        service
            .require(
                &actor(),
                &approval_instance::CANCEL_BLOCKED,
                AccessTarget::record("instance").unwrap(),
                &mut SnapshotExecutor::default()
            )
            .await,
        Err(Error::ValidationError(_))
    ));
}

#[tokio::test]
async fn management_uses_admin_source_scope_without_adding_source_menu_qualification() {
    let mut missing = grants();
    missing.retain(|permission| permission != &sales_order::DETAIL);
    let (service, _) = setup(vec![("manager".into(), missing)], instance());
    service
        .require(
            &actor(),
            &approval_instance::CANCEL_BLOCKED,
            AccessTarget::record("instance").unwrap(),
            &mut SnapshotExecutor::default(),
        )
        .await
        .unwrap();
    let (service, identity) = setup(vec![("manager".into(), grants())], instance());
    identity.state.lock().unwrap().denied_scope = true;
    assert!(matches!(
        service
            .require(
                &actor(),
                &approval_instance::CANCEL_BLOCKED,
                AccessTarget::record("instance").unwrap(),
                &mut SnapshotExecutor::default()
            )
            .await,
        Err(Error::Forbidden(_))
    ));
}

#[tokio::test]
async fn unsubmitted_upgrade_hydrates_real_sources_and_rejects_caller_objects() {
    let granted = vec![
        approval_instance::UPGRADE_BINDING,
        approval_instance::INSTANCE_LIST,
        approval_admin::SALES_ORDER_DEFINITION,
        sales_order::DETAIL,
    ];
    let (service, identity) = setup(vec![("manager".into(), granted)], instance());
    let mut candidate = instance();
    candidate.id.clear();
    candidate.version.clear();
    candidate.relations.values_mut().next().unwrap().objects.clear();
    service
        .require(
            &actor(),
            &approval_instance::UPGRADE_BINDING,
            AccessTarget::Candidate(candidate.clone()),
            &mut SnapshotExecutor::default(),
        )
        .await
        .unwrap();
    assert_eq!(identity.permissions_calls.load(Ordering::SeqCst), 1);
    candidate.relations.values_mut().next().unwrap().objects.push(object("sales_order", "order", "user"));
    assert!(matches!(
        service
            .require(
                &actor(),
                &approval_instance::UPGRADE_BINDING,
                AccessTarget::Candidate(candidate),
                &mut SnapshotExecutor::default()
            )
            .await,
        Err(Error::ValidationError(_))
    ));
}

#[tokio::test]
async fn non_transactional_management_revalidates_the_original_admin_scope_context() {
    let granted = vec![approval_instance::INSTANCE_LIST, approval_admin::SALES_ORDER_RUNTIME];
    let (service, identity) = setup(vec![("manager".into(), granted)], instance());
    identity.scopes.lock().unwrap().insert(
        (sales_order::DETAIL, sales_order::DETAIL),
        ResolvedScope { role_clauses: vec![], user_limit: None },
    );
    service
        .require(
            &actor(),
            &internal::APPROVAL_MANAGEMENT_READ,
            AccessTarget::record("instance").unwrap(),
            &mut ReadExecutor::default(),
        )
        .await
        .unwrap();
    identity.scopes.lock().unwrap().insert(
        (internal::APPROVAL_MANAGEMENT_READ, sales_order::DETAIL),
        ResolvedScope { role_clauses: vec![], user_limit: None },
    );
    assert!(matches!(
        service
            .require(
                &actor(),
                &internal::APPROVAL_MANAGEMENT_READ,
                AccessTarget::record("instance").unwrap(),
                &mut ReadExecutor::default()
            )
            .await,
        Err(Error::Forbidden(_))
    ));
}

#[tokio::test]
async fn receipt_management_keeps_ledger_in_the_same_admin_role_and_requires_whole_sources() {
    let entry = approval_instance::CANCEL_BLOCKED;
    let base =
        vec![entry.clone(), approval_instance::INSTANCE_LIST, approval_admin::CUSTOMER_RECEIPT_RUNTIME];
    let mut root = instance();
    root.subject_type = FactValue::Known("customer_receipt".into());
    let receipt = ObjectFacts {
        resource: "customer_receipt".into(),
        id: "receipt".into(),
        version: "1".into(),
        relations: [
            (
                RelationKey::parse("sales_source").unwrap(),
                RelatedObjects {
                    expected_ids: ["order".into()].into(),
                    objects: vec![object("sales_order", "order", "user")],
                },
            ),
            (RelationKey::parse("purchase_source").unwrap(), RelatedObjects::default()),
            (RelationKey::parse("settlement_source").unwrap(), RelatedObjects::default()),
        ]
        .into(),
        ..Default::default()
    };
    root.relations.insert(
        RelationKey::parse("subject_source").unwrap(),
        RelatedObjects { expected_ids: ["receipt".into()].into(), objects: vec![receipt] },
    );
    let split = vec![
        ("manager".into(), base.clone()),
        ("ledger".into(), vec![policy_permission::FINANCE_LEDGER_READ]),
    ];
    let (service, _) = setup(split, root.clone());
    assert!(matches!(
        service
            .require(
                &actor(),
                &entry,
                AccessTarget::record("instance").unwrap(),
                &mut SnapshotExecutor::default()
            )
            .await,
        Err(Error::Forbidden(_))
    ));
    let mut combined = base;
    combined.push(policy_permission::FINANCE_LEDGER_READ);
    let (service, _) = setup(vec![("complete".into(), combined.clone())], root.clone());
    service
        .require(
            &actor(),
            &entry,
            AccessTarget::record("instance").unwrap(),
            &mut SnapshotExecutor::default(),
        )
        .await
        .unwrap();
    root.relations.values_mut().next().unwrap().objects[0]
        .relations
        .get_mut(&RelationKey::parse("sales_source").unwrap())
        .unwrap()
        .objects
        .clear();
    let (service, _) = setup(vec![("complete".into(), combined)], root);
    assert!(matches!(
        service
            .require(
                &actor(),
                &entry,
                AccessTarget::record("instance").unwrap(),
                &mut SnapshotExecutor::default()
            )
            .await,
        Err(Error::Forbidden(_))
    ));
}

#[tokio::test]
async fn all_approval_source_deadlines_are_checked_after_the_last_async_revalidation() {
    use std::collections::BTreeSet;
    use std::sync::atomic::AtomicU64;

    use application_core::AuditActor;
    use async_trait::async_trait;
    use entity_core::BaseModel;
    use erp_core::common::time::Instant;
    use erp_permissions::purchase_order;
    use persistence_core::Executor;

    use super::super::facilities::Facilities;
    use crate::entity::authorization::{AccessContext, ResourceTargets};
    use crate::entity::organization::{OrgMembership, OrgValidity};
    use crate::entity::resource_access::ResourceKind;
    use crate::service::access_control::resolve::AuthorizedDataScope;
    use crate::{QualifiedPermissions, Result};

    struct ExpiringSources {
        identity: Identity,
        deadline: Instant,
        purchase_reads: AtomicU64,
        delay: bool,
    }
    #[async_trait]
    impl Facilities for ExpiringSources {
        async fn permissions(
            &self,
            actor: &AuditActor,
            permissions: &[Permission],
            executor: &mut dyn Executor,
        ) -> Result<Option<QualifiedPermissions>> {
            self.identity.permissions(actor, permissions, executor).await
        }
        async fn scope(
            &self,
            context: &AccessContext,
            source: &Permission,
            required: &[Permission],
            qualified: &QualifiedPermissions,
            executor: &mut dyn Executor,
        ) -> Result<AuthorizedDataScope> {
            let mut access = self.identity.scope(context, source, required, qualified, executor).await?;
            if source == &sales_order::DETAIL {
                access.facts.organizations.memberships.push(OrgMembership {
                    base: BaseModel::fake(),
                    user_id: actor().id().into(),
                    org_unit_id: "sales".into(),
                    validity: OrgValidity {
                        valid_from: Instant::from_unix_secs(1),
                        valid_to: Some(self.deadline),
                    },
                    changed_by: "admin".into(),
                    reason: "test".into(),
                });
            }
            if source == &purchase_order::DETAIL
                && self.purchase_reads.fetch_add(1, Ordering::SeqCst) > 0
                && self.delay
            {
                tokio::time::sleep(std::time::Duration::from_secs(2)).await;
            }
            Ok(access)
        }
        async fn grant_revision(
            &self,
            actor: &AuditActor,
            resource: ResourceKind,
            executor: &mut dyn Executor,
        ) -> Result<u64> {
            self.identity.grant_revision(actor, resource, executor).await
        }
        async fn grants(
            &self,
            context: &AccessContext,
            resource: ResourceKind,
            ids: &ResourceTargets,
            executor: &mut dyn Executor,
        ) -> Result<BTreeSet<String>> {
            self.identity.grants(context, resource, ids, executor).await
        }
    }
    for delay in [false, true] {
        let identity = Identity::default();
        identity.state.lock().unwrap().grants = vec![(
            "manager".into(),
            vec![
                approval_instance::INSTANCE_LIST,
                approval_admin::SALES_ORDER_RUNTIME,
                approval_admin::PURCHASE_ORDER_RUNTIME,
            ],
        )];
        let facilities = Arc::new(ExpiringSources {
            identity,
            delay,
            deadline: Instant::from_unix_secs(Instant::now().unix_secs() + 2),
            purchase_reads: AtomicU64::new(0),
        });
        let mut sales = instance();
        sales.id = "a-sales".into();
        let mut purchase = instance();
        purchase.id = "b-purchase".into();
        purchase.subject_type = FactValue::Known("purchase_order".into());
        purchase.relations.values_mut().next().unwrap().objects[0].resource = "purchase_order".into();
        let mut builder = complete();
        builder.facilities = facilities;
        builder.facts.insert("approval_instance".into(), Arc::new(Facts::new(vec![sales, purchase])));
        let service = builder.build().unwrap();
        let result = service
            .require(
                &actor(),
                &internal::APPROVAL_MANAGEMENT_READ,
                AccessTarget::Records(ResourceTargets::new(&["a-sales", "b-purchase"]).unwrap()),
                &mut ReadExecutor::default(),
            )
            .await;
        if delay {
            assert!(matches!(result, Err(Error::ConflictError(_))));
        } else {
            result.unwrap();
        }
    }
}

#[tokio::test]
async fn ordinary_instance_reads_do_not_require_current_business_sources() {
    for history in [false, true] {
        let mut root = instance();
        root.relations.clear();
        root.owner_id = FactValue::Known(Some(if history { "other" } else { "user" }.into()));
        root.participants =
            FactValue::Known(if history { ["user".into()].into() } else { Default::default() });
        let (service, _) = setup(vec![("reader".into(), vec![approval_instance::INSTANCE_LIST])], root);
        service
            .require(
                &actor(),
                &internal::APPROVAL_INSTANCE_READ,
                AccessTarget::record("instance").unwrap(),
                &mut SnapshotExecutor::default(),
            )
            .await
            .unwrap();
    }
}

#[tokio::test]
async fn current_instance_responsibility_requires_same_role_decision_qualification() {
    let mut root = instance();
    root.relations.clear();
    root.owner_id = FactValue::Known(Some("starter".into()));
    root.assignee_id = FactValue::Known(Some("user".into()));
    let (service, _) = setup(
        vec![("reader".into(), vec![approval_instance::INSTANCE_LIST, approval_instance::SUBMIT_DECISION])],
        root.clone(),
    );
    service
        .require(
            &actor(),
            &internal::APPROVAL_INSTANCE_READ,
            AccessTarget::record("instance").unwrap(),
            &mut SnapshotExecutor::default(),
        )
        .await
        .unwrap();
    let (service, _) = setup(
        vec![
            ("reader".into(), vec![approval_instance::INSTANCE_LIST]),
            ("decider".into(), vec![approval_instance::SUBMIT_DECISION]),
        ],
        root,
    );
    assert!(
        service
            .require(
                &actor(),
                &internal::APPROVAL_INSTANCE_READ,
                AccessTarget::record("instance").unwrap(),
                &mut SnapshotExecutor::default()
            )
            .await
            .is_err()
    );
}

#[tokio::test]
async fn ordinary_instance_management_fallback_keeps_source_grants_and_rejects_missing_sources() {
    use async_trait::async_trait;
    use persistence_core::Executor;

    use crate::entity::authorization::{AccessContext, ResourceTargets};
    use crate::entity::resource_access::ResourceKind;
    use crate::{AccessFactsPort, Result};

    struct ConditionalFacts {
        root: ObjectFacts,
        expanded: ObjectFacts,
    }
    #[async_trait]
    impl AccessFactsPort for ConditionalFacts {
        async fn load(
            &self,
            context: &AccessContext,
            _: &ResourceTargets,
            _: &mut dyn Executor,
        ) -> Result<Vec<ObjectFacts>> {
            Ok(vec![if context.permission == internal::APPROVAL_MANAGEMENT_READ {
                self.expanded.clone()
            } else {
                self.root.clone()
            }])
        }
    }
    for complete_source in [true, false] {
        let mut expanded = instance();
        expanded.owner_id = FactValue::Known(Some("starter".into()));
        expanded.relations.values_mut().next().unwrap().objects[0].owner_id =
            FactValue::Known(Some("other".into()));
        if !complete_source {
            expanded.relations.values_mut().next().unwrap().objects.clear();
        }
        let mut root = expanded.clone();
        root.relations.clear();
        let identity = Arc::new(Identity::default());
        {
            let mut state = identity.state.lock().unwrap();
            state.grants = vec![(
                "manager".into(),
                vec![approval_instance::INSTANCE_LIST, approval_admin::SALES_ORDER_RUNTIME],
            )];
            state.denied_scope = true;
        }
        identity.record_grants.lock().unwrap().insert((ResourceKind::SalesOrder, "order".into()));
        let mut builder = complete();
        builder.facilities = identity;
        builder.facts.insert("approval_instance".into(), Arc::new(ConditionalFacts { root, expanded }));
        let service = builder.build().unwrap();
        let result = service
            .require(
                &actor(),
                &internal::APPROVAL_INSTANCE_READ,
                AccessTarget::record("instance").unwrap(),
                &mut SnapshotExecutor::default(),
            )
            .await;
        if complete_source {
            result.unwrap();
        } else {
            assert!(result.is_err());
        }
    }
}
