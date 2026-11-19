//! 任务主流程通过真实统一require校验完整资格、持久化种类、来源及终态回放。
use std::sync::Arc;

use erp_permissions::{
    Permission, approval_instance, catalog, internal, legacy_import, purchase_order,
    purchase_payment_request, receivable, supplier, supplier_fulfillment, supplier_offering, supplier_portal,
    work_item,
};
use persistence_core::ReadExecutor;
use persistence_core::testing::SnapshotExecutor;

use super::fixtures::{Facts, Identity, actor, complete, object};
use crate::entity::access_control::ScopeClause;
use crate::entity::authorization::{AccessTarget, FactValue, ObjectFacts, RelatedObjects, RelationKey};
use crate::entity::resource_access::ResourceKind;
use crate::{AuthorizationService, Error};

// 按用例替换任务办理角色授权，测试缺项和跨角色拼接。
fn grant(identity: &Identity, roles: &[(&str, &[Permission])]) {
    identity.state.lock().unwrap().grants =
        roles.iter().map(|(role, permissions)| ((*role).into(), permissions.to_vec())).collect();
}

// 构造归当前用户、无当前指派的已完成任务，按用例指定种类和来源资源。
fn task(kind: &str, source: &str) -> ObjectFacts {
    ObjectFacts {
        subject_type: FactValue::Known(kind.into()),
        subject_source_resource: FactValue::Known(source.into()),
        assignee_id: FactValue::Known(None),
        completed: FactValue::Known(true),
        ..object("work_item", "task", "user")
    }
}

// 将一条真实业务来源连到任务上，同时登记预期引用标识。
fn related(root: &mut ObjectFacts, relation: &str, source: ObjectFacts) {
    root.relations.insert(
        RelationKey::parse(relation).unwrap(),
        RelatedObjects { expected_ids: [source.id.clone()].into(), objects: vec![source] },
    );
}

// 为真实任务办理入口接入指定身份和工作项事实。
fn bound(identity: Arc<Identity>, facts: Arc<Facts>) -> AuthorizationService {
    let mut builder = complete();
    builder.facilities = identity;
    builder.facts.insert("work_item".into(), facts);
    builder.build().unwrap()
}

// 办理审批任务须满足完整裁决资格；已结束任务仍保留所有者读取路径。
#[tokio::test]
async fn approval_execution_keeps_complete_decision_qualification_and_terminal_owner_reading() {
    let identity = Arc::new(Identity::default());
    let mut item = task("DOCUMENT_APPROVAL", "sales_order");
    item.completed = FactValue::Known(false);
    related(&mut item, "approval_instance", object("approval_instance", "instance", "user"));
    let facts = Arc::new(Facts::new(vec![item]));
    let mut instance = object("approval_instance", "instance", "user");
    instance.subject_type = FactValue::Known("sales_order".into());
    related(&mut instance, "subject_source", object("sales_order", "order", "user"));

    let mut builder = complete();
    builder.facilities = identity.clone();
    builder.facts.insert("work_item".into(), facts.clone());
    builder.facts.insert("approval_instance".into(), Arc::new(Facts::new(vec![instance])));
    let service = builder.build().unwrap();

    for decision in [
        vec![],
        vec![("history", vec![approval_instance::COPIES_HISTORY])],
        vec![
            ("history", vec![approval_instance::COPIES_HISTORY]),
            ("decision", vec![approval_instance::SUBMIT_DECISION]),
        ],
        vec![("approver", vec![approval_instance::COPIES_HISTORY, approval_instance::SUBMIT_DECISION])],
    ] {
        let qualified = decision.iter().any(|(_, permissions)| permissions.len() == 2);
        let mut roles = vec![("task-reader", vec![work_item::DETAIL])];
        roles.extend(decision);
        let roles =
            roles.iter().map(|(name, permissions)| (*name, permissions.as_slice())).collect::<Vec<_>>();
        grant(&identity, &roles);
        let result = service
            .require(
                &actor(),
                &internal::WORK_ITEM_EXECUTE,
                AccessTarget::Task { id: "task".into(), version: "v1".into() },
                &mut SnapshotExecutor::default(),
            )
            .await;
        if qualified {
            result.unwrap();
        } else {
            assert!(matches!(result, Err(Error::Forbidden(_))), "{result:?}");
        }
    }

    facts.objects.lock().unwrap()[0].completed = FactValue::Known(true);
    grant(&identity, &[("reader", &[work_item::DETAIL, approval_instance::COPIES_HISTORY])]);
    service
        .require(
            &actor(),
            &internal::WORK_ITEM_EXECUTE,
            AccessTarget::Task { id: "task".into(), version: "v1".into() },
            &mut SnapshotExecutor::default(),
        )
        .await
        .unwrap();

    facts.objects.lock().unwrap()[0].subject_source_resource = FactValue::Known("unknown".into());
    assert!(matches!(
        service
            .require(
                &actor(),
                &internal::WORK_ITEM_EXECUTE,
                AccessTarget::Task { id: "task".into(), version: "v1".into() },
                &mut SnapshotExecutor::default(),
            )
            .await,
        Err(Error::ValidationError(_))
    ));
}

// 办理只接受带当前版本的真实任务标识，不能改用普通记录目标。
#[tokio::test]
async fn task_execution_requires_exact_task_identity_and_version() {
    let identity = Arc::new(Identity::default());
    let facts = Arc::new(Facts::new(vec![task("IMPORT_BUSINESS_CONFIRMATION", "LEGACY_IMPORT_BATCH")]));
    let service = bound(identity.clone(), facts);
    grant(&identity, &[("executor", &[work_item::DETAIL, legacy_import::BATCH_DETAIL])]);
    let mut executor = SnapshotExecutor::default();

    assert!(matches!(
        service
            .require(
                &actor(),
                &internal::WORK_ITEM_EXECUTE,
                AccessTarget::record("task").unwrap(),
                &mut executor,
            )
            .await,
        Err(Error::ValidationError(_))
    ));
    assert!(matches!(
        service
            .require(
                &actor(),
                &internal::WORK_ITEM_EXECUTE,
                AccessTarget::Task { id: "task".into(), version: "stale".into() },
                &mut executor,
            )
            .await,
        Err(Error::ConflictError(message)) if message == "任务版本已改变"
    ));

    service
        .require(
            &actor(),
            &internal::WORK_ITEM_EXECUTE,
            AccessTarget::Task { id: "task".into(), version: "v1".into() },
            &mut executor,
        )
        .await
        .unwrap();
}

// 终态回放无需当前指派，但仍要求完整资格和快照中的精确任务目标。
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
            AccessTarget::Task { id: "task".into(), version: "v1".into() },
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
                AccessTarget::Task { id: "task".into(), version: "v1".into() },
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
                AccessTarget::Task { id: "task".into(), version: "v1".into() },
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

// 任务办理沿用原业务动作的完整同角色权限，不能跨角色凑齐。
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
            AccessTarget::Task { id: "task".into(), version: "v1".into() },
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
                    AccessTarget::Task { id: "task".into(), version: "v1".into() },
                    &mut SnapshotExecutor::default()
                )
                .await,
            Err(Error::Forbidden(_))
        ));
    }
}

// 采购任务办理保留历史范围和针对确切来源记录的显式读取补权。
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
            AccessTarget::Task { id: "task".into(), version: "v1".into() },
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
                AccessTarget::Task { id: "task".into(), version: "v1".into() },
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
            AccessTarget::Task { id: "task".into(), version: "v1".into() },
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
                AccessTarget::Task { id: "task".into(), version: "v1".into() },
                &mut SnapshotExecutor::default()
            )
            .await,
        Err(Error::ValidationError(_))
    ));
}

// 真实任务的主体缺失、未知或与来源种类错配时，不放行办理。
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
                    AccessTarget::Task { id: "task".into(), version: "v1".into() },
                    &mut SnapshotExecutor::default()
                )
                .await,
            Err(Error::ValidationError(_))
        ));
    }
}

// 门户任务分别核对原业务来源资格，并校验真实供应商来源范围。
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
            AccessTarget::Task { id: "task".into(), version: "v1".into() },
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
                AccessTarget::Task { id: "task".into(), version: "v1".into() },
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
                AccessTarget::Task { id: "task".into(), version: "v1".into() },
                &mut SnapshotExecutor::default()
            )
            .await,
        Err(Error::Forbidden(_))
    ));
}

// 供给变更物料同时检查两套独立来源范围，且引用必须完整。
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

// 供应商异常任务的全部外层办理动作必须由同一角色满足。
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
                AccessTarget::Task { id: "task".into(), version: "v1".into() },
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
                            AccessTarget::Task { id: "task".into(), version: "v1".into() },
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
                            AccessTarget::Task { id: "task".into(), version: "v1".into() },
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

// 财务执行任务仍要求同一角色具备所有原办理动作。
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
                AccessTarget::Task { id: "task".into(), version: "v1".into() },
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
                            AccessTarget::Task { id: "task".into(), version: "v1".into() },
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
                            AccessTarget::Task { id: "task".into(), version: "v1".into() },
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

// 供给更新任务须同时落在独立读取和审核范围内。
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
            AccessTarget::Task { id: "task".into(), version: "v1".into() },
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
                        AccessTarget::Task { id: "task".into(), version: "v1".into() },
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
                        AccessTarget::Task { id: "task".into(), version: "v1".into() },
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
