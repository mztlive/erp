//! 应用入口切换后，配置与历史报表仍执行原功能资格和数据范围。
#![cfg(test)]
use std::collections::BTreeMap;
use std::sync::Arc;
use std::sync::atomic::Ordering;

use erp_permissions::{cost, customer, internal, purchase_order, sales_order};
use mongodb::bson::doc;
use persistence_core::ReadExecutor;
use persistence_core::testing::SnapshotExecutor;

use super::fixtures::{Facts, Identity, actor, complete, object, service};
use crate::entity::access_control::{ResolvedScope, ScopeClause};
use crate::entity::authorization::FactValue;
use crate::entity::resource_access::ResourceKind;
use crate::repository::authorization::fixtures::execute;
use crate::{AccessTarget, Error};

// 期间费用依据只检查成本列表资格，不要求销售列表或对象范围。
#[tokio::test]
async fn period_basis_requires_cost_list_without_requiring_sales_list_or_object_scope() {
    let identity = Arc::new(Identity::default());
    let service = service(identity.clone(), Arc::new(Facts::default()));
    for permissions in [vec![], vec![sales_order::LIST]] {
        identity.state.lock().unwrap().grants = vec![("reader".into(), permissions)];
        assert!(matches!(
            service
                .require(
                    &actor(),
                    &internal::PROFIT_LOSS_PERIOD_BASIS,
                    AccessTarget::Operation,
                    &mut SnapshotExecutor::default()
                )
                .await,
            Err(Error::Forbidden(_))
        ));
    }

    identity.state.lock().unwrap().grants = vec![("cost".into(), vec![cost::ENTRY_LIST])];
    identity.state.lock().unwrap().denied_scope = true;
    service
        .require(
            &actor(),
            &internal::PROFIT_LOSS_PERIOD_BASIS,
            AccessTarget::Operation,
            &mut SnapshotExecutor::default(),
        )
        .await
        .unwrap();

    assert!(matches!(
        service
            .require(
                &actor(),
                &internal::PROFIT_LOSS_PERIOD_BASIS,
                AccessTarget::Operation,
                &mut ReadExecutor::default()
            )
            .await,
        Err(Error::ValidationError(_))
    ));
    assert!(matches!(
        service
            .require(
                &actor(),
                &internal::PROFIT_LOSS_PERIOD_BASIS,
                AccessTarget::record("order").unwrap(),
                &mut SnapshotExecutor::default()
            )
            .await,
        Err(Error::ValidationError(_))
    ));
}

// 质量历史需独立客户资格，但结果仍沿用销售列表的记录范围。
#[tokio::test]
async fn quality_history_keeps_independent_customer_qualification_and_sales_list_membership() {
    let identity = Arc::new(Identity::default());
    let service = service(identity.clone(), Arc::new(Facts::default()));
    for permissions in [vec![sales_order::LIST], vec![customer::LIST]] {
        identity.state.lock().unwrap().grants = vec![("reader".into(), permissions)];
        assert!(matches!(
            service
                .query(&actor(), &internal::CUSTOMER_QUALITY_HISTORY, &mut SnapshotExecutor::default())
                .await,
            Err(Error::Forbidden(_))
        ));
    }

    identity.state.lock().unwrap().grants =
        vec![("sales".into(), vec![sales_order::LIST]), ("customers".into(), vec![customer::LIST])];
    identity.role_scopes.lock().unwrap().extend([
        (("sales".into(), sales_order::LIST), ScopeClause { self_owned: true, ..Default::default() }),
        (("customers".into(), customer::LIST), ScopeClause { company: true, ..Default::default() }),
    ]);

    let mut executor = SnapshotExecutor::default();
    let history = service.query(&actor(), &internal::CUSTOMER_QUALITY_HISTORY, &mut executor).await.unwrap();
    let baseline =
        service.query(&actor(), &sales_order::LIST, &mut SnapshotExecutor::default()).await.unwrap();
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
    assert_eq!(
        rows.iter().map(|row| row.get_str("id").unwrap()).collect::<Vec<_>>(),
        ["mine", "participated"]
    );
    service.validate(&history, &mut executor).await.unwrap();
}

// 交付选项独立要求更新和详情权限，列表继续使用原详情范围。
#[tokio::test]
async fn delivery_options_requires_update_and_detail_independently_without_changing_detail_scope() {
    let identity = Arc::new(Identity::default());
    let service = service(identity.clone(), Arc::new(Facts::default()));
    for permissions in [vec![purchase_order::DETAIL], vec![purchase_order::SAVE_DRAFT]] {
        identity.state.lock().unwrap().grants = vec![("buyer".into(), permissions)];
        assert!(matches!(
            service
                .query(&actor(), &internal::PURCHASE_DELIVERY_OPTIONS_READ, &mut SnapshotExecutor::default())
                .await,
            Err(Error::Forbidden(_))
        ));
    }

    identity.state.lock().unwrap().grants = vec![
        ("reader".into(), vec![purchase_order::DETAIL]),
        ("editor".into(), vec![purchase_order::SAVE_DRAFT]),
    ];
    identity.role_scopes.lock().unwrap().insert(
        ("reader".into(), purchase_order::DETAIL),
        ScopeClause { company: true, ..Default::default() },
    );
    identity.scopes.lock().unwrap().insert(
        (internal::PURCHASE_DELIVERY_OPTIONS_READ, internal::PURCHASE_DELIVERY_OPTIONS_READ),
        ResolvedScope {
            role_clauses: vec![ScopeClause { company: true, ..Default::default() }],
            user_limit: None,
        },
    );

    let mut executor = SnapshotExecutor::default();
    let access =
        service.query(&actor(), &internal::PURCHASE_DELIVERY_OPTIONS_READ, &mut executor).await.unwrap();
    let detail =
        service.query(&actor(), &purchase_order::DETAIL, &mut SnapshotExecutor::default()).await.unwrap();
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
    assert_eq!(rows.iter().map(|row| row.get_str("id").unwrap()).collect::<Vec<_>>(), ["active"]);
    service.validate(&access, &mut executor).await.unwrap();
}

// 费用采购来源只要求采购创建资格，不接受销售历史参与，但允许销售单记录读取授权并复验授权修订。
#[tokio::test]
async fn purchase_expense_source_requires_purchase_create_rejects_history_and_accepts_sales_record_grant() {
    let identity = Arc::new(Identity::default());
    let mut historical = object("sales_order", "order", "other");
    historical.history_participants = FactValue::Known(["user".into()].into());
    let facts = Arc::new(Facts::new(vec![historical]));
    let mut builder = complete();
    builder.facilities = identity.clone();
    builder.facts.insert("sales_order".into(), facts.clone());
    let service = builder.build().unwrap();

    identity.state.lock().unwrap().grants = vec![("sales".into(), vec![sales_order::DETAIL])];
    let mut executor = SnapshotExecutor::default();
    assert!(matches!(
        service
            .require(
                &actor(),
                &internal::PURCHASE_EXPENSE_SOURCE,
                AccessTarget::record("order").unwrap(),
                &mut executor,
            )
            .await,
        Err(Error::Forbidden(_))
    ));
    assert_eq!(facts.calls.load(Ordering::SeqCst), 0);

    identity.state.lock().unwrap().grants = vec![("buyer".into(), vec![purchase_order::CREATE])];
    assert!(matches!(
        service
            .require(
                &actor(),
                &internal::PURCHASE_EXPENSE_SOURCE,
                AccessTarget::record("order").unwrap(),
                &mut executor,
            )
            .await,
        Err(Error::Forbidden(_))
    ));

    identity.record_grants.lock().unwrap().insert((ResourceKind::SalesOrder, "order".into()));
    service
        .require(
            &actor(),
            &internal::PURCHASE_EXPENSE_SOURCE,
            AccessTarget::record("order").unwrap(),
            &mut executor,
        )
        .await
        .unwrap();

    let mut read_executor = ReadExecutor::default();
    let access =
        service.query(&actor(), &internal::PURCHASE_EXPENSE_SOURCE, &mut read_executor).await.unwrap();
    let rows = execute(
        vec![
            doc! {"id":"mine", "owner":"user", "deleted_at":0},
            doc! {"id":"participated", "owner":"other", "history":["user"], "deleted_at":0},
        ],
        access.stages(),
        &BTreeMap::new(),
        &doc! {},
    );
    assert_eq!(rows.iter().map(|row| row.get_str("id").unwrap()).collect::<Vec<_>>(), ["mine"]);

    identity.grant_revision.fetch_add(1, Ordering::SeqCst);
    assert!(matches!(service.validate(&access, &mut read_executor).await, Err(Error::ConflictError(_))));
}
