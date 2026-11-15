//! 任务主流程通过真实统一require校验完整资格、持久化种类、来源及终态回放。
use std::sync::Arc;

use erp_permissions::{
    Permission, catalog, internal, legacy_import, purchase_order, purchase_payment_request, receivable,
    supplier, supplier_fulfillment, supplier_offering, supplier_portal, work_item,
};
use persistence_core::ReadExecutor;
use persistence_core::testing::SnapshotExecutor;

use super::fixtures::{Facts, Identity, actor, complete, object};
use crate::entity::access_control::ScopeClause;
use crate::entity::authorization::{AccessTarget, FactValue, ObjectFacts, RelatedObjects, RelationKey};
use crate::entity::resource_access::ResourceKind;
use crate::{AuthorizationService, Error};

fn grant(identity: &Identity, roles: &[(&str, &[Permission])]) {
    identity.state.lock().unwrap().grants =
        roles.iter().map(|(role, permissions)| ((*role).into(), permissions.to_vec())).collect();
}

fn task(kind: &str, source: &str) -> ObjectFacts {
    ObjectFacts {
        subject_type: FactValue::Known(kind.into()),
        subject_source_resource: FactValue::Known(source.into()),
        assignee_id: FactValue::Known(None),
        completed: FactValue::Known(true),
        ..object("work_item", "task", "user")
    }
}

fn related(root: &mut ObjectFacts, relation: &str, source: ObjectFacts) {
    root.relations.insert(
        RelationKey::parse(relation).unwrap(),
        RelatedObjects { expected_ids: [source.id.clone()].into(), objects: vec![source] },
    );
}

fn bound(identity: Arc<Identity>, facts: Arc<Facts>) -> AuthorizationService {
    let mut builder = complete();
    builder.facilities = identity;
    builder.facts.insert("work_item".into(), facts);
    builder.build().unwrap()
}

#[tokio::test]
async fn terminal_replay_retains_full_qualification_without_inventing_current_assignment() {
    let identity = Arc::new(Identity::default());
    let facts = Arc::new(Facts::new(vec![task("IMPORT_BUSINESS_CONFIRMATION", "LEGACY_IMPORT_BATCH")]));
    let service = bound(identity.clone(), facts.clone());
    grant(&identity, &[("task-reader", &[work_item::DETAIL]), ("import", &[legacy_import::BATCH_DETAIL])]);
    service
        .require(
            &actor(),
            &internal::WORK_ITEM_EXECUTE,
            AccessTarget::record("task").unwrap(),
            &mut SnapshotExecutor::default(),
        )
        .await
        .unwrap();
    grant(&identity, &[("task-reader", &[work_item::DETAIL])]);
    assert!(matches!(
        service
            .require(
                &actor(),
                &internal::WORK_ITEM_EXECUTE,
                AccessTarget::record("task").unwrap(),
                &mut SnapshotExecutor::default()
            )
            .await,
        Err(Error::Forbidden(_))
    ));
    assert!(matches!(
        service
            .require(
                &actor(),
                &internal::WORK_ITEM_EXECUTE,
                AccessTarget::record("task").unwrap(),
                &mut ReadExecutor::default()
            )
            .await,
        Err(Error::ValidationError(_))
    ));
    assert!(matches!(
        service
            .require(
                &actor(),
                &internal::WORK_ITEM_EXECUTE,
                AccessTarget::Operation,
                &mut SnapshotExecutor::default()
            )
            .await,
        Err(Error::ValidationError(_))
    ));
}

#[tokio::test]
async fn task_execution_cannot_split_the_original_complete_permissions_between_roles() {
    let identity = Arc::new(Identity::default());
    let mut item = task("PURCHASE_PAYMENT_ARRANGEMENT", "purchase_order");
    related(&mut item, "order_source", object("purchase_order", "purchase", "user"));
    let facts = Arc::new(Facts::new(vec![item]));
    let service = bound(identity.clone(), facts.clone());
    let full = [purchase_order::DETAIL, purchase_payment_request::CREATE, purchase_payment_request::SUBMIT];
    grant(&identity, &[("task-reader", &[work_item::DETAIL]), ("executor", &full)]);
    service
        .require(
            &actor(),
            &internal::WORK_ITEM_EXECUTE,
            AccessTarget::record("task").unwrap(),
            &mut SnapshotExecutor::default(),
        )
        .await
        .unwrap();
    for missing in 0..full.len() {
        let partial = full
            .iter()
            .enumerate()
            .filter(|(i, _)| *i != missing)
            .map(|(_, permission)| permission.clone())
            .collect::<Vec<_>>();
        grant(
            &identity,
            &[
                ("task-reader", &[work_item::DETAIL]),
                ("partial", &partial),
                ("other-role", &full[missing..missing + 1]),
            ],
        );
        assert!(matches!(
            service
                .require(
                    &actor(),
                    &internal::WORK_ITEM_EXECUTE,
                    AccessTarget::record("task").unwrap(),
                    &mut SnapshotExecutor::default()
                )
                .await,
            Err(Error::Forbidden(_))
        ));
    }
}

#[tokio::test]
async fn task_execution_preserves_purchase_history_scope_and_exact_read_grants() {
    let identity = Arc::new(Identity::default());
    let mut source = object("purchase_order", "purchase", "other");
    source.history_participants = FactValue::Known([actor().id().into()].into());
    let mut item = task("PURCHASE_SOURCE_UPDATE", "purchase_order");
    related(&mut item, "order_source", source);
    let facts = Arc::new(Facts::new(vec![item]));
    let service = bound(identity.clone(), facts.clone());
    grant(&identity, &[("task-reader", &[work_item::DETAIL]), ("executor", &[purchase_order::DETAIL])]);
    service
        .require(
            &actor(),
            &internal::WORK_ITEM_EXECUTE,
            AccessTarget::record("task").unwrap(),
            &mut SnapshotExecutor::default(),
        )
        .await
        .unwrap();
    facts.objects.lock().unwrap()[0].relations.values_mut().next().unwrap().objects[0].history_participants =
        FactValue::Known(Default::default());
    assert!(matches!(
        service
            .require(
                &actor(),
                &internal::WORK_ITEM_EXECUTE,
                AccessTarget::record("task").unwrap(),
                &mut SnapshotExecutor::default()
            )
            .await,
        Err(Error::Forbidden(_))
    ));
    identity.record_grants.lock().unwrap().insert((ResourceKind::PurchaseOrder, "purchase".into()));
    service
        .require(
            &actor(),
            &internal::WORK_ITEM_EXECUTE,
            AccessTarget::record("task").unwrap(),
            &mut SnapshotExecutor::default(),
        )
        .await
        .unwrap();
    facts.objects.lock().unwrap()[0].relations.clear();
    assert!(matches!(
        service
            .require(
                &actor(),
                &internal::WORK_ITEM_EXECUTE,
                AccessTarget::record("task").unwrap(),
                &mut SnapshotExecutor::default()
            )
            .await,
        Err(Error::ValidationError(_))
    ));
}

#[tokio::test]
async fn task_execution_rejects_unknown_missing_or_mismatched_persisted_subjects() {
    let identity = Arc::new(Identity::default());
    let facts = Arc::new(Facts::new(vec![task("IMPORT_BUSINESS_CONFIRMATION", "LEGACY_IMPORT_BATCH")]));
    let service = bound(identity, facts.clone());
    for broken in [
        task("UNKNOWN_TASK", "LEGACY_IMPORT_BATCH"),
        task("IMPORT_BUSINESS_CONFIRMATION", "purchase_order"),
        ObjectFacts {
            subject_type: FactValue::Missing,
            ..task("IMPORT_BUSINESS_CONFIRMATION", "LEGACY_IMPORT_BATCH")
        },
    ] {
        facts.objects.lock().unwrap()[0] = broken;
        assert!(matches!(
            service
                .require(
                    &actor(),
                    &internal::WORK_ITEM_EXECUTE,
                    AccessTarget::record("task").unwrap(),
                    &mut SnapshotExecutor::default()
                )
                .await,
            Err(Error::ValidationError(_))
        ));
    }
}

#[tokio::test]
async fn portal_task_requires_original_independent_sources_and_real_supplier_scope() {
    let identity = Arc::new(Identity::default());
    let mut material = object("supplier_portal_material", "request", "other");
    material.subject_type = FactValue::Known("new_product".into());
    related(&mut material, "supplier", object("supplier", "supplier", "user"));
    let mut item = task("SUPPLIER_PORTAL_REVIEW", "supplier_portal_request");
    related(&mut item, "portal_source", material);
    let facts = Arc::new(Facts::new(vec![item]));
    let service = bound(identity.clone(), facts.clone());
    let execution =
        [supplier_portal::ADMIN_APPLICATION, supplier_portal::ADMIN_REVIEW, supplier_portal::ADMIN_IMPACTS];
    grant(
        &identity,
        &[
            ("task-reader", &[work_item::DETAIL]),
            ("portal-executor", &execution),
            ("supplier-reader", &[supplier::DETAIL]),
            ("catalog-create", &[catalog::PRODUCT_PRODUCT_CREATE]),
            ("offering-create", &[supplier_offering::CREATE]),
        ],
    );
    service
        .require(
            &actor(),
            &internal::WORK_ITEM_EXECUTE,
            AccessTarget::record("task").unwrap(),
            &mut SnapshotExecutor::default(),
        )
        .await
        .unwrap();
    {
        let mut objects = facts.objects.lock().unwrap();
        let source = &mut objects[0].relations.values_mut().next().unwrap().objects[0];
        source.relations.values_mut().next().unwrap().objects[0].owner_id =
            FactValue::Known(Some("other".into()));
    }
    assert!(matches!(
        service
            .require(
                &actor(),
                &internal::WORK_ITEM_EXECUTE,
                AccessTarget::record("task").unwrap(),
                &mut SnapshotExecutor::default()
            )
            .await,
        Err(Error::Forbidden(_))
    ));
    {
        let mut objects = facts.objects.lock().unwrap();
        let source = &mut objects[0].relations.values_mut().next().unwrap().objects[0];
        source.relations.values_mut().next().unwrap().objects[0].owner_id =
            FactValue::Known(Some("user".into()));
    }
    grant(
        &identity,
        &[
            ("task-reader", &[work_item::DETAIL]),
            ("portal-executor", &execution),
            ("supplier-reader", &[supplier::DETAIL]),
            ("catalog-create", &[catalog::PRODUCT_PRODUCT_CREATE]),
        ],
    );
    assert!(matches!(
        service
            .require(
                &actor(),
                &internal::WORK_ITEM_EXECUTE,
                AccessTarget::record("task").unwrap(),
                &mut SnapshotExecutor::default()
            )
            .await,
        Err(Error::Forbidden(_))
    ));
}

#[tokio::test]
async fn offering_update_material_checks_both_independent_source_ranges_and_complete_references() {
    let identity = Arc::new(Identity::default());
    grant(
        &identity,
        &[("supplier-reader", &[supplier::DETAIL]), ("offering-reader", &[supplier_offering::LIST])],
    );
    let mut material = object("supplier_portal_material", "material", "other");
    related(&mut material, "supplier", object("supplier", "supplier", "user"));
    related(&mut material, "offering", object("supplier_offering", "offering", "other"));
    let facts = Arc::new(Facts::new(vec![material]));
    let mut builder = complete();
    builder.facilities = identity;
    builder.facts.insert("supplier_portal_material".into(), facts.clone());
    let service = builder.build().unwrap();
    let target = || AccessTarget::record("material").unwrap();
    assert!(matches!(
        service
            .require(
                &actor(),
                &internal::PORTAL_MATERIAL_READ_OFFERING,
                target(),
                &mut SnapshotExecutor::default()
            )
            .await,
        Err(Error::Forbidden(_))
    ));
    facts.objects.lock().unwrap()[0]
        .relations
        .get_mut(&RelationKey::parse("offering").unwrap())
        .unwrap()
        .objects[0]
        .owner_id = FactValue::Known(Some("user".into()));
    service
        .require(
            &actor(),
            &internal::PORTAL_MATERIAL_READ_OFFERING,
            target(),
            &mut SnapshotExecutor::default(),
        )
        .await
        .unwrap();
    facts.objects.lock().unwrap()[0]
        .relations
        .get_mut(&RelationKey::parse("offering").unwrap())
        .unwrap()
        .objects
        .clear();
    assert!(matches!(
        service
            .require(
                &actor(),
                &internal::PORTAL_MATERIAL_READ_OFFERING,
                target(),
                &mut SnapshotExecutor::default()
            )
            .await,
        Err(Error::Forbidden(_))
    ));
}

#[tokio::test]
async fn supplier_exception_tasks_keep_all_outer_execution_requirements_in_one_role() {
    for (kind, source, read, actions) in [
        (
            "INTEGRATION_RESULT_UNKNOWN",
            "SUPPLIER_FULFILLMENT_ORDER",
            supplier_fulfillment::ORDER_DETAIL,
            vec![
                supplier_fulfillment::EXECUTION_SETUP_SUPPLIER_ORDER_EXECUTION_SETUP,
                supplier_fulfillment::ORDER_INVESTIGATION,
                supplier_fulfillment::ORDER_TASK_COMPLETION,
            ],
        ),
        (
            "BUSINESS_EXCEPTION",
            "SUPPLIER_FULFILLMENT_ORDER",
            supplier_fulfillment::ORDER_DETAIL,
            vec![
                supplier_fulfillment::EXECUTION_SETUP_SUPPLIER_ORDER_EXECUTION_SETUP,
                supplier_fulfillment::ORDER_INVESTIGATION,
                supplier_fulfillment::ORDER_TASK_COMPLETION,
            ],
        ),
        (
            "BUSINESS_EXCEPTION",
            "SUPPLIER_OFFERING",
            supplier_offering::LIST,
            vec![supplier_offering::COMPLETE_SUPPLY_EXCEPTION_TASK],
        ),
    ] {
        let identity = Arc::new(Identity::default());
        let facts = Arc::new(Facts::new(vec![task(kind, source)]));
        let service = bound(identity.clone(), facts);
        let mut full = actions;
        full.push(read);
        full.sort();
        full.dedup();
        grant(&identity, &[("task-reader", &[work_item::DETAIL]), ("executor", &full)]);
        service
            .require(
                &actor(),
                &internal::WORK_ITEM_EXECUTE,
                AccessTarget::record("task").unwrap(),
                &mut SnapshotExecutor::default(),
            )
            .await
            .unwrap();
        for missing in 0..full.len() {
            let partial = full
                .iter()
                .enumerate()
                .filter(|(i, _)| *i != missing)
                .map(|(_, permission)| permission.clone())
                .collect::<Vec<_>>();
            grant(&identity, &[("task-reader", &[work_item::DETAIL]), ("partial", &partial)]);
            assert!(
                matches!(
                    service
                        .require(
                            &actor(),
                            &internal::WORK_ITEM_EXECUTE,
                            AccessTarget::record("task").unwrap(),
                            &mut SnapshotExecutor::default()
                        )
                        .await,
                    Err(Error::Forbidden(_))
                ),
                "{kind}/{source} 缺少 {} 必须拒绝",
                full[missing]
            );
            grant(
                &identity,
                &[
                    ("task-reader", &[work_item::DETAIL]),
                    ("partial", &partial),
                    ("other-role", &full[missing..missing + 1]),
                ],
            );
            assert!(
                matches!(
                    service
                        .require(
                            &actor(),
                            &internal::WORK_ITEM_EXECUTE,
                            AccessTarget::record("task").unwrap(),
                            &mut SnapshotExecutor::default()
                        )
                        .await,
                    Err(Error::Forbidden(_))
                ),
                "{kind}/{source} 拆分 {} 到其他角色必须拒绝",
                full[missing]
            );
        }
    }
}

#[tokio::test]
async fn financial_execution_tasks_require_every_original_action_in_one_role() {
    for (kind, source, full) in [
        (
            "SUPPLIER_PAYMENT_EXECUTION",
            "purchase_payment_request",
            vec![purchase_payment_request::DETAIL, purchase_payment_request::CREDIT_LIST],
        ),
        (
            "SALES_INVOICE_EXECUTION",
            "receivable_account",
            vec![
                receivable::ACCOUNT_LIST,
                receivable::ACCOUNT_DETAIL,
                receivable::INVOICE_LIST,
                receivable::INVOICE_DETAIL,
                receivable::INVOICE_FILES_INVOICE_CREATE_WITH_FILES,
                receivable::INVOICE_REQUEST_INVOICE_REQUEST_LIST,
                receivable::INVOICE_REQUEST_INVOICE_REQUEST_DETAIL,
                receivable::INVOICE_FILES_INVOICE_COMMIT_WITH_FILES,
            ],
        ),
    ] {
        let identity = Arc::new(Identity::default());
        let service = bound(identity.clone(), Arc::new(Facts::new(vec![task(kind, source)])));
        grant(&identity, &[("task-reader", &[work_item::DETAIL]), ("finance-executor", &full)]);
        service
            .require(
                &actor(),
                &internal::WORK_ITEM_EXECUTE,
                AccessTarget::record("task").unwrap(),
                &mut SnapshotExecutor::default(),
            )
            .await
            .unwrap();
        for missing in 0..full.len() {
            let partial = full
                .iter()
                .enumerate()
                .filter(|(i, _)| *i != missing)
                .map(|(_, permission)| permission.clone())
                .collect::<Vec<_>>();
            grant(&identity, &[("task-reader", &[work_item::DETAIL]), ("partial", &partial)]);
            assert!(
                matches!(
                    service
                        .require(
                            &actor(),
                            &internal::WORK_ITEM_EXECUTE,
                            AccessTarget::record("task").unwrap(),
                            &mut SnapshotExecutor::default()
                        )
                        .await,
                    Err(Error::Forbidden(_))
                ),
                "{kind}缺少{}必须拒绝",
                full[missing]
            );
            grant(
                &identity,
                &[
                    ("task-reader", &[work_item::DETAIL]),
                    ("partial", &partial),
                    ("other-role", &full[missing..missing + 1]),
                ],
            );
            assert!(
                matches!(
                    service
                        .require(
                            &actor(),
                            &internal::WORK_ITEM_EXECUTE,
                            AccessTarget::record("task").unwrap(),
                            &mut SnapshotExecutor::default()
                        )
                        .await,
                    Err(Error::Forbidden(_))
                ),
                "{kind}拆分{}到其他角色必须拒绝",
                full[missing]
            );
        }
    }
}

#[tokio::test]
async fn offering_update_task_requires_both_independent_read_and_review_ranges() {
    let identity = Arc::new(Identity::default());
    let mut material = object("supplier_portal_material", "request", "other");
    material.subject_type = FactValue::Known("offering_update".into());
    related(&mut material, "supplier", object("supplier", "supplier", "user"));
    related(&mut material, "offering", object("supplier_offering", "offering", "other"));
    let mut item = task("SUPPLIER_PORTAL_REVIEW", "supplier_portal_request");
    related(&mut item, "portal_source", material);
    let service = bound(identity.clone(), Arc::new(Facts::new(vec![item])));
    let execution =
        [supplier_portal::ADMIN_APPLICATION, supplier_portal::ADMIN_REVIEW, supplier_portal::ADMIN_IMPACTS];
    grant(
        &identity,
        &[
            ("task-reader", &[work_item::DETAIL]),
            ("portal-executor", &execution),
            ("supplier-reader", &[supplier::DETAIL]),
            ("offering-reader", &[supplier_offering::LIST]),
            ("offering-editor", &[supplier_offering::REVISE]),
        ],
    );
    let company = ScopeClause { company: true, ..Default::default() };
    {
        let mut scopes = identity.role_scopes.lock().unwrap();
        scopes.insert(("offering-reader".into(), supplier_offering::LIST), company.clone());
        scopes.insert(("offering-editor".into(), supplier_offering::REVISE), company.clone());
    }
    service
        .require(
            &actor(),
            &internal::WORK_ITEM_EXECUTE,
            AccessTarget::record("task").unwrap(),
            &mut SnapshotExecutor::default(),
        )
        .await
        .unwrap();
    for (role, permission) in
        [("offering-reader", supplier_offering::LIST), ("offering-editor", supplier_offering::REVISE)]
    {
        identity
            .role_scopes
            .lock()
            .unwrap()
            .insert((role.into(), permission.clone()), ScopeClause::default());
        assert!(
            matches!(
                service
                    .require(
                        &actor(),
                        &internal::WORK_ITEM_EXECUTE,
                        AccessTarget::record("task").unwrap(),
                        &mut SnapshotExecutor::default()
                    )
                    .await,
                Err(Error::Forbidden(_))
            ),
            "独立来源{}范围不符必须拒绝",
            permission
        );
        identity.role_scopes.lock().unwrap().insert((role.into(), permission), company.clone());
    }
    for missing in [supplier_offering::LIST, supplier_offering::REVISE] {
        let supplied = [supplier_offering::LIST, supplier_offering::REVISE]
            .into_iter()
            .filter(|permission| *permission != missing)
            .collect::<Vec<_>>();
        grant(
            &identity,
            &[
                ("task-reader", &[work_item::DETAIL]),
                ("portal-executor", &execution),
                ("supplier-reader", &[supplier::DETAIL]),
                ("remaining-source", &supplied),
            ],
        );
        assert!(
            matches!(
                service
                    .require(
                        &actor(),
                        &internal::WORK_ITEM_EXECUTE,
                        AccessTarget::record("task").unwrap(),
                        &mut SnapshotExecutor::default()
                    )
                    .await,
                Err(Error::Forbidden(_))
            ),
            "独立来源{}功能资格缺失必须拒绝",
            missing
        );
    }
}
