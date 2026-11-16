//! 银行账号揭示只检查当前功能资格，并保留快照事务边界。
use std::sync::Arc;
use std::sync::atomic::Ordering;

use erp_permissions::{party, payable};
use persistence_core::testing::SnapshotExecutor;
use persistence_core::{Executor, NoTransaction, ReadExecutor};

use super::fixtures::{Facts, Identity, actor, service};
use crate::{AccessTarget, Error};

#[tokio::test]
async fn bank_reveal_allows_current_field_qualifications_only_in_snapshot() {
    let identity = Arc::new(Identity::default());
    identity.state.lock().unwrap().grants =
        vec![("bank-reader".into(), vec![party::BANK_ACCOUNT_DETAIL, payable::PAYMENT_RECIPIENT_REVEAL])];
    let facts = Arc::new(Facts::default());
    let service = service(identity.clone(), facts.clone());
    let mut executor = SnapshotExecutor::default();
    for permission in [party::BANK_ACCOUNT_DETAIL, payable::PAYMENT_RECIPIENT_REVEAL] {
        service.require(&actor(), &permission, AccessTarget::Operation, &mut executor).await.unwrap();
    }
    assert_eq!(facts.calls.load(Ordering::SeqCst), 0);
    assert_eq!(identity.permissions_calls.load(Ordering::SeqCst), 2);

    identity.state.lock().unwrap().grants = vec![("bank-reader".into(), vec![party::BANK_ACCOUNT_DETAIL])];
    assert!(matches!(
        service
            .require(&actor(), &payable::PAYMENT_RECIPIENT_REVEAL, AccessTarget::Operation, &mut executor,)
            .await,
        Err(Error::Forbidden(_))
    ));
    assert_eq!(facts.calls.load(Ordering::SeqCst), 0);
}

#[tokio::test]
async fn bank_reveal_rejects_non_snapshot_and_record_or_task_targets_before_identity() {
    let identity = Arc::new(Identity::default());
    let service = service(identity.clone(), Arc::new(Facts::default()));
    let permission = payable::PAYMENT_RECIPIENT_REVEAL;
    for executor in [&mut NoTransaction as &mut dyn Executor, &mut ReadExecutor::default()] {
        assert!(matches!(
            service.require(&actor(), &permission, AccessTarget::Operation, executor).await,
            Err(Error::ValidationError(message)) if message == "写入授权必须使用快照事务"
        ));
    }
    for target in [
        AccessTarget::record("account").unwrap(),
        AccessTarget::Task { id: "task".into(), version: "1".into() },
    ] {
        assert!(matches!(
            service.require(&actor(), &permission, target, &mut SnapshotExecutor::default()).await,
            Err(Error::ValidationError(message)) if message == "此权限不支持该授权目标"
        ));
    }
    assert_eq!(identity.permissions_calls.load(Ordering::SeqCst), 0);
}
