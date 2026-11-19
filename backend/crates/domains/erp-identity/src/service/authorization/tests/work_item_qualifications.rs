//! 接收者、形成前执行资格和展示共用固定任务组合，测试执行真实公共入口。
use std::sync::Arc;

use erp_core::common::time::Instant;
use erp_permissions::{
    Permission, approval_instance, fulfillment, integration_ops, internal, inventory, purchase_order,
    purchase_payment_request, sales_inquiry, sales_order, supplier_settlement, work_item,
};
use persistence_core::testing::SnapshotExecutor;
use persistence_core::{Executor, ReadExecutor};

use super::fixtures::{Facts, Identity, actor, complete, object};
use crate::entity::access_control::ScopeClause;
use crate::entity::authorization::{AccessTarget, FactValue, ObjectFacts, RelatedObjects, RelationKey};
use crate::entity::resource_access::ResourceKind;
use crate::{AuthorizationService, Error};

// 只替换身份和工作项事实，形成、接收与展示仍使用正式授权规则。
fn service(identity: Arc<Identity>, facts: Arc<Facts>) -> AuthorizationService {
    let mut builder = complete();
    builder.facilities = identity;
    builder.facts.insert("work_item".into(), facts);
    builder.build().unwrap()
}

// 为接收和形成测试设置直接角色权限，便于区分同角色组合与独立来源资格。
fn grant(identity: &Identity, roles: &[(&str, &[Permission])]) {
    identity.state.lock().unwrap().grants =
        roles.iter().map(|(role, permissions)| ((*role).into(), permissions.to_vec())).collect();
}

// 构造形成前的候选，仅写入服务端确认的任务种类和来源种类。
fn candidate(kind: &str, source: &str) -> ObjectFacts {
    ObjectFacts {
        resource: "work_item".into(),
        subject_type: FactValue::Known(kind.into()),
        subject_source_resource: FactValue::Known(source.into()),
        ..Default::default()
    }
}

// 构造负责人为他人的采购付款任务，使当前账号通过真实来源证明接收资格。
fn task() -> ObjectFacts {
    ObjectFacts {
        id: "task".into(),
        version: "v1".into(),
        owner_id: FactValue::Known(Some("other".into())),
        org_unit_id: FactValue::Known(None),
        assignee_id: FactValue::Known(Some("other".into())),
        relations: [(
            RelationKey::parse("order_source").unwrap(),
            RelatedObjects {
                expected_ids: ["purchase".into()].into(),
                objects: vec![object("purchase_order", "purchase", "user")],
            },
        )]
        .into(),
        ..candidate("PURCHASE_PAYMENT_ARRANGEMENT", "purchase_order")
    }
}

// 列出客户验收形成所需的五项执行权限，不加入销售单详情权限。
fn acceptance() -> [Permission; 5] {
    [
        fulfillment::CUSTOMER_ACCEPTANCE_CUSTOMER_ACCEPTANCE_LIST,
        fulfillment::ACCEPTANCE_EVIDENCE_CUSTOMER_ACCEPTANCE_EVIDENCE,
        fulfillment::CUSTOMER_ACCEPTANCE_CUSTOMER_ACCEPTANCE_CREATE,
        fulfillment::CUSTOMER_ACCEPTANCE_CUSTOMER_ACCEPTANCE_COMMIT,
        sales_order::EVIDENCE_SALES_ORDER_EVIDENCE_DOWNLOAD,
    ]
}

// 形成结算任务只检查审核决定资格，不提前要求读取权限。
#[tokio::test]
async fn settlement_formation_requires_review_decision_without_adding_read_permissions() {
    let identity = Arc::new(Identity::default());
    let service = service(identity.clone(), Arc::new(Facts::default()));
    let target =
        AccessTarget::Candidate(candidate("SUPPLIER_SETTLEMENT_REVIEW", "supplier_settlement_statement"));
    grant(&identity, &[("reviewer", &[supplier_settlement::STATEMENT_REVIEW_DECIDE])]);
    service
        .require(
            &actor(),
            &internal::WORK_ITEM_EXECUTION_QUALIFICATION,
            target.clone(),
            &mut SnapshotExecutor::default(),
        )
        .await
        .unwrap();

    grant(&identity, &[("reader", &[supplier_settlement::STATEMENT_DETAIL, work_item::DETAIL])]);
    assert!(matches!(
        service
            .require(
                &actor(),
                &internal::WORK_ITEM_EXECUTION_QUALIFICATION,
                target,
                &mut SnapshotExecutor::default()
            )
            .await,
        Err(Error::Forbidden(_))
    ));
}

// 结算任务接收资格针对精确任务判断，读取和审核权限必须由同一角色持有。
#[tokio::test]
async fn settlement_recipient_requires_same_role_read_and_review_on_the_exact_task() {
    let identity = Arc::new(Identity::default());
    let settlement = ObjectFacts {
        id: "settlement-task".into(),
        version: "3".into(),
        owner_id: FactValue::Known(Some("other".into())),
        org_unit_id: FactValue::Known(None),
        assignee_id: FactValue::Known(Some("other".into())),
        ..candidate("SUPPLIER_SETTLEMENT_REVIEW", "supplier_settlement_statement")
    };
    let service = service(identity.clone(), Arc::new(Facts::new(vec![settlement])));
    let required = [supplier_settlement::STATEMENT_DETAIL, supplier_settlement::STATEMENT_REVIEW_DECIDE];
    let target = AccessTarget::Task { id: "settlement-task".into(), version: "3".into() };
    grant(&identity, &[("reviewer", &required)]);
    service
        .require(&actor(), &internal::WORK_ITEM_RECIPIENT, target.clone(), &mut SnapshotExecutor::default())
        .await
        .unwrap();

    for grants in [
        vec![("reader", &required[..1])],
        vec![("reviewer", &required[1..])],
        vec![("reader", &required[..1]), ("reviewer", &required[1..])],
    ] {
        grant(&identity, &grants);
        assert!(matches!(
            service
                .require(
                    &actor(),
                    &internal::WORK_ITEM_RECIPIENT,
                    target.clone(),
                    &mut SnapshotExecutor::default()
                )
                .await,
            Err(Error::Forbidden(_))
        ));
    }

    grant(&identity, &[("reviewer", &required)]);
    assert!(
        service
            .require(
                &actor(),
                &internal::WORK_ITEM_RECIPIENT,
                AccessTarget::Task { id: "settlement-task".into(), version: "2".into() },
                &mut SnapshotExecutor::default()
            )
            .await
            .is_err()
    );
}

// 形成询价任务所需的执行动作必须由同一角色完整持有。
#[tokio::test]
async fn inquiry_formation_requires_complete_same_role_execution_permissions() {
    let identity = Arc::new(Identity::default());
    let service = service(identity.clone(), Arc::new(Facts::default()));
    let required = [
        sales_inquiry::QUERY_PROCUREMENT_DETAIL,
        sales_inquiry::COMMANDS_PROCUREMENT_SAVE_FEEDBACK,
        sales_inquiry::COMMANDS_PROCUREMENT_PUBLISH,
    ];
    let target = AccessTarget::Candidate(candidate("SALES_INQUIRY_PROCESSING", "sales_inquiry"));
    grant(&identity, &[("buyer", &required)]);
    service
        .require(
            &actor(),
            &internal::WORK_ITEM_EXECUTION_QUALIFICATION,
            target.clone(),
            &mut SnapshotExecutor::default(),
        )
        .await
        .unwrap();

    for missing in 0..required.len() {
        let partial = required
            .iter()
            .enumerate()
            .filter(|(index, _)| *index != missing)
            .map(|(_, permission)| permission.clone())
            .collect::<Vec<_>>();
        grant(&identity, &[("partial", &partial), ("other", &required[missing..missing + 1])]);
        assert!(matches!(
            service
                .require(
                    &actor(),
                    &internal::WORK_ITEM_EXECUTION_QUALIFICATION,
                    target.clone(),
                    &mut SnapshotExecutor::default()
                )
                .await,
            Err(Error::Forbidden(_))
        ));
    }
}

// 形成前只检查原执行权限子集，但不允许跨角色拼接。
#[tokio::test]
async fn formation_checks_execution_only_and_rejects_split_roles() {
    let identity = Arc::new(Identity::default());
    let service = service(identity.clone(), Arc::new(Facts::default()));
    let required = acceptance();
    grant(&identity, &[("executor", &required)]);
    let target = AccessTarget::Candidate(candidate("CUSTOMER_ACCEPTANCE_REGISTRATION", "sales_order"));
    service
        .require(
            &actor(),
            &internal::WORK_ITEM_EXECUTION_QUALIFICATION,
            target.clone(),
            &mut SnapshotExecutor::default(),
        )
        .await
        .unwrap();

    for missing in 0..required.len() {
        let partial = required
            .iter()
            .enumerate()
            .filter(|(index, _)| *index != missing)
            .map(|(_, permission)| permission.clone())
            .collect::<Vec<_>>();
        grant(&identity, &[("partial", &partial), ("other", &required[missing..missing + 1])]);
        assert!(matches!(
            service
                .require(
                    &actor(),
                    &internal::WORK_ITEM_EXECUTION_QUALIFICATION,
                    target.clone(),
                    &mut SnapshotExecutor::default()
                )
                .await,
            Err(Error::Forbidden(_))
        ));
    }
}

// 形成返工任务仍要有工作项详情权限，不额外要求来源读取。
#[tokio::test]
async fn formation_retains_rework_detail_requirement_without_adding_source_read() {
    let identity = Arc::new(Identity::default());
    let service = service(identity.clone(), Arc::new(Facts::default()));
    let target = AccessTarget::Candidate(candidate("APPROVAL_REWORK", "sales_order"));
    grant(&identity, &[("source-reader", &[sales_order::DETAIL])]);
    assert!(matches!(
        service
            .require(
                &actor(),
                &internal::WORK_ITEM_EXECUTION_QUALIFICATION,
                target.clone(),
                &mut SnapshotExecutor::default()
            )
            .await,
        Err(Error::Forbidden(_))
    ));
    grant(&identity, &[("task-reader", &[work_item::DETAIL])]);
    service
        .require(
            &actor(),
            &internal::WORK_ITEM_EXECUTION_QUALIFICATION,
            target,
            &mut SnapshotExecutor::default(),
        )
        .await
        .unwrap();
}

// 形成预检拒绝非法候选、未知种类、到期、非快照或账号停用。
#[tokio::test]
async fn formation_rejects_untrusted_shape_unknown_kind_expiry_and_non_snapshot() {
    let identity = Arc::new(Identity::default());
    let service = service(identity.clone(), Arc::new(Facts::default()));

    for invalid in [
        candidate("UNKNOWN", "sales_order"),
        candidate("CUSTOMER_ACCEPTANCE_REGISTRATION", "wrong"),
        ObjectFacts {
            id: "persisted".into(),
            ..candidate("CUSTOMER_ACCEPTANCE_REGISTRATION", "sales_order")
        },
        ObjectFacts { version: "v1".into(), ..candidate("CUSTOMER_ACCEPTANCE_REGISTRATION", "sales_order") },
    ] {
        assert!(matches!(
            service
                .require(
                    &actor(),
                    &internal::WORK_ITEM_EXECUTION_QUALIFICATION,
                    AccessTarget::Candidate(invalid),
                    &mut SnapshotExecutor::default()
                )
                .await,
            Err(Error::ValidationError(_))
        ));
    }

    let valid = candidate("CUSTOMER_ACCEPTANCE_REGISTRATION", "sales_order");
    assert!(matches!(
        service
            .require(
                &actor(),
                &internal::WORK_ITEM_EXECUTION_QUALIFICATION,
                AccessTarget::Candidate(valid.clone()),
                &mut ReadExecutor::default()
            )
            .await,
        Err(Error::ValidationError(_))
    ));

    let expired = ObjectFacts { valid_until: Some(Instant::from_unix_secs(1)), ..valid.clone() };
    assert!(matches!(
        service
            .require(
                &actor(),
                &internal::WORK_ITEM_EXECUTION_QUALIFICATION,
                AccessTarget::Candidate(expired),
                &mut SnapshotExecutor::default()
            )
            .await,
        Err(Error::ConflictError(_))
    ));

    identity.state.lock().unwrap().active = false;
    assert!(matches!(
        service
            .require(
                &actor(),
                &internal::WORK_ITEM_EXECUTION_QUALIFICATION,
                AccessTarget::Candidate(valid),
                &mut SnapshotExecutor::default()
            )
            .await,
        Err(Error::Forbidden(_))
    ));
}

// 接收资格以精确任务和真实来源为依据，不要求候选接收人已是当前所有者。
#[tokio::test]
async fn recipient_loads_exact_task_without_requiring_current_ownership() {
    let identity = Arc::new(Identity::default());
    let facts = Arc::new(Facts::new(vec![task()]));
    let service = service(identity.clone(), facts.clone());
    let required =
        [purchase_order::DETAIL, purchase_payment_request::CREATE, purchase_payment_request::SUBMIT];
    grant(&identity, &[("executor", &required), ("source-reader", &[work_item::DETAIL])]);
    let target = AccessTarget::Task { id: "task".into(), version: "v1".into() };
    service
        .require(&actor(), &internal::WORK_ITEM_RECIPIENT, target.clone(), &mut SnapshotExecutor::default())
        .await
        .unwrap();

    for (id, version) in [("task", "old"), ("missing", "v1")] {
        assert!(
            service
                .require(
                    &actor(),
                    &internal::WORK_ITEM_RECIPIENT,
                    AccessTarget::Task { id: id.into(), version: version.into() },
                    &mut SnapshotExecutor::default()
                )
                .await
                .is_err()
        );
    }

    grant(
        &identity,
        &[("partial", &required[..2]), ("other", &required[2..]), ("source-reader", &[work_item::DETAIL])],
    );
    assert!(matches!(
        service
            .require(
                &actor(),
                &internal::WORK_ITEM_RECIPIENT,
                target.clone(),
                &mut SnapshotExecutor::default()
            )
            .await,
        Err(Error::Forbidden(_))
    ));

    facts.objects.lock().unwrap()[0].subject_type = FactValue::Known("UNKNOWN".into());
    assert!(matches!(
        service
            .require(&actor(), &internal::WORK_ITEM_RECIPIENT, target, &mut SnapshotExecutor::default())
            .await,
        Err(Error::ValidationError(_))
    ));

    assert!(matches!(
        service
            .require(
                &actor(),
                &internal::WORK_ITEM_RECIPIENT,
                AccessTarget::Candidate(task()),
                &mut SnapshotExecutor::default()
            )
            .await,
        Err(Error::ValidationError(_))
    ));
}

// 任务的独立来源资格可由不同角色提供，但缺少真实来源时不得接收。
#[tokio::test]
async fn recipient_preserves_independent_source_roles_and_rejects_missing_real_source() {
    let identity = Arc::new(Identity::default());
    let facts = Arc::new(Facts::new(vec![task()]));
    let service = service(identity.clone(), facts.clone());
    let required =
        [purchase_order::DETAIL, purchase_payment_request::CREATE, purchase_payment_request::SUBMIT];
    grant(&identity, &[("executor", &required), ("source", &[work_item::DETAIL])]);
    identity.role_scopes.lock().unwrap().extend([
        (("executor".into(), purchase_order::DETAIL), ScopeClause::default()),
        (("source".into(), purchase_order::DETAIL), ScopeClause { self_owned: true, ..Default::default() }),
    ]);
    let target = AccessTarget::Task { id: "task".into(), version: "v1".into() };
    service
        .require(&actor(), &internal::WORK_ITEM_RECIPIENT, target.clone(), &mut SnapshotExecutor::default())
        .await
        .unwrap();

    grant(&identity, &[("executor", &required)]);
    assert!(matches!(
        service
            .require(
                &actor(),
                &internal::WORK_ITEM_RECIPIENT,
                target.clone(),
                &mut SnapshotExecutor::default()
            )
            .await,
        Err(Error::Forbidden(_))
    ));

    facts.objects.lock().unwrap()[0].relations.clear();
    assert!(matches!(
        service
            .require(&actor(), &internal::WORK_ITEM_RECIPIENT, target, &mut SnapshotExecutor::default())
            .await,
        Err(Error::ValidationError(_))
    ));
}

// 履约任务接收沿用工作项读取角色，不附带订单菜单权限。
#[tokio::test]
async fn fulfillment_recipient_preserves_task_read_roles_without_adding_order_menu_permissions() {
    let identity = Arc::new(Identity::default());
    let mut item = task();
    item.subject_type = FactValue::Known("FULFILLMENT_OPERATION".into());
    item.subject_source_resource = FactValue::Known("delivery".into());
    item.relations.values_mut().next().unwrap().objects = vec![object("sales_order", "purchase", "user")];
    let facts = Arc::new(Facts::new(vec![item]));
    let service = service(identity.clone(), facts.clone());
    let full = [
        fulfillment::DELIVERY_DELIVERY_LIST,
        fulfillment::DELIVERY_DELIVERY_DETAIL,
        fulfillment::DELIVERY_DELIVERY_UPDATE,
        fulfillment::DELIVERY_DELIVERY_POST,
    ];
    grant(&identity, &[("executor", &full), ("task-reader", &[work_item::DETAIL])]);
    let target = AccessTarget::Task { id: "task".into(), version: "v1".into() };
    service
        .require(&actor(), &internal::WORK_ITEM_RECIPIENT, target.clone(), &mut SnapshotExecutor::default())
        .await
        .unwrap();

    grant(&identity, &[("executor", &full)]);
    assert!(matches!(
        service
            .require(
                &actor(),
                &internal::WORK_ITEM_RECIPIENT,
                target.clone(),
                &mut SnapshotExecutor::default()
            )
            .await,
        Err(Error::Forbidden(_))
    ));

    grant(&identity, &[("executor", &full), ("task-reader", &[work_item::DETAIL])]);
    {
        let mut roots = facts.objects.lock().unwrap();
        let source = &mut roots[0].relations.values_mut().next().unwrap().objects[0];
        source.owner_id = FactValue::Known(Some("other".into()));
        source.history_participants = FactValue::Known([actor().id().into()].into());
    }
    assert!(matches!(
        service
            .require(
                &actor(),
                &internal::WORK_ITEM_RECIPIENT,
                target.clone(),
                &mut SnapshotExecutor::default()
            )
            .await,
        Err(Error::Forbidden(_))
    ));

    identity.record_grants.lock().unwrap().insert((ResourceKind::SalesOrder, "purchase".into()));
    service
        .require(&actor(), &internal::WORK_ITEM_RECIPIENT, target, &mut SnapshotExecutor::default())
        .await
        .unwrap();
}

// 采购付款任务的接收资格保留历史参与和针对该采购单的显式读取补权。
#[tokio::test]
async fn recipient_purchase_source_retains_history_and_explicit_record_read_supplements() {
    let identity = Arc::new(Identity::default());
    let mut item = task();
    {
        let source = &mut item.relations.values_mut().next().unwrap().objects[0];
        source.owner_id = FactValue::Known(Some("other".into()));
        source.history_participants = FactValue::Known([actor().id().into()].into());
    }
    let facts = Arc::new(Facts::new(vec![item]));
    let service = service(identity.clone(), facts.clone());
    let full = [purchase_order::DETAIL, purchase_payment_request::CREATE, purchase_payment_request::SUBMIT];
    grant(&identity, &[("executor", &full), ("task-reader", &[work_item::DETAIL])]);
    let target = AccessTarget::Task { id: "task".into(), version: "v1".into() };
    service
        .require(&actor(), &internal::WORK_ITEM_RECIPIENT, target.clone(), &mut SnapshotExecutor::default())
        .await
        .unwrap();

    facts.objects.lock().unwrap()[0].relations.values_mut().next().unwrap().objects[0].history_participants =
        FactValue::Known(Default::default());
    assert!(matches!(
        service
            .require(
                &actor(),
                &internal::WORK_ITEM_RECIPIENT,
                target.clone(),
                &mut SnapshotExecutor::default()
            )
            .await,
        Err(Error::Forbidden(_))
    ));

    identity.record_grants.lock().unwrap().insert((ResourceKind::PurchaseOrder, "purchase".into()));
    service
        .require(&actor(), &internal::WORK_ITEM_RECIPIENT, target, &mut SnapshotExecutor::default())
        .await
        .unwrap();
}

// 列表中的任务资格投影与单个接收者检查采用同一套完整任务组合。
#[tokio::test]
async fn query_qualification_and_recipient_use_the_same_complete_task_combinations() {
    let identity = Arc::new(Identity::default());
    let service = service(identity.clone(), Arc::new(Facts::new(vec![task()])));
    let full = [purchase_order::DETAIL, purchase_payment_request::CREATE, purchase_payment_request::SUBMIT];
    for allowed in [true, false] {
        let partial = if allowed { full.as_slice() } else { &full[..2] };
        grant(
            &identity,
            &[
                ("reader", &[work_item::LIST, work_item::DETAIL, work_item::MANAGE]),
                ("executor", partial),
                ("other", &full[2..]),
                ("decision", &[approval_instance::COPIES_HISTORY, approval_instance::SUBMIT_DECISION]),
            ],
        );
        for permission in [work_item::LIST, work_item::DETAIL, work_item::MANAGE] {
            let mut executor = SnapshotExecutor::default();
            let access = service.query(&actor(), &permission, &mut executor).await.unwrap();
            assert_eq!(
                access
                    .execution_qualified("PURCHASE_PAYMENT_ARRANGEMENT", "purchase_order", &mut executor)
                    .unwrap(),
                allowed
            );
            assert!(access.decision_qualified(&mut executor).unwrap());
            service.validate(&access, &mut executor).await.unwrap();

            assert_eq!(
                service
                    .require(
                        &actor(),
                        &internal::WORK_ITEM_RECIPIENT,
                        AccessTarget::Task { id: "task".into(), version: "v1".into() },
                        &mut executor
                    )
                    .await
                    .is_ok(),
                allowed
            );
        }
    }
}

// 资格投影只接受对应查询的有效凭证，拒绝到期或跨阶段复用。
#[tokio::test]
async fn query_qualification_rejects_other_queries_expiry_and_cross_stage_reuse() {
    let service = complete().build().unwrap();
    let mut executor = SnapshotExecutor::default();

    let other = service.query(&actor(), &sales_inquiry::QUERY_SALES_LIST, &mut executor).await.unwrap();
    assert!(matches!(other.decision_qualified(&mut executor), Err(Error::ValidationError(_))));

    let mut access = service.query(&actor(), &work_item::LIST, &mut executor).await.unwrap();
    assert!(
        access.execution_qualified("PURCHASE_PAYMENT_ARRANGEMENT", "purchase_order", &mut executor).unwrap()
    );
    access.validity.valid_until = Some(Instant::from_unix_secs(1));
    assert!(matches!(access.decision_qualified(&mut executor), Err(Error::ConflictError(_))));

    let access = service.query(&actor(), &work_item::LIST, &mut executor).await.unwrap();
    executor.next_read();
    assert!(matches!(access.decision_qualified(&mut executor), Err(Error::ValidationError(_))));

    let access = service.query(&actor(), &work_item::LIST, &mut executor).await.unwrap();
    executor.restart();
    assert!(matches!(access.decision_qualified(&mut executor), Err(Error::ValidationError(_))));
}

// 形成结算任务的资格不能替代按钮展示或实际办理所需的完整资格。
#[tokio::test]
async fn settlement_formation_does_not_prove_display_or_task_execution_qualification() {
    let identity = Arc::new(Identity::default());
    let kind = "SUPPLIER_SETTLEMENT_REVIEW";
    let source = "supplier_settlement_statement";
    let item = ObjectFacts {
        subject_type: FactValue::Known(kind.into()),
        subject_source_resource: FactValue::Known(source.into()),
        assignee_id: FactValue::Known(None),
        completed: FactValue::Known(true),
        ..object("work_item", "task", "user")
    };
    let service = service(identity.clone(), Arc::new(Facts::new(vec![item])));
    let required = [supplier_settlement::STATEMENT_REVIEW_DECIDE, supplier_settlement::STATEMENT_DETAIL];
    let task_reader = [work_item::LIST, work_item::DETAIL];
    let target = AccessTarget::Task { id: "task".into(), version: "v1".into() };

    for (reviewer, other_reader, allowed) in [
        (&required[..1], &required[..0], false),
        (&required[..1], &required[1..], false),
        (&required[..], &required[..0], true),
    ] {
        grant(&identity, &[("task-reader", &task_reader), ("reviewer", reviewer), ("other", other_reader)]);
        let mut executor = SnapshotExecutor::default();
        service
            .require(
                &actor(),
                &internal::WORK_ITEM_EXECUTION_QUALIFICATION,
                AccessTarget::Candidate(candidate(kind, source)),
                &mut executor,
            )
            .await
            .unwrap();

        let access = service.query(&actor(), &work_item::LIST, &mut executor).await.unwrap();
        assert_eq!(access.execution_qualified(kind, source, &mut executor).unwrap(), allowed);
        service.validate(&access, &mut executor).await.unwrap();

        for permission in [internal::WORK_ITEM_RECIPIENT, internal::WORK_ITEM_EXECUTE] {
            let result = service.require(&actor(), &permission, target.clone(), &mut executor).await;
            if allowed {
                result.unwrap();
            } else {
                assert!(matches!(result, Err(Error::Forbidden(_))), "{result:?}");
            }
        }
    }
}

// 执行权限子集显式为空也不能跳过展示所需的来源读取。
#[tokio::test]
async fn explicit_empty_execution_subset_still_requires_source_read_for_display() {
    let identity = Arc::new(Identity::default());
    let service = service(identity.clone(), Arc::new(Facts::default()));
    let mut executor = SnapshotExecutor::default();

    let source_read = [integration_ops::ERROR_TASK_DETAIL];
    for (source_reader, allowed) in [(&source_read[..0], false), (&source_read[..], true)] {
        grant(&identity, &[("task-reader", &[work_item::LIST]), ("source-reader", source_reader)]);
        service
            .require(
                &actor(),
                &internal::WORK_ITEM_EXECUTION_QUALIFICATION,
                AccessTarget::Candidate(candidate("BUSINESS_EXCEPTION", "integration_error_task")),
                &mut executor,
            )
            .await
            .unwrap();

        let access = service.query(&actor(), &work_item::LIST, &mut executor).await.unwrap();
        assert_eq!(
            access
                .execution_qualified("BUSINESS_EXCEPTION", "integration_error_task", &mut executor)
                .unwrap(),
            allowed,
        );
        service.validate(&access, &mut executor).await.unwrap();
    }
}

// 展示返工资格时，来源读取和工作项详情必须属于同一角色。
#[tokio::test]
async fn rework_display_requires_source_and_work_item_detail_in_the_same_role() {
    let identity = Arc::new(Identity::default());
    let service = service(identity.clone(), Arc::new(Facts::default()));
    let required = [work_item::DETAIL, inventory::STOCK_ADJUSTMENT_DETAIL];
    for (rework, allowed) in [(&required[..1], false), (&required[..], true)] {
        grant(
            &identity,
            &[("list-reader", &[work_item::LIST]), ("rework", rework), ("source-reader", &required[1..])],
        );
        let mut executor = SnapshotExecutor::default();
        let access = service.query(&actor(), &work_item::LIST, &mut executor).await.unwrap();
        assert_eq!(
            access.execution_qualified("APPROVAL_REWORK", "stock_adjustment", &mut executor).unwrap(),
            allowed,
        );
        service.validate(&access, &mut executor).await.unwrap();
    }
}

// 任务种类和来源各自合法，也不代表未登记的组合可获授权。
#[tokio::test]
async fn known_kind_and_known_source_do_not_authorize_an_unregistered_pair() {
    let kind = "APPROVAL_REWORK";
    let source = "integration_error_task";
    let item = ObjectFacts {
        subject_type: FactValue::Known(kind.into()),
        subject_source_resource: FactValue::Known(source.into()),
        assignee_id: FactValue::Known(None),
        completed: FactValue::Known(true),
        ..object("work_item", "task", "user")
    };
    let service = service(Arc::new(Identity::default()), Arc::new(Facts::new(vec![item])));
    let mut executor = SnapshotExecutor::default();

    let access = service.query(&actor(), &work_item::LIST, &mut executor).await.unwrap();
    assert!(matches!(
        access.execution_qualified(kind, source, &mut executor),
        Err(Error::ValidationError(_))
    ));
    service.validate(&access, &mut executor).await.unwrap();

    assert!(matches!(
        service
            .require(
                &actor(),
                &internal::WORK_ITEM_EXECUTION_QUALIFICATION,
                AccessTarget::Candidate(candidate(kind, source)),
                &mut executor,
            )
            .await,
        Err(Error::ValidationError(_))
    ));
    for permission in [internal::WORK_ITEM_RECIPIENT, internal::WORK_ITEM_EXECUTE] {
        assert!(matches!(
            service
                .require(
                    &actor(),
                    &permission,
                    AccessTarget::Task { id: "task".into(), version: "v1".into() },
                    &mut executor,
                )
                .await,
            Err(Error::ValidationError(_))
        ));
    }
}

// 非事务查询中的可选资格若被撤销，原结果不得交付。
#[tokio::test]
async fn optional_qualification_revocation_invalidates_the_original_nontransaction_query() {
    let identity = Arc::new(Identity::default());
    let service = service(identity.clone(), Arc::new(Facts::default()));
    grant(
        &identity,
        &[
            ("reader", &[work_item::LIST]),
            (
                "executor",
                &[purchase_order::DETAIL, purchase_payment_request::CREATE, purchase_payment_request::SUBMIT],
            ),
        ],
    );
    let mut executor = ReadExecutor::default();
    let access = service.query(&actor(), &work_item::LIST, &mut executor).await.unwrap();
    assert!(
        access.execution_qualified("PURCHASE_PAYMENT_ARRANGEMENT", "purchase_order", &mut executor).unwrap()
    );

    grant(&identity, &[("reader", &[work_item::LIST])]);
    assert!(matches!(service.validate(&access, &mut executor).await, Err(Error::ConflictError(_))));
}
