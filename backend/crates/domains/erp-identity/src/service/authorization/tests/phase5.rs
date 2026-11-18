//! 阶段五业务内部入口经真实统一服务执行的资格与对象行为。
#![cfg(test)]
use std::sync::Arc;
use std::sync::atomic::Ordering;

use erp_permissions::{
    Permission, bulk_job, catalog, customer, file_asset, internal, supplier, supplier_offering,
};
use persistence_core::testing::SnapshotExecutor;

use super::fixtures::{Facts, Identity, actor, complete, object};
use crate::entity::authorization::{AccessTarget, FactValue, RelatedObjects, RelationKey};
use crate::{AuthorizationService, Error};

fn bound(identity: Arc<Identity>, resource: &str, facts: Arc<Facts>) -> AuthorizationService {
    let mut builder = complete();
    builder.facilities = identity;
    builder.facts.insert(resource.into(), facts);
    builder.build().unwrap()
}

fn grant(identity: &Identity, roles: &[(&str, &[Permission])]) {
    identity.state.lock().unwrap().grants =
        roles.iter().map(|(role, permissions)| ((*role).into(), permissions.to_vec())).collect();
}

#[tokio::test]
async fn customer_history_allows_viewing_but_current_selection_requires_current_scope() {
    let identity = Arc::new(Identity::default());
    grant(&identity, &[("sales", &[customer::DETAIL])]);
    let mut customer = object("customer", "one", "other");
    customer.history_participants = FactValue::Known(["user".into()].into());
    let facts = Arc::new(Facts::new(vec![customer]));
    let service = bound(identity.clone(), "customer", facts.clone());
    let mut executor = SnapshotExecutor::default();

    service
        .require(&actor(), &customer::DETAIL, AccessTarget::record("one").unwrap(), &mut executor)
        .await
        .unwrap();
    assert!(matches!(
        service
            .require(
                &actor(),
                &internal::CUSTOMER_SELECT,
                AccessTarget::record("one").unwrap(),
                &mut executor
            )
            .await,
        Err(Error::Forbidden(_))
    ));

    facts.objects.lock().unwrap()[0].owner_id = FactValue::Known(Some("user".into()));
    service
        .require(&actor(), &internal::CUSTOMER_SELECT, AccessTarget::record("one").unwrap(), &mut executor)
        .await
        .unwrap();

    grant(&identity, &[("sales", &[])]);
    assert!(matches!(
        service
            .require(
                &actor(),
                &internal::CUSTOMER_SELECT,
                AccessTarget::record("one").unwrap(),
                &mut executor
            )
            .await,
        Err(Error::Forbidden(_))
    ));
}

#[tokio::test]
async fn file_transfer_requires_real_ownership_even_without_assignable_menu_permission() {
    let identity = Arc::new(Identity::default());
    grant(&identity, &[("active-role", &[])]);
    let facts = Arc::new(Facts::new(vec![object("file_asset", "asset", "other")]));
    let service = bound(identity, "file_asset", facts.clone());
    let mut executor = SnapshotExecutor::default();
    assert!(matches!(
        service
            .require(&actor(), &file_asset::TRANSFER, AccessTarget::record("asset").unwrap(), &mut executor)
            .await,
        Err(Error::Forbidden(_))
    ));
    facts.objects.lock().unwrap()[0].owner_id = FactValue::Known(Some("user".into()));
    service
        .require(&actor(), &file_asset::TRANSFER, AccessTarget::record("asset").unwrap(), &mut executor)
        .await
        .unwrap();
}

#[tokio::test]
async fn internal_previews_keep_owner_checks_without_requesting_a_menu_grant() {
    for permission in [internal::CATALOG_PREVIEW_DETAIL, internal::SALES_INQUIRY_IMPORT_PREVIEW] {
        let identity = Arc::new(Identity::default());
        grant(&identity, &[("active-role", &[])]);
        let facts = Arc::new(Facts::new(vec![object(permission.resource(), "preview", "other")]));
        let service = bound(identity.clone(), permission.resource(), facts.clone());
        let mut executor = SnapshotExecutor::default();
        assert!(matches!(
            service
                .require(&actor(), &permission, AccessTarget::record("preview").unwrap(), &mut executor)
                .await,
            Err(Error::Forbidden(_))
        ));
        facts.objects.lock().unwrap()[0].owner_id = FactValue::Known(Some("user".into()));
        service
            .require(&actor(), &permission, AccessTarget::record("preview").unwrap(), &mut executor)
            .await
            .unwrap();
        identity.state.lock().unwrap().active = false;
        assert!(matches!(
            service
                .require(&actor(), &permission, AccessTarget::record("preview").unwrap(), &mut executor)
                .await,
            Err(Error::Forbidden(_))
        ));
    }
}

#[tokio::test]
async fn background_administrators_can_read_all_jobs_but_domain_import_reads_stay_owned() {
    let identity = Arc::new(Identity::default());
    grant(&identity, &[("role-sysadmin", &[bulk_job::BACKGROUND_JOB_DETAIL, catalog::PRODUCT_PRODUCT_LIST])]);
    let facts = Arc::new(Facts::new(vec![object("background_job", "job", "other")]));
    let service = bound(identity.clone(), "background_job", facts.clone());
    let mut executor = SnapshotExecutor::default();
    service
        .require(
            &actor(),
            &bulk_job::BACKGROUND_JOB_DETAIL,
            AccessTarget::record("job").unwrap(),
            &mut executor,
        )
        .await
        .unwrap();
    assert!(matches!(
        service
            .require(
                &actor(),
                &internal::PRODUCT_IMPORT_JOB_READ,
                AccessTarget::record("job").unwrap(),
                &mut executor
            )
            .await,
        Err(Error::Forbidden(_))
    ));
    facts.objects.lock().unwrap()[0].owner_id = FactValue::Known(Some("user".into()));
    service
        .require(
            &actor(),
            &internal::PRODUCT_IMPORT_JOB_READ,
            AccessTarget::record("job").unwrap(),
            &mut executor,
        )
        .await
        .unwrap();
    grant(&identity, &[("ordinary", &[bulk_job::BACKGROUND_JOB_DETAIL])]);
    facts.objects.lock().unwrap()[0].owner_id = FactValue::Known(Some("other".into()));
    assert!(matches!(
        service
            .require(
                &actor(),
                &bulk_job::BACKGROUND_JOB_DETAIL,
                AccessTarget::record("job").unwrap(),
                &mut executor
            )
            .await,
        Err(Error::Forbidden(_))
    ));
}

#[tokio::test]
async fn cancelling_all_jobs_requires_both_function_grant_and_administrator_role() {
    let identity = Arc::new(Identity::default());
    let service = bound(identity.clone(), "background_job", Arc::new(Facts::default()));
    let mut executor = SnapshotExecutor::default();
    for (role, permissions, allowed) in [
        ("ordinary", vec![bulk_job::BACKGROUND_JOB_CANCEL_ALL], false),
        ("role-sysadmin", vec![], false),
        ("role-sysadmin", vec![bulk_job::BACKGROUND_JOB_CANCEL_ALL], true),
    ] {
        grant(&identity, &[(role, &permissions)]);
        assert_eq!(
            service
                .require(
                    &actor(),
                    &bulk_job::BACKGROUND_JOB_CANCEL_ALL,
                    AccessTarget::Operation,
                    &mut executor
                )
                .await
                .is_ok(),
            allowed
        );
    }
}

#[tokio::test]
async fn supplier_material_requires_every_independent_business_qualification() {
    let identity = Arc::new(Identity::default());
    let mut material = object("supplier_portal_material", "material", "other");
    material.relations.insert(
        RelationKey::parse("supplier").unwrap(),
        RelatedObjects {
            expected_ids: ["supplier".into()].into(),
            objects: vec![object("supplier", "supplier", "user")],
        },
    );
    let facts = Arc::new(Facts::new(vec![material]));
    let service = bound(identity.clone(), "supplier_portal_material", facts.clone());
    let mut executor = SnapshotExecutor::default();
    grant(
        &identity,
        &[
            ("supplier-reader", &[supplier::DETAIL]),
            ("catalog-editor", &[catalog::PRODUCT_PRODUCT_CREATE]),
            ("offering-editor", &[supplier_offering::CREATE]),
        ],
    );
    service
        .require(
            &actor(),
            &internal::PORTAL_MATERIAL_REVIEW_NEW_PRODUCT,
            AccessTarget::record("material").unwrap(),
            &mut executor,
        )
        .await
        .unwrap();

    let before = facts.calls.load(Ordering::SeqCst);
    grant(
        &identity,
        &[("supplier-reader", &[supplier::DETAIL]), ("catalog-editor", &[catalog::PRODUCT_PRODUCT_CREATE])],
    );
    assert!(matches!(
        service
            .require(
                &actor(),
                &internal::PORTAL_MATERIAL_REVIEW_NEW_PRODUCT,
                AccessTarget::record("material").unwrap(),
                &mut executor
            )
            .await,
        Err(Error::Forbidden(_))
    ));
    assert_eq!(facts.calls.load(Ordering::SeqCst), before);

    grant(&identity, &[("supplier-editor", &[supplier::PROFILE_UPDATE])]);
    service
        .require(
            &actor(),
            &internal::PORTAL_MATERIAL_REVIEW_COOPERATION,
            AccessTarget::record("material").unwrap(),
            &mut executor,
        )
        .await
        .unwrap();

    assert!(matches!(
        service
            .require(
                &actor(),
                &internal::PORTAL_MATERIAL_READ,
                AccessTarget::record("material").unwrap(),
                &mut executor
            )
            .await,
        Err(Error::Forbidden(_))
    ));
}

#[tokio::test]
async fn quality_current_query_intersects_independent_scopes_before_result_limits() {
    use std::collections::BTreeMap;

    use erp_permissions::sales_order;
    use mongodb::bson::doc;

    use crate::repository::authorization::fixtures::execute;

    let identity = Arc::new(Identity::default());
    grant(&identity, &[("sales", &[sales_order::LIST]), ("customers", &[customer::LIST])]);
    let service = bound(identity, "sales_order", Arc::new(Facts::default()));
    let mut executor = SnapshotExecutor::default();
    let access = service.query(&actor(), &internal::CUSTOMER_QUALITY_CURRENT, &mut executor).await.unwrap();
    let collections = BTreeMap::from([(
        "customer_rows".into(),
        vec![
            doc! {"id":"owned", "owner":"user", "deleted_at":0},
            doc! {"id":"other", "owner":"other", "history":["user"], "deleted_at":0},
        ],
    )]);
    let mut orders = (0..10_002)
        .map(|index| {
            doc! {
                "id":format!("outside-{index}"), "owner":"other", "customer":["owned"],
                "history":["user"], "deleted_at":0,
            }
        })
        .collect::<Vec<_>>();
    orders.extend([
        doc! {"id":"customer-history-only", "owner":"user", "customer":["other"], "deleted_at":0},
        doc! {"id":"missing-customer", "owner":"user", "customer":["missing"], "deleted_at":0},
        doc! {"id":"allowed-1", "owner":"user", "customer":["owned"], "deleted_at":0, "amount":100},
        doc! {"id":"allowed-2", "owner":"user", "customer":["owned"], "deleted_at":0, "amount":200},
    ]);
    let mut pipeline = access.stages().to_vec();
    pipeline.push(doc! {"$limit":10_001});
    let rows = execute(orders.clone(), &pipeline, &collections, &doc! {});
    assert_eq!(
        rows.iter().map(|row| row.get_str("id").unwrap()).collect::<Vec<_>>(),
        ["allowed-1", "allowed-2"]
    );
    assert_eq!(rows[0].get_i32("amount").unwrap(), 100);
    let mut first_page = access.stages().to_vec();
    first_page.push(doc! {"$limit":1});
    assert_eq!(execute(orders, &first_page, &collections, &doc! {})[0].get_str("id").unwrap(), "allowed-1");
    service.validate(&access, &mut executor).await.unwrap();
}

#[tokio::test]
async fn quality_current_query_requires_customer_qualification_and_preserves_empty_customer_scope() {
    use std::collections::BTreeMap;

    use erp_permissions::sales_order;
    use mongodb::bson::doc;

    use crate::entity::access_control::ResolvedScope;
    use crate::repository::authorization::fixtures::execute;

    let identity = Arc::new(Identity::default());
    grant(&identity, &[("sales", &[sales_order::LIST])]);
    let service = bound(identity.clone(), "sales_order", Arc::new(Facts::default()));
    let mut executor = SnapshotExecutor::default();
    assert!(matches!(
        service.query(&actor(), &internal::CUSTOMER_QUALITY_CURRENT, &mut executor).await,
        Err(Error::Forbidden(_))
    ));
    grant(&identity, &[("sales", &[sales_order::LIST]), ("customers", &[customer::LIST])]);
    identity
        .scopes
        .lock()
        .unwrap()
        .insert((customer::LIST, customer::LIST), ResolvedScope { role_clauses: vec![], user_limit: None });
    let access = service.query(&actor(), &internal::CUSTOMER_QUALITY_CURRENT, &mut executor).await.unwrap();
    let rows = execute(
        vec![doc! {"id":"order", "owner":"user", "customer":["owned"], "deleted_at":0}],
        access.stages(),
        &BTreeMap::from([(
            "customer_rows".into(),
            vec![doc! {"id":"owned", "owner":"user", "deleted_at":0}],
        )]),
        &doc! {},
    );
    assert!(rows.is_empty());
    service.validate(&access, &mut executor).await.unwrap();
}

#[tokio::test]
async fn quality_current_customer_candidates_use_list_qualification_without_historical_supplement() {
    use std::collections::BTreeMap;

    use mongodb::bson::doc;

    use crate::repository::authorization::fixtures::execute;

    let identity = Arc::new(Identity::default());
    grant(&identity, &[("customers", &[customer::LIST])]);
    let service = bound(identity, "customer", Arc::new(Facts::default()));
    let mut executor = SnapshotExecutor::default();
    let access = service.query(&actor(), &internal::CUSTOMER_QUALITY_CUSTOMERS, &mut executor).await.unwrap();
    let rows = execute(
        vec![
            doc! {"id":"mine", "owner":"user", "deleted_at":0},
            doc! {"id":"history-only", "owner":"other", "history":["user"], "deleted_at":0},
            doc! {"id":"other", "owner":"other", "deleted_at":0},
        ],
        access.stages(),
        &BTreeMap::new(),
        &doc! {},
    );
    assert_eq!(rows.iter().map(|row| row.get_str("id").unwrap()).collect::<Vec<_>>(), ["mine"]);
    service.validate(&access, &mut executor).await.unwrap();
}

#[tokio::test]
async fn transfer_requires_the_real_uploader_even_with_preview_permission() {
    let identity = Arc::new(Identity::default());
    let facts = Arc::new(Facts::new(vec![object("file_asset", "asset", "other")]));
    let service = bound(identity.clone(), "file_asset", facts.clone());
    let mut executor = SnapshotExecutor::default();
    for (permissions, owner, transfer, owned) in [
        (vec![], "other", false, false),
        (vec![], "user", true, true),
        (vec![file_asset::PREVIEW], "other", false, false),
        (vec![file_asset::PREVIEW], "user", true, true),
    ] {
        grant(&identity, &[("files", &permissions)]);
        facts.objects.lock().unwrap()[0].owner_id = FactValue::Known(Some(owner.into()));
        for (permission, expected) in [(file_asset::TRANSFER, transfer), (file_asset::OWNED_TRANSFER, owned)]
        {
            let result = service
                .require(&actor(), &permission, AccessTarget::record("asset").unwrap(), &mut executor)
                .await;
            assert_eq!(result.is_ok(), expected, "{permission}: {owner}");
            if !expected {
                assert!(matches!(result, Err(Error::Forbidden(_))));
            }
        }
    }
    facts.objects.lock().unwrap()[0].owner_id = FactValue::Missing;
    assert!(matches!(
        service
            .require(&actor(), &file_asset::TRANSFER, AccessTarget::record("asset").unwrap(), &mut executor)
            .await,
        Err(Error::ValidationError(_))
    ));
    identity.state.lock().unwrap().active = false;
    assert!(matches!(
        service
            .require(&actor(), &file_asset::TRANSFER, AccessTarget::record("asset").unwrap(), &mut executor)
            .await,
        Err(Error::Forbidden(_))
    ));
}

#[tokio::test]
async fn whole_finance_qualification_comes_from_one_role_and_not_from_plan_binding() {
    use erp_permissions::{cost, policy_permission, sales_order};
    let identity = Arc::new(Identity::default());
    let service = bound(identity.clone(), "cost_entry", Arc::new(Facts::default()));
    let mut executor = SnapshotExecutor::default();
    let entry = [cost::ENTRY_LIST, sales_order::LIST];
    let ledger = [policy_permission::FINANCE_LEDGER_READ];
    for (roles, expected) in [
        (vec![("entry", entry.as_slice())], false),
        (vec![("entry", entry.as_slice()), ("ledger", ledger.as_slice())], false),
    ] {
        grant(&identity, &roles);
        let access = service.query(&actor(), &cost::ENTRY_LIST, &mut executor).await.unwrap();
        assert!(access.whole_stages().is_ok());
        assert_eq!(access.has_whole_qualification(), expected);
        service.validate(&access, &mut executor).await.unwrap();
    }
    grant(
        &identity,
        &[("whole", &[cost::ENTRY_LIST, sales_order::LIST, policy_permission::FINANCE_LEDGER_READ])],
    );
    identity.state.lock().unwrap().denied_scope = true;
    let access = service.query(&actor(), &cost::ENTRY_LIST, &mut executor).await.unwrap();
    assert!(access.has_whole_qualification());
    service.validate(&access, &mut executor).await.unwrap();
}

#[tokio::test]
async fn supplier_handover_checks_both_original_and_target_ownership() {
    use crate::entity::access_control::{ResolvedScope, ScopeClause};
    use crate::entity::authorization::ObjectFacts;
    let identity = Arc::new(Identity::default());
    grant(&identity, &[("supplier", &[supplier::HANDOVER])]);
    identity.scopes.lock().unwrap().insert(
        (supplier::HANDOVER, supplier::HANDOVER),
        ResolvedScope {
            role_clauses: vec![ScopeClause { org_unit_ids: ["allowed".into()].into(), ..Default::default() }],
            user_limit: None,
        },
    );
    let mut original = object("supplier", "supplier", "original-owner");
    original.org_unit_id = FactValue::Known(Some("allowed".into()));
    let facts = Arc::new(Facts::new(vec![original]));
    let service = bound(identity, "supplier", facts.clone());
    let mut executor = SnapshotExecutor::default();
    for (original_org, target_org, expected) in
        [("allowed", "allowed", true), ("allowed", "outside", false), ("outside", "allowed", false)]
    {
        facts.objects.lock().unwrap()[0].org_unit_id = FactValue::Known(Some(original_org.into()));
        let target = ObjectFacts {
            resource: "supplier".into(),
            owner_id: FactValue::Known(Some("recipient".into())),
            org_unit_id: FactValue::Known(Some(target_org.into())),
            ..Default::default()
        };
        let result = service
            .require(
                &actor(),
                &supplier::HANDOVER,
                AccessTarget::OwnershipChange { id: "supplier".into(), candidate: target },
                &mut executor,
            )
            .await;
        assert_eq!(result.is_ok(), expected, "{original_org} -> {target_org}");
        if !expected {
            assert!(matches!(result, Err(Error::Forbidden(_))));
        }
    }
}

#[tokio::test]
async fn settlement_commands_use_central_preparer_and_assignee_policies() {
    use erp_permissions::supplier_settlement as settlement;

    use crate::entity::access_control::{ResolvedScope, ScopeClause};
    let identity = Arc::new(Identity::default());
    grant(
        &identity,
        &[("settlement", &[settlement::STATEMENT_REFRESH, settlement::STATEMENT_SUBMIT_REVIEW])],
    );
    for permission in [
        settlement::STATEMENT_REFRESH,
        settlement::STATEMENT_SUBMIT_REVIEW,
        internal::SETTLEMENT_REFRESH_PREPARED,
        internal::SETTLEMENT_VOID_PREPARED,
        internal::SETTLEMENT_RESOLVE_DIFFERENCE,
    ] {
        identity.scopes.lock().unwrap().insert(
            (permission.clone(), permission),
            ResolvedScope {
                role_clauses: vec![ScopeClause { company: true, ..Default::default() }],
                user_limit: None,
            },
        );
    }

    let mut statement = object("supplier_settlement_statement", "statement", "other");
    statement.assignee_id = FactValue::Known(Some("other".into()));
    let facts = Arc::new(Facts::new(vec![statement]));
    let service = bound(identity.clone(), "supplier_settlement_statement", facts.clone());
    let mut executor = SnapshotExecutor::default();
    for (owner, assignee, preparer, handler) in [
        ("other", "other", false, false),
        ("user", "other", true, false),
        ("other", "user", false, true),
        ("user", "user", true, true),
    ] {
        {
            let mut objects = facts.objects.lock().unwrap();
            objects[0].owner_id = FactValue::Known(Some(owner.into()));
            objects[0].assignee_id = FactValue::Known(Some(assignee.into()));
        }
        for (permission, expected) in [
            (settlement::STATEMENT_REFRESH, true),
            (internal::SETTLEMENT_REFRESH_PREPARED, preparer),
            (internal::SETTLEMENT_VOID_PREPARED, preparer),
            (settlement::STATEMENT_SUBMIT_REVIEW, preparer),
            (internal::SETTLEMENT_RESOLVE_DIFFERENCE, handler),
        ] {
            let result = service
                .require(&actor(), &permission, AccessTarget::record("statement").unwrap(), &mut executor)
                .await;
            assert_eq!(result.is_ok(), expected, "{permission}, {owner}/{assignee}");
            if !expected {
                assert!(matches!(result, Err(Error::Forbidden(_))));
            }
        }
    }

    grant(&identity, &[("no-functional-grant", &[])]);
    assert!(matches!(
        service
            .require(
                &actor(),
                &internal::SETTLEMENT_REFRESH_PREPARED,
                AccessTarget::record("statement").unwrap(),
                &mut executor
            )
            .await,
        Err(Error::Forbidden(_))
    ));
}

#[tokio::test]
async fn copy_detail_query_filters_recipient_and_finishes_its_original_read_stage() {
    use std::collections::BTreeMap;

    use mongodb::bson::doc;

    use crate::repository::authorization::fixtures::execute;
    let identity = Arc::new(Identity::default());
    grant(&identity, &[("active", &[])]);
    let service = bound(identity, "approval_copy", Arc::new(Facts::default()));
    let mut executor = SnapshotExecutor::default();
    let access = service.query(&actor(), &internal::APPROVAL_COPY_DETAIL, &mut executor).await.unwrap();
    let rows = execute(
        vec![
            doc! {"id":"mine","owner":"user","deleted_at":0},
            doc! {"id":"other","owner":"another","deleted_at":0},
        ],
        access.stages(),
        &BTreeMap::new(),
        &doc! {},
    );
    assert_eq!(rows.iter().map(|row| row.get_str("id").unwrap()).collect::<Vec<_>>(), ["mine"]);
    access.ensure_current(&mut executor).unwrap();
    service.validate(&access, &mut executor).await.unwrap();
    assert!(access.ensure_current(&mut executor).is_err());
}

#[tokio::test]
async fn organization_scope_preserves_fixed_profile_qualification_and_snapshot_boundaries() {
    use erp_permissions::{admin, organization};
    use persistence_core::{Executor, ReadExecutor};
    let identity = Arc::new(Identity::default());
    let service = bound(identity.clone(), "org_unit", Arc::new(Facts::default()));
    let mut executor = SnapshotExecutor::default();
    grant(&identity, &[("org", &[organization::CHANGE]), ("account", &[admin::ACCOUNT_UPDATE_ADMIN])]);
    assert!(matches!(
        service
            .organization_scope(&actor(), &internal::ORGANIZATION_PERSON_PROFILE_CHANGE, &mut executor)
            .await,
        Err(Error::Forbidden(_))
    ));

    grant(&identity, &[("combined", &[organization::CHANGE, admin::ACCOUNT_UPDATE_ADMIN])]);
    let scope = service
        .organization_scope(&actor(), &internal::ORGANIZATION_PERSON_PROFILE_CHANGE, &mut executor)
        .await
        .unwrap();
    scope.ensure_current(&mut executor).unwrap();
    executor.next_read();
    assert!(scope.ensure_current(&mut executor).is_err());
    assert!(
        service
            .organization_scope(&actor(), &organization::CHANGE, &mut ReadExecutor::default())
            .await
            .is_err()
    );
    assert!(service.organization_scope(&actor(), &customer::LIST, &mut executor).await.is_err());
}
