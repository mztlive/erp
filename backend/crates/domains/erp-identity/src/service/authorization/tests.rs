//! 用替身设施执行真实 Builder 和公共 API；不连接数据库。
mod access_inspection;
mod approval_cancel;
mod approval_decision;
mod approval_management;
mod bank_reveal;
mod contract_import;
mod entry_migration;
mod fixtures;
mod integration_difference;
mod inventory_warehouses;
mod invoice_request_commands;
mod management_contract;
mod migration_fixes;
mod phase5;
mod phase5_regressions;
mod phase5_review_fixes;
mod phase6_read_entries;
mod policy_bundles;
mod purchase_entries;
mod read_delivery_expiry;
mod reverse_commit;
mod review_handover_delivery;
mod review_management_query;
mod review_query_targets;
mod sales_change_draft;
mod supplier_offering_detail;
mod work_item_detail;
mod work_item_execution;
mod work_item_qualifications;

use std::collections::BTreeSet;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use async_trait::async_trait;
use erp_core::common::time::Instant;
use erp_permissions::{
    approval_instance, integration_ops, purchase_order, purchase_payment_request, receivable, returns,
    sales_inquiry as inquiry, sales_order,
};
use fixtures::*;
use persistence_core::testing::SnapshotExecutor;
use persistence_core::{Executor, ReadExecutor};
use serde_json::json;

use super::*;
use crate::entity::access_control::{ResolvedScope, ScopeClause};
use crate::entity::authorization::{
    AccessContext, AccessTarget, FactKind, FactValue, ObjectFacts, QueryOutput, RelatedObjects, RelationKey,
};
use crate::entity::resource_access::ResourceKind;
use crate::repository::authorization::testing::{receipt_projection, task_members};
use crate::{Error, ResourceTargets};

// 完整绑定可通过启动校验；缺项、重复登记、未知项或资源错配均被拒绝。
#[test]
fn builder_accepts_complete_bindings_and_rejects_missing_duplicate_unknown_and_mismatched_bindings() {
    complete().build().unwrap();
    assert!(
        matches!(empty().build(), Err(Error::ValidationError(message)) if message.contains("缺少资源 Facts"))
    );
    let mut missing = complete();
    missing.mappings.remove(&inquiry::QUERY_SALES_LIST);
    assert!(
        matches!(missing.build(), Err(Error::ValidationError(message)) if message.contains("缺少查询 Mapping"))
    );

    let facts = Arc::new(Facts::default());
    assert!(matches!(empty().facts("unknown", facts.clone()), Err(Error::ValidationError(_))));
    assert!(matches!(
        empty().facts("sales_inquiry", facts.clone()).unwrap().facts("sales_inquiry", facts),
        Err(Error::ValidationError(_))
    ));

    let map = mapping(&inquiry::QUERY_SALES_LIST);
    assert!(matches!(
        empty()
            .mapping(&inquiry::QUERY_SALES_LIST, map.clone())
            .unwrap()
            .mapping(&inquiry::QUERY_SALES_LIST, map),
        Err(Error::ValidationError(_))
    ));
    assert!(matches!(
        empty().mapping(&inquiry::COMMANDS_SALES_SAVE, mapping(&inquiry::QUERY_SALES_LIST)),
        Err(Error::ValidationError(_))
    ));
    assert!(matches!(
        empty().mapping(&Permission::new("unwired", "list"), mapping(&inquiry::QUERY_SALES_LIST)),
        Err(Error::ValidationError(_))
    ));
    assert!(matches!(
        empty().mapping(&sales_order::LIST, mapping(&inquiry::QUERY_SALES_LIST)),
        Err(Error::ValidationError(_))
    ));
}

// 服务启动前就校验查询所需的事实能力、来源关系和输出形状。
#[test]
fn builder_checks_fact_capabilities_sources_and_output_before_any_request() {
    let permission = &inquiry::QUERY_SALES_LIST;
    let mut map = mapping(permission);
    Arc::get_mut(&mut map).unwrap().model.facts.remove(&FactKind::Organization);
    let mut builder = complete();
    builder.mappings.insert(permission.clone(), map);
    assert!(matches!(builder.build(), Err(Error::ValidationError(_))));

    let mut map = mapping(permission);
    Arc::get_mut(&mut map).unwrap().model.output = QueryOutput::Share { fields: ["id".into()].into() };
    let mut builder = complete();
    builder.mappings.insert(permission.clone(), map);
    assert!(matches!(builder.build(), Err(Error::ValidationError(_))));

    let permission = &receivable::CUSTOMER_RECEIPT_LIST;
    let mut map = mapping(permission);
    Arc::get_mut(&mut map).unwrap().relations.clear();
    let mut builder = complete();
    builder.mappings.insert(permission.clone(), map);
    assert!(matches!(builder.build(), Err(Error::ValidationError(_))));
}

// 采购创建用于查询时，映射必须提供任务负责人事实。
#[test]
fn builder_requires_assignment_for_purchase_creation_queries() {
    let permission = purchase_order::CREATE;
    let mut map = mapping(&permission);
    assert!(Arc::get_mut(&mut map).unwrap().model.facts.remove(&FactKind::Assignee));
    let mut builder = complete();
    builder.mappings.insert(permission.clone(), map);

    assert!(matches!(
        builder.build(),
        Err(Error::ValidationError(message))
            if message.contains(&format!("授权绑定 {permission}")) && message.contains("Assignee")
    ));
}

// 采购创建查询不支持份额或存在性输出，即使映射还缺少负责人事实。
#[test]
fn purchase_creation_query_binding_still_rejects_restricted_outputs() {
    let permission = purchase_order::CREATE;
    for output in [
        QueryOutput::Share { fields: ["id".into()].into() },
        QueryOutput::Existence { fields: ["id".into()].into() },
    ] {
        let mut map = mapping(&permission);
        let model = &mut Arc::get_mut(&mut map).unwrap().model;
        model.output = output;
        model.facts.remove(&FactKind::Assignee);
        let mut builder = complete();
        builder.mappings.insert(permission.clone(), map);
        assert!(matches!(
            builder.build(),
            Err(Error::ValidationError(message)) if message == "此权限不支持受限输出"
        ));
    }
}

// 采购创建查询按负责人过滤；写入只接受服务端候选或带版本的精确任务。
#[tokio::test]
async fn purchase_creation_keeps_its_candidate_and_exact_task_contracts() {
    let permission = purchase_order::CREATE;
    let facts = Arc::new(Facts::new(vec![ObjectFacts {
        assignee_id: FactValue::Known(Some("user".into())),
        ..object(permission.resource(), "task", "user")
    }]));
    let mut builder = complete();
    builder.facts.insert("work_item".into(), facts.clone());
    let service = builder.build().unwrap();
    let mut read = ReadExecutor::default();
    let access = service.query(&actor(), &permission, &mut read).await.unwrap();
    assert_eq!(
        task_members(access.stages(), &[("mine", "user", false, ""), ("theirs", "other", false, "")]),
        ["mine"]
    );
    service.validate(&access, &mut read).await.unwrap();

    let mut executor = SnapshotExecutor::default();
    service
        .require(
            &actor(),
            &permission,
            AccessTarget::Candidate(candidate(permission.resource(), "user")),
            &mut executor,
        )
        .await
        .unwrap();
    assert!(matches!(
        service.require(&actor(), &permission, AccessTarget::record("task").unwrap(), &mut executor).await,
        Err(Error::ValidationError(message)) if message == "此权限不支持该授权目标"
    ));
    assert_eq!(facts.calls.load(Ordering::SeqCst), 0);
    service
        .require(
            &actor(),
            &permission,
            AccessTarget::Task { id: "task".into(), version: "v1".into() },
            &mut executor,
        )
        .await
        .unwrap();
    assert_eq!(facts.calls.load(Ordering::SeqCst), 1);
}

// 对象授权以已加载的归属为准，拒绝缺失事实；没有功能资格时不读取对象。
#[tokio::test]
async fn require_checks_real_owner_and_missing_facts_and_stops_before_facts_without_qualification() {
    let identity = Arc::new(Identity::default());
    let port = Arc::new(Facts::new(vec![object("sales_inquiry", "one", "user")]));
    let service = service(identity.clone(), port.clone());
    let mut ex = SnapshotExecutor::default();
    service
        .require(&actor(), &inquiry::COMMANDS_SALES_SAVE, AccessTarget::record("one").unwrap(), &mut ex)
        .await
        .unwrap();
    assert_eq!(port.calls.load(Ordering::SeqCst), 1);
    assert_eq!(identity.permissions_calls.load(Ordering::SeqCst), 1);

    port.objects.lock().unwrap()[0].owner_id = FactValue::Known(Some("other".into()));
    assert!(matches!(
        service
            .require(&actor(), &inquiry::COMMANDS_SALES_SAVE, AccessTarget::record("one").unwrap(), &mut ex)
            .await,
        Err(Error::Forbidden(_))
    ));

    port.objects.lock().unwrap()[0].owner_id = FactValue::Missing;
    assert!(matches!(
        service
            .require(&actor(), &inquiry::COMMANDS_SALES_SAVE, AccessTarget::record("one").unwrap(), &mut ex)
            .await,
        Err(Error::ValidationError(_))
    ));

    identity.state.lock().unwrap().grants = vec![];
    let calls = port.calls.load(Ordering::SeqCst);
    assert!(matches!(
        service
            .require(&actor(), &inquiry::COMMANDS_SALES_SAVE, AccessTarget::record("one").unwrap(), &mut ex)
            .await,
        Err(Error::Forbidden(_))
    ));
    assert_eq!(port.calls.load(Ordering::SeqCst), calls);

    assert!(matches!(
        service.require(&actor(), &inquiry::COMMANDS_SALES_SAVE, AccessTarget::Operation, &mut ex).await,
        Err(Error::ValidationError(_))
    ));
    assert!(matches!(
        service
            .require(
                &actor(),
                &inquiry::COMMANDS_SALES_SAVE,
                AccessTarget::record("one").unwrap(),
                &mut ReadExecutor::default()
            )
            .await,
        Err(Error::ValidationError(_))
    ));
}

// 创建只检查服务端候选归属；改归属须同时通过原对象和目标归属的检查。
#[tokio::test]
async fn candidates_and_ownership_change_check_server_destination_and_original() {
    let port = Arc::new(Facts::new(vec![object("sales_inquiry", "one", "user")]));
    let service = service(Arc::new(Identity::default()), port.clone());
    let mut ex = SnapshotExecutor::default();
    let destination = candidate("sales_inquiry", "user");
    service
        .require(
            &actor(),
            &inquiry::COMMANDS_SALES_CREATE,
            AccessTarget::Candidate(destination.clone()),
            &mut ex,
        )
        .await
        .unwrap();
    assert_eq!(port.calls.load(Ordering::SeqCst), 0);

    for (owner, allowed) in [("user", true), ("other", false)] {
        let target =
            AccessTarget::OwnershipChange { id: "one".into(), candidate: candidate("sales_inquiry", owner) };
        let result = service.require(&actor(), &inquiry::COMMANDS_SALES_SAVE, target, &mut ex).await;
        if allowed {
            result.unwrap();
        } else {
            assert!(matches!(result, Err(Error::Forbidden(_))));
        }
    }

    let mut forged = destination;
    forged.version = "persisted".into();
    assert!(matches!(
        service
            .require(&actor(), &inquiry::COMMANDS_SALES_CREATE, AccessTarget::Candidate(forged), &mut ex)
            .await,
        Err(Error::ValidationError(_))
    ));
}

// 精确任务需要当前版本和指派，附加权限不能由另一个角色补齐。
#[tokio::test]
async fn exact_tasks_require_version_assignment_and_same_role_extra_permission() {
    let task = ObjectFacts {
        assignee_id: FactValue::Known(Some("user".into())),
        subject_type: FactValue::Known("stock_adjustment".into()),
        ..object("approval_instance", "task", "user")
    };
    let port = Arc::new(Facts::new(vec![task]));
    let identity = Arc::new(Identity::default());
    let mut builder = complete();
    builder.facilities = identity.clone();
    builder.facts.insert("approval_instance".into(), port.clone());
    let service = builder.build().unwrap();
    let mut ex = SnapshotExecutor::default();
    let target = || AccessTarget::Task { id: "task".into(), version: "v1".into() };

    service.require(&actor(), &approval_instance::SUBMIT_DECISION, target(), &mut ex).await.unwrap();
    assert!(matches!(
        service
            .require(
                &actor(),
                &approval_instance::SUBMIT_DECISION,
                AccessTarget::record("task").unwrap(),
                &mut ex
            )
            .await,
        Err(Error::ValidationError(_))
    ));
    assert!(matches!(
        service
            .require(
                &actor(),
                &approval_instance::SUBMIT_DECISION,
                AccessTarget::Task { id: "task".into(), version: "old".into() },
                &mut ex
            )
            .await,
        Err(Error::ConflictError(_))
    ));

    port.objects.lock().unwrap()[0].assignee_id = FactValue::Known(Some("other".into()));
    assert!(matches!(
        service.require(&actor(), &approval_instance::SUBMIT_DECISION, target(), &mut ex).await,
        Err(Error::Forbidden(_))
    ));

    identity.state.lock().unwrap().grants = vec![
        ("decide".into(), vec![approval_instance::SUBMIT_DECISION]),
        ("read".into(), vec![approval_instance::INSTANCE_LIST]),
    ];
    assert!(matches!(
        service.require(&actor(), &approval_instance::SUBMIT_DECISION, target(), &mut ex).await,
        Err(Error::Forbidden(_))
    ));
}

// 询价列表按归属过滤；快照内交付不重取身份，普通读取交付复验身份和范围版本。
#[tokio::test]
async fn query_compiles_owner_condition_and_validate_respects_snapshot_and_read_dependencies() {
    let identity = Arc::new(Identity::default());
    let service = service(identity.clone(), Arc::new(Facts::default()));
    let mut ex = SnapshotExecutor::default();
    let access = service.query(&actor(), &inquiry::QUERY_SALES_LIST, &mut ex).await.unwrap();
    assert_eq!(inquiry_members(&access), vec!["one"]);
    let calls = identity.permissions_calls.load(Ordering::SeqCst);
    service.validate(&access, &mut ex).await.unwrap();
    assert_eq!(identity.permissions_calls.load(Ordering::SeqCst), calls);
    assert!(matches!(service.validate(&access, &mut ex).await, Err(Error::ValidationError(_))));

    let mut ex = ReadExecutor::default();
    let access = service.query(&actor(), &inquiry::QUERY_SALES_LIST, &mut ex).await.unwrap();
    service.validate(&access, &mut ex).await.unwrap();

    let access = service.query(&actor(), &inquiry::QUERY_SALES_LIST, &mut ex).await.unwrap();
    identity.revision.fetch_add(1, Ordering::SeqCst);
    assert!(
        matches!(service.validate(&access, &mut ex).await, Err(Error::ConflictError(message)) if message.starts_with("DATA_SCOPE_CHANGED"))
    );

    let access = service.query(&actor(), &inquiry::QUERY_SALES_LIST, &mut ex).await.unwrap();
    identity.state.lock().unwrap().active = false;
    assert!(matches!(service.validate(&access, &mut ex).await, Err(Error::ConflictError(_))));
}

// 批量功能资格区分停用账号、无授权和权限分属不同角色的情况。
#[tokio::test]
async fn functional_queries_preserve_disabled_empty_and_independent_grants() {
    let identity = Arc::new(Identity::default());
    let service = service(identity.clone(), Arc::new(Facts::default()));
    let mut ex = ReadExecutor::default();
    identity.state.lock().unwrap().grants = vec![
        ("read".into(), vec![inquiry::QUERY_SALES_LIST]),
        ("write".into(), vec![inquiry::COMMANDS_SALES_SAVE]),
    ];
    let permissions = [inquiry::QUERY_SALES_LIST, inquiry::COMMANDS_SALES_SAVE];
    let qualified = service.permissions(&actor(), &permissions, &mut ex).await.unwrap().unwrap();
    assert_eq!(
        qualified.permissions().into_vec().into_iter().collect::<BTreeSet<_>>(),
        permissions.clone().into()
    );
    assert!(!qualified.same_role(&permissions));
    identity.state.lock().unwrap().grants = vec![];
    assert!(
        service
            .permissions(&actor(), &permissions, &mut ex)
            .await
            .unwrap()
            .unwrap()
            .permissions()
            .into_vec()
            .is_empty()
    );
    assert!(!service.has_permission(&actor(), &permissions[0], &mut ex).await.unwrap());
    identity.state.lock().unwrap().active = false;
    assert!(service.permissions(&actor(), &permissions, &mut ex).await.unwrap().is_none());
}

// 空数据范围只返回空结果；身份取证失败不得当成空范围交付。
#[tokio::test]
async fn query_denies_empty_scope_and_propagates_evidence_failures() {
    let identity = Arc::new(Identity::default());
    let service = service(identity.clone(), Arc::new(Facts::default()));
    let mut ex = ReadExecutor::default();
    identity.state.lock().unwrap().denied_scope = true;
    let access = service.query(&actor(), &inquiry::QUERY_SALES_LIST, &mut ex).await.unwrap();
    assert!(inquiry_members(&access).is_empty());
    identity.state.lock().unwrap().fail = true;
    assert!(
        matches!(service.validate(&access, &mut ex).await, Err(Error::Internal(message)) if message == "identity storage unavailable")
    );
    assert!(matches!(
        service.has_permission(&actor(), &inquiry::QUERY_SALES_LIST, &mut ex).await,
        Err(Error::Internal(_))
    ));
}

// 查询凭证只能用于原服务和原读取阶段，换执行器、重新查询或重启事务后不得复用。
#[tokio::test]
async fn credentials_reject_other_service_executor_next_query_restart_and_raw_session() {
    let service = service(Arc::new(Identity::default()), Arc::new(Facts::default()));
    let other = complete().build().unwrap();
    let mut ex = SnapshotExecutor::default();
    let access = service.query(&actor(), &inquiry::QUERY_SALES_LIST, &mut ex).await.unwrap();
    assert!(matches!(other.validate(&access, &mut ex).await, Err(Error::ValidationError(_))));
    assert!(matches!(
        service.validate(&access, &mut SnapshotExecutor::default()).await,
        Err(Error::ValidationError(_))
    ));
    let newer = service.query(&actor(), &inquiry::QUERY_SALES_LIST, &mut ex).await.unwrap();
    assert!(matches!(service.validate(&access, &mut ex).await, Err(Error::ValidationError(_))));
    ex.restart();
    assert!(matches!(service.validate(&newer, &mut ex).await, Err(Error::ValidationError(_))));
    let access = service.query(&actor(), &inquiry::QUERY_SALES_LIST, &mut ex).await.unwrap();
    ex.session();
    assert!(matches!(service.validate(&access, &mut ex).await, Err(Error::ValidationError(_))));
}

// 动态映射在查询时准备，已到期则拒绝；交付时非事务复验版本，快照内不重复准备。
#[tokio::test]
async fn dynamic_mapping_revisions_and_expiry_are_checked_by_public_delivery_api() {
    let mut map = mapping(&inquiry::QUERY_SALES_LIST);
    Arc::get_mut(&mut map).unwrap().dynamic = true;
    let mut builder = complete();
    builder.mappings.insert(inquiry::QUERY_SALES_LIST, map.clone());
    let service = builder.build().unwrap();
    assert_eq!(map.prepares.load(Ordering::SeqCst), 0);

    let mut ex = ReadExecutor::default();
    let access = service.query(&actor(), &inquiry::QUERY_SALES_LIST, &mut ex).await.unwrap();
    assert_eq!(map.prepares.load(Ordering::SeqCst), 1);
    service.validate(&access, &mut ex).await.unwrap();
    assert_eq!(map.prepares.load(Ordering::SeqCst), 2);

    let access = service.query(&actor(), &inquiry::QUERY_SALES_LIST, &mut ex).await.unwrap();
    map.revision.fetch_add(1, Ordering::SeqCst);
    assert!(matches!(service.validate(&access, &mut ex).await, Err(Error::ConflictError(_))));

    let mut snapshot = SnapshotExecutor::default();
    let access = service.query(&actor(), &inquiry::QUERY_SALES_LIST, &mut snapshot).await.unwrap();
    let calls = map.prepares.load(Ordering::SeqCst);
    service.validate(&access, &mut snapshot).await.unwrap();
    assert_eq!(map.prepares.load(Ordering::SeqCst), calls);

    *map.expires.lock().unwrap() = Some(Instant::from_unix_secs(1));
    assert!(matches!(
        service.query(&actor(), &inquiry::QUERY_SALES_LIST, &mut snapshot).await,
        Err(Error::ConflictError(_))
    ));
}

// 不能复验版本的动态映射只支持快照，准备过程中也不得切换读取阶段。
#[tokio::test]
async fn dynamic_mapping_without_revision_only_supports_snapshot_and_cannot_change_stage() {
    let mut map = mapping(&inquiry::QUERY_SALES_LIST);
    let data = Arc::get_mut(&mut map).unwrap();
    data.dynamic = true;
    data.reliable = false;
    let mut builder = complete();
    builder.mappings.insert(inquiry::QUERY_SALES_LIST, map.clone());
    let service = builder.build().unwrap();
    assert!(matches!(
        service.query(&actor(), &inquiry::QUERY_SALES_LIST, &mut ReadExecutor::default()).await,
        Err(Error::ValidationError(_))
    ));
    let mut ex = SnapshotExecutor::default();
    let access = service.query(&actor(), &inquiry::QUERY_SALES_LIST, &mut ex).await.unwrap();
    service.validate(&access, &mut ex).await.unwrap();
    map.change_stage.store(true, Ordering::SeqCst);
    assert!(matches!(
        service.query(&actor(), &inquiry::QUERY_SALES_LIST, &mut ex).await,
        Err(Error::ValidationError(_))
    ));
}

// 整条记录授权不能替代功能权限，列表交付还要复验记录授权版本。
#[tokio::test]
async fn whole_record_reads_require_functional_permission_and_revalidate_grant_revision() {
    let identity = Arc::new(Identity::default());
    identity.state.lock().unwrap().denied_scope = true;
    identity.record_grants.lock().unwrap().insert((ResourceKind::SalesOrder, "one".into()));
    let port = Arc::new(Facts::new(vec![object("sales_order", "one", "other")]));
    let mut builder = complete();
    builder.facilities = identity.clone();
    builder.facts.insert("sales_order".into(), port);
    let service = builder.build().unwrap();
    let mut ex = ReadExecutor::default();
    service
        .require(&actor(), &sales_order::DETAIL, AccessTarget::record("one").unwrap(), &mut ex)
        .await
        .unwrap();

    let access = service.query(&actor(), &sales_order::LIST, &mut ex).await.unwrap();
    identity.grant_revision.fetch_add(1, Ordering::SeqCst);
    assert!(matches!(service.validate(&access, &mut ex).await, Err(Error::ConflictError(_))));

    identity.state.lock().unwrap().grants.clear();
    assert!(matches!(
        service.require(&actor(), &sales_order::DETAIL, AccessTarget::record("one").unwrap(), &mut ex).await,
        Err(Error::Forbidden(_))
    ));
}

// 按真实引用加载创建所需的采购来源，拒绝不完整或由调用方冒充的来源。
#[tokio::test]
async fn source_creation_loads_actual_references_and_rejects_missing_or_fabricated_sources() {
    let identity = Arc::new(Identity::default());
    let permission = purchase_payment_request::CREATE;
    identity.state.lock().unwrap().grants = vec![("entry".into(), vec![permission.clone()])];
    let port = Arc::new(Facts::new(vec![object("purchase_order", "purchase", "user")]));
    let mut builder = complete();
    builder.facilities = identity;
    builder.facts.insert("purchase_order".into(), port.clone());
    let service = builder.build().unwrap();
    let candidate = ObjectFacts {
        relations: [(
            RelationKey::parse("purchase_source").unwrap(),
            RelatedObjects { expected_ids: ["purchase".into()].into(), objects: vec![] },
        )]
        .into(),
        ..candidate(permission.resource(), "user")
    };
    let mut ex = SnapshotExecutor::default();

    service
        .require(&actor(), &permission, AccessTarget::Candidate(candidate.clone()), &mut ex)
        .await
        .unwrap();

    port.objects.lock().unwrap().clear();
    assert!(matches!(
        service.require(&actor(), &permission, AccessTarget::Candidate(candidate.clone()), &mut ex).await,
        Err(Error::Forbidden(_))
    ));

    let mut fabricated = candidate;
    fabricated.relations.values_mut().next().unwrap().objects.push(object(
        "purchase_order",
        "purchase",
        "user",
    ));
    assert!(matches!(
        service.require(&actor(), &permission, AccessTarget::Candidate(fabricated), &mut ex).await,
        Err(Error::ValidationError(_))
    ));
}

// 退款的不同来源各按自己的合格角色和范围检查，不能互相代补。
#[tokio::test]
async fn independent_refund_origins_keep_their_own_scope_roles() {
    let permission = returns::CUSTOMER_REFUND_CUSTOMER_REFUND_SUBMIT;
    let receipt = receivable::CUSTOMER_RECEIPT_DETAIL;
    let receivable = receivable::ACCOUNT_DETAIL;
    let identity = Arc::new(Identity::default());
    identity
        .scopes
        .lock()
        .unwrap()
        .insert((receipt, sales_order::DETAIL), ResolvedScope { role_clauses: vec![], user_limit: None });
    identity.scopes.lock().unwrap().insert(
        (receivable.clone(), sales_order::DETAIL),
        ResolvedScope {
            role_clauses: vec![ScopeClause { self_owned: true, ..Default::default() }],
            user_limit: None,
        },
    );
    let origin = ObjectFacts {
        relations: [(
            RelationKey::parse("sales_source").unwrap(),
            RelatedObjects {
                expected_ids: ["sale".into()].into(),
                objects: vec![object("sales_order", "sale", "user")],
            },
        )]
        .into(),
        ..object(receivable.resource(), "account", "user")
    };
    let refund = ObjectFacts {
        relations: [
            (RelationKey::parse("receipt_source").unwrap(), RelatedObjects::default()),
            (
                RelationKey::parse("receivable_source").unwrap(),
                RelatedObjects { expected_ids: ["account".into()].into(), objects: vec![origin] },
            ),
        ]
        .into(),
        ..object(permission.resource(), "refund", "user")
    };
    let mut builder = complete();
    builder.facilities = identity.clone();
    builder.facts.insert(permission.resource().into(), Arc::new(Facts::new(vec![refund])));
    let service = builder.build().unwrap();
    let mut ex = SnapshotExecutor::default();
    service.require(&actor(), &permission, AccessTarget::record("refund").unwrap(), &mut ex).await.unwrap();

    identity.state.lock().unwrap().grants = vec![("entry".into(), vec![permission.clone()])];
    assert!(matches!(
        service.require(&actor(), &permission, AccessTarget::record("refund").unwrap(), &mut ex).await,
        Err(Error::Forbidden(_))
    ));
}

// 查询时不得换掉启动时绑定的来源映射。
#[tokio::test]
async fn startup_source_bindings_cannot_be_replaced_at_query_time() {
    let permission = receivable::ACCOUNT_LIST;
    let map = mapping(&permission);
    let mut builder = complete();
    builder.mappings.insert(permission.clone(), map.clone());
    let service = builder.build().unwrap();
    map.replace_source.store(true, Ordering::SeqCst);
    assert!(matches!(
        service.query(&actor(), &permission, &mut ReadExecutor::default()).await,
        Err(Error::ValidationError(_))
    ));
}

// 纯操作只判断功能权限，不接受拿记录目标替代无对象操作。
#[tokio::test]
async fn operation_only_permissions_use_rbac_and_cannot_be_replaced_with_record_targets() {
    let permission = integration_ops::ERROR_TASK_CREATE;
    let identity = Arc::new(Identity::default());
    let service = service(identity.clone(), Arc::new(Facts::default()));
    let mut ex = SnapshotExecutor::default();
    service.require(&actor(), &permission, AccessTarget::Operation, &mut ex).await.unwrap();
    assert!(matches!(
        service.require(&actor(), &permission, AccessTarget::record("record").unwrap(), &mut ex).await,
        Err(Error::ValidationError(_))
    ));
    identity.state.lock().unwrap().grants.clear();
    assert!(matches!(
        service.require(&actor(), &permission, AccessTarget::Operation, &mut ex).await,
        Err(Error::Forbidden(_))
    ));
}

// 构造付款任务引用的采购订单来源，用于检查组合资格。
fn payment_sources() -> std::collections::BTreeMap<RelationKey, RelatedObjects> {
    [(
        RelationKey::parse("purchase_source").unwrap(),
        RelatedObjects {
            expected_ids: ["purchase".into()].into(),
            objects: vec![object("purchase_order", "purchase", "user")],
        },
    )]
    .into()
}

// 付款任务和列表都要求同一角色持有动作与详情权限；资格不足时不读取任务。
#[tokio::test]
async fn payment_requires_detail_on_the_same_role_for_tasks_and_queries() {
    let permission = purchase_payment_request::CREDIT_LIST;
    let detail = purchase_payment_request::DETAIL;
    let identity = Arc::new(Identity::default());
    let task = ObjectFacts {
        relations: payment_sources(),
        assignee_id: FactValue::Known(Some("user".into())),
        ..object(permission.resource(), "task", "user")
    };
    let port = Arc::new(Facts::new(vec![task]));
    let mut builder = complete();
    builder.facilities = identity.clone();
    builder.facts.insert(rules::task_facts_resource(&permission).into(), port.clone());
    let service = builder.build().unwrap();
    let target = || AccessTarget::Task { id: "task".into(), version: "v1".into() };
    let mut ex = SnapshotExecutor::default();
    for (grants, allowed) in [
        (vec![("cashier".into(), vec![permission.clone()])], false),
        (vec![("cashier".into(), vec![permission.clone()]), ("reader".into(), vec![detail.clone()])], false),
        (vec![("cashier".into(), vec![permission.clone(), detail])], true),
    ] {
        identity.state.lock().unwrap().grants = grants;
        let before = port.calls.load(Ordering::SeqCst);
        let result = service.require(&actor(), &permission, target(), &mut ex).await;
        let query = service.query(&actor(), &permission, &mut ex).await;
        if allowed {
            result.unwrap();
            service.validate(&query.unwrap(), &mut ex).await.unwrap();
        } else {
            assert!(matches!(result, Err(Error::Forbidden(_))));
            assert!(matches!(query, Err(Error::Forbidden(_))));
            assert_eq!(port.calls.load(Ordering::SeqCst), before);
        }
    }
    port.objects.lock().unwrap()[0].assignee_id = FactValue::Known(Some("other".into()));
    assert!(matches!(
        service.require(&actor(), &permission, target(), &mut ex).await,
        Err(Error::Forbidden(_))
    ));
}

// 固定已有读取权限的查询绑定，防止目录调整时意外丢失。
#[test]
fn existing_read_permissions_keep_their_query_bindings() {
    for permission in super::rules::queries::permissions() {
        empty().mapping(permission, mapping(permission)).unwrap();
    }
}

// 存在性查询只交付行标识，不能在映射中暴露其他载荷字段。
#[tokio::test]
async fn existence_bindings_reject_payload_and_only_deliver_the_row_identity() {
    let permission = receivable::CUSTOMER_RECEIPT_LIST;
    for field in ["total_amount", "amount", "payload", "source.customer_name"] {
        let mut map = mapping(&permission);
        Arc::get_mut(&mut map).unwrap().model.output =
            QueryOutput::Existence { fields: ["id".into(), field.into()].into() };
        assert!(matches!(empty().mapping(&permission, map.clone()), Err(Error::ValidationError(_))));
        let mut builder = complete();
        builder.mappings.insert(permission.clone(), map);
        assert!(matches!(builder.build(), Err(Error::ValidationError(_))));
    }

    let identity = Arc::new(Identity::default());
    identity.state.lock().unwrap().grants = vec![("viewer".into(), vec![permission.clone()])];
    let mut map = mapping(&permission);
    Arc::get_mut(&mut map).unwrap().model.output = QueryOutput::Existence { fields: ["id".into()].into() };
    let mut builder = complete();
    builder.facilities = identity;
    builder.mappings.remove(&permission);
    let service = builder.mapping(&permission, map).unwrap().build().unwrap();
    let mut ex = ReadExecutor::default();
    let access = service.query(&actor(), &permission, &mut ex).await.unwrap();
    let rows = receipt_projection(&access);
    service.validate(&access, &mut ex).await.unwrap();
    assert_eq!(rows, json!([{"id": "receipt"}]));
}

// 固定收款列表在份额与整账资格下的来源计划和游标指纹。
#[tokio::test]
async fn receipt_query_keeps_plan_and_cursor_fingerprints() {
    use erp_permissions::policy_permission::FINANCE_LEDGER_READ;

    use crate::entity::fingerprint::Fingerprint;
    use crate::repository::fingerprint::BsonFingerprint;

    let permission = receivable::CUSTOMER_RECEIPT_LIST;
    let identity = Arc::new(Identity::default());
    let mut map = mapping(&permission);
    Arc::get_mut(&mut map).unwrap().model.output = QueryOutput::Existence { fields: ["id".into()].into() };
    let mut builder = complete();
    builder.facilities = identity.clone();
    builder.mappings.insert(permission.clone(), map);
    let service = builder.build().unwrap();
    let sales_source = RelationKey::parse("sales_source").unwrap();
    let mut fingerprints = Vec::new();

    for grants in [vec![permission.clone()], vec![permission.clone(), FINANCE_LEDGER_READ]] {
        identity.state.lock().unwrap().grants = vec![("finance".into(), grants)];
        let mut executor = ReadExecutor::default();
        let access = service.query(&actor(), &permission, &mut executor).await.unwrap();
        let mut plans = Fingerprint::new("receipt-source-plans-test");
        plans.sequence(access.whole_stages().unwrap(), Fingerprint::document);
        plans.sequence(access.source_stages(&sales_source).unwrap(), Fingerprint::document);
        fingerprints.push((access.version(), plans.finish()));
        service.validate(&access, &mut executor).await.unwrap();
    }

    assert_eq!(
        fingerprints,
        [
            ("439a3e5d8e24eedf4ae4461f308c9001".into(), "066677e8522a9e6bcf3dd84e756c147e".into()),
            ("6c42ba6db4e51b53c20bb8c4c54cca3b".into(), "44323f26c0035dc090357ccc101509c1".into()),
        ]
    );
}

// 固定整条收款查询的根计划、三类来源计划及游标指纹。
#[tokio::test]
async fn full_receipt_query_keeps_root_source_and_cursor_fingerprints() {
    use erp_permissions::policy_permission::FINANCE_LEDGER_READ;

    use crate::entity::fingerprint::Fingerprint;
    use crate::repository::fingerprint::BsonFingerprint;

    let permission = receivable::CUSTOMER_RECEIPT_LIST;
    let identity = Arc::new(Identity::default());
    identity.state.lock().unwrap().grants =
        vec![("finance".into(), vec![permission.clone(), FINANCE_LEDGER_READ])];
    let mut builder = complete();
    builder.facilities = identity;
    let service = builder.build().unwrap();
    let mut executor = ReadExecutor::default();
    let access = service.query(&actor(), &permission, &mut executor).await.unwrap();
    assert_eq!(access.model().output, QueryOutput::Object);
    assert!(std::ptr::eq(access.stages(), access.whole_stages().unwrap()));
    assert!(access.has_whole_qualification());
    let mut sources = Fingerprint::new("full-receipt-source-plans-test");
    for key in ["sales_source", "purchase_source", "settlement_source"] {
        sources.sequence(
            access.source_stages(&RelationKey::parse(key).unwrap()).unwrap(),
            Fingerprint::document,
        );
    }
    service.validate(&access, &mut executor).await.unwrap();
    assert_eq!(
        (access.version().as_str(), sources.finish().as_str()),
        ("b26f237dfd02ee15dbec15a16a55a9c4", "d6fed4f8ff65a20744df24dd30066b05")
    );
}

// 共用写入权限的任务查询可以列出合格任务，但不放宽精确目标或快照写入要求。
#[tokio::test]
async fn shared_write_permissions_support_queries_without_relaxing_task_or_transaction_checks() {
    for permission in [inquiry::QUERY_PROCUREMENT_INTERNAL_QUOTES, purchase_payment_request::CREDIT_LIST] {
        let mut builder = complete();
        builder.mappings.remove(&permission);
        let samples = [
            ("open", "user", false, "other"),
            ("completed", "user", true, "other"),
            ("participated", "user", true, "user"),
            ("unassigned", "other", false, "user"),
        ];
        let objects = samples
            .iter()
            .map(|(id, assignee, completed, participant)| ObjectFacts {
                assignee_id: FactValue::Known(Some((*assignee).into())),
                completed: FactValue::Known(*completed),
                participants: FactValue::Known([(*participant).into()].into()),
                relations: if permission == purchase_payment_request::CREDIT_LIST {
                    payment_sources()
                } else {
                    Default::default()
                },
                ..object(permission.resource(), id, "user")
            })
            .collect();
        builder.facts.insert(rules::task_facts_resource(&permission).into(), Arc::new(Facts::new(objects)));
        let service = builder.mapping(&permission, mapping(&permission)).unwrap().build().unwrap();

        let mut ex = ReadExecutor::default();
        let access = service.query(&actor(), &permission, &mut ex).await.unwrap();
        let members = task_members(access.stages(), &samples);
        let expected = if permission == inquiry::QUERY_PROCUREMENT_INTERNAL_QUOTES {
            vec!["open", "participated"]
        } else {
            vec!["open", "completed", "participated"]
        };
        assert_eq!(members, expected);
        if permission == purchase_payment_request::CREDIT_LIST {
            assert_eq!(
                task_members(access.whole_stages().unwrap(), &samples),
                ["open", "completed", "participated", "unassigned"]
            );
        }
        service.validate(&access, &mut ex).await.unwrap();

        for (id, ..) in samples {
            let target = || AccessTarget::Task { id: id.into(), version: "v1".into() };
            let result =
                service.require(&actor(), &permission, target(), &mut SnapshotExecutor::default()).await;
            if expected.contains(&id) {
                result.unwrap();
            } else {
                assert!(matches!(result, Err(Error::Forbidden(_))));
            }
            assert!(matches!(
                service.require(&actor(), &permission, target(), &mut ex).await,
                Err(Error::ValidationError(_))
            ));
        }
    }
}

// 内部报价任务的查询映射必须同时具备完成状态和参与人事实。
#[test]
fn internal_quote_query_requires_completion_and_participation_mapping() {
    let permission = inquiry::QUERY_PROCUREMENT_INTERNAL_QUOTES;
    for fact in [FactKind::Completed, FactKind::Participants] {
        let mut map = mapping(&permission);
        Arc::get_mut(&mut map).unwrap().model.facts.remove(&fact);
        let mut builder = complete();
        builder.mappings.remove(&permission);
        assert!(matches!(builder.mapping(&permission, map).unwrap().build(), Err(Error::ValidationError(_))));
    }
}

// 执行内部报价任务检查时，完成状态或参与人事实缺失即拒绝。
#[tokio::test]
async fn internal_quote_task_rejects_missing_completion_or_participation_facts() {
    let permission = inquiry::QUERY_PROCUREMENT_INTERNAL_QUOTES;
    let task = ObjectFacts {
        assignee_id: FactValue::Known(Some("user".into())),
        completed: FactValue::Known(false),
        participants: FactValue::Known(BTreeSet::new()),
        ..object(permission.resource(), "task", "user")
    };
    let mut no_completion = task.clone();
    no_completion.completed = FactValue::Missing;
    let mut no_participation = task;
    no_participation.participants = FactValue::Missing;
    for task in [no_completion, no_participation] {
        let mut builder = complete();
        builder
            .facts
            .insert(rules::task_facts_resource(&permission).into(), Arc::new(Facts::new(vec![task])));
        let service = builder.build().unwrap();
        let result = service
            .require(
                &actor(),
                &permission,
                AccessTarget::Task { id: "task".into(), version: "v1".into() },
                &mut SnapshotExecutor::default(),
            )
            .await;
        assert!(matches!(result, Err(Error::ValidationError(_))));
    }
}

// 在首次读取和非事务复验之间切换事实，用于模拟持久化数据变化。
struct ReloadFacts {
    // 初次授权所见的付款申请及其来源。
    first: Vec<ObjectFacts>,
    // 复验时读取的付款申请及其来源。
    second: Vec<ObjectFacts>,
    // 区分首次读取和后续读取的调用次数。
    calls: AtomicUsize,
}

// 用顺序可控的事实端口模拟非事务重复读取。
#[async_trait]
impl AccessFactsPort for ReloadFacts {
    // 首次调用返回初始事实，后续调用返回复验事实。
    async fn load(
        &self,
        _: &AccessContext,
        _: &ResourceTargets,
        _: &mut dyn Executor,
    ) -> Result<Vec<ObjectFacts>> {
        Ok(if self.calls.fetch_add(1, Ordering::SeqCst) == 0 {
            self.first.clone()
        } else {
            self.second.clone()
        })
    }
}

// 构造两条各自引用同一组采购单的付款申请，作为复验基线。
fn payment_requests() -> Vec<ObjectFacts> {
    ["request_a", "request_b"]
        .into_iter()
        .map(|id| ObjectFacts {
            relations: [(
                RelationKey::parse("purchase_source").unwrap(),
                RelatedObjects {
                    expected_ids: ["purchase_a".into(), "purchase_b".into()].into(),
                    objects: vec![
                        object("purchase_order", "purchase_a", "user"),
                        object("purchase_order", "purchase_b", "user"),
                    ],
                },
            )]
            .into(),
            ..object("purchase_payment_request", id, "user")
        })
        .collect()
}

// 用指定的第二批事实走一遍非事务批量详情授权和复验。
async fn reloaded_requests(second: Vec<ObjectFacts>) -> Result<()> {
    let port = Arc::new(ReloadFacts { first: payment_requests(), second, calls: AtomicUsize::new(0) });
    let mut builder = complete();
    builder.facts.insert("purchase_payment_request".into(), port);
    let service = builder.build().unwrap();
    service
        .require(
            &actor(),
            &purchase_payment_request::DETAIL,
            AccessTarget::Records(ResourceTargets::new(&["request_a", "request_b"]).unwrap()),
            &mut ReadExecutor::default(),
        )
        .await
}

// 非事务复验按集合比较，根记录或来源的返回顺序变化不影响结果。
#[tokio::test]
async fn nontransaction_revalidation_ignores_root_and_source_order() {
    for (roots, sources) in [(true, false), (false, true), (true, true)] {
        let mut second = payment_requests();
        if roots {
            second.reverse();
        }
        if sources {
            for root in &mut second {
                for relation in root.relations.values_mut() {
                    relation.objects.reverse();
                }
            }
        }
        reloaded_requests(second).await.unwrap();
    }
}

// 非事务复验拒绝版本、归属或引用集变化，也拒绝重复的根记录或来源。
#[tokio::test]
async fn nontransaction_revalidation_still_rejects_changed_facts_and_invalid_sets() {
    let mut version = payment_requests();
    version[0].version = "v2".into();
    let mut source_version = payment_requests();
    source_version[0].relations.values_mut().next().unwrap().objects[0].version = "v2".into();
    let mut owner = payment_requests();
    owner[0].relations.values_mut().next().unwrap().objects[0].owner_id =
        FactValue::Known(Some("other".into()));
    let mut references = payment_requests();
    let relation = references[0].relations.values_mut().next().unwrap();
    relation.expected_ids.remove("purchase_b");
    relation.objects.pop();

    for changed in [version, source_version, owner, references] {
        assert!(matches!(reloaded_requests(changed).await, Err(Error::ConflictError(_))));
    }

    let mut duplicate = payment_requests();
    let relation = duplicate[0].relations.values_mut().next().unwrap();
    relation.objects.push(relation.objects[0].clone());
    assert!(matches!(reloaded_requests(duplicate).await, Err(Error::ValidationError(_))));
    let mut duplicate = payment_requests();
    duplicate[1] = duplicate[0].clone();
    assert!(matches!(reloaded_requests(duplicate).await, Err(Error::NotFound(_))));
}
