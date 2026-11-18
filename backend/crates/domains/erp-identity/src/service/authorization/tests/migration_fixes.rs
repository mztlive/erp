//! 第五阶段调用修复执行真实公共授权 API，不读取数据库。
#![cfg(test)]
use std::sync::Arc;
use std::sync::atomic::Ordering;

use erp_core::common::time::Instant;
use erp_permissions::{Permission, purchase_order, work_item};
use mongodb::bson::{Document, doc};
use persistence_core::testing::SnapshotExecutor;
use persistence_core::{Executor, ReadExecutor};

use super::fixtures::{Facts, Identity, actor, complete, object};
use crate::entity::access_control::{ResolvedScope, ScopeClause};
use crate::entity::authorization::{AccessTarget, FactValue};
use crate::repository::authorization::fixtures::execute;
use crate::{AuthorizationService, CompiledAccess, Error};

fn grant(identity: &Identity, roles: &[(&str, &[Permission])]) {
    identity.state.lock().unwrap().grants =
        roles.iter().map(|(role, permissions)| ((*role).into(), permissions.to_vec())).collect();
}

fn service(identity: Arc<Identity>, resource: &str, facts: Arc<Facts>) -> AuthorizationService {
    let mut builder = complete();
    builder.facilities = identity;
    builder.facts.insert(resource.into(), facts);
    builder.build().unwrap()
}

fn scope(company: bool) -> ResolvedScope {
    ResolvedScope {
        role_clauses: vec![ScopeClause { company, self_owned: !company, ..Default::default() }],
        user_limit: None,
    }
}

#[tokio::test]
async fn sending_purchase_email_requires_both_independent_grants_before_loading_the_record() {
    let identity = Arc::new(Identity::default());
    let send = purchase_order::CORRESPONDENCE_SEND_PURCHASE_EMAIL;
    identity.scopes.lock().unwrap().insert((send.clone(), send.clone()), scope(true));
    let facts = Arc::new(Facts::new(vec![object("purchase_order", "purchase", "other")]));
    let service = service(identity.clone(), "purchase_order", facts.clone());
    let mut executor = SnapshotExecutor::default();
    for permissions in [vec![send.clone()], vec![purchase_order::DETAIL], vec![]] {
        grant(&identity, &[("role", &permissions)]);
        let result =
            service.require(&actor(), &send, AccessTarget::record("purchase").unwrap(), &mut executor).await;
        assert!(matches!(result, Err(Error::Forbidden(_))));
    }
    assert_eq!(facts.calls.load(Ordering::SeqCst), 0);

    grant(&identity, &[("sender", std::slice::from_ref(&send)), ("reader", &[purchase_order::DETAIL])]);
    service.require(&actor(), &send, AccessTarget::record("purchase").unwrap(), &mut executor).await.unwrap();
    assert_eq!(facts.calls.load(Ordering::SeqCst), 1);

    facts.objects.lock().unwrap().clear();
    assert!(matches!(
        service.require(&actor(), &send, AccessTarget::record("purchase").unwrap(), &mut executor).await,
        Err(Error::NotFound(_))
    ));
}

#[tokio::test]
async fn sending_purchase_email_cannot_use_an_objectless_or_nontransaction_target() {
    let identity = Arc::new(Identity::default());
    let facts = Arc::new(Facts::new(vec![object("purchase_order", "purchase", "user")]));
    let service = service(identity.clone(), "purchase_order", facts.clone());
    let send = purchase_order::CORRESPONDENCE_SEND_PURCHASE_EMAIL;
    assert!(matches!(
        service.require(&actor(), &send, AccessTarget::Operation, &mut SnapshotExecutor::default()).await,
        Err(Error::ValidationError(_))
    ));
    assert!(matches!(
        service
            .require(&actor(), &send, AccessTarget::record("purchase").unwrap(), &mut ReadExecutor::default())
            .await,
        Err(Error::ValidationError(_))
    ));
    assert_eq!(identity.permissions_calls.load(Ordering::SeqCst), 0);
    assert_eq!(facts.calls.load(Ordering::SeqCst), 0);
}

#[tokio::test]
async fn work_item_management_accepts_the_original_snapshot_and_rejects_nontransaction_prechecks() {
    let identity = Arc::new(Identity::default());
    grant(&identity, &[("manager", &[work_item::MANAGE])]);
    let facts = Arc::new(Facts::new(vec![object("work_item", "task", "user")]));
    let service = service(identity.clone(), "work_item", facts.clone());
    let target = || AccessTarget::record("task").unwrap();
    assert!(matches!(
        service.require(&actor(), &work_item::MANAGE, target(), &mut ReadExecutor::default()).await,
        Err(Error::ValidationError(_))
    ));
    assert_eq!(identity.permissions_calls.load(Ordering::SeqCst), 0);

    let mut executor = SnapshotExecutor::default();
    service.require(&actor(), &work_item::MANAGE, target(), &mut executor).await.unwrap();
    assert_eq!(facts.calls.load(Ordering::SeqCst), 1);

    facts.objects.lock().unwrap()[0].owner_id = FactValue::Known(Some("other".into()));
    assert!(matches!(
        service.require(&actor(), &work_item::MANAGE, target(), &mut executor).await,
        Err(Error::Forbidden(_))
    ));
}

fn members(stages: &[Document]) -> Vec<String> {
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

fn task_service(
    identity: Arc<Identity>,
    permission: &Permission,
    read_company: bool,
    manage_company: bool,
) -> AuthorizationService {
    identity.role_scopes.lock().unwrap().extend([
        (("reader".into(), permission.clone()), scope(read_company).role_clauses[0].clone()),
        (("manager".into(), work_item::MANAGE), scope(manage_company).role_clauses[0].clone()),
    ]);
    service(identity, "work_item", Arc::new(Facts::default()))
}

#[tokio::test]
async fn task_read_and_management_use_their_own_roles_without_ending_the_read_stage() {
    for permission in [work_item::LIST, work_item::DETAIL] {
        for (read_company, manage_company) in [(true, false), (false, true)] {
            let identity = Arc::new(Identity::default());
            grant(
                &identity,
                &[("reader", std::slice::from_ref(&permission)), ("manager", &[work_item::MANAGE])],
            );
            let service = task_service(identity.clone(), &permission, read_company, manage_company);
            let mut executor = SnapshotExecutor::default();
            let access = service.query(&actor(), &permission, &mut executor).await.unwrap();
            let read = if read_company { vec!["mine", "other"] } else { vec!["mine"] };
            let manage = if manage_company { vec!["mine", "other"] } else { vec!["mine"] };
            assert_eq!(members(access.stages()), read);
            assert_eq!(members(access.management_stages().unwrap()), manage);
            access.ensure_current(&mut executor).unwrap();
            service.validate(&access, &mut executor).await.unwrap();
            assert_eq!(identity.permissions_calls.load(Ordering::SeqCst), 1);
            assert!(access.ensure_current(&mut executor).is_err());
        }
    }
}

#[tokio::test]
async fn missing_management_qualification_preserves_reads_and_revocation_invalidates_delivery() {
    let identity = Arc::new(Identity::default());
    grant(&identity, &[("reader", &[work_item::LIST])]);
    let service = task_service(identity.clone(), &work_item::LIST, true, true);
    let mut executor = ReadExecutor::default();
    let access = service.query(&actor(), &work_item::LIST, &mut executor).await.unwrap();
    assert_eq!(members(access.stages()), ["mine", "other"]);
    assert!(members(access.management_stages().unwrap()).is_empty());
    service.validate(&access, &mut executor).await.unwrap();

    grant(&identity, &[("reader", &[work_item::LIST]), ("manager", &[work_item::MANAGE])]);
    let access = service.query(&actor(), &work_item::LIST, &mut executor).await.unwrap();
    assert_eq!(members(access.management_stages().unwrap()), ["mine", "other"]);

    grant(&identity, &[("reader", &[work_item::LIST])]);
    assert!(matches!(service.validate(&access, &mut executor).await, Err(Error::ConflictError(_))));
}

#[tokio::test]
async fn task_delivery_checks_the_original_deadline_after_management_projection() {
    let identity = Arc::new(Identity::default());
    grant(&identity, &[("reader", &[work_item::LIST]), ("manager", &[work_item::MANAGE])]);
    let deadline = Instant::from_unix_secs(4_102_444_800);
    identity.scope_deadlines.lock().unwrap().insert(work_item::MANAGE, deadline);
    let service = task_service(identity, &work_item::LIST, true, true);
    let mut executor = SnapshotExecutor::default();
    let mut access: CompiledAccess = service.query(&actor(), &work_item::LIST, &mut executor).await.unwrap();
    assert_eq!(members(access.management_stages().unwrap()), ["mine", "other"]);
    assert_eq!(access.proof.valid_until, Some(deadline));
    // 推进冻结到期边界，模拟展示读取完成时已经到期，不依赖真实等待。
    access.proof.valid_until = Some(Instant::from_unix_secs(1));
    assert!(matches!(service.validate(&access, &mut executor).await, Err(Error::ConflictError(_))));
    assert!(executor.boundary().unwrap().is_snapshot());
}
