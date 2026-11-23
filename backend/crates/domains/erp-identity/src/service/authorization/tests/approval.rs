//! 审批裁决、撤回、管理及管理列表查询的统一授权测试。
use std::collections::BTreeSet;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use application_core::AuditActor;
use async_trait::async_trait;
use entity_core::BaseModel;
use erp_core::common::time::Instant;
use erp_permissions::{
    Permission, approval_admin, approval_instance, internal, inventory, policy_permission, purchase_order,
    sales_order,
};
use persistence_core::testing::SnapshotExecutor;
use persistence_core::{Executor, ReadExecutor};

use super::super::facilities::Facilities;
use super::fixtures::{AuthHarness, Facts, Identity, actor, attach_relation, complete, object};
use crate::entity::access_control::{ResolvedScope, ScopeClause};
use crate::entity::authorization::{
    AccessContext, AccessRule, AccessTarget, FactValue, ObjectFacts, RelatedObjects, RelationKey,
    ResourceTargets,
};
use crate::entity::organization::{OrgMembership, OrgValidity};
use crate::entity::resource_access::{ResourceKind, SourceKey};
use crate::ports::AccessFactsPort;
use crate::repository::authorization::AccessValidity;
use crate::repository::authorization::testing::management_members;
use crate::service::access_control::resolve::AuthorizedDataScope;
use crate::{Error, QualifiedPermissions, Result};

fn decision_task(kind: &str, resource: &str) -> ObjectFacts {
    let mut task = object("approval_instance", "task", "user");
    task.subject_type = FactValue::Known(kind.into());
    task.assignee_id = FactValue::Known(Some("user".into()));
    attach_relation(&mut task, "subject_source", object(resource, "order", "other"));
    let mut inst = object("approval_instance", "instance", "submitter");
    inst.subject_type = FactValue::Known(kind.into());
    attach_relation(&mut task, "decision_instance", inst);
    task
}

fn decision_key(resource: ResourceKind, instance: &str) -> SourceKey {
    SourceKey {
        rule: match resource {
            ResourceKind::SalesOrder => "sales_order_approval_assigned",
            ResourceKind::PurchaseOrder => "purchase_order_approval_assigned",
        }
        .into(),
        business_type: "approval_instance".into(),
        business_id: instance.into(),
        recipient: "user".into(),
    }
}

fn task_target() -> AccessTarget {
    AccessTarget::Task { id: "task".into(), version: "v1".into() }
}

fn stock_cancel_instance(owner: &str) -> ObjectFacts {
    let mut source = object("stock_adjustment", "adjustment", "other");
    source.warehouse_id = FactValue::Known(Some("warehouse".into()));
    attach_relation(&mut source, "warehouse", object("warehouse", "warehouse", "other"));
    let mut inst = object("approval_instance", "instance", owner);
    inst.subject_type = FactValue::Known("stock_adjustment".into());
    attach_relation(&mut inst, "subject_source", source);
    inst
}

fn management_instance() -> ObjectFacts {
    let mut root = ObjectFacts {
        resource: "approval_instance".into(),
        id: "instance".into(),
        version: "1".into(),
        subject_type: FactValue::Known("sales_order".into()),
        assignee_id: FactValue::Known(Some("another-approver".into())),
        participants: FactValue::Known(Default::default()),
        completed: FactValue::Known(false),
        ..Default::default()
    };
    attach_relation(&mut root, "subject_source", object("sales_order", "order", "user"));
    root
}

fn sales_cancel_blocked_grants() -> [Permission; 4] {
    [
        approval_instance::CANCEL_BLOCKED,
        approval_instance::INSTANCE_LIST,
        approval_admin::SALES_ORDER_RUNTIME,
        sales_order::DETAIL,
    ]
}

// ── 审批裁决 ──────────────────────────────────────────────────────────────────

#[tokio::test]
async fn decision_uses_approver_role_scope_without_requiring_source_menu_permission() {
    for (kind, resource, permission) in [
        ("sales_order", "sales_order", sales_order::DETAIL),
        ("purchase_order", "purchase_order", purchase_order::DETAIL),
    ] {
        let h = AuthHarness::with_facts("approval_instance", vec![decision_task(kind, resource)]);
        h.grants(&[("approver", &[approval_instance::INSTANCE_LIST, approval_instance::SUBMIT_DECISION])]);
        h.set_role_scope("approver", &permission, ScopeClause { company: true, ..Default::default() });
        h.require_snap(&approval_instance::SUBMIT_DECISION, task_target()).await.unwrap();

        h.identity.role_scopes.lock().unwrap().clear();
        assert!(matches!(
            h.require_snap(&approval_instance::SUBMIT_DECISION, task_target()).await,
            Err(Error::Forbidden(_))
        ));
    }
}

#[tokio::test]
async fn only_current_instance_recipient_rule_and_real_target_grant_can_supplement_scope() {
    for (kind, resource, target_kind) in [
        ("sales_order", "sales_order", ResourceKind::SalesOrder),
        ("purchase_order", "purchase_order", ResourceKind::PurchaseOrder),
    ] {
        let h = AuthHarness::with_facts("approval_instance", vec![decision_task(kind, resource)]);
        h.grants(&[("approver", &[approval_instance::INSTANCE_LIST, approval_instance::SUBMIT_DECISION])]);

        h.add_record_grant(target_kind, "order");
        assert!(matches!(
            h.require_snap(&approval_instance::SUBMIT_DECISION, task_target()).await,
            Err(Error::Forbidden(_))
        ));

        for source in [
            decision_key(target_kind, "different-instance"),
            SourceKey { recipient: "other".into(), ..decision_key(target_kind, "instance") },
            SourceKey { rule: "manual_read".into(), ..decision_key(target_kind, "instance") },
        ] {
            h.add_source_grant(&source, target_kind, "order");
            assert!(matches!(
                h.require_snap(&approval_instance::SUBMIT_DECISION, task_target()).await,
                Err(Error::Forbidden(_))
            ));
        }

        h.add_source_grant(&decision_key(target_kind, "instance"), target_kind, "unrelated-order");
        assert!(matches!(
            h.require_snap(&approval_instance::SUBMIT_DECISION, task_target()).await,
            Err(Error::Forbidden(_))
        ));

        h.add_source_grant(&decision_key(target_kind, "instance"), target_kind, "order");
        h.require_snap(&approval_instance::SUBMIT_DECISION, task_target()).await.unwrap();
    }
}

#[tokio::test]
async fn wrong_source_missing_instance_split_roles_and_reassignment_fail_closed() {
    let h = AuthHarness::with_facts("approval_instance", vec![decision_task("sales_order", "sales_order")]);
    h.grants(&[("approver", &[approval_instance::INSTANCE_LIST, approval_instance::SUBMIT_DECISION])]);
    h.add_source_grant(
        &decision_key(ResourceKind::SalesOrder, "instance"),
        ResourceKind::SalesOrder,
        "order",
    );
    h.require_snap(&approval_instance::SUBMIT_DECISION, task_target()).await.unwrap();

    h.grants(&[
        ("reader", &[approval_instance::INSTANCE_LIST]),
        ("decider", &[approval_instance::SUBMIT_DECISION]),
    ]);
    assert!(matches!(
        h.require_snap(&approval_instance::SUBMIT_DECISION, task_target()).await,
        Err(Error::Forbidden(_))
    ));

    h.grants(&[("approver", &[approval_instance::INSTANCE_LIST, approval_instance::SUBMIT_DECISION])]);
    h.mutate_first("approval_instance", |t| t.assignee_id = FactValue::Known(Some("other".into())));
    assert!(matches!(
        h.require_snap(&approval_instance::SUBMIT_DECISION, task_target()).await,
        Err(Error::Forbidden(_))
    ));

    h.set_objects("approval_instance", vec![decision_task("sales_order", "purchase_order")]);
    assert!(matches!(
        h.require_snap(&approval_instance::SUBMIT_DECISION, task_target()).await,
        Err(Error::ValidationError(_))
    ));

    let mut missing = decision_task("sales_order", "sales_order");
    missing.relations.remove(&RelationKey::parse("decision_instance").unwrap());
    h.set_objects("approval_instance", vec![missing]);
    assert!(matches!(
        h.require_snap(&approval_instance::SUBMIT_DECISION, task_target()).await,
        Err(Error::ValidationError(_))
    ));
}

// ── 库存审批撤回 ──────────────────────────────────────────────────────────────

fn stock_cancel_harness(owner: &str) -> AuthHarness {
    let h = AuthHarness::with_facts("approval_instance", vec![stock_cancel_instance(owner)]);
    h.set_scope(
        &inventory::STOCK_ADJUSTMENT_DETAIL,
        &inventory::STOCK_ADJUSTMENT_DETAIL,
        ResolvedScope {
            role_clauses: vec![ScopeClause {
                warehouse_ids: ["warehouse".into()].into(),
                ..Default::default()
            }],
            user_limit: None,
        },
    );
    h
}

#[tokio::test]
async fn original_submitter_requires_same_role_cancel_and_read_without_stock_detail_menu_or_scope() {
    let h = stock_cancel_harness("user");
    h.set_denied_scope(true);
    h.identity.scopes.lock().unwrap().clear();
    let required = [inventory::STOCK_ADJUSTMENT_CANCEL_APPROVAL, approval_instance::INSTANCE_LIST];
    h.assert_same_role_required(&[], &required, &inventory::STOCK_ADJUSTMENT_CANCEL_APPROVAL, || {
        AccessTarget::record("instance").unwrap()
    })
    .await;
}

#[tokio::test]
async fn runtime_admin_preserves_same_role_management_and_independent_stock_detail() {
    let h = stock_cancel_harness("other");
    let required = [
        inventory::STOCK_ADJUSTMENT_CANCEL_APPROVAL,
        approval_instance::INSTANCE_LIST,
        approval_admin::STOCK_ADJUSTMENT_RUNTIME,
    ];
    let target = || AccessTarget::record("instance").unwrap();
    h.assert_same_role_required(
        &[("stock-reader", &[inventory::STOCK_ADJUSTMENT_DETAIL])],
        &required,
        &inventory::STOCK_ADJUSTMENT_CANCEL_APPROVAL,
        target,
    )
    .await;

    h.grants(&[("manager", &required)]);
    assert!(matches!(
        h.require_snap(&inventory::STOCK_ADJUSTMENT_CANCEL_APPROVAL, target()).await,
        Err(Error::Forbidden(_))
    ));

    h.grants(&[("manager", &required), ("stock-reader", &[inventory::STOCK_ADJUSTMENT_DETAIL])]);
    h.identity.scopes.lock().unwrap().values_mut().next().unwrap().role_clauses[0].warehouse_ids =
        ["other-warehouse".into()].into();
    assert!(matches!(
        h.require_snap(&inventory::STOCK_ADJUSTMENT_CANCEL_APPROVAL, target()).await,
        Err(Error::Forbidden(_))
    ));
}

#[tokio::test]
async fn cancellation_rejects_missing_sources_wrong_types_and_unversioned_facts_even_for_submitter() {
    let h = stock_cancel_harness("user");
    h.grants(&[(
        "submitter",
        &[inventory::STOCK_ADJUSTMENT_CANCEL_APPROVAL, approval_instance::INSTANCE_LIST],
    )]);
    let perm = &inventory::STOCK_ADJUSTMENT_CANCEL_APPROVAL;
    let target = || AccessTarget::record("instance").unwrap();
    for invalid in ["missing-source", "wrong-type", "missing-warehouse", "wrong-warehouse", "version"] {
        let mut root = stock_cancel_instance("user");
        match invalid {
            "missing-source" => root.relations.values_mut().next().unwrap().objects.clear(),
            "wrong-type" => root.subject_type = FactValue::Known("sales_order".into()),
            "missing-warehouse" => root.relations.values_mut().next().unwrap().objects[0].relations.clear(),
            "wrong-warehouse" => {
                root.relations.values_mut().next().unwrap().objects[0].warehouse_id =
                    FactValue::Known(Some("other-warehouse".into()))
            },
            "version" => root.version.clear(),
            _ => unreachable!(),
        }
        h.set_objects("approval_instance", vec![root]);
        assert!(matches!(h.require_snap(perm, target()).await, Err(Error::ValidationError(_))), "{invalid}");
    }
    assert!(matches!(h.require_snap(perm, AccessTarget::Operation).await, Err(Error::ValidationError(_))));
    assert!(matches!(h.require_read(perm, target()).await, Err(Error::ValidationError(_))));
}

// ── 审批管理与回退 ────────────────────────────────────────────────────────────

fn management_harness(root: ObjectFacts) -> AuthHarness {
    AuthHarness::with_multi_facts(&[
        ("approval_instance", vec![root]),
        ("sales_order", vec![object("sales_order", "order", "user")]),
    ])
}

#[tokio::test]
async fn non_assigned_type_administrator_can_manage_a_fully_readable_real_source() {
    let h = management_harness(management_instance());
    h.grants(&[("manager", &sales_cancel_blocked_grants())]);
    h.require_snap(&approval_instance::CANCEL_BLOCKED, AccessTarget::record("instance").unwrap())
        .await
        .unwrap();
    assert_eq!(h.permission_calls(), 1);
}

#[tokio::test]
async fn management_rejects_split_role_admin_and_a_different_subject_type_admin() {
    let h = management_harness(management_instance());
    let all = sales_cancel_blocked_grants();
    let target = || AccessTarget::record("instance").unwrap();
    h.grants(&[("entry", &all[..2]), ("admin", &all[2..])]);
    assert!(matches!(
        h.require_snap(&approval_instance::CANCEL_BLOCKED, target()).await,
        Err(Error::Forbidden(_))
    ));

    let mut wrong = all;
    wrong[2] = approval_admin::PURCHASE_ORDER_RUNTIME;
    h.grants(&[("wrong", &wrong)]);
    assert!(matches!(
        h.require_snap(&approval_instance::CANCEL_BLOCKED, target()).await,
        Err(Error::Forbidden(_))
    ));
}

#[tokio::test]
async fn management_rejects_an_incomplete_real_source_even_with_type_admin() {
    let mut root = management_instance();
    root.relations.values_mut().next().unwrap().objects.clear();
    let h = management_harness(root);
    h.grants(&[("manager", &sales_cancel_blocked_grants())]);
    assert!(matches!(
        h.require_snap(&approval_instance::CANCEL_BLOCKED, AccessTarget::record("instance").unwrap()).await,
        Err(Error::ValidationError(_))
    ));
}

#[tokio::test]
async fn management_uses_admin_source_scope_without_adding_source_menu_qualification() {
    let h = management_harness(management_instance());
    let mut missing = sales_cancel_blocked_grants().to_vec();
    missing.retain(|p| p != &sales_order::DETAIL);
    h.grants(&[("manager", &missing)]);
    h.require_snap(&approval_instance::CANCEL_BLOCKED, AccessTarget::record("instance").unwrap())
        .await
        .unwrap();

    h.grants(&[("manager", &sales_cancel_blocked_grants())]);
    h.set_denied_scope(true);
    assert!(matches!(
        h.require_snap(&approval_instance::CANCEL_BLOCKED, AccessTarget::record("instance").unwrap()).await,
        Err(Error::Forbidden(_))
    ));
}

#[tokio::test]
async fn unsubmitted_upgrade_hydrates_real_sources_and_rejects_caller_objects() {
    let h = management_harness(management_instance());
    h.grants(&[(
        "manager",
        &[
            approval_instance::UPGRADE_BINDING,
            approval_instance::INSTANCE_LIST,
            approval_admin::SALES_ORDER_DEFINITION,
            sales_order::DETAIL,
        ],
    )]);
    let mut candidate = management_instance();
    candidate.id.clear();
    candidate.version.clear();
    candidate.relations.values_mut().next().unwrap().objects.clear();
    h.require_snap(&approval_instance::UPGRADE_BINDING, AccessTarget::Candidate(candidate.clone()))
        .await
        .unwrap();
    assert_eq!(h.permission_calls(), 1);

    candidate.relations.values_mut().next().unwrap().objects.push(object("sales_order", "order", "user"));
    assert!(matches!(
        h.require_snap(&approval_instance::UPGRADE_BINDING, AccessTarget::Candidate(candidate)).await,
        Err(Error::ValidationError(_))
    ));
}

#[tokio::test]
async fn non_transactional_management_revalidates_the_original_admin_scope_context() {
    let h = management_harness(management_instance());
    h.grants(&[("manager", &[approval_instance::INSTANCE_LIST, approval_admin::SALES_ORDER_RUNTIME])]);
    h.set_scope(
        &sales_order::DETAIL,
        &sales_order::DETAIL,
        ResolvedScope { role_clauses: vec![], user_limit: None },
    );
    h.require_read(&internal::APPROVAL_MANAGEMENT_READ, AccessTarget::record("instance").unwrap())
        .await
        .unwrap();

    h.set_scope(
        &internal::APPROVAL_MANAGEMENT_READ,
        &sales_order::DETAIL,
        ResolvedScope { role_clauses: vec![], user_limit: None },
    );
    assert!(matches!(
        h.require_read(&internal::APPROVAL_MANAGEMENT_READ, AccessTarget::record("instance").unwrap()).await,
        Err(Error::Forbidden(_))
    ));
}

#[tokio::test]
async fn receipt_management_keeps_ledger_in_the_same_admin_role_and_requires_whole_sources() {
    let entry = approval_instance::CANCEL_BLOCKED;
    let base = [entry.clone(), approval_instance::INSTANCE_LIST, approval_admin::CUSTOMER_RECEIPT_RUNTIME];
    let mut root = management_instance();
    root.subject_type = FactValue::Known("customer_receipt".into());
    let mut receipt = ObjectFacts {
        resource: "customer_receipt".into(),
        id: "receipt".into(),
        version: "1".into(),
        ..Default::default()
    };
    attach_relation(&mut receipt, "sales_source", object("sales_order", "order", "user"));
    receipt.relations.insert(RelationKey::parse("purchase_source").unwrap(), RelatedObjects::default());
    receipt.relations.insert(RelationKey::parse("settlement_source").unwrap(), RelatedObjects::default());
    attach_relation(&mut root, "subject_source", receipt);

    let h = management_harness(root);
    let target = || AccessTarget::record("instance").unwrap();
    let mut wrong_type = base.clone();
    wrong_type[2] = approval_admin::RECEIPT_REVERSAL_RUNTIME;
    h.grants(&[("manager", &wrong_type)]);
    assert!(matches!(
        h.require_snap(&entry, target()).await,
        Err(Error::Forbidden(m)) if m == "当前账号不具备该主体类型的同角色完整审批管理资格"
    ));

    h.grants(&[("manager", &base), ("ledger", &[policy_permission::FINANCE_LEDGER_READ])]);
    assert!(matches!(
        h.require_snap(&entry, target()).await,
        Err(Error::Forbidden(m)) if m == "审批管理的真实来源缺少完整同角色资格"
    ));

    let mut combined = base.to_vec();
    combined.push(policy_permission::FINANCE_LEDGER_READ);
    h.grants(&[("complete", &combined)]);
    h.require_snap(&entry, target()).await.unwrap();

    h.mutate_first("approval_instance", |inst| {
        inst.relations.values_mut().next().unwrap().objects[0]
            .relations
            .get_mut(&RelationKey::parse("sales_source").unwrap())
            .unwrap()
            .objects
            .clear();
    });
    assert!(matches!(h.require_snap(&entry, target()).await, Err(Error::Forbidden(_))));
}

#[tokio::test]
async fn all_approval_source_deadlines_are_checked_after_the_last_async_revalidation() {
    struct ExpiringSources {
        identity: Identity,
        deadline: Instant,
        purchase_reads: AtomicU64,
        expire_during_revalidation: bool,
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
                access.organizations.memberships.push(OrgMembership {
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
                && self.expire_during_revalidation
            {
                AccessValidity::set_test_time(self.deadline);
            }
            Ok(access)
        }
        async fn grant_revision(&self, a: &AuditActor, r: ResourceKind, e: &mut dyn Executor) -> Result<u64> {
            self.identity.grant_revision(a, r, e).await
        }
        async fn grants(
            &self,
            c: &AccessContext,
            r: ResourceKind,
            ids: &ResourceTargets,
            e: &mut dyn Executor,
        ) -> Result<BTreeSet<String>> {
            self.identity.grants(c, r, ids, e).await
        }
    }

    let deadline = Instant::from_unix_secs(4_102_444_800);
    let before = Instant::from_unix_secs(deadline.unix_secs() - 1);
    for expire_during_revalidation in [false, true] {
        AccessValidity::with_test_time(before, async {
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
                expire_during_revalidation,
                deadline,
                purchase_reads: AtomicU64::new(0),
            });
            let mut sales = management_instance();
            sales.id = "a-sales".into();
            let mut purchase = management_instance();
            purchase.id = "b-purchase".into();
            purchase.subject_type = FactValue::Known("purchase_order".into());
            purchase.relations.values_mut().next().unwrap().objects[0].resource = "purchase_order".into();
            let mut builder = complete();
            builder.facilities = facilities.clone();
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
            assert_eq!(facilities.purchase_reads.load(Ordering::SeqCst), 2);
            if expire_during_revalidation {
                assert!(matches!(result, Err(Error::ConflictError(m)) if m == "DATA_SCOPE_CHANGED：授权关系已到期"));
            } else {
                result.unwrap();
            }
        })
        .await;
    }
}

#[tokio::test]
async fn ordinary_instance_reads_do_not_require_current_business_sources() {
    for history in [false, true] {
        let mut root = management_instance();
        root.relations.clear();
        root.owner_id = FactValue::Known(Some(if history { "other" } else { "user" }.into()));
        root.participants =
            FactValue::Known(if history { ["user".into()].into() } else { Default::default() });
        let h = management_harness(root);
        h.grants(&[("reader", &[approval_instance::INSTANCE_LIST])]);
        h.require_snap(&internal::APPROVAL_INSTANCE_READ, AccessTarget::record("instance").unwrap())
            .await
            .unwrap();
    }
}

#[tokio::test]
async fn current_instance_responsibility_requires_same_role_decision_qualification() {
    let mut root = management_instance();
    root.relations.clear();
    root.owner_id = FactValue::Known(Some("starter".into()));
    root.assignee_id = FactValue::Known(Some("user".into()));
    let h = management_harness(root);
    let target = || AccessTarget::record("instance").unwrap();

    h.grants(&[("reader", &[approval_instance::INSTANCE_LIST, approval_instance::SUBMIT_DECISION])]);
    h.require_snap(&internal::APPROVAL_INSTANCE_READ, target()).await.unwrap();

    h.grants(&[
        ("reader", &[approval_instance::INSTANCE_LIST]),
        ("decider", &[approval_instance::SUBMIT_DECISION]),
    ]);
    assert!(h.require_snap(&internal::APPROVAL_INSTANCE_READ, target()).await.is_err());
}

#[tokio::test]
async fn ordinary_instance_management_fallback_keeps_source_grants_and_rejects_missing_sources() {
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
        let mut expanded = management_instance();
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
        let result = builder
            .build()
            .unwrap()
            .require(
                &actor(),
                &internal::APPROVAL_INSTANCE_READ,
                AccessTarget::record("instance").unwrap(),
                &mut SnapshotExecutor::default(),
            )
            .await;
        assert_eq!(result.is_ok(), complete_source);
    }
}

// ── 审批管理列表查询 ──────────────────────────────────────────────────────────

fn management_query_harness() -> AuthHarness {
    let mut root = object("approval_instance", "sales", "user");
    root.subject_type = FactValue::Known("sales_order".into());
    attach_relation(&mut root, "subject_source", object("sales_order", "sale", "user"));
    let h = AuthHarness::with_facts("approval_instance", vec![root]);
    h.grants(&[("manager", &[approval_instance::INSTANCE_LIST, approval_admin::SALES_ORDER_RUNTIME])]);
    h
}

#[tokio::test]
async fn management_query_keeps_type_qualification_real_source_and_object_membership_together() {
    let h = management_query_harness();
    let mut ex = SnapshotExecutor::default();
    let perm = &internal::APPROVAL_MANAGEMENT_READ;
    let access = h.query(perm, &mut ex).await.unwrap();
    assert_eq!(management_members(&access), ["sales"]);
    h.validate(&access, &mut ex).await.unwrap();
    h.require(perm, AccessTarget::record("sales").unwrap(), &mut ex).await.unwrap();

    h.set_denied_scope(true);
    let access = h.query(perm, &mut ex).await.unwrap();
    assert!(management_members(&access).is_empty());
    h.validate(&access, &mut ex).await.unwrap();
    assert!(matches!(
        h.require(perm, AccessTarget::record("sales").unwrap(), &mut ex).await,
        Err(Error::Forbidden(_))
    ));

    h.set_denied_scope(false);
    h.mutate_first("approval_instance", |inst| inst.relations.values_mut().next().unwrap().objects.clear());
    assert!(matches!(
        h.require(perm, AccessTarget::record("sales").unwrap(), &mut ex).await,
        Err(Error::ValidationError(_))
    ));
}

#[tokio::test]
async fn management_query_does_not_splice_type_admin_from_another_role_or_other_type() {
    let h = management_query_harness();
    let mut ex = SnapshotExecutor::default();
    h.grants(&[
        ("entry", &[approval_instance::INSTANCE_LIST]),
        ("admin", &[approval_admin::SALES_ORDER_RUNTIME]),
    ]);
    let access = h.query(&internal::APPROVAL_MANAGEMENT_READ, &mut ex).await.unwrap();
    assert!(management_members(&access).is_empty());
    h.validate(&access, &mut ex).await.unwrap();

    h.grants(&[(
        "purchase-manager",
        &[approval_instance::INSTANCE_LIST, approval_admin::PURCHASE_ORDER_RUNTIME],
    )]);
    let access = h.query(&internal::APPROVAL_MANAGEMENT_READ, &mut ex).await.unwrap();
    assert_eq!(management_members(&access), ["purchase"]);
    h.validate(&access, &mut ex).await.unwrap();
}

#[tokio::test]
async fn refund_management_query_requires_same_role_ledger_only_for_funds_sources() {
    let h = management_query_harness();
    let refund_admin = [
        approval_instance::INSTANCE_LIST,
        approval_admin::CUSTOMER_REFUND_RUNTIME,
        approval_admin::SUPPLIER_REFUND_RUNTIME,
    ];
    let ledger = policy_permission::FINANCE_LEDGER_READ;
    let mut ex = ReadExecutor::default();
    for combined in [false, true] {
        if combined {
            let mut grants = refund_admin.to_vec();
            grants.push(ledger.clone());
            h.grants(&[("manager", &grants)]);
        } else {
            h.grants(&[("manager", &refund_admin), ("ledger", std::slice::from_ref(&ledger))]);
        }
        let access = h.query(&internal::APPROVAL_MANAGEMENT_READ, &mut ex).await.unwrap();
        let AccessRule::Any(partitions) = &access.revalidation.as_ref().unwrap().rules.root else {
            panic!("管理列表必须保留两个退款类型的账户来源");
        };
        let keys = partitions
            .iter()
            .map(|p| match p {
                AccessRule::Related { relation, .. } => relation.as_str(),
                _ => panic!("退款分区必须检查真实来源"),
            })
            .collect::<Vec<_>>();
        let expected: &[&str] = if combined {
            &[
                "management_customer_refund_customer_receipt",
                "management_customer_refund_receivable_account",
                "management_supplier_refund_supplier_payment",
                "management_supplier_refund_payable_account",
            ]
        } else {
            &["management_customer_refund_receivable_account", "management_supplier_refund_payable_account"]
        };
        assert_eq!(keys.as_slice(), expected);
    }
}

#[tokio::test]
async fn management_query_retains_scope_expiry_and_rejects_revocation_before_delivery() {
    let h = management_query_harness();
    let deadline = Instant::from_unix_secs(4_102_444_800);
    h.identity.scope_deadlines.lock().unwrap().insert(internal::APPROVAL_MANAGEMENT_READ, deadline);
    let mut ex = SnapshotExecutor::default();
    let mut access = h.query(&internal::APPROVAL_MANAGEMENT_READ, &mut ex).await.unwrap();
    assert_eq!(access.validity.valid_until, Some(deadline));
    assert_eq!(management_members(&access), ["sales"]);
    access.validity.valid_until = Some(Instant::from_unix_secs(1));
    assert!(matches!(h.validate(&access, &mut ex).await, Err(Error::ConflictError(_))));

    let mut ex = ReadExecutor::default();
    let access = h.query(&internal::APPROVAL_MANAGEMENT_READ, &mut ex).await.unwrap();
    h.grants(&[("entry", &[approval_instance::INSTANCE_LIST])]);
    assert!(matches!(h.validate(&access, &mut ex).await, Err(Error::ConflictError(_))));
}
