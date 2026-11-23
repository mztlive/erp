#![cfg(test)]
//! 客户、合同、销售单、销售变更、开票、质量分析及文件流转授权测试。
use std::collections::BTreeMap;

use erp_core::common::time::Instant;
use erp_permissions::{
    contract, cost, customer, file_asset, fulfillment, internal, receivable, sales_inquiry, sales_order,
    sales_review, sales_selection,
};
use mongodb::bson::doc;
use persistence_core::testing::SnapshotExecutor;
use persistence_core::{Executor, NoTransaction, ReadExecutor};

use super::fixtures::{AuthHarness, attach_relation, candidate, inquiry_members, object};
use crate::Error;
use crate::entity::access_control::{ResolvedScope, ScopeClause};
use crate::entity::authorization::{AccessTarget, FactValue, ObjectFacts};
use crate::entity::resource_access::ResourceKind;
use crate::repository::authorization::fixtures::execute;

// ── 详情与复制链接有效期 ──────────────────────────────────────────────────────

async fn require_current_delivery(ex: &mut dyn Executor) {
    let h = AuthHarness::new();
    for perm in [
        sales_selection::BOOKLET_COPY_LINK,
        sales_selection::BOOKLET_DETAIL,
        sales_selection::PROPOSAL_DETAIL,
        sales_inquiry::QUERY_SALES_DETAIL,
    ] {
        let access = h.query(&perm, ex).await.unwrap();
        access.ensure_current(ex).unwrap();
        h.validate(&access, ex).await.unwrap();

        let mut expired = h.query(&perm, ex).await.unwrap();
        expired.ensure_current(ex).unwrap();
        expired.validity.valid_until = Some(Instant::from_unix_secs(1));
        assert!(matches!(
            h.validate(&expired, ex).await,
            Err(Error::ConflictError(m)) if m.starts_with("DATA_SCOPE_CHANGED")
        ));
    }
}

#[tokio::test]
async fn detail_and_copy_link_queries_reject_expired_delivery_in_snapshot_and_read() {
    require_current_delivery(&mut SnapshotExecutor::default()).await;
    require_current_delivery(&mut ReadExecutor::default()).await;
}

// ── 销售变更草稿、开票命令与选单依据 ──────────────────────────────────────────

#[tokio::test]
async fn draft_read_requires_both_independent_entries_and_never_adds_historical_read_supplements() {
    let mut historical = object("sales_order", "historical", "other");
    historical.history_participants = FactValue::Known(["user".into()].into());
    let h = AuthHarness::with_facts("sales_order", vec![historical]);
    h.add_record_grant(ResourceKind::SalesOrder, "historical");
    let perm = &internal::SALES_CHANGE_DRAFT_READ;
    for incomplete in [sales_order::HANDOVER, sales_review::SALES_CHANGE_DRAFT] {
        h.grants(&[("incomplete", std::slice::from_ref(&incomplete))]);
        assert!(matches!(h.query(perm, &mut SnapshotExecutor::default()).await, Err(Error::Forbidden(_))));
    }

    h.grants(&[("source", &[sales_order::HANDOVER]), ("entry", &[sales_review::SALES_CHANGE_DRAFT])]);
    let mut ex = SnapshotExecutor::default();
    let access = h.query(perm, &mut ex).await.unwrap();
    assert_eq!(inquiry_members(&access), ["one"]);
    h.validate(&access, &mut ex).await.unwrap();
    assert!(matches!(access.ensure_current(&mut ex), Err(Error::ValidationError(_))));
    assert!(matches!(
        h.require(perm, AccessTarget::record("historical").unwrap(), &mut ex).await,
        Err(Error::Forbidden(_))
    ));

    let mut expiring = h.query(perm, &mut ex).await.unwrap();
    expiring.validity.valid_until = Some(Instant::from_unix_secs(1));
    assert!(matches!(h.validate(&expiring, &mut ex).await, Err(Error::ConflictError(_))));
}

#[tokio::test]
async fn invoice_request_commands_require_the_original_action_and_its_roles_source_scope() {
    for (entry, action, other_action) in [
        (
            internal::INVOICE_REQUEST_SUBMIT_SOURCE,
            receivable::INVOICE_REQUEST_INVOICE_REQUEST_SUBMIT,
            receivable::INVOICE_REQUEST_INVOICE_REQUEST_CANCEL,
        ),
        (
            internal::INVOICE_REQUEST_CANCEL_SOURCE,
            receivable::INVOICE_REQUEST_INVOICE_REQUEST_CANCEL,
            receivable::INVOICE_REQUEST_INVOICE_REQUEST_SUBMIT,
        ),
    ] {
        let make_req = |owner| {
            let mut req = object("sales_invoice_request", "request", "user");
            attach_relation(&mut req, "sales_source", object("sales_order", "order", owner));
            req
        };
        let h = AuthHarness::with_facts("sales_invoice_request", vec![make_req("user")]);
        h.set_role_scope(
            "handler",
            &sales_order::DETAIL,
            ScopeClause { self_owned: true, ..Default::default() },
        );
        h.set_role_scope("reader", &sales_order::DETAIL, ScopeClause { company: true, ..Default::default() });

        h.grants(&[("handler", std::slice::from_ref(&action))]);
        h.require_snap(&entry, AccessTarget::record("request").unwrap()).await.unwrap();

        h.grants(&[("handler", std::slice::from_ref(&action)), ("reader", &[sales_order::DETAIL])]);
        h.set_objects("sales_invoice_request", vec![make_req("other")]);
        assert!(matches!(
            h.require_snap(&entry, AccessTarget::record("request").unwrap()).await,
            Err(Error::Forbidden(_))
        ));

        h.set_objects("sales_invoice_request", vec![make_req("user")]);
        h.grants(&[("handler", std::slice::from_ref(&other_action)), ("reader", &[sales_order::DETAIL])]);
        assert!(matches!(
            h.require_snap(&entry, AccessTarget::record("request").unwrap()).await,
            Err(Error::Forbidden(_))
        ));
    }
}

#[tokio::test]
async fn selection_order_basis_and_inquiry_previews_enforce_qualifications_and_snapshots() {
    let mut proposal = object("sales_selection_proposal", "proposal", "user");
    attach_relation(&mut proposal, "customer", object("customer", "customer", "user"));
    let h = AuthHarness::with_facts("sales_selection_proposal", vec![proposal]);
    let mut ex = SnapshotExecutor::default();
    let perm = &internal::SALES_SELECTION_ORDER_BASIS;
    h.grants(&[("proposal", &[sales_selection::PROPOSAL_DETAIL]), ("customer", &[customer::DETAIL])]);
    assert!(matches!(h.query(perm, &mut ex).await, Err(Error::Forbidden(_))));
    assert_eq!(h.calls("sales_selection_proposal"), 0);

    h.grants(&[
        ("proposal", &[sales_selection::PROPOSAL_DETAIL]),
        ("customer", &[customer::DETAIL]),
        ("creation", &[sales_order::CREATE]),
    ]);
    h.require(perm, AccessTarget::record("proposal").unwrap(), &mut ex).await.unwrap();
    let access = h.query(perm, &mut ex).await.unwrap();
    h.validate(&access, &mut ex).await.unwrap();

    h.mutate_first("sales_selection_proposal", |p| p.owner_id = FactValue::Known(Some("other".into())));
    assert!(matches!(
        h.require(perm, AccessTarget::record("proposal").unwrap(), &mut ex).await,
        Err(Error::Forbidden(_))
    ));

    // 询价写资格预览须用快照事务
    let h = AuthHarness::with_facts("sales_inquiry", vec![object("sales_inquiry", "inquiry", "user")]);
    for (perm, target) in [
        (sales_inquiry::CONVERSION_SALES_BASIS, AccessTarget::record("inquiry").unwrap()),
        (sales_inquiry::IMPORT_SALES_IMPORT_PREVIEW, AccessTarget::record("inquiry").unwrap()),
        (internal::SALES_INQUIRY_IMPORT_CREATE, AccessTarget::Candidate(candidate("sales_inquiry", "user"))),
    ] {
        let calls = h.calls("sales_inquiry");
        assert!(matches!(
            h.require_read(&perm, target.clone()).await,
            Err(Error::ValidationError(m)) if m == "写入授权必须使用快照事务"
        ));
        assert_eq!(h.calls("sales_inquiry"), calls);
        h.require_snap(&perm, target).await.unwrap();
    }
}

// ── 合同识别与分享 ────────────────────────────────────────────────────────────

#[tokio::test]
async fn contract_import_and_sales_sharing_preserve_qualifications_and_boundaries() {
    let h = AuthHarness::with_facts("sales_inquiry", vec![]);
    let mut ex = SnapshotExecutor::default();
    for granted in [&[][..], &[contract::DETAIL][..], &[internal::CONTRACT_IMPORT][..]] {
        h.grants(&[("role", granted)]);
        assert!(matches!(
            h.require(&internal::CONTRACT_IMPORT, AccessTarget::Operation, &mut ex).await,
            Err(Error::Forbidden(_))
        ));
    }
    h.grants(&[("role", &[contract::CREATE])]);
    h.require(&internal::CONTRACT_IMPORT, AccessTarget::Operation, &mut ex).await.unwrap();
    h.set_active(false);
    assert!(matches!(
        h.require(&internal::CONTRACT_IMPORT, AccessTarget::Operation, &mut ex).await,
        Err(Error::Forbidden(_))
    ));
    h.set_active(true);
    h.set_fail(true);
    assert!(matches!(
        h.require(&internal::CONTRACT_IMPORT, AccessTarget::Operation, &mut ex).await,
        Err(Error::Internal(_))
    ));
    assert_eq!(h.calls("sales_inquiry"), 0);

    h.set_fail(false);
    for ex in [&mut NoTransaction as &mut dyn Executor, &mut ReadExecutor::default()] {
        assert!(matches!(
            h.require(&internal::CONTRACT_IMPORT, AccessTarget::Operation, ex).await,
            Err(Error::ValidationError(_))
        ));
    }
    assert!(matches!(
        h.require_snap(&contract::CREATE, AccessTarget::Operation).await,
        Err(Error::ValidationError(_))
    ));
    assert!(matches!(
        h.require_snap(&internal::CONTRACT_IMPORT, AccessTarget::record("task").unwrap()).await,
        Err(Error::ValidationError(_))
    ));

    // 合同销售分享
    let h = AuthHarness::with_facts("contract", vec![object("contract", "contract", "user")]);
    let target = || AccessTarget::record("contract").unwrap();
    let perm = &internal::CONTRACT_SALES_ACCESS;
    h.grants(&[
        ("contract-writer", &[contract::TERMINATE]),
        ("assignment-manager", &[customer::ASSIGNMENT_APPLY]),
    ]);
    h.require_snap(perm, target()).await.unwrap();
    for granted in [&[contract::TERMINATE][..], &[customer::ASSIGNMENT_APPLY][..], std::slice::from_ref(perm)]
    {
        h.grants(&[("incomplete", granted)]);
        assert!(matches!(h.require_snap(perm, target()).await, Err(Error::Forbidden(_))));
    }
    h.grants(&[("writer", &[contract::TERMINATE])]);
    h.require_snap(&contract::TERMINATE, target()).await.unwrap();

    h.grants(&[("writer", &[contract::TERMINATE, customer::ASSIGNMENT_APPLY])]);
    h.mutate_first("contract", |c| c.owner_id = FactValue::Known(Some("other".into())));
    assert!(matches!(h.require_snap(perm, target()).await, Err(Error::Forbidden(_))));
    h.mutate_first("contract", |c| c.owner_id = FactValue::Known(Some("user".into())));
    assert!(matches!(h.require_read(perm, target()).await, Err(Error::ValidationError(_))));
    assert!(matches!(h.require_snap(perm, AccessTarget::Operation).await, Err(Error::ValidationError(_))));
}

// ── 客户、销售创建提交、额外成本、履约创建及文件/预览归属 ─────────────────────

#[tokio::test]
async fn customer_selection_creation_receipt_and_owner_change_keep_current_scope_rules() {
    // 客户历史读取 vs 当前选用
    let mut cust = object("customer", "one", "other");
    cust.history_participants = FactValue::Known(["user".into()].into());
    let h = AuthHarness::with_facts("customer", vec![cust]);
    h.grants(&[("sales", &[customer::DETAIL])]);
    h.require_snap(&customer::DETAIL, AccessTarget::record("one").unwrap()).await.unwrap();
    assert!(matches!(
        h.require_snap(&internal::CUSTOMER_SELECT, AccessTarget::record("one").unwrap()).await,
        Err(Error::Forbidden(_))
    ));
    h.mutate_first("customer", |c| c.owner_id = FactValue::Known(Some("user".into())));
    h.require_snap(&internal::CUSTOMER_SELECT, AccessTarget::record("one").unwrap()).await.unwrap();
    h.grants(&[("sales", &[])]);
    assert!(matches!(
        h.require_snap(&internal::CUSTOMER_SELECT, AccessTarget::record("one").unwrap()).await,
        Err(Error::Forbidden(_))
    ));

    // 客户创建回包、候选及归属变更到期
    let h = AuthHarness::with_facts("customer", vec![object("customer", "customer", "user")]);
    h.grants(&[("creator", &[customer::CREATE])]);
    let receipt = &internal::CUSTOMER_CREATED_RESULT_READ;
    h.require_snap(receipt, AccessTarget::record("customer").unwrap()).await.unwrap();
    for target in [AccessTarget::Operation, AccessTarget::Candidate(candidate("customer", "user"))] {
        assert!(matches!(h.require_snap(receipt, target).await, Err(Error::ValidationError(_))));
    }
    h.mutate_first("customer", |c| {
        c.owner_id = FactValue::Known(Some("other".into()));
        c.history_participants = FactValue::Known(["user".into()].into());
    });
    assert!(matches!(
        h.require_snap(receipt, AccessTarget::record("customer").unwrap()).await,
        Err(Error::Forbidden(_))
    ));
    assert!(matches!(
        h.require_snap(receipt, AccessTarget::record("missing").unwrap()).await,
        Err(Error::NotFound(_))
    ));
    h.grants(&[("reader", &[customer::DETAIL])]);
    let calls = h.calls("customer");
    assert!(matches!(
        h.require_snap(receipt, AccessTarget::record("customer").unwrap()).await,
        Err(Error::Forbidden(_))
    ));
    assert_eq!(h.calls("customer"), calls);

    h.mutate_first("customer", |c| c.owner_id = FactValue::Known(Some("user".into())));
    h.grants(&[("customer", &[customer::CREATE, customer::UPDATE])]);
    h.require_snap(&customer::CREATE, AccessTarget::Candidate(candidate("customer", "user"))).await.unwrap();
    assert!(matches!(
        h.require_snap(&customer::CREATE, AccessTarget::record("customer").unwrap()).await,
        Err(Error::ValidationError(_))
    ));
    for (expiry, expired) in [(None, false), (Some(Instant::from_unix_secs(1)), true)] {
        let dest = ObjectFacts { valid_until: expiry, ..candidate("customer", "user") };
        let res = h
            .require_snap(
                &customer::UPDATE,
                AccessTarget::OwnershipChange { id: "customer".into(), candidate: dest },
            )
            .await;
        if expired {
            assert!(matches!(res, Err(Error::ConflictError(_))));
        } else {
            res.unwrap();
        }
    }
}

#[tokio::test]
async fn create_and_submit_and_extra_cost_preserve_joint_roles_and_detail_rules() {
    let sales_cand = |org: &str| ObjectFacts {
        org_unit_id: FactValue::Known(Some(org.into())),
        ..candidate("sales_order", "user")
    };
    let persisted = ObjectFacts { id: "order".into(), version: "1".into(), ..sales_cand("org-a") };
    let h = AuthHarness::with_facts("sales_order", vec![persisted]);
    h.grants(&[("creator", &[sales_order::CREATE]), ("joint", &[sales_order::CREATE, sales_order::SUBMIT])]);
    h.set_role_scope("creator", &sales_order::CREATE, ScopeClause { company: true, ..Default::default() });
    h.set_role_scope(
        "joint",
        &sales_order::CREATE,
        ScopeClause { org_unit_ids: ["org-b".into()].into(), ..Default::default() },
    );
    h.set_role_scope("joint", &sales_order::SUBMIT, ScopeClause { company: true, ..Default::default() });
    for perm in [sales_order::CREATE, sales_order::SUBMIT] {
        h.require_snap(&perm, AccessTarget::Candidate(sales_cand("org-a"))).await.unwrap();
    }
    for org in ["org-a", "org-b"] {
        h.mutate_first("sales_order", |o| o.org_unit_id = FactValue::Known(Some(org.into())));
        for target in [AccessTarget::Candidate(sales_cand(org)), AccessTarget::record("order").unwrap()] {
            let res = h.require_snap(&internal::SALES_CREATE_AND_SUBMIT, target).await;
            assert_eq!(res.is_ok(), org == "org-b");
        }
    }

    // 额外成本读取
    let h = AuthHarness::with_facts("sales_order", vec![object("sales_order", "order", "user")]);
    let mut ex = SnapshotExecutor::default();
    let perm = &internal::SALES_ORDER_EXTRA_COST_READ;
    h.grants(&[("sales", &[sales_order::DETAIL])]);
    assert!(matches!(
        h.require(perm, AccessTarget::record("order").unwrap(), &mut ex).await,
        Err(Error::Forbidden(_))
    ));
    assert_eq!(h.calls("sales_order"), 0);
    assert!(matches!(h.query(perm, &mut ex).await, Err(Error::Forbidden(_))));

    h.grants(&[("sales", &[sales_order::DETAIL]), ("cost", &[cost::ENTRY_LIST])]);
    let access = h.query(perm, &mut ex).await.unwrap();
    assert_eq!(inquiry_members(&access), ["one"]);
    h.validate(&access, &mut ex).await.unwrap();

    h.set_scope(
        perm,
        &sales_order::DETAIL,
        ResolvedScope {
            role_clauses: vec![ScopeClause { org_unit_ids: ["allowed".into()].into(), ..Default::default() }],
            user_limit: None,
        },
    );
    for (org, grant, allowed) in
        [("allowed", false, true), ("outside", false, false), ("outside", true, true)]
    {
        h.mutate_first("sales_order", |o| o.org_unit_id = FactValue::Known(Some(org.into())));
        if grant {
            h.add_record_grant(ResourceKind::SalesOrder, "order");
        }
        assert_eq!(h.require(perm, AccessTarget::record("order").unwrap(), &mut ex).await.is_ok(), allowed);
    }
    h.grants(&[("sales", &[sales_order::DETAIL])]);
    assert!(matches!(
        h.require(perm, AccessTarget::record("order").unwrap(), &mut ex).await,
        Err(Error::Forbidden(_))
    ));
}

#[tokio::test]
async fn fulfillment_creation_file_transfer_and_internal_previews_enforce_ownership_and_active_grants() {
    // 履约创建
    let h = AuthHarness::new();
    for perm in [
        fulfillment::DELIVERY_DELIVERY_CREATE,
        fulfillment::ELECTRONIC_DELIVERY_ELECTRONIC_DELIVERY_CREATE,
        fulfillment::PURCHASE_RECEIPT_PURCHASE_RECEIPT_CREATE,
        fulfillment::SERVICE_FULFILLMENT_SERVICE_FULFILLMENT_CREATE,
    ] {
        h.grants(&[("creator", &[])]);
        assert!(matches!(h.require_snap(&perm, AccessTarget::Operation).await, Err(Error::Forbidden(_))));
        h.grants(&[("creator", std::slice::from_ref(&perm))]);
        h.require_snap(&perm, AccessTarget::Operation).await.unwrap();
        h.set_active(false);
        assert!(matches!(h.require_snap(&perm, AccessTarget::Operation).await, Err(Error::Forbidden(_))));
        h.set_active(true);
    }

    // 文件流转要求真实上传人（预览权不能代替）
    let h = AuthHarness::with_facts("file_asset", vec![object("file_asset", "asset", "other")]);
    for (perms, owner, expected) in [
        (&[][..], "other", false),
        (&[][..], "user", true),
        (&[file_asset::PREVIEW][..], "other", false),
        (&[file_asset::PREVIEW][..], "user", true),
    ] {
        h.grants(&[("files", perms)]);
        h.mutate_first("file_asset", |o| o.owner_id = FactValue::Known(Some(owner.into())));
        for perm in [file_asset::TRANSFER, file_asset::OWNED_TRANSFER] {
            let res = h.require_snap(&perm, AccessTarget::record("asset").unwrap()).await;
            assert_eq!(res.is_ok(), expected, "{perm}: {owner}");
        }
    }
    h.mutate_first("file_asset", |o| o.owner_id = FactValue::Missing);
    assert!(matches!(
        h.require_snap(&file_asset::TRANSFER, AccessTarget::record("asset").unwrap()).await,
        Err(Error::ValidationError(_))
    ));
    h.set_active(false);
    assert!(matches!(
        h.require_snap(&file_asset::TRANSFER, AccessTarget::record("asset").unwrap()).await,
        Err(Error::Forbidden(_))
    ));

    // 内部预览保留归属与有效账号检查
    for perm in [internal::CATALOG_PREVIEW_DETAIL, internal::SALES_INQUIRY_IMPORT_PREVIEW] {
        let h = AuthHarness::with_facts(perm.resource(), vec![object(perm.resource(), "preview", "other")]);
        h.grants(&[("active-role", &[])]);
        assert!(matches!(
            h.require_snap(&perm, AccessTarget::record("preview").unwrap()).await,
            Err(Error::Forbidden(_))
        ));
        h.mutate_first(perm.resource(), |o| o.owner_id = FactValue::Known(Some("user".into())));
        h.require_snap(&perm, AccessTarget::record("preview").unwrap()).await.unwrap();
        h.set_active(false);
        assert!(matches!(
            h.require_snap(&perm, AccessTarget::record("preview").unwrap()).await,
            Err(Error::Forbidden(_))
        ));
    }
}

// ── 经营质量与期间费用分析 ────────────────────────────────────────────────────

#[tokio::test]
async fn quality_and_profit_loss_queries_preserve_scope_intersections_and_period_basis() {
    // 质量现状在分页限额前取交集
    let h = AuthHarness::new();
    let mut ex = SnapshotExecutor::default();
    h.grants(&[("sales", &[sales_order::LIST]), ("customers", &[customer::LIST])]);
    let access = h.query(&internal::CUSTOMER_QUALITY_CURRENT, &mut ex).await.unwrap();
    let collections = BTreeMap::from([(
        "customer_rows".into(),
        vec![
            doc! {"id":"owned", "owner":"user", "deleted_at":0},
            doc! {"id":"other", "owner":"other", "history":["user"], "deleted_at":0},
        ],
    )]);
    let mut orders: Vec<_> = (0..10_002)
        .map(|i| doc! {"id":format!("outside-{i}"), "owner":"other", "customer":["owned"], "history":["user"], "deleted_at":0})
        .collect();
    orders.extend([
        doc! {"id":"customer-history-only", "owner":"user", "customer":["other"], "deleted_at":0},
        doc! {"id":"missing-customer", "owner":"user", "customer":["missing"], "deleted_at":0},
        doc! {"id":"allowed-1", "owner":"user", "customer":["owned"], "deleted_at":0, "amount":100},
        doc! {"id":"allowed-2", "owner":"user", "customer":["owned"], "deleted_at":0, "amount":200},
    ]);
    let mut pipeline = access.stages().to_vec();
    pipeline.push(doc! {"$limit": 10_001});
    let rows = execute(orders.clone(), &pipeline, &collections, &doc! {});
    assert_eq!(rows.iter().map(|r| r.get_str("id").unwrap()).collect::<Vec<_>>(), ["allowed-1", "allowed-2"]);
    assert_eq!(rows[0].get_i32("amount").unwrap(), 100);
    let mut first_page = access.stages().to_vec();
    first_page.push(doc! {"$limit": 1});
    assert_eq!(execute(orders, &first_page, &collections, &doc! {})[0].get_str("id").unwrap(), "allowed-1");
    h.validate(&access, &mut ex).await.unwrap();

    // 缺少客户资格或空客户范围
    h.grants(&[("sales", &[sales_order::LIST])]);
    assert!(matches!(h.query(&internal::CUSTOMER_QUALITY_CURRENT, &mut ex).await, Err(Error::Forbidden(_))));
    h.grants(&[("sales", &[sales_order::LIST]), ("customers", &[customer::LIST])]);
    h.set_scope(&customer::LIST, &customer::LIST, ResolvedScope { role_clauses: vec![], user_limit: None });
    let access = h.query(&internal::CUSTOMER_QUALITY_CURRENT, &mut ex).await.unwrap();
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
    h.validate(&access, &mut ex).await.unwrap();

    // 质量客户候选不叠加历史补权
    let h = AuthHarness::new();
    h.grants(&[("customers", &[customer::LIST])]);
    let access = h.query(&internal::CUSTOMER_QUALITY_CUSTOMERS, &mut ex).await.unwrap();
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
    assert_eq!(rows.iter().map(|r| r.get_str("id").unwrap()).collect::<Vec<_>>(), ["mine"]);
    h.validate(&access, &mut ex).await.unwrap();

    // 质量历史沿用销售列表范围并要求独立客户资格
    let h = AuthHarness::new();
    for perms in [&[sales_order::LIST][..], &[customer::LIST][..]] {
        h.grants(&[("reader", perms)]);
        assert!(matches!(
            h.query(&internal::CUSTOMER_QUALITY_HISTORY, &mut SnapshotExecutor::default()).await,
            Err(Error::Forbidden(_))
        ));
    }
    h.grants(&[("sales", &[sales_order::LIST]), ("customers", &[customer::LIST])]);
    h.set_role_scope("sales", &sales_order::LIST, ScopeClause { self_owned: true, ..Default::default() });
    h.set_role_scope("customers", &customer::LIST, ScopeClause { company: true, ..Default::default() });
    let history = h.query(&internal::CUSTOMER_QUALITY_HISTORY, &mut ex).await.unwrap();
    let baseline = h.query(&sales_order::LIST, &mut SnapshotExecutor::default()).await.unwrap();
    assert_eq!(history.stages(), baseline.stages());
    let rows = execute(
        vec![
            doc! {"id":"mine", "owner":"user", "deleted_at":0},
            doc! {"id":"participated", "owner":"other", "history":["user"], "deleted_at":0},
            doc! {"id":"outside", "owner":"other", "deleted_at":0},
            doc! {"id":"deleted", "owner":"user", "deleted_at":1},
        ],
        history.stages(),
        &BTreeMap::new(),
        &doc! {},
    );
    assert_eq!(rows.iter().map(|r| r.get_str("id").unwrap()).collect::<Vec<_>>(), ["mine", "participated"]);
    h.validate(&history, &mut ex).await.unwrap();

    // 期间费用依据只要求成本列表资格
    let h = AuthHarness::new();
    for perms in [&[][..], &[sales_order::LIST][..]] {
        h.grants(&[("reader", perms)]);
        assert!(matches!(
            h.require_snap(&internal::PROFIT_LOSS_PERIOD_BASIS, AccessTarget::Operation).await,
            Err(Error::Forbidden(_))
        ));
    }
    h.grants(&[("cost", &[cost::ENTRY_LIST])]);
    h.set_denied_scope(true);
    h.require_snap(&internal::PROFIT_LOSS_PERIOD_BASIS, AccessTarget::Operation).await.unwrap();
    assert!(matches!(
        h.require_read(&internal::PROFIT_LOSS_PERIOD_BASIS, AccessTarget::Operation).await,
        Err(Error::ValidationError(_))
    ));
    assert!(matches!(
        h.require_snap(&internal::PROFIT_LOSS_PERIOD_BASIS, AccessTarget::record("order").unwrap()).await,
        Err(Error::ValidationError(_))
    ));
}
