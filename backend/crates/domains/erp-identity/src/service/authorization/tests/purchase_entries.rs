//! 采购写入口通过真实 require 同时检查独立业务资格及来源采购规则。
use std::sync::Arc;
use std::sync::atomic::Ordering;

use erp_permissions::{Permission, internal, purchase_order};
use persistence_core::ReadExecutor;
use persistence_core::testing::SnapshotExecutor;

use super::fixtures::{Facts, Identity, actor, candidate, complete, object};
use crate::entity::access_control::{ResolvedScope, ScopeClause};
use crate::entity::authorization::{AccessTarget, FactValue};
use crate::entity::resource_access::ResourceKind;
use crate::{AuthorizationService, Error};

/// 明确业务入口与来源动作，测试执行真实固定规则而非检查规则声明。
fn entries() -> Vec<(Permission, Vec<Permission>, Permission)> {
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

/// 将真实采购事实接入完整生产 Builder，设施不访问数据库。
fn service(identity: Arc<Identity>, facts: Arc<Facts>) -> AuthorizationService {
    let mut builder = complete();
    builder.facilities = identity;
    builder.facts.insert("purchase_order".into(), facts);
    builder.build().unwrap()
}

#[tokio::test]
async fn purchase_entries_reject_missing_entry_or_source_before_facts_and_accept_independent_roles() {
    for (entry, source, independent) in entries() {
        let identity = Arc::new(Identity::default());
        let facts = Arc::new(Facts::new(vec![object("purchase_order", "order", "user")]));
        let service = service(identity.clone(), facts.clone());
        let mut executor = SnapshotExecutor::default();
        for granted in [source.clone(), vec![independent.clone()]] {
            identity.state.lock().unwrap().grants = vec![("incomplete".into(), granted)];
            assert!(matches!(
                service
                    .require(&actor(), &entry, AccessTarget::record("order").unwrap(), &mut executor)
                    .await,
                Err(Error::Forbidden(_))
            ));
            assert_eq!(facts.calls.load(Ordering::SeqCst), 0);
        }

        identity.state.lock().unwrap().grants =
            vec![("source".into(), source), ("entry".into(), vec![independent])];
        service
            .require(&actor(), &entry, AccessTarget::record("order").unwrap(), &mut executor)
            .await
            .unwrap();

        identity.state.lock().unwrap().active = false;
        let calls = facts.calls.load(Ordering::SeqCst);
        assert!(matches!(
            service.require(&actor(), &entry, AccessTarget::record("order").unwrap(), &mut executor).await,
            Err(Error::Forbidden(_))
        ));
        assert_eq!(facts.calls.load(Ordering::SeqCst), calls);
    }
}

#[tokio::test]
async fn purchase_draft_requires_one_role_with_both_source_actions_besides_entry_qualification() {
    let identity = Arc::new(Identity::default());
    identity.state.lock().unwrap().grants = vec![
        ("update".into(), vec![purchase_order::SAVE_DRAFT]),
        ("submit".into(), vec![purchase_order::SUBMIT]),
        ("entry".into(), vec![purchase_order::PURCHASE_CHANGE_SUBMIT]),
    ];
    let facts = Arc::new(Facts::new(vec![object("purchase_order", "order", "user")]));
    let service = service(identity, facts.clone());
    assert!(matches!(
        service
            .require(
                &actor(),
                &internal::PURCHASE_CHANGE_SOURCE,
                AccessTarget::record("order").unwrap(),
                &mut SnapshotExecutor::default(),
            )
            .await,
        Err(Error::Forbidden(_))
    ));
    assert_eq!(facts.calls.load(Ordering::SeqCst), 0);
}

#[tokio::test]
async fn purchase_entries_preserve_source_limits_and_never_accept_historical_read_supplements() {
    for (entry, source, independent) in entries() {
        let identity = Arc::new(Identity::default());
        identity.state.lock().unwrap().grants =
            vec![("source".into(), source), ("entry".into(), vec![independent])];
        identity.scopes.lock().unwrap().insert(
            (entry.clone(), entry.clone()),
            ResolvedScope {
                role_clauses: vec![ScopeClause { self_owned: true, ..Default::default() }],
                user_limit: None,
            },
        );
        identity.record_grants.lock().unwrap().insert((ResourceKind::PurchaseOrder, "order".into()));
        let mut historical = object("purchase_order", "order", "other");
        historical.history_participants = FactValue::Known(["user".into()].into());
        let facts = Arc::new(Facts::new(vec![historical]));
        let service = service(identity, facts.clone());
        let mut executor = SnapshotExecutor::default();
        assert!(matches!(
            service.require(&actor(), &entry, AccessTarget::record("order").unwrap(), &mut executor).await,
            Err(Error::Forbidden(_))
        ));
        facts.objects.lock().unwrap()[0].owner_id = FactValue::Known(Some("user".into()));
        service
            .require(&actor(), &entry, AccessTarget::record("order").unwrap(), &mut executor)
            .await
            .unwrap();
        facts.objects.lock().unwrap()[0].org_unit_id = FactValue::Missing;
        assert!(matches!(
            service.require(&actor(), &entry, AccessTarget::record("order").unwrap(), &mut executor).await,
            Err(Error::ValidationError(_))
        ));
    }
}

#[tokio::test]
async fn purchase_entries_require_persisted_records_and_snapshot_write_authorization() {
    let service = service(Arc::new(Identity::default()), Arc::new(Facts::default()));
    for (entry, _, _) in entries() {
        for target in [AccessTarget::Operation, AccessTarget::Candidate(candidate("purchase_order", "user"))]
        {
            assert!(matches!(
                service.require(&actor(), &entry, target, &mut SnapshotExecutor::default()).await,
                Err(Error::ValidationError(_))
            ));
        }
        assert!(matches!(
            service
                .require(
                    &actor(),
                    &entry,
                    AccessTarget::record("order").unwrap(),
                    &mut ReadExecutor::default(),
                )
                .await,
            Err(Error::ValidationError(_))
        ));
    }
}
