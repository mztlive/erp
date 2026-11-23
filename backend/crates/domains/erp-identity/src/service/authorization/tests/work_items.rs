#![cfg(test)]
//! 工作项详情、管理投影、办理执行、接收者、形成预检及展示资格的统一授权测试。
use std::collections::BTreeMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use async_trait::async_trait;
use erp_core::common::time::Instant;
use erp_permissions::{
    Permission, approval_admin, approval_instance, catalog, fulfillment, integration_ops, internal,
    inventory, legacy_import, purchase_order, purchase_payment_request, receivable, sales_inquiry,
    sales_order, supplier, supplier_fulfillment, supplier_offering, supplier_portal, supplier_settlement,
    work_item,
};
use mongodb::bson::{Document, doc};
use persistence_core::testing::SnapshotExecutor;
use persistence_core::{Executor, ReadExecutor};

use super::fixtures::{
    AuthHarness, Facts, Identity, actor, attach_relation, company_scope, complete, mapping, object,
    self_scope,
};
use crate::entity::access_control::{ResolvedScope, ScopeClause};
use crate::entity::authorization::{
    AccessContext, AccessTarget, FactValue, ObjectFacts, RelationKey, ResourceTargets,
};
use crate::entity::resource_access::ResourceKind;
use crate::ports::AccessFactsPort;
use crate::repository::authorization::AccessValidity;
use crate::repository::authorization::fixtures::execute;
use crate::{AuthorizationService, CompiledAccess, Error, Result};

fn wi_candidate(kind: &str, source: &str) -> ObjectFacts {
    ObjectFacts {
        resource: "work_item".into(),
        subject_type: FactValue::Known(kind.into()),
        subject_source_resource: FactValue::Known(source.into()),
        ..Default::default()
    }
}

fn completed_task(kind: &str, source: &str) -> ObjectFacts {
    ObjectFacts {
        subject_type: FactValue::Known(kind.into()),
        subject_source_resource: FactValue::Known(source.into()),
        assignee_id: FactValue::Known(None),
        completed: FactValue::Known(true),
        ..object("work_item", "task", "user")
    }
}

fn order_rework_task(resource: &str, owner: &str, history: bool) -> ObjectFacts {
    let mut source = object(resource, "order", owner);
    if history {
        source.history_participants = FactValue::Known([actor().id().into()].into());
    }
    let mut item = ObjectFacts {
        subject_type: FactValue::Known("APPROVAL_REWORK".into()),
        subject_source_resource: FactValue::Known(resource.into()),
        ..object("work_item", "task", "user")
    };
    attach_relation(&mut item, "order_source", source);
    item
}

fn payment_arrangement_task() -> ObjectFacts {
    let mut item = ObjectFacts {
        id: "task".into(),
        version: "v1".into(),
        owner_id: FactValue::Known(Some("other".into())),
        org_unit_id: FactValue::Known(None),
        assignee_id: FactValue::Known(Some("other".into())),
        ..wi_candidate("PURCHASE_PAYMENT_ARRANGEMENT", "purchase_order")
    };
    attach_relation(&mut item, "order_source", object("purchase_order", "purchase", "user"));
    item
}

fn wi_task_target() -> AccessTarget {
    AccessTarget::Task { id: "task".into(), version: "v1".into() }
}

fn eval_rows(
    access: &CompiledAccess,
    tasks: Vec<Document>,
    sources: BTreeMap<String, Vec<Document>>,
) -> Vec<String> {
    execute(tasks, access.stages(), &sources, &doc! {})
        .into_iter()
        .map(|row| row.get_str("id").unwrap().into())
        .collect()
}

fn sample_task_members(stages: &[Document]) -> Vec<String> {
    let rows = vec![
        doc! {"id":"mine","owner":"user","deleted_at":0,"detail_other":["mine"]},
        doc! {"id":"other","owner":"other","deleted_at":0,"detail_other":["other"]},
        doc! {"id":"deleted","owner":"user","deleted_at":1,"detail_other":["deleted"]},
    ];
    let collections = [("work_item_rows".into(), rows.clone())].into();
    execute(rows, stages, &collections, &doc! {})
        .iter()
        .map(|row| row.get_str("id").unwrap().to_owned())
        .collect()
}

// ── 工作项详情、管理投影与非事务复验 ──────────────────────────────────────────

#[tokio::test]
async fn detail_order_scope_company_history_and_missing_source_rules() {
    // 列表与单条授权共用订单范围，不加订单菜单资格
    for (resource, key, history_allowed) in
        [("sales_order", "detail_sales", false), ("purchase_order", "detail_purchase", true)]
    {
        for (owner, history) in [("user", false), ("other", false), ("other", true)] {
            let h = AuthHarness::with_facts("work_item", vec![order_rework_task(resource, owner, history)]);
            h.grants(&[("task-reader", &[work_item::DETAIL])]);
            let mut ex = SnapshotExecutor::default();
            let access = h.query(&work_item::DETAIL, &mut ex).await.unwrap();
            let res = eval_rows(
                &access,
                vec![doc! {"id":"task","owner":"user","deleted_at":0,key:["order"]}],
                [(format!("{resource}_rows"), vec![doc! {"id":"order","owner":owner,"history":if history {"user"} else {""},"deleted_at":0}])].into(),
            );
            let allowed = owner == "user" || (history && history_allowed);
            assert_eq!(res, if allowed { vec!["task"] } else { vec![] });
            h.validate(&access, &mut ex).await.unwrap();
            assert_eq!(
                h.require(&work_item::DETAIL, AccessTarget::record("task").unwrap(), &mut ex).await.is_ok(),
                allowed
            );
        }
    }

    // 公司任务范围仍保留采购历史补权和个人限缩
    for (company_src, owner, history, allowed) in [
        (false, "other", true, true),
        (false, "other", false, false),
        (true, "other", true, false),
        (true, "user", false, true),
    ] {
        let mut task = order_rework_task("purchase_order", owner, history);
        task.owner_id = FactValue::Known(Some("other-task-owner".into()));
        let h = AuthHarness::with_facts("work_item", vec![task]);
        h.grants(&[("task-reader", &[work_item::DETAIL])]);
        h.set_company_scope(&work_item::DETAIL, &work_item::DETAIL);
        h.set_scope(
            &work_item::DETAIL,
            &purchase_order::DETAIL,
            ResolvedScope {
                role_clauses: vec![ScopeClause {
                    company: company_src,
                    self_owned: !company_src,
                    ..Default::default()
                }],
                user_limit: company_src.then(|| ScopeClause { self_owned: true, ..Default::default() }),
            },
        );
        let mut ex = SnapshotExecutor::default();
        let access = h.query(&work_item::DETAIL, &mut ex).await.unwrap();
        let res = eval_rows(
            &access,
            vec![doc! {"id":"task","owner":"other-task-owner","deleted_at":0,"detail_purchase":["order"]}],
            [(
                "purchase_order_rows".into(),
                vec![
                    doc! {"id":"order","owner":owner,"history":if history {"user"} else {""},"deleted_at":0},
                ],
            )]
            .into(),
        );
        assert_eq!(res, if allowed { vec!["task"] } else { vec![] });
        h.validate(&access, &mut ex).await.unwrap();
        assert_eq!(
            h.require(&work_item::DETAIL, AccessTarget::record("task").unwrap(), &mut ex).await.is_ok(),
            allowed
        );
    }

    // 缺少订单来源只影响详情，不过滤列表
    let h = AuthHarness::with_facts("work_item", vec![order_rework_task("purchase_order", "user", false)]);
    h.grants(&[("reader", &[work_item::LIST, work_item::DETAIL])]);
    let mut ex = SnapshotExecutor::default();
    let row = doc! {"id":"task","owner":"user","deleted_at":0,"detail_purchase":["missing"]};
    let list = h.query(&work_item::LIST, &mut ex).await.unwrap();
    assert_eq!(eval_rows(&list, vec![row.clone()], BTreeMap::new()), ["task"]);
    h.validate(&list, &mut ex).await.unwrap();
    let detail = h.query(&work_item::DETAIL, &mut ex).await.unwrap();
    assert!(eval_rows(&detail, vec![row], BTreeMap::new()).is_empty());
    h.validate(&detail, &mut ex).await.unwrap();
}

#[tokio::test]
async fn portal_and_approval_detail_and_source_mapping_expiry_rules() {
    // 门户申请详情只要求供应商读取，不放大到供给更新
    let h = AuthHarness::new();
    h.grants(&[("task-reader", &[work_item::DETAIL]), ("supplier-reader", &[supplier::DETAIL])]);
    let mut ex = SnapshotExecutor::default();
    let access = h.query(&work_item::DETAIL, &mut ex).await.unwrap();
    let sources = [
        (
            "supplier_portal_material_rows".into(),
            vec![doc! {"id":"request","deleted_at":0,"supplier":["supplier"]}],
        ),
        ("supplier_rows".into(), vec![doc! {"id":"supplier","owner":"user","deleted_at":0}]),
    ]
    .into();
    assert_eq!(
        eval_rows(
            &access,
            vec![
                doc! {"id":"read","owner":"user","deleted_at":0,"detail_portal_application":["request"]},
                doc! {"id":"update","owner":"user","deleted_at":0,"detail_portal_offering":["request"]},
            ],
            sources,
        ),
        ["read"]
    );
    h.validate(&access, &mut ex).await.unwrap();

    // 审批详情终态所有者与裁决资格
    for can_decide in [false, true] {
        let decision: &[Permission] = if can_decide {
            &[approval_instance::COPIES_HISTORY, approval_instance::SUBMIT_DECISION]
        } else {
            &[approval_instance::COPIES_HISTORY]
        };
        h.grants(&[("task-reader", &[work_item::DETAIL]), ("approval-reader", decision)]);
        let access = h.query(&work_item::DETAIL, &mut ex).await.unwrap();
        let tasks = vec![
            doc! {"id":"open","owner":"user","completed":false,"deleted_at":0,"detail_approval_owner":["open"]},
            doc! {"id":"terminal","owner":"user","completed":true,"deleted_at":0,"detail_approval_owner":["terminal"]},
        ];
        assert_eq!(
            eval_rows(&access, tasks.clone(), [("work_item_rows".into(), tasks)].into()),
            if can_decide { vec!["open", "terminal"] } else { vec!["terminal"] }
        );
        h.validate(&access, &mut ex).await.unwrap();
    }

    // 审批管理者详情需要真实类型和完整来源
    for (manager, src_owner) in [(true, "user"), (false, "user"), (true, "other")] {
        let mgmt: &[Permission] = if manager {
            &[approval_instance::INSTANCE_LIST, approval_admin::SALES_ORDER_RUNTIME]
        } else {
            &[approval_instance::INSTANCE_LIST]
        };
        h.grants(&[
            ("task-reader", &[work_item::DETAIL]),
            ("approval-reader", &[approval_instance::COPIES_HISTORY]),
            ("manager", mgmt),
        ]);
        h.set_company_scope(&work_item::DETAIL, &work_item::DETAIL);
        let access = h.query(&work_item::DETAIL, &mut ex).await.unwrap();
        let sources = [
            (
                "approval_instance_rows".into(),
                vec![doc! {"id":"instance","deleted_at":0,"management_sales_order_sales_order":["sale"]}],
            ),
            ("sales_order_rows".into(), vec![doc! {"id":"sale","owner":src_owner,"deleted_at":0}]),
        ]
        .into();
        let res = eval_rows(
            &access,
            vec![doc! {"id":"task","owner":"other","deleted_at":0,"detail_approval_management":["instance"]}],
            sources,
        );
        assert_eq!(res, if manager && src_owner == "user" { vec!["task"] } else { vec![] });
        h.validate(&access, &mut ex).await.unwrap();
    }

    // 来源映射静态到期时间纳入交付校验
    let mut builder = complete();
    let mut task_map = mapping(&work_item::DETAIL);
    let purchase = mapping(&purchase_order::DETAIL);
    let deadline = Instant::from_unix_secs(Instant::now().unix_secs() + 100);
    *purchase.expires.lock().unwrap() = Some(deadline);
    Arc::get_mut(&mut task_map)
        .unwrap()
        .relations
        .insert(RelationKey::parse("detail_purchase").unwrap(), purchase);
    builder.mappings.insert(work_item::DETAIL, task_map);
    let service = builder.build().unwrap();
    let access = service.query(&actor(), &work_item::DETAIL, &mut ex).await.unwrap();
    assert_eq!(access.validity.valid_until, Some(deadline));
    AccessValidity::with_test_time(deadline, async {
        assert!(matches!(
            service.validate(&access, &mut ex).await,
            Err(Error::ConflictError(m)) if m == "DATA_SCOPE_CHANGED：授权关系已到期"
        ));
    })
    .await;
}

#[tokio::test]
async fn approval_detail_revalidates_management_source_in_read_and_reuses_snapshot() {
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
    let mgmt_inst = || {
        let mut inst = object("approval_instance", "instance", "other");
        inst.subject_type = FactValue::Known("sales_order".into());
        attach_relation(&mut inst, "subject_source", object("sales_order", "order", "user"));
        inst
    };
    let make_svc = |current: ObjectFacts| -> (AuthorizationService, Arc<ApprovalFacts>) {
        let identity = Arc::new(Identity::default());
        identity.state.lock().unwrap().grants = vec![(
            "manager".into(),
            vec![
                work_item::DETAIL,
                approval_instance::COPIES_HISTORY,
                approval_instance::INSTANCE_LIST,
                approval_admin::SALES_ORDER_RUNTIME,
            ],
        )];
        identity.scopes.lock().unwrap().insert((work_item::DETAIL, work_item::DETAIL), company_scope());
        let mut task = object("work_item", "task", "other");
        task.subject_type = FactValue::Known("DOCUMENT_APPROVAL".into());
        task.subject_source_resource = FactValue::Known("sales_order".into());
        task.completed = FactValue::Known(false);
        attach_relation(&mut task, "approval_instance", object("approval_instance", "instance", "other"));
        let sources = Arc::new(ApprovalFacts { original: mgmt_inst(), current, calls: AtomicU64::new(0) });
        let mut builder = complete();
        builder.facilities = identity;
        builder.facts.insert("work_item".into(), Arc::new(Facts::new(vec![task])));
        builder.facts.insert("approval_instance".into(), sources.clone());
        (builder.build().unwrap(), sources)
    };

    for change in ["unchanged", "owner", "version", "missing"] {
        let mut current = mgmt_inst();
        let rel = current.relations.get_mut(&RelationKey::parse("subject_source").unwrap()).unwrap();
        match change {
            "owner" => rel.objects[0].owner_id = FactValue::Known(Some("other".into())),
            "version" => rel.objects[0].version = "v2".into(),
            "missing" => rel.objects.clear(),
            _ => {},
        }
        let (service, facts) = make_svc(current);
        let res = service
            .require(
                &actor(),
                &work_item::DETAIL,
                AccessTarget::record("task").unwrap(),
                &mut ReadExecutor::default(),
            )
            .await;
        if change == "unchanged" {
            res.unwrap();
        } else {
            assert!(
                matches!(res, Err(Error::ConflictError(m)) if m.starts_with("DATA_SCOPE_CHANGED")),
                "{change}"
            );
        }
        assert_eq!(facts.calls.load(Ordering::SeqCst), 2);
    }

    let (service, facts) = make_svc(mgmt_inst());
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

#[tokio::test]
async fn work_item_management_projection_and_deadline_preserve_roles_and_stage() {
    // 快照管理校验
    let h = AuthHarness::with_facts("work_item", vec![object("work_item", "task", "user")]);
    h.grants(&[("manager", &[work_item::MANAGE])]);
    assert!(matches!(
        h.require_read(&work_item::MANAGE, AccessTarget::record("task").unwrap()).await,
        Err(Error::ValidationError(_))
    ));
    assert_eq!(h.permission_calls(), 0);
    h.require_snap(&work_item::MANAGE, AccessTarget::record("task").unwrap()).await.unwrap();
    assert_eq!(h.calls("work_item"), 1);
    h.mutate_first("work_item", |o| o.owner_id = FactValue::Known(Some("other".into())));
    assert!(matches!(
        h.require_snap(&work_item::MANAGE, AccessTarget::record("task").unwrap()).await,
        Err(Error::Forbidden(_))
    ));

    // 读取与管理各自角色范围
    for perm in [work_item::LIST, work_item::DETAIL] {
        for (read_co, mgmt_co) in [(true, false), (false, true)] {
            let h = AuthHarness::new();
            h.grants(&[("reader", std::slice::from_ref(&perm)), ("manager", &[work_item::MANAGE])]);
            h.set_role_scope(
                "reader",
                &perm,
                if read_co { company_scope() } else { self_scope() }.role_clauses[0].clone(),
            );
            h.set_role_scope(
                "manager",
                &work_item::MANAGE,
                if mgmt_co { company_scope() } else { self_scope() }.role_clauses[0].clone(),
            );
            let mut ex = SnapshotExecutor::default();
            let access = h.query(&perm, &mut ex).await.unwrap();
            assert_eq!(
                sample_task_members(access.stages()),
                if read_co { vec!["mine", "other"] } else { vec!["mine"] }
            );
            assert_eq!(
                sample_task_members(access.management_stages().unwrap()),
                if mgmt_co { vec!["mine", "other"] } else { vec!["mine"] }
            );
            access.ensure_current(&mut ex).unwrap();
            h.validate(&access, &mut ex).await.unwrap();
            assert_eq!(h.permission_calls(), 1);
            assert!(access.ensure_current(&mut ex).is_err());
        }
    }

    // 缺少管理资格清空管理投影，撤销管理资格拒绝交付，保留管理截止时间
    let h = AuthHarness::new();
    h.set_role_scope("reader", &work_item::LIST, company_scope().role_clauses[0].clone());
    h.set_role_scope("manager", &work_item::MANAGE, company_scope().role_clauses[0].clone());
    h.grants(&[("reader", &[work_item::LIST])]);
    let mut read_ex = ReadExecutor::default();
    let access = h.query(&work_item::LIST, &mut read_ex).await.unwrap();
    assert_eq!(sample_task_members(access.stages()), ["mine", "other"]);
    assert!(sample_task_members(access.management_stages().unwrap()).is_empty());
    h.validate(&access, &mut read_ex).await.unwrap();

    h.grants(&[("reader", &[work_item::LIST]), ("manager", &[work_item::MANAGE])]);
    let access = h.query(&work_item::LIST, &mut read_ex).await.unwrap();
    assert_eq!(sample_task_members(access.management_stages().unwrap()), ["mine", "other"]);
    h.grants(&[("reader", &[work_item::LIST])]);
    assert!(matches!(h.validate(&access, &mut read_ex).await, Err(Error::ConflictError(_))));

    let deadline = Instant::from_unix_secs(4_102_444_800);
    h.grants(&[("reader", &[work_item::LIST]), ("manager", &[work_item::MANAGE])]);
    h.identity.scope_deadlines.lock().unwrap().insert(work_item::MANAGE, deadline);
    let mut snap = SnapshotExecutor::default();
    let mut access = h.query(&work_item::LIST, &mut snap).await.unwrap();
    assert_eq!(sample_task_members(access.management_stages().unwrap()), ["mine", "other"]);
    assert_eq!(access.validity.valid_until, Some(deadline));
    access.validity.valid_until = Some(Instant::from_unix_secs(1));
    assert!(matches!(h.validate(&access, &mut snap).await, Err(Error::ConflictError(_))));
    assert!(snap.boundary().unwrap().is_snapshot());
}

// ── 任务办理执行 ──────────────────────────────────────────────────────────────

#[tokio::test]
async fn work_item_execution_preserves_approval_exact_task_replay_and_purchase_history() {
    // 审批任务办理与终态所有者路径
    let mut item = completed_task("DOCUMENT_APPROVAL", "sales_order");
    item.completed = FactValue::Known(false);
    attach_relation(&mut item, "approval_instance", object("approval_instance", "instance", "user"));
    let mut inst = object("approval_instance", "instance", "user");
    inst.subject_type = FactValue::Known("sales_order".into());
    attach_relation(&mut inst, "subject_source", object("sales_order", "order", "user"));
    let h = AuthHarness::with_multi_facts(&[("work_item", vec![item]), ("approval_instance", vec![inst])]);
    h.assert_same_role_required(
        &[("task-reader", &[work_item::DETAIL])],
        &[approval_instance::COPIES_HISTORY, approval_instance::SUBMIT_DECISION],
        &internal::WORK_ITEM_EXECUTE,
        wi_task_target,
    )
    .await;
    h.mutate_first("work_item", |w| w.completed = FactValue::Known(true));
    h.grants(&[("reader", &[work_item::DETAIL, approval_instance::COPIES_HISTORY])]);
    h.require_snap(&internal::WORK_ITEM_EXECUTE, wi_task_target()).await.unwrap();
    h.mutate_first("work_item", |w| w.subject_source_resource = FactValue::Known("unknown".into()));
    assert!(matches!(
        h.require_snap(&internal::WORK_ITEM_EXECUTE, wi_task_target()).await,
        Err(Error::ValidationError(_))
    ));

    // 精确任务标识/版本、终态回放及未知/错配主体拒绝
    let h = AuthHarness::with_facts(
        "work_item",
        vec![completed_task("IMPORT_BUSINESS_CONFIRMATION", "LEGACY_IMPORT_BATCH")],
    );
    h.grants(&[("task-reader", &[work_item::DETAIL]), ("import", &[legacy_import::BATCH_DETAIL])]);
    assert!(matches!(
        h.require_snap(&internal::WORK_ITEM_EXECUTE, AccessTarget::record("task").unwrap()).await,
        Err(Error::ValidationError(_))
    ));
    assert!(matches!(
        h.require_snap(&internal::WORK_ITEM_EXECUTE, AccessTarget::Task { id: "task".into(), version: "stale".into() }).await,
        Err(Error::ConflictError(m)) if m == "任务版本已改变"
    ));
    h.require_snap(&internal::WORK_ITEM_EXECUTE, wi_task_target()).await.unwrap();
    h.grants(&[("task-reader", &[work_item::DETAIL])]);
    assert!(matches!(
        h.require_snap(&internal::WORK_ITEM_EXECUTE, wi_task_target()).await,
        Err(Error::Forbidden(_))
    ));
    assert!(matches!(
        h.require_read(&internal::WORK_ITEM_EXECUTE, wi_task_target()).await,
        Err(Error::ValidationError(_))
    ));
    assert!(matches!(
        h.require_snap(&internal::WORK_ITEM_EXECUTE, AccessTarget::Operation).await,
        Err(Error::ValidationError(_))
    ));

    let h = AuthHarness::with_facts(
        "work_item",
        vec![completed_task("IMPORT_BUSINESS_CONFIRMATION", "LEGACY_IMPORT_BATCH")],
    );
    for broken in [
        completed_task("UNKNOWN_TASK", "LEGACY_IMPORT_BATCH"),
        completed_task("IMPORT_BUSINESS_CONFIRMATION", "purchase_order"),
        ObjectFacts {
            subject_type: FactValue::Missing,
            ..completed_task("IMPORT_BUSINESS_CONFIRMATION", "LEGACY_IMPORT_BATCH")
        },
    ] {
        h.set_objects("work_item", vec![broken]);
        assert!(matches!(
            h.require_snap(&internal::WORK_ITEM_EXECUTE, wi_task_target()).await,
            Err(Error::ValidationError(_))
        ));
    }

    // 采购任务保留历史范围和精确记录补权
    let mut source = object("purchase_order", "purchase", "other");
    source.history_participants = FactValue::Known([actor().id().into()].into());
    let mut item = completed_task("PURCHASE_SOURCE_UPDATE", "purchase_order");
    attach_relation(&mut item, "order_source", source);
    let h = AuthHarness::with_facts("work_item", vec![item]);
    h.grants(&[("task-reader", &[work_item::DETAIL]), ("executor", &[purchase_order::DETAIL])]);
    h.require_snap(&internal::WORK_ITEM_EXECUTE, wi_task_target()).await.unwrap();
    h.mutate_first("work_item", |w| {
        w.relations.values_mut().next().unwrap().objects[0].history_participants =
            FactValue::Known(Default::default());
    });
    assert!(matches!(
        h.require_snap(&internal::WORK_ITEM_EXECUTE, wi_task_target()).await,
        Err(Error::Forbidden(_))
    ));
    h.add_record_grant(ResourceKind::PurchaseOrder, "purchase");
    h.require_snap(&internal::WORK_ITEM_EXECUTE, wi_task_target()).await.unwrap();
    h.mutate_first("work_item", |w| w.relations.clear());
    assert!(matches!(
        h.require_snap(&internal::WORK_ITEM_EXECUTE, wi_task_target()).await,
        Err(Error::ValidationError(_))
    ));
}

#[tokio::test]
async fn work_item_execution_requires_same_role_permissions_across_business_and_finance_tasks() {
    let mut payment_item = completed_task("PURCHASE_PAYMENT_ARRANGEMENT", "purchase_order");
    attach_relation(&mut payment_item, "order_source", object("purchase_order", "purchase", "user"));

    let cases: Vec<(ObjectFacts, Vec<Permission>)> = vec![
        (
            payment_item,
            vec![purchase_order::DETAIL, purchase_payment_request::CREATE, purchase_payment_request::SUBMIT],
        ),
        (
            completed_task("INTEGRATION_RESULT_UNKNOWN", "SUPPLIER_FULFILLMENT_ORDER"),
            vec![
                supplier_fulfillment::ORDER_DETAIL,
                supplier_fulfillment::EXECUTION_SETUP_SUPPLIER_ORDER_EXECUTION_SETUP,
                supplier_fulfillment::ORDER_INVESTIGATION,
                supplier_fulfillment::ORDER_TASK_COMPLETION,
            ],
        ),
        (
            completed_task("BUSINESS_EXCEPTION", "SUPPLIER_FULFILLMENT_ORDER"),
            vec![
                supplier_fulfillment::ORDER_DETAIL,
                supplier_fulfillment::EXECUTION_SETUP_SUPPLIER_ORDER_EXECUTION_SETUP,
                supplier_fulfillment::ORDER_INVESTIGATION,
                supplier_fulfillment::ORDER_TASK_COMPLETION,
            ],
        ),
        (
            completed_task("BUSINESS_EXCEPTION", "SUPPLIER_OFFERING"),
            vec![supplier_offering::LIST, supplier_offering::COMPLETE_SUPPLY_EXCEPTION_TASK],
        ),
        (
            completed_task("SUPPLIER_PAYMENT_EXECUTION", "purchase_payment_request"),
            vec![purchase_payment_request::DETAIL, purchase_payment_request::CREDIT_LIST],
        ),
        (
            completed_task("SALES_INVOICE_EXECUTION", "receivable_account"),
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
    ];

    for (task_obj, full) in cases {
        let h = AuthHarness::with_facts("work_item", vec![task_obj]);
        h.assert_same_role_required(
            &[("task-reader", &[work_item::DETAIL])],
            &full,
            &internal::WORK_ITEM_EXECUTE,
            wi_task_target,
        )
        .await;
    }
}

#[tokio::test]
async fn portal_tasks_and_offering_update_materials_enforce_independent_source_scopes() {
    let portal_exec =
        [supplier_portal::ADMIN_APPLICATION, supplier_portal::ADMIN_REVIEW, supplier_portal::ADMIN_IMPACTS];

    // 新品门户审核任务
    let mut material = object("supplier_portal_material", "request", "other");
    material.subject_type = FactValue::Known("new_product".into());
    attach_relation(&mut material, "supplier", object("supplier", "supplier", "user"));
    let mut item = completed_task("SUPPLIER_PORTAL_REVIEW", "supplier_portal_request");
    attach_relation(&mut item, "portal_source", material);
    let h = AuthHarness::with_facts("work_item", vec![item]);
    h.grants(&[
        ("task-reader", &[work_item::DETAIL]),
        ("portal-executor", &portal_exec),
        ("supplier-reader", &[supplier::DETAIL]),
        ("catalog-create", &[catalog::PRODUCT_PRODUCT_CREATE]),
        ("offering-create", &[supplier_offering::CREATE]),
    ]);
    h.require_snap(&internal::WORK_ITEM_EXECUTE, wi_task_target()).await.unwrap();
    h.mutate_first("work_item", |w| {
        w.relations.values_mut().next().unwrap().objects[0].relations.values_mut().next().unwrap().objects
            [0]
        .owner_id = FactValue::Known(Some("other".into()));
    });
    assert!(matches!(
        h.require_snap(&internal::WORK_ITEM_EXECUTE, wi_task_target()).await,
        Err(Error::Forbidden(_))
    ));
    h.mutate_first("work_item", |w| {
        w.relations.values_mut().next().unwrap().objects[0].relations.values_mut().next().unwrap().objects
            [0]
        .owner_id = FactValue::Known(Some("user".into()));
    });
    h.grants(&[
        ("task-reader", &[work_item::DETAIL]),
        ("portal-executor", &portal_exec),
        ("supplier-reader", &[supplier::DETAIL]),
        ("catalog-create", &[catalog::PRODUCT_PRODUCT_CREATE]),
    ]);
    assert!(matches!(
        h.require_snap(&internal::WORK_ITEM_EXECUTE, wi_task_target()).await,
        Err(Error::Forbidden(_))
    ));

    // 供给变更物料读取检查两套独立来源范围
    let mut mat = object("supplier_portal_material", "material", "other");
    attach_relation(&mut mat, "supplier", object("supplier", "supplier", "user"));
    attach_relation(&mut mat, "offering", object("supplier_offering", "offering", "other"));
    let h = AuthHarness::with_facts("supplier_portal_material", vec![mat]);
    h.grants(&[("supplier-reader", &[supplier::DETAIL]), ("offering-reader", &[supplier_offering::LIST])]);
    assert!(matches!(
        h.require_snap(&internal::PORTAL_MATERIAL_READ_OFFERING, AccessTarget::record("material").unwrap())
            .await,
        Err(Error::Forbidden(_))
    ));
    h.mutate_first("supplier_portal_material", |m| {
        m.relations.get_mut(&RelationKey::parse("offering").unwrap()).unwrap().objects[0].owner_id =
            FactValue::Known(Some("user".into()));
    });
    h.require_snap(&internal::PORTAL_MATERIAL_READ_OFFERING, AccessTarget::record("material").unwrap())
        .await
        .unwrap();
    h.mutate_first("supplier_portal_material", |m| {
        m.relations.get_mut(&RelationKey::parse("offering").unwrap()).unwrap().objects.clear();
    });
    assert!(matches!(
        h.require_snap(&internal::PORTAL_MATERIAL_READ_OFFERING, AccessTarget::record("material").unwrap())
            .await,
        Err(Error::Forbidden(_))
    ));

    // 供给更新任务要求读取与审核双独立范围与资格
    let mut mat = object("supplier_portal_material", "request", "other");
    mat.subject_type = FactValue::Known("offering_update".into());
    attach_relation(&mut mat, "supplier", object("supplier", "supplier", "user"));
    attach_relation(&mut mat, "offering", object("supplier_offering", "offering", "other"));
    let mut item = completed_task("SUPPLIER_PORTAL_REVIEW", "supplier_portal_request");
    attach_relation(&mut item, "portal_source", mat);
    let h = AuthHarness::with_facts("work_item", vec![item]);
    h.grants(&[
        ("task-reader", &[work_item::DETAIL]),
        ("portal-executor", &portal_exec),
        ("supplier-reader", &[supplier::DETAIL]),
        ("offering-reader", &[supplier_offering::LIST]),
        ("offering-editor", &[supplier_offering::REVISE]),
    ]);
    let co = ScopeClause { company: true, ..Default::default() };
    h.set_role_scope("offering-reader", &supplier_offering::LIST, co.clone());
    h.set_role_scope("offering-editor", &supplier_offering::REVISE, co.clone());
    h.require_snap(&internal::WORK_ITEM_EXECUTE, wi_task_target()).await.unwrap();
    for (role, perm) in
        [("offering-reader", supplier_offering::LIST), ("offering-editor", supplier_offering::REVISE)]
    {
        h.set_role_scope(role, &perm, ScopeClause::default());
        assert!(matches!(
            h.require_snap(&internal::WORK_ITEM_EXECUTE, wi_task_target()).await,
            Err(Error::Forbidden(_))
        ));
        h.set_role_scope(role, &perm, co.clone());
    }
    for supplied in [&[supplier_offering::REVISE][..], &[supplier_offering::LIST][..]] {
        h.grants(&[
            ("task-reader", &[work_item::DETAIL]),
            ("portal-executor", &portal_exec),
            ("supplier-reader", &[supplier::DETAIL]),
            ("remaining-source", supplied),
        ]);
        assert!(matches!(
            h.require_snap(&internal::WORK_ITEM_EXECUTE, wi_task_target()).await,
            Err(Error::Forbidden(_))
        ));
    }
}

// ── 任务形成、接收与列表资格投影 ──────────────────────────────────────────────

#[tokio::test]
async fn formation_qualifications_validate_same_role_subsets_and_reject_invalid_candidates() {
    let h = AuthHarness::new();
    // 结算形成只检查审核决定
    let settlement_cand = || {
        AccessTarget::Candidate(wi_candidate("SUPPLIER_SETTLEMENT_REVIEW", "supplier_settlement_statement"))
    };
    h.grants(&[("reviewer", &[supplier_settlement::STATEMENT_REVIEW_DECIDE])]);
    h.require_snap(&internal::WORK_ITEM_EXECUTION_QUALIFICATION, settlement_cand()).await.unwrap();
    h.grants(&[("reader", &[supplier_settlement::STATEMENT_DETAIL, work_item::DETAIL])]);
    assert!(matches!(
        h.require_snap(&internal::WORK_ITEM_EXECUTION_QUALIFICATION, settlement_cand()).await,
        Err(Error::Forbidden(_))
    ));

    // 询价形成与客户验收形成须同角色持有全部执行权限
    let acceptance = [
        fulfillment::CUSTOMER_ACCEPTANCE_CUSTOMER_ACCEPTANCE_LIST,
        fulfillment::ACCEPTANCE_EVIDENCE_CUSTOMER_ACCEPTANCE_EVIDENCE,
        fulfillment::CUSTOMER_ACCEPTANCE_CUSTOMER_ACCEPTANCE_CREATE,
        fulfillment::CUSTOMER_ACCEPTANCE_CUSTOMER_ACCEPTANCE_COMMIT,
        sales_order::EVIDENCE_SALES_ORDER_EVIDENCE_DOWNLOAD,
    ];
    for (kind, source, required) in [
        (
            "SALES_INQUIRY_PROCESSING",
            "sales_inquiry",
            vec![
                sales_inquiry::QUERY_PROCUREMENT_DETAIL,
                sales_inquiry::COMMANDS_PROCUREMENT_SAVE_FEEDBACK,
                sales_inquiry::COMMANDS_PROCUREMENT_PUBLISH,
            ],
        ),
        ("CUSTOMER_ACCEPTANCE_REGISTRATION", "sales_order", acceptance.to_vec()),
    ] {
        h.assert_same_role_required(&[], &required, &internal::WORK_ITEM_EXECUTION_QUALIFICATION, || {
            AccessTarget::Candidate(wi_candidate(kind, source))
        })
        .await;
    }

    // 返工形成要求工作项详情而非来源读取
    let rework = || AccessTarget::Candidate(wi_candidate("APPROVAL_REWORK", "sales_order"));
    h.grants(&[("source-reader", &[sales_order::DETAIL])]);
    assert!(matches!(
        h.require_snap(&internal::WORK_ITEM_EXECUTION_QUALIFICATION, rework()).await,
        Err(Error::Forbidden(_))
    ));
    h.grants(&[("task-reader", &[work_item::DETAIL])]);
    h.require_snap(&internal::WORK_ITEM_EXECUTION_QUALIFICATION, rework()).await.unwrap();

    // 非法候选、非快照、到期与停用账号
    let h = AuthHarness::new();
    for invalid in [
        wi_candidate("UNKNOWN", "sales_order"),
        wi_candidate("CUSTOMER_ACCEPTANCE_REGISTRATION", "wrong"),
        ObjectFacts {
            id: "persisted".into(),
            ..wi_candidate("CUSTOMER_ACCEPTANCE_REGISTRATION", "sales_order")
        },
        ObjectFacts {
            version: "v1".into(),
            ..wi_candidate("CUSTOMER_ACCEPTANCE_REGISTRATION", "sales_order")
        },
    ] {
        assert!(matches!(
            h.require_snap(&internal::WORK_ITEM_EXECUTION_QUALIFICATION, AccessTarget::Candidate(invalid))
                .await,
            Err(Error::ValidationError(_))
        ));
    }
    let valid = wi_candidate("CUSTOMER_ACCEPTANCE_REGISTRATION", "sales_order");
    assert!(matches!(
        h.require_read(&internal::WORK_ITEM_EXECUTION_QUALIFICATION, AccessTarget::Candidate(valid.clone()))
            .await,
        Err(Error::ValidationError(_))
    ));
    let expired = ObjectFacts { valid_until: Some(Instant::from_unix_secs(1)), ..valid.clone() };
    assert!(matches!(
        h.require_snap(&internal::WORK_ITEM_EXECUTION_QUALIFICATION, AccessTarget::Candidate(expired)).await,
        Err(Error::ConflictError(_))
    ));
    h.set_active(false);
    assert!(matches!(
        h.require_snap(&internal::WORK_ITEM_EXECUTION_QUALIFICATION, AccessTarget::Candidate(valid)).await,
        Err(Error::Forbidden(_))
    ));
}

#[tokio::test]
async fn recipient_qualifications_enforce_exact_tasks_independent_sources_and_supplements() {
    // 结算接收人同角色与版本
    let settlement = ObjectFacts {
        id: "settlement-task".into(),
        version: "3".into(),
        owner_id: FactValue::Known(Some("other".into())),
        org_unit_id: FactValue::Known(None),
        assignee_id: FactValue::Known(Some("other".into())),
        ..wi_candidate("SUPPLIER_SETTLEMENT_REVIEW", "supplier_settlement_statement")
    };
    let h = AuthHarness::with_facts("work_item", vec![settlement]);
    let s_target = || AccessTarget::Task { id: "settlement-task".into(), version: "3".into() };
    let s_req = [supplier_settlement::STATEMENT_DETAIL, supplier_settlement::STATEMENT_REVIEW_DECIDE];
    h.assert_same_role_required(&[], &s_req, &internal::WORK_ITEM_RECIPIENT, s_target).await;
    h.grants(&[("reviewer", &s_req)]);
    assert!(
        h.require_snap(
            &internal::WORK_ITEM_RECIPIENT,
            AccessTarget::Task { id: "settlement-task".into(), version: "2".into() }
        )
        .await
        .is_err()
    );

    // 采购付款接收人：精确任务、独立来源角色、历史与显式记录补权
    let h = AuthHarness::with_facts("work_item", vec![payment_arrangement_task()]);
    let p_req = [purchase_order::DETAIL, purchase_payment_request::CREATE, purchase_payment_request::SUBMIT];
    h.assert_same_role_required(
        &[("source-reader", &[work_item::DETAIL])],
        &p_req,
        &internal::WORK_ITEM_RECIPIENT,
        wi_task_target,
    )
    .await;
    h.grants(&[("executor", &p_req), ("source-reader", &[work_item::DETAIL])]);
    for (id, ver) in [("task", "old"), ("missing", "v1")] {
        assert!(
            h.require_snap(
                &internal::WORK_ITEM_RECIPIENT,
                AccessTarget::Task { id: id.into(), version: ver.into() }
            )
            .await
            .is_err()
        );
    }
    assert!(matches!(
        h.require_snap(&internal::WORK_ITEM_RECIPIENT, AccessTarget::Candidate(payment_arrangement_task()))
            .await,
        Err(Error::ValidationError(_))
    ));
    h.set_role_scope("executor", &purchase_order::DETAIL, ScopeClause::default());
    h.set_role_scope(
        "source-reader",
        &purchase_order::DETAIL,
        ScopeClause { self_owned: true, ..Default::default() },
    );
    h.require_snap(&internal::WORK_ITEM_RECIPIENT, wi_task_target()).await.unwrap();
    h.grants(&[("executor", &p_req)]);
    assert!(matches!(
        h.require_snap(&internal::WORK_ITEM_RECIPIENT, wi_task_target()).await,
        Err(Error::Forbidden(_))
    ));

    h.identity.role_scopes.lock().unwrap().clear();
    h.grants(&[("executor", &p_req), ("task-reader", &[work_item::DETAIL])]);
    h.mutate_first("work_item", |w| {
        let src = &mut w.relations.values_mut().next().unwrap().objects[0];
        src.owner_id = FactValue::Known(Some("other".into()));
        src.history_participants = FactValue::Known([actor().id().into()].into());
    });
    h.require_snap(&internal::WORK_ITEM_RECIPIENT, wi_task_target()).await.unwrap();
    h.mutate_first("work_item", |w| {
        w.relations.values_mut().next().unwrap().objects[0].history_participants =
            FactValue::Known(Default::default());
    });
    assert!(matches!(
        h.require_snap(&internal::WORK_ITEM_RECIPIENT, wi_task_target()).await,
        Err(Error::Forbidden(_))
    ));
    h.add_record_grant(ResourceKind::PurchaseOrder, "purchase");
    h.require_snap(&internal::WORK_ITEM_RECIPIENT, wi_task_target()).await.unwrap();
    h.mutate_first("work_item", |w| w.relations.clear());
    assert!(matches!(
        h.require_snap(&internal::WORK_ITEM_RECIPIENT, wi_task_target()).await,
        Err(Error::ValidationError(_))
    ));
    h.mutate_first("work_item", |w| w.subject_type = FactValue::Known("UNKNOWN".into()));
    assert!(matches!(
        h.require_snap(&internal::WORK_ITEM_RECIPIENT, wi_task_target()).await,
        Err(Error::ValidationError(_))
    ));

    // 履约任务接收人沿用工作项读取角色且拒绝销售历史补权（接受显式记录授权）
    let mut deliv = payment_arrangement_task();
    deliv.subject_type = FactValue::Known("FULFILLMENT_OPERATION".into());
    deliv.subject_source_resource = FactValue::Known("delivery".into());
    deliv.relations.values_mut().next().unwrap().objects = vec![object("sales_order", "purchase", "user")];
    let h = AuthHarness::with_facts("work_item", vec![deliv]);
    let full = [
        fulfillment::DELIVERY_DELIVERY_LIST,
        fulfillment::DELIVERY_DELIVERY_DETAIL,
        fulfillment::DELIVERY_DELIVERY_UPDATE,
        fulfillment::DELIVERY_DELIVERY_POST,
    ];
    h.grants(&[("executor", &full), ("task-reader", &[work_item::DETAIL])]);
    h.require_snap(&internal::WORK_ITEM_RECIPIENT, wi_task_target()).await.unwrap();
    h.grants(&[("executor", &full)]);
    assert!(matches!(
        h.require_snap(&internal::WORK_ITEM_RECIPIENT, wi_task_target()).await,
        Err(Error::Forbidden(_))
    ));
    h.grants(&[("executor", &full), ("task-reader", &[work_item::DETAIL])]);
    h.mutate_first("work_item", |w| {
        let src = &mut w.relations.values_mut().next().unwrap().objects[0];
        src.owner_id = FactValue::Known(Some("other".into()));
        src.history_participants = FactValue::Known([actor().id().into()].into());
    });
    assert!(matches!(
        h.require_snap(&internal::WORK_ITEM_RECIPIENT, wi_task_target()).await,
        Err(Error::Forbidden(_))
    ));
    h.add_record_grant(ResourceKind::SalesOrder, "purchase");
    h.require_snap(&internal::WORK_ITEM_RECIPIENT, wi_task_target()).await.unwrap();
}

#[tokio::test]
async fn query_qualification_projections_align_with_recipient_and_reject_stale_or_unregistered_pairs() {
    // 列表资格投影与接收人检查一致
    let h = AuthHarness::with_facts("work_item", vec![payment_arrangement_task()]);
    let full = [purchase_order::DETAIL, purchase_payment_request::CREATE, purchase_payment_request::SUBMIT];
    for allowed in [true, false] {
        let partial = if allowed { full.as_slice() } else { &full[..2] };
        h.grants(&[
            ("reader", &[work_item::LIST, work_item::DETAIL, work_item::MANAGE]),
            ("executor", partial),
            ("other", &full[2..]),
            ("decision", &[approval_instance::COPIES_HISTORY, approval_instance::SUBMIT_DECISION]),
        ]);
        for perm in [work_item::LIST, work_item::DETAIL, work_item::MANAGE] {
            let mut ex = SnapshotExecutor::default();
            let access = h.query(&perm, &mut ex).await.unwrap();
            assert_eq!(
                access
                    .execution_qualified("PURCHASE_PAYMENT_ARRANGEMENT", "purchase_order", &mut ex)
                    .unwrap(),
                allowed
            );
            assert!(access.decision_qualified(&mut ex).unwrap());
            h.validate(&access, &mut ex).await.unwrap();
            assert_eq!(
                h.require(&internal::WORK_ITEM_RECIPIENT, wi_task_target(), &mut ex).await.is_ok(),
                allowed
            );
        }
    }

    // 非对应查询、到期、切阶段和重启事务均拒绝
    let h = AuthHarness::new();
    let mut ex = SnapshotExecutor::default();
    let other = h.query(&sales_inquiry::QUERY_SALES_LIST, &mut ex).await.unwrap();
    assert!(matches!(other.decision_qualified(&mut ex), Err(Error::ValidationError(_))));
    let mut access = h.query(&work_item::LIST, &mut ex).await.unwrap();
    assert!(access.execution_qualified("PURCHASE_PAYMENT_ARRANGEMENT", "purchase_order", &mut ex).unwrap());
    access.validity.valid_until = Some(Instant::from_unix_secs(1));
    assert!(matches!(access.decision_qualified(&mut ex), Err(Error::ConflictError(_))));
    let access = h.query(&work_item::LIST, &mut ex).await.unwrap();
    ex.next_read();
    assert!(matches!(access.decision_qualified(&mut ex), Err(Error::ValidationError(_))));
    let access = h.query(&work_item::LIST, &mut ex).await.unwrap();
    ex.restart();
    assert!(matches!(access.decision_qualified(&mut ex), Err(Error::ValidationError(_))));

    // 结算形成 != 展示/办理、空执行子集仍需来源读、返工同角色、未登记组合及非事务可选资格撤销
    let kind = "SUPPLIER_SETTLEMENT_REVIEW";
    let src = "supplier_settlement_statement";
    let h = AuthHarness::with_facts("work_item", vec![completed_task(kind, src)]);
    let s_req = [supplier_settlement::STATEMENT_REVIEW_DECIDE, supplier_settlement::STATEMENT_DETAIL];
    for (rev, other_r, allowed) in [
        (&s_req[..1], &s_req[..0], false),
        (&s_req[..1], &s_req[1..], false),
        (&s_req[..], &s_req[..0], true),
    ] {
        h.grants(&[
            ("task-reader", &[work_item::LIST, work_item::DETAIL]),
            ("reviewer", rev),
            ("other", other_r),
        ]);
        let mut ex = SnapshotExecutor::default();
        h.require(
            &internal::WORK_ITEM_EXECUTION_QUALIFICATION,
            AccessTarget::Candidate(wi_candidate(kind, src)),
            &mut ex,
        )
        .await
        .unwrap();
        let access = h.query(&work_item::LIST, &mut ex).await.unwrap();
        assert_eq!(access.execution_qualified(kind, src, &mut ex).unwrap(), allowed);
        h.validate(&access, &mut ex).await.unwrap();
        for p in [internal::WORK_ITEM_RECIPIENT, internal::WORK_ITEM_EXECUTE] {
            assert_eq!(h.require(&p, wi_task_target(), &mut ex).await.is_ok(), allowed);
        }
    }

    let err_detail = [integration_ops::ERROR_TASK_DETAIL];
    for (sr, allowed) in [(&err_detail[..0], false), (&err_detail[..], true)] {
        h.grants(&[("task-reader", &[work_item::LIST]), ("source-reader", sr)]);
        let mut ex = SnapshotExecutor::default();
        h.require(
            &internal::WORK_ITEM_EXECUTION_QUALIFICATION,
            AccessTarget::Candidate(wi_candidate("BUSINESS_EXCEPTION", "integration_error_task")),
            &mut ex,
        )
        .await
        .unwrap();
        let access = h.query(&work_item::LIST, &mut ex).await.unwrap();
        assert_eq!(
            access.execution_qualified("BUSINESS_EXCEPTION", "integration_error_task", &mut ex).unwrap(),
            allowed
        );
        h.validate(&access, &mut ex).await.unwrap();
    }

    let rw_req = [work_item::DETAIL, inventory::STOCK_ADJUSTMENT_DETAIL];
    for (rw, allowed) in [(&rw_req[..1], false), (&rw_req[..], true)] {
        h.grants(&[("list-reader", &[work_item::LIST]), ("rework", rw), ("source-reader", &rw_req[1..])]);
        let mut ex = SnapshotExecutor::default();
        let access = h.query(&work_item::LIST, &mut ex).await.unwrap();
        assert_eq!(
            access.execution_qualified("APPROVAL_REWORK", "stock_adjustment", &mut ex).unwrap(),
            allowed
        );
        h.validate(&access, &mut ex).await.unwrap();
    }

    // 未登记组合
    let unreg_kind = "APPROVAL_REWORK";
    let unreg_src = "integration_error_task";
    let h = AuthHarness::with_facts("work_item", vec![completed_task(unreg_kind, unreg_src)]);
    let mut ex = SnapshotExecutor::default();
    let access = h.query(&work_item::LIST, &mut ex).await.unwrap();
    assert!(matches!(
        access.execution_qualified(unreg_kind, unreg_src, &mut ex),
        Err(Error::ValidationError(_))
    ));
    h.validate(&access, &mut ex).await.unwrap();
    assert!(matches!(
        h.require(
            &internal::WORK_ITEM_EXECUTION_QUALIFICATION,
            AccessTarget::Candidate(wi_candidate(unreg_kind, unreg_src)),
            &mut ex
        )
        .await,
        Err(Error::ValidationError(_))
    ));
    for p in [internal::WORK_ITEM_RECIPIENT, internal::WORK_ITEM_EXECUTE] {
        assert!(matches!(h.require(&p, wi_task_target(), &mut ex).await, Err(Error::ValidationError(_))));
    }

    // 非事务可选资格撤销
    h.grants(&[("reader", &[work_item::LIST]), ("executor", &full)]);
    let mut read_ex = ReadExecutor::default();
    let access = h.query(&work_item::LIST, &mut read_ex).await.unwrap();
    assert!(
        access.execution_qualified("PURCHASE_PAYMENT_ARRANGEMENT", "purchase_order", &mut read_ex).unwrap()
    );
    h.grants(&[("reader", &[work_item::LIST])]);
    assert!(matches!(h.validate(&access, &mut read_ex).await, Err(Error::ConflictError(_))));
}
