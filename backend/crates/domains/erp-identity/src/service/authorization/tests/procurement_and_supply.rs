#![cfg(test)]
//! 采购、供应商、供货目录、接口连接、履约、交接与结算授权测试。
use std::collections::BTreeMap;
use std::sync::atomic::Ordering;

use erp_core::common::time::Instant;
use erp_permissions::{
    Permission, catalog, integration_ops, internal, purchase_order, returns, sales_inquiry, sales_order,
    supplier, supplier_api, supplier_fulfillment, supplier_offering, supplier_settlement as settlement,
};
use mongodb::bson::doc;
use persistence_core::testing::SnapshotExecutor;
use persistence_core::{Executor, ReadExecutor};

use super::fixtures::{
    AuthHarness, attach_relation, candidate, company_scope, complete, inquiry_members, object, self_scope,
};
use crate::Error;
use crate::entity::access_control::{ResolvedScope, ScopeClause};
use crate::entity::authorization::{AccessTarget, FactValue, ObjectFacts, RelationKey};
use crate::entity::resource_access::ResourceKind;
use crate::repository::authorization::fixtures::execute;

// ── 采购写入口、采购邮件、退货来源、交付选项与费用来源 ────────────────────────

fn purchase_write_entries() -> Vec<(Permission, Vec<Permission>, Permission)> {
    vec![
        (
            internal::PURCHASE_CHANGE_CREATE_SOURCE,
            vec![purchase_order::SAVE_DRAFT],
            purchase_order::PURCHASE_CHANGE_CREATE,
        ),
        (
            internal::PURCHASE_CHANGE_SUBMIT_SOURCE,
            vec![purchase_order::SUBMIT],
            purchase_order::PURCHASE_CHANGE_SUBMIT,
        ),
        (
            internal::PURCHASE_CHANGE_CANCEL_SOURCE,
            vec![purchase_order::CANCEL_APPROVAL],
            purchase_order::PURCHASE_CHANGE_CANCEL_APPROVAL,
        ),
        (
            internal::PURCHASE_CHANGE_VOID_SOURCE,
            vec![purchase_order::SAVE_DRAFT],
            purchase_order::CHANGE_VOID_PURCHASE_CHANGE_VOID,
        ),
        (
            internal::PURCHASE_EXPENSE_CONFIRM_SOURCE,
            vec![purchase_order::SAVE_DRAFT],
            purchase_order::EXPENSE_EXPENSE_OCCURRENCE_CONFIRM,
        ),
        (
            internal::PURCHASE_CHANGE_SOURCE,
            vec![purchase_order::SAVE_DRAFT, purchase_order::SUBMIT],
            purchase_order::PURCHASE_CHANGE_SUBMIT,
        ),
    ]
}

#[tokio::test]
async fn purchase_entries_enforce_independent_and_same_role_qualifications_and_snapshot_records() {
    for (entry, source, independent) in purchase_write_entries() {
        let h = AuthHarness::with_facts("purchase_order", vec![object("purchase_order", "order", "user")]);
        let mut ex = SnapshotExecutor::default();
        for granted in [source.clone(), vec![independent.clone()]] {
            h.grants(&[("incomplete", &granted)]);
            assert!(matches!(
                h.require(&entry, AccessTarget::record("order").unwrap(), &mut ex).await,
                Err(Error::Forbidden(_))
            ));
            assert_eq!(h.calls("purchase_order"), 0);
        }
        h.grants(&[("source", &source), ("entry", std::slice::from_ref(&independent))]);
        h.require(&entry, AccessTarget::record("order").unwrap(), &mut ex).await.unwrap();

        h.set_active(false);
        let calls = h.calls("purchase_order");
        assert!(matches!(
            h.require(&entry, AccessTarget::record("order").unwrap(), &mut ex).await,
            Err(Error::Forbidden(_))
        ));
        assert_eq!(h.calls("purchase_order"), calls);
        h.set_active(true);

        // 历史参与与记录授权不能补足写入来源
        h.set_scope(&entry, &entry, self_scope());
        h.add_record_grant(ResourceKind::PurchaseOrder, "order");
        let mut historical = object("purchase_order", "order", "other");
        historical.history_participants = FactValue::Known(["user".into()].into());
        h.set_objects("purchase_order", vec![historical]);
        assert!(matches!(
            h.require(&entry, AccessTarget::record("order").unwrap(), &mut ex).await,
            Err(Error::Forbidden(_))
        ));
        h.mutate_first("purchase_order", |o| o.owner_id = FactValue::Known(Some("user".into())));
        h.require(&entry, AccessTarget::record("order").unwrap(), &mut ex).await.unwrap();
        h.mutate_first("purchase_order", |o| o.org_unit_id = FactValue::Missing);
        assert!(matches!(
            h.require(&entry, AccessTarget::record("order").unwrap(), &mut ex).await,
            Err(Error::ValidationError(_))
        ));

        // 目标形态与快照事务校验
        for target in [AccessTarget::Operation, AccessTarget::Candidate(candidate("purchase_order", "user"))]
        {
            assert!(matches!(h.require_snap(&entry, target).await, Err(Error::ValidationError(_))));
        }
        assert!(matches!(
            h.require_read(&entry, AccessTarget::record("order").unwrap()).await,
            Err(Error::ValidationError(_))
        ));
    }

    // 采购草稿需要同一角色持有两项来源权限
    let h = AuthHarness::with_facts("purchase_order", vec![object("purchase_order", "order", "user")]);
    h.grants(&[
        ("update", &[purchase_order::SAVE_DRAFT]),
        ("submit", &[purchase_order::SUBMIT]),
        ("entry", &[purchase_order::PURCHASE_CHANGE_SUBMIT]),
    ]);
    assert!(matches!(
        h.require_snap(&internal::PURCHASE_CHANGE_SOURCE, AccessTarget::record("order").unwrap()).await,
        Err(Error::Forbidden(_))
    ));
    assert_eq!(h.calls("purchase_order"), 0);
}

#[tokio::test]
async fn purchase_change_replies_return_source_and_email_checks_preserve_boundaries() {
    // 采购变更回包
    for (perm, entry, source) in [
        (
            internal::PURCHASE_CHANGE_SUBMITTED_RESULT_READ,
            purchase_order::PURCHASE_CHANGE_SUBMIT,
            purchase_order::SUBMIT,
        ),
        (
            internal::PURCHASE_CHANGE_VOIDED_RESULT_READ,
            purchase_order::CHANGE_VOID_PURCHASE_CHANGE_VOID,
            purchase_order::SAVE_DRAFT,
        ),
    ] {
        let h =
            AuthHarness::with_facts("purchase_order", vec![object("purchase_order", "purchase", "other")]);
        h.set_company_scope(&perm, &perm);
        let target = || AccessTarget::record("purchase").unwrap();
        for granted in [entry.clone(), source.clone()] {
            h.grants(&[("incomplete", std::slice::from_ref(&granted))]);
            assert!(matches!(h.require_snap(&perm, target()).await, Err(Error::Forbidden(_))));
        }
        assert_eq!(h.calls("purchase_order"), 0);
        h.grants(&[("entry", &[entry]), ("source", &[source])]);
        let mut ex = SnapshotExecutor::default();
        h.require(&perm, target(), &mut ex).await.unwrap();
        ex.next_read();
        h.set_active(false);
        assert!(matches!(h.require(&perm, target(), &mut ex).await, Err(Error::Forbidden(_))));
    }

    // 采购退货来源
    let h = AuthHarness::with_facts("purchase_order", vec![object("purchase_order", "purchase", "other")]);
    let perm = &internal::PURCHASE_RETURN_SOURCE;
    h.set_company_scope(perm, perm);
    let required = [purchase_order::SAVE_DRAFT, returns::PURCHASE_RETURN_PURCHASE_RETURN_ORDER_CREATE];
    h.grants(&[("operator", &required)]);
    h.require_snap(perm, AccessTarget::record("purchase").unwrap()).await.unwrap();
    assert_eq!(h.calls("purchase_order"), 1);
    for roles in [
        vec![("operator", &required[..1])],
        vec![("operator", &required[1..])],
        vec![("editor", &required[..1]), ("creator", &required[1..])],
        vec![("reader", &[purchase_order::DETAIL][..])],
    ] {
        h.grants(&roles);
        let calls = h.calls("purchase_order");
        assert!(matches!(
            h.require_snap(perm, AccessTarget::record("purchase").unwrap()).await,
            Err(Error::Forbidden(_))
        ));
        assert_eq!(h.calls("purchase_order"), calls);
    }
    h.grants(&[("operator", &required)]);
    assert!(matches!(
        h.require_read(perm, AccessTarget::record("purchase").unwrap()).await,
        Err(Error::ValidationError(_))
    ));

    // 发送采购邮件
    let h = AuthHarness::with_facts("purchase_order", vec![object("purchase_order", "purchase", "other")]);
    let send = &purchase_order::CORRESPONDENCE_SEND_PURCHASE_EMAIL;
    h.set_company_scope(send, send);
    for perms in [std::slice::from_ref(send), &[purchase_order::DETAIL][..], &[][..]] {
        h.grants(&[("role", perms)]);
        assert!(matches!(
            h.require_snap(send, AccessTarget::record("purchase").unwrap()).await,
            Err(Error::Forbidden(_))
        ));
    }
    assert_eq!(h.calls("purchase_order"), 0);
    h.grants(&[("sender", std::slice::from_ref(send)), ("reader", &[purchase_order::DETAIL])]);
    h.require_snap(send, AccessTarget::record("purchase").unwrap()).await.unwrap();
    assert_eq!(h.calls("purchase_order"), 1);
    h.set_objects("purchase_order", vec![]);
    assert!(matches!(
        h.require_snap(send, AccessTarget::record("purchase").unwrap()).await,
        Err(Error::NotFound(_))
    ));
    let p_calls = h.permission_calls();
    assert!(matches!(h.require_snap(send, AccessTarget::Operation).await, Err(Error::ValidationError(_))));
    assert!(matches!(
        h.require_read(send, AccessTarget::record("purchase").unwrap()).await,
        Err(Error::ValidationError(_))
    ));
    assert_eq!(h.permission_calls(), p_calls);
}

#[tokio::test]
async fn delivery_options_and_purchase_expense_source_keep_scope_and_record_grant_rules() {
    // 交付选项
    let h = AuthHarness::new();
    for perm in [purchase_order::DETAIL, purchase_order::SAVE_DRAFT] {
        h.grants(&[("buyer", std::slice::from_ref(&perm))]);
        assert!(matches!(
            h.query(&internal::PURCHASE_DELIVERY_OPTIONS_READ, &mut SnapshotExecutor::default()).await,
            Err(Error::Forbidden(_))
        ));
    }
    h.grants(&[("reader", &[purchase_order::DETAIL]), ("editor", &[purchase_order::SAVE_DRAFT])]);
    h.set_role_scope("reader", &purchase_order::DETAIL, ScopeClause { company: true, ..Default::default() });
    h.set_company_scope(&internal::PURCHASE_DELIVERY_OPTIONS_READ, &internal::PURCHASE_DELIVERY_OPTIONS_READ);
    let mut ex = SnapshotExecutor::default();
    let access = h.query(&internal::PURCHASE_DELIVERY_OPTIONS_READ, &mut ex).await.unwrap();
    let detail = h.query(&purchase_order::DETAIL, &mut SnapshotExecutor::default()).await.unwrap();
    assert_eq!(access.stages(), detail.stages());
    let rows = execute(
        vec![
            doc! {"id":"active", "owner":"another", "deleted_at":0},
            doc! {"id":"deleted", "owner":"user", "deleted_at":1},
        ],
        access.stages(),
        &BTreeMap::new(),
        &doc! {},
    );
    assert_eq!(rows.iter().map(|r| r.get_str("id").unwrap()).collect::<Vec<_>>(), ["active"]);
    h.validate(&access, &mut ex).await.unwrap();

    // 费用采购来源
    let mut historical = object("sales_order", "order", "other");
    historical.history_participants = FactValue::Known(["user".into()].into());
    let h = AuthHarness::with_facts("sales_order", vec![historical]);
    let perm = &internal::PURCHASE_EXPENSE_SOURCE;
    h.grants(&[("sales", &[sales_order::DETAIL])]);
    assert!(matches!(
        h.require_snap(perm, AccessTarget::record("order").unwrap()).await,
        Err(Error::Forbidden(_))
    ));
    assert_eq!(h.calls("sales_order"), 0);
    h.grants(&[("buyer", &[purchase_order::CREATE])]);
    assert!(matches!(
        h.require_snap(perm, AccessTarget::record("order").unwrap()).await,
        Err(Error::Forbidden(_))
    ));
    h.add_record_grant(ResourceKind::SalesOrder, "order");
    h.require_snap(perm, AccessTarget::record("order").unwrap()).await.unwrap();

    let mut read_ex = ReadExecutor::default();
    let access = h.query(perm, &mut read_ex).await.unwrap();
    let rows = execute(
        vec![
            doc! {"id":"mine", "owner":"user", "deleted_at":0},
            doc! {"id":"participated", "owner":"other", "history":["user"], "deleted_at":0},
        ],
        access.stages(),
        &BTreeMap::new(),
        &doc! {},
    );
    assert_eq!(rows.iter().map(|r| r.get_str("id").unwrap()).collect::<Vec<_>>(), ["mine"]);
    h.identity.grant_revision.fetch_add(1, Ordering::SeqCst);
    assert!(matches!(h.validate(&access, &mut read_ex).await, Err(Error::ConflictError(_))));
}

// ── 供应商、供给、连接协议、履约与差异 ────────────────────────────────────────

#[tokio::test]
async fn supplier_offering_detail_create_replay_and_availability_enforce_separate_qualifications() {
    // 供给详情独立资格 + Mapping + 到期
    let h =
        AuthHarness::with_facts("supplier_offering", vec![object("supplier_offering", "offering", "user")]);
    let mut ex = SnapshotExecutor::default();
    h.grants(&[("list-reader", &[supplier_offering::LIST])]);
    assert!(matches!(
        h.require(&supplier_offering::DETAIL, AccessTarget::record("offering").unwrap(), &mut ex).await,
        Err(Error::Forbidden(_))
    ));
    assert!(matches!(h.query(&supplier_offering::DETAIL, &mut ex).await, Err(Error::Forbidden(_))));
    assert_eq!(h.calls("supplier_offering"), 0);

    h.grants(&[("detail-reader", &[supplier_offering::DETAIL])]);
    h.require(&supplier_offering::DETAIL, AccessTarget::record("offering").unwrap(), &mut ex).await.unwrap();
    let mut access = h.query(&supplier_offering::DETAIL, &mut ex).await.unwrap();
    access.ensure_current(&mut ex).unwrap();
    h.validate(&access, &mut ex).await.unwrap();
    assert_eq!(h.calls("supplier_offering"), 1);
    assert!(matches!(h.query(&supplier_offering::LIST, &mut ex).await, Err(Error::Forbidden(_))));
    access = h.query(&supplier_offering::DETAIL, &mut ex).await.unwrap();
    access.validity.valid_until = Some(Instant::from_unix_secs(1));
    assert!(
        matches!(h.validate(&access, &mut ex).await, Err(Error::ConflictError(m)) if m.starts_with("DATA_SCOPE_CHANGED"))
    );

    let mut missing = complete();
    missing.mappings.remove(&supplier_offering::DETAIL);
    assert!(
        matches!(missing.build(), Err(Error::ValidationError(m)) if m == "缺少查询 Mapping：supplier_offering:detail")
    );

    // 供给创建回放
    let replay = &internal::SUPPLIER_OFFERING_CREATED_RESULT_READ;
    h.grants(&[("creator", &[supplier_offering::CREATE])]);
    h.require(replay, AccessTarget::record("offering").unwrap(), &mut ex).await.unwrap();
    assert!(matches!(
        h.require(&supplier_offering::CREATE, AccessTarget::record("offering").unwrap(), &mut ex).await,
        Err(Error::ValidationError(_))
    ));
    h.grants(&[("reader", &[supplier_offering::LIST])]);
    assert!(matches!(
        h.require(replay, AccessTarget::record("offering").unwrap(), &mut ex).await,
        Err(Error::Forbidden(_))
    ));
    h.grants(&[("creator", &[supplier_offering::CREATE])]);
    h.set_denied_scope(true);
    assert!(matches!(
        h.require(replay, AccessTarget::record("offering").unwrap(), &mut ex).await,
        Err(Error::Forbidden(_))
    ));
    h.set_active(false);
    assert!(matches!(
        h.require(replay, AccessTarget::record("offering").unwrap(), &mut ex).await,
        Err(Error::Forbidden(_))
    ));
    h.set_active(true);
    h.set_denied_scope(false);

    // 供给可用性
    let avail = &internal::SUPPLIER_OFFERING_AVAILABILITY_UPDATE;
    let calls = h.calls("supplier_offering");
    for p in [supplier_offering::REVISE, supplier_offering::UPDATE_AVAILABILITY] {
        h.grants(&[("partial", std::slice::from_ref(&p))]);
        assert!(matches!(
            h.require(avail, AccessTarget::record("offering").unwrap(), &mut ex).await,
            Err(Error::Forbidden(_))
        ));
    }
    assert_eq!(h.calls("supplier_offering"), calls);
    h.grants(&[
        ("maintenance", &[supplier_offering::REVISE]),
        ("availability", &[supplier_offering::UPDATE_AVAILABILITY]),
    ]);
    h.require(avail, AccessTarget::record("offering").unwrap(), &mut ex).await.unwrap();
    h.mutate_first("supplier_offering", |o| o.owner_id = FactValue::Known(Some("other".into())));
    assert!(matches!(
        h.require(avail, AccessTarget::record("offering").unwrap(), &mut ex).await,
        Err(Error::Forbidden(_))
    ));
    assert!(matches!(
        h.require(avail, AccessTarget::Operation, &mut ex).await,
        Err(Error::ValidationError(_))
    ));
}

#[tokio::test]
async fn supplier_material_import_completion_and_connections_keep_independent_qualifications() {
    // 供应商门户物料
    let mut material = object("supplier_portal_material", "material", "other");
    attach_relation(&mut material, "supplier", object("supplier", "supplier", "user"));
    let h = AuthHarness::with_facts("supplier_portal_material", vec![material]);
    h.grants(&[
        ("supplier-reader", &[supplier::DETAIL]),
        ("catalog-editor", &[catalog::PRODUCT_PRODUCT_CREATE]),
        ("offering-editor", &[supplier_offering::CREATE]),
    ]);
    h.require_snap(&internal::PORTAL_MATERIAL_REVIEW_NEW_PRODUCT, AccessTarget::record("material").unwrap())
        .await
        .unwrap();
    let calls = h.calls("supplier_portal_material");
    h.grants(&[
        ("supplier-reader", &[supplier::DETAIL]),
        ("catalog-editor", &[catalog::PRODUCT_PRODUCT_CREATE]),
    ]);
    assert!(matches!(
        h.require_snap(
            &internal::PORTAL_MATERIAL_REVIEW_NEW_PRODUCT,
            AccessTarget::record("material").unwrap()
        )
        .await,
        Err(Error::Forbidden(_))
    ));
    assert_eq!(h.calls("supplier_portal_material"), calls);
    h.grants(&[("supplier-editor", &[supplier::PROFILE_UPDATE])]);
    h.require_snap(&internal::PORTAL_MATERIAL_REVIEW_COOPERATION, AccessTarget::record("material").unwrap())
        .await
        .unwrap();
    assert!(matches!(
        h.require_snap(&internal::PORTAL_MATERIAL_READ, AccessTarget::record("material").unwrap()).await,
        Err(Error::Forbidden(_))
    ));

    // 供应商导入与补全回放
    let h = AuthHarness::new();
    for (active, perms, allowed) in [
        (true, &[][..], false),
        (true, &[supplier::IMPORT_SUPPLIER_IMPORT][..], true),
        (false, &[supplier::IMPORT_SUPPLIER_IMPORT][..], false),
    ] {
        h.grants(&[("importer", perms)]);
        h.set_active(active);
        assert_eq!(
            h.require_snap(&internal::SUPPLIER_IMPORT_SUBMIT, AccessTarget::Operation).await.is_ok(),
            allowed
        );
    }
    h.set_active(true);
    let completion_req = [
        supplier_fulfillment::EXECUTION_SETUP_SUPPLIER_ORDER_EXECUTION_SETUP,
        supplier_fulfillment::ORDER_INVESTIGATION,
        supplier_fulfillment::ORDER_TASK_COMPLETION,
    ];
    h.assert_same_role_required(
        &[],
        &completion_req,
        &internal::SUPPLIER_FULFILLMENT_COMPLETED_RESULT_READ,
        || AccessTarget::Operation,
    )
    .await;
    h.grants(&[("operator", &completion_req)]);
    h.set_active(false);
    assert!(matches!(
        h.require_snap(&internal::SUPPLIER_FULFILLMENT_COMPLETED_RESULT_READ, AccessTarget::Operation).await,
        Err(Error::Forbidden(_))
    ));

    // 连接详情、目录读取与协议操作
    let mut connection = object("supplier_api_connection", "connection", "user");
    attach_relation(&mut connection, "supplier", object("supplier", "supplier", "user"));
    let h = AuthHarness::with_facts("supplier_api_connection", vec![connection.clone()]);
    let mut read_ex = ReadExecutor::default();
    h.grants(&[("connection", &[supplier_api::CONNECTION_DETAIL])]);
    assert!(matches!(
        h.query(&supplier_api::CONNECTION_DETAIL, &mut read_ex).await,
        Err(Error::Forbidden(_))
    ));
    h.grants(&[("connection", &[supplier_api::CONNECTION_DETAIL]), ("supplier", &[supplier::DETAIL])]);
    let access = h.query(&supplier_api::CONNECTION_DETAIL, &mut read_ex).await.unwrap();
    h.validate(&access, &mut read_ex).await.unwrap();
    h.require_snap(&internal::CONNECTION_CATALOG_READ, AccessTarget::record("connection").unwrap())
        .await
        .unwrap();
    h.set_scope(
        &supplier::DETAIL,
        &supplier::DETAIL,
        ResolvedScope { role_clauses: vec![ScopeClause::default()], user_limit: None },
    );
    assert!(matches!(
        h.require_snap(&internal::CONNECTION_CATALOG_READ, AccessTarget::record("connection").unwrap()).await,
        Err(Error::Forbidden(_))
    ));

    for (entry, qual) in [
        (internal::CONNECTION_PROTOCOL_READ, supplier_api::DANGAOSHUSHU_READ),
        (internal::CONNECTION_REFERENCE_TICKETS, supplier_api::DANGAOSHUSHU_REFERENCE_TICKETS),
    ] {
        let h = AuthHarness::with_facts("supplier_api_connection", vec![connection.clone()]);
        h.grants(&[("connection", &[supplier_api::CONNECTION_DETAIL]), ("supplier", &[supplier::DETAIL])]);
        assert!(matches!(
            h.require_snap(&entry, AccessTarget::record("connection").unwrap()).await,
            Err(Error::Forbidden(_))
        ));
        assert_eq!(h.calls("supplier_api_connection"), 0);
        h.grants(&[
            ("connection", &[supplier_api::CONNECTION_DETAIL]),
            ("supplier", &[supplier::DETAIL]),
            ("protocol", &[qual]),
        ]);
        h.require_snap(&entry, AccessTarget::record("connection").unwrap()).await.unwrap();
        h.mutate_first("supplier_api_connection", |c| {
            c.relations.get_mut(&RelationKey::parse("supplier").unwrap()).unwrap().objects.clear();
        });
        assert!(h.require_snap(&entry, AccessTarget::record("connection").unwrap()).await.is_err());
    }
}

#[tokio::test]
async fn supplier_fulfillment_orders_exception_tasks_and_direct_differences_enforce_exact_targets() {
    // 供应商下单、执行准备与取消/退款回包
    let h = AuthHarness::with_facts(
        "supplier_fulfillment_order",
        vec![object("supplier_fulfillment_order", "order", "user")],
    );
    let submit = &supplier_fulfillment::ORDER_SUBMIT;
    h.grants(&[("role", std::slice::from_ref(submit))]);
    let mut ex = SnapshotExecutor::default();
    h.require(submit, AccessTarget::Candidate(candidate(submit.resource(), "user")), &mut ex).await.unwrap();
    assert_eq!(h.calls("supplier_fulfillment_order"), 0);
    h.require(submit, AccessTarget::record("order").unwrap(), &mut ex).await.unwrap();
    let access = h.query(submit, &mut ex).await.unwrap();
    h.validate(&access, &mut ex).await.unwrap();
    assert!(matches!(access.ensure_current(&mut ex), Err(Error::ValidationError(_))));
    h.grants(&[("reader", &[supplier_fulfillment::ORDER_DETAIL])]);
    assert!(matches!(h.query(submit, &mut ex).await, Err(Error::Forbidden(_))));
    assert!(matches!(
        h.require_read(submit, AccessTarget::record("order").unwrap()).await,
        Err(Error::ValidationError(_))
    ));
    h.clear_grants();
    for target in [
        AccessTarget::Candidate(candidate(submit.resource(), "user")),
        AccessTarget::record("order").unwrap(),
    ] {
        assert!(matches!(h.require(submit, target, &mut ex).await, Err(Error::Forbidden(_))));
    }

    for perm in [supplier_fulfillment::ORDER_CANCEL, supplier_fulfillment::ORDER_REFUND] {
        h.grants(&[("command", std::slice::from_ref(&perm))]);
        let access = h.query(&perm, &mut ex).await.unwrap();
        h.validate(&access, &mut ex).await.unwrap();
        assert!(matches!(h.validate(&access, &mut ex).await, Err(Error::ValidationError(_))));
        let access = h.query(&perm, &mut ex).await.unwrap();
        ex.next_read();
        assert!(matches!(h.validate(&access, &mut ex).await, Err(Error::ValidationError(_))));
        h.grants(&[("detail-only", &[supplier_fulfillment::ORDER_DETAIL])]);
        assert!(matches!(h.query(&perm, &mut ex).await, Err(Error::Forbidden(_))));
    }

    // 异常任务完成必须精确任务目标
    let task_perm = &supplier_offering::COMPLETE_SUPPLY_EXCEPTION_TASK;
    let task = ObjectFacts {
        assignee_id: FactValue::Known(Some("user".into())),
        ..object(task_perm.resource(), "task", "user")
    };
    let h = AuthHarness::with_facts("work_item", vec![task]);
    assert!(matches!(
        h.require_snap(task_perm, AccessTarget::Operation).await,
        Err(Error::ValidationError(_))
    ));
    assert_eq!(h.calls("work_item"), 0);
    let t = |v: &str| AccessTarget::Task { id: "task".into(), version: v.into() };
    h.require_snap(task_perm, t("v1")).await.unwrap();
    assert!(matches!(h.require_snap(task_perm, t("stale")).await, Err(Error::ConflictError(_))));
    h.mutate_first("work_item", |o| o.assignee_id = FactValue::Known(Some("other".into())));
    assert!(matches!(h.require_snap(task_perm, t("v1")).await, Err(Error::Forbidden(_))));

    // 无正式任务对账差异
    let diff = |handler: Option<&str>| ObjectFacts {
        assignee_id: FactValue::Known(handler.map(str::to_string)),
        ..object("reconciliation_difference", "difference", "user")
    };
    let h = AuthHarness::with_facts("reconciliation_difference", vec![diff(Some("user"))]);
    let diff_perm = &integration_ops::DIFFERENCE_DECISION;
    assert!(matches!(
        h.require_snap(diff_perm, AccessTarget::Task { id: "difference".into(), version: "v1".into() }).await,
        Err(Error::ValidationError(_))
    ));
    assert!(matches!(
        h.require_read(diff_perm, AccessTarget::record("difference").unwrap()).await,
        Err(Error::ValidationError(_))
    ));
    assert_eq!(h.permission_calls(), 0);
    assert_eq!(h.calls("reconciliation_difference"), 0);

    h.require_snap(diff_perm, AccessTarget::record("difference").unwrap()).await.unwrap();
    for handler in [Some("other"), None] {
        h.set_objects("reconciliation_difference", vec![diff(handler)]);
        assert!(matches!(
            h.require_snap(diff_perm, AccessTarget::record("difference").unwrap()).await,
            Err(Error::Forbidden(_))
        ));
    }
    h.set_objects("reconciliation_difference", vec![diff(Some("user"))]);
    h.mutate_first("reconciliation_difference", |d| d.assignee_id = FactValue::Missing);
    assert!(matches!(
        h.require_snap(diff_perm, AccessTarget::record("difference").unwrap()).await,
        Err(Error::ValidationError(_))
    ));
    h.clear_grants();
    let reads = h.calls("reconciliation_difference");
    assert!(matches!(
        h.require_snap(diff_perm, AccessTarget::record("difference").unwrap()).await,
        Err(Error::Forbidden(_))
    ));
    assert_eq!(h.calls("reconciliation_difference"), reads);
}

// ── 交接、结算命令与履约列表到期 ──────────────────────────────────────────────

#[tokio::test]
async fn handover_settlement_commands_and_fulfillment_list_delivery_keep_contracts() {
    let dest = |res: &str, owner: &str, org: &str| ObjectFacts {
        id: "record".into(),
        org_unit_id: FactValue::Known(Some(org.into())),
        ..candidate(res, owner)
    };
    let persisted = |res: &str, owner: &str, org: &str| ObjectFacts {
        org_unit_id: FactValue::Known(Some(org.into())),
        ..object(res, "record", owner)
    };
    let oc = |cand| AccessTarget::OwnershipChange { id: "record".into(), candidate: cand };

    // 供应商交接
    let mut sup = object("supplier", "supplier", "original-owner");
    sup.org_unit_id = FactValue::Known(Some("allowed".into()));
    let h = AuthHarness::with_facts("supplier", vec![sup]);
    h.grants(&[("supplier", &[supplier::HANDOVER])]);
    h.set_scope(
        &supplier::HANDOVER,
        &supplier::HANDOVER,
        ResolvedScope {
            role_clauses: vec![ScopeClause { org_unit_ids: ["allowed".into()].into(), ..Default::default() }],
            user_limit: None,
        },
    );
    for (orig_org, target_org, expected) in
        [("allowed", "allowed", true), ("allowed", "outside", false), ("outside", "allowed", false)]
    {
        h.mutate_first("supplier", |o| o.org_unit_id = FactValue::Known(Some(orig_org.into())));
        let target = ObjectFacts {
            resource: "supplier".into(),
            owner_id: FactValue::Known(Some("recipient".into())),
            org_unit_id: FactValue::Known(Some(target_org.into())),
            ..Default::default()
        };
        assert_eq!(
            h.require_snap(
                &supplier::HANDOVER,
                AccessTarget::OwnershipChange { id: "supplier".into(), candidate: target }
            )
            .await
            .is_ok(),
            expected
        );
    }

    // 结算交接完整事实校验
    let perm = &settlement::STATEMENT_HANDOVER;
    let res = perm.resource();
    let h = AuthHarness::with_facts(res, vec![persisted(res, "original-owner", "original-org")]);
    h.set_company_scope(perm, perm);
    let target = dest(res, "new-owner", "new-org");
    h.require_snap(perm, oc(target.clone())).await.unwrap();
    assert_eq!(h.calls(res), 1);
    for invalid in [
        ObjectFacts { owner_id: FactValue::Missing, ..target.clone() },
        ObjectFacts { org_unit_id: FactValue::Missing, ..target.clone() },
        ObjectFacts { resource: "supplier".into(), ..target.clone() },
        ObjectFacts { version: "persisted-version".into(), ..target.clone() },
    ] {
        assert!(matches!(h.require_snap(perm, oc(invalid)).await, Err(Error::ValidationError(_))));
    }
    h.mutate_first(res, |o| o.org_unit_id = FactValue::Missing);
    assert!(matches!(h.require_snap(perm, oc(target.clone())).await, Err(Error::ValidationError(_))));
    h.set_objects(res, vec![]);
    assert!(matches!(h.require_snap(perm, oc(target)).await, Err(Error::NotFound(_))));

    // 个人限缩应用于原归属和目标归属两侧
    let inq_save = &sales_inquiry::COMMANDS_SALES_SAVE;
    let inq_res = inq_save.resource();
    for (limit, legal, outside) in [
        (
            ScopeClause { self_owned: true, ..Default::default() },
            dest(inq_res, "user", "new-org"),
            dest(inq_res, "other-owner", "new-org"),
        ),
        (
            ScopeClause { org_unit_ids: ["allowed".into()].into(), ..Default::default() },
            dest(inq_res, "new-owner", "allowed"),
            dest(inq_res, "new-owner", "outside"),
        ),
    ] {
        let h = AuthHarness::with_facts(inq_res, vec![persisted(inq_res, "user", "allowed")]);
        h.set_scope(inq_save, inq_save, ResolvedScope { user_limit: Some(limit), ..company_scope() });
        h.require_snap(inq_save, oc(legal.clone())).await.unwrap();
        assert!(matches!(h.require_snap(inq_save, oc(outside)).await, Err(Error::Forbidden(_))));
        h.set_objects(inq_res, vec![persisted(inq_res, "outside-owner", "outside")]);
        assert!(matches!(h.require_snap(inq_save, oc(legal)).await, Err(Error::Forbidden(_))));
    }

    // 结算编制人与处理人政策
    let mut stmt = object("supplier_settlement_statement", "statement", "other");
    stmt.assignee_id = FactValue::Known(Some("other".into()));
    let h = AuthHarness::with_facts("supplier_settlement_statement", vec![stmt]);
    h.grants(&[("settlement", &[settlement::STATEMENT_REFRESH, settlement::STATEMENT_SUBMIT_REVIEW])]);
    for p in [
        settlement::STATEMENT_REFRESH,
        settlement::STATEMENT_SUBMIT_REVIEW,
        internal::SETTLEMENT_REFRESH_PREPARED,
        internal::SETTLEMENT_VOID_PREPARED,
        internal::SETTLEMENT_RESOLVE_DIFFERENCE,
    ] {
        h.set_company_scope(&p, &p);
    }
    for (owner, assignee, preparer, handler) in [
        ("other", "other", false, false),
        ("user", "other", true, false),
        ("other", "user", false, true),
        ("user", "user", true, true),
    ] {
        h.mutate_first("supplier_settlement_statement", |s| {
            s.owner_id = FactValue::Known(Some(owner.into()));
            s.assignee_id = FactValue::Known(Some(assignee.into()));
        });
        for (p, expected) in [
            (settlement::STATEMENT_REFRESH, true),
            (internal::SETTLEMENT_REFRESH_PREPARED, preparer),
            (internal::SETTLEMENT_VOID_PREPARED, preparer),
            (settlement::STATEMENT_SUBMIT_REVIEW, preparer),
            (internal::SETTLEMENT_RESOLVE_DIFFERENCE, handler),
        ] {
            assert_eq!(
                h.require_snap(&p, AccessTarget::record("statement").unwrap()).await.is_ok(),
                expected
            );
        }
    }
    h.grants(&[("none", &[])]);
    assert!(matches!(
        h.require_snap(&internal::SETTLEMENT_REFRESH_PREPARED, AccessTarget::record("statement").unwrap())
            .await,
        Err(Error::Forbidden(_))
    ));

    // 履约列表取数后到期拒绝
    let list = &supplier_fulfillment::ORDER_LIST;
    let h = AuthHarness::new();
    h.set_company_scope(list, list);
    let deadline = Instant::from_unix_secs(4_102_444_800);
    h.identity.scope_deadlines.lock().unwrap().insert(list.clone(), deadline);
    let mut ex = SnapshotExecutor::default();
    let access = h.query(list, &mut ex).await.unwrap();
    assert_eq!(inquiry_members(&access), ["one", "other"]);
    assert_eq!(access.validity.valid_until, Some(deadline));
    h.validate(&access, &mut ex).await.unwrap();
    assert!(matches!(access.ensure_current(&mut ex), Err(Error::ValidationError(_))));

    let mut access = h.query(list, &mut ex).await.unwrap();
    access.ensure_current(&mut ex).unwrap();
    access.validity.valid_until = Some(Instant::from_unix_secs(1));
    assert!(
        matches!(h.validate(&access, &mut ex).await, Err(Error::ConflictError(m)) if m.starts_with("DATA_SCOPE_CHANGED"))
    );
    assert_eq!(h.permission_calls(), 2);
    assert!(ex.boundary().unwrap().is_snapshot());
}
