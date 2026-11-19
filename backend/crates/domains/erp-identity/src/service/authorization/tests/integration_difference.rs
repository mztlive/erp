//! 无正式任务差异沿真实对象核对资格与当前责任，不伪造任务版本。
use std::sync::Arc;
use std::sync::atomic::Ordering;

use erp_permissions::integration_ops;
use persistence_core::ReadExecutor;
use persistence_core::testing::SnapshotExecutor;

use super::fixtures::{Facts, Identity, actor, complete, object};
use crate::entity::authorization::{AccessTarget, FactValue, ObjectFacts};
use crate::{AuthorizationService, Error};

// 接入对账差异事实，以便通过公共入口测试无正式任务的处理动作。
fn service(identity: Arc<Identity>, facts: Arc<Facts>) -> AuthorizationService {
    let mut builder = complete();
    builder.facilities = identity;
    builder.facts.insert("reconciliation_difference".into(), facts);
    builder.build().unwrap()
}

// 构造真实差异记录，按用例设置当前处理人或明确不指派。
fn difference(handler: Option<&str>) -> ObjectFacts {
    ObjectFacts {
        assignee_id: FactValue::Known(handler.map(str::to_string)),
        ..object("reconciliation_difference", "difference", "user")
    }
}

// 只有具备完整资格且仍为当前处理人的账号可以直接处理差异。
#[tokio::test]
async fn direct_difference_decision_requires_current_handler_and_complete_qualification() {
    let identity = Arc::new(Identity::default());
    let facts = Arc::new(Facts::new(vec![difference(Some("user"))]));
    let service = service(identity.clone(), facts.clone());
    let target = || AccessTarget::record("difference").unwrap();
    let permission = &integration_ops::DIFFERENCE_DECISION;
    let mut executor = SnapshotExecutor::default();
    service.require(&actor(), permission, target(), &mut executor).await.unwrap();

    for handler in [Some("other"), None] {
        facts.objects.lock().unwrap()[0] = difference(handler);
        assert!(matches!(
            service.require(&actor(), permission, target(), &mut executor).await,
            Err(Error::Forbidden(_))
        ));
    }

    facts.objects.lock().unwrap()[0] = difference(Some("user"));
    facts.objects.lock().unwrap()[0].assignee_id = FactValue::Missing;
    assert!(matches!(
        service.require(&actor(), permission, target(), &mut executor).await,
        Err(Error::ValidationError(_))
    ));

    identity.state.lock().unwrap().grants.clear();
    let reads = facts.calls.load(Ordering::SeqCst);
    assert!(matches!(
        service.require(&actor(), permission, target(), &mut executor).await,
        Err(Error::Forbidden(_))
    ));
    assert_eq!(facts.calls.load(Ordering::SeqCst), reads);
}

// 没有正式任务时拒绝伪造任务和非事务预检，而且不读取身份或事实。
#[tokio::test]
async fn direct_difference_decision_rejects_fake_tasks_and_nontransaction_prechecks() {
    let identity = Arc::new(Identity::default());
    let facts = Arc::new(Facts::new(vec![difference(Some("user"))]));
    let service = service(identity.clone(), facts.clone());
    let permission = &integration_ops::DIFFERENCE_DECISION;
    assert!(matches!(
        service
            .require(
                &actor(),
                permission,
                AccessTarget::Task { id: "difference".into(), version: "v1".into() },
                &mut SnapshotExecutor::default(),
            )
            .await,
        Err(Error::ValidationError(_))
    ));

    assert!(matches!(
        service
            .require(
                &actor(),
                permission,
                AccessTarget::record("difference").unwrap(),
                &mut ReadExecutor::default(),
            )
            .await,
        Err(Error::ValidationError(_))
    ));
    assert_eq!(identity.permissions_calls.load(Ordering::SeqCst), 0);
    assert_eq!(facts.calls.load(Ordering::SeqCst), 0);
}
