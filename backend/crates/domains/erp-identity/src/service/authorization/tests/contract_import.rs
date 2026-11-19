//! 合同识别功能资格不替代归档候选检查；供应商下单沿用完整对象规则。
use std::sync::Arc;
use std::sync::atomic::Ordering;

use erp_permissions::{contract, customer, internal, supplier_fulfillment};
use persistence_core::testing::SnapshotExecutor;
use persistence_core::{Executor, NoTransaction, ReadExecutor};

use super::fixtures::{Facts, Identity, actor, candidate, complete, object, service};
use crate::entity::authorization::FactValue;
use crate::{AccessTarget, Error};

// 合同识别任务实时检查原合同创建资格，不为功能操作读取业务事实。
#[tokio::test]
async fn import_tasks_require_current_contract_creation_qualification_without_business_fact_reads() {
    let identity = Arc::new(Identity::default());
    let facts = Arc::new(Facts::default());
    let service = service(identity.clone(), facts.clone());
    let mut executor = SnapshotExecutor::default();
    for grants in [vec![], vec![contract::DETAIL], vec![internal::CONTRACT_IMPORT]] {
        identity.state.lock().unwrap().grants = vec![("role".into(), grants)];
        assert!(matches!(
            service
                .require(&actor(), &internal::CONTRACT_IMPORT, AccessTarget::Operation, &mut executor)
                .await,
            Err(Error::Forbidden(_))
        ));
    }

    identity.state.lock().unwrap().grants = vec![("role".into(), vec![contract::CREATE])];
    service
        .require(&actor(), &internal::CONTRACT_IMPORT, AccessTarget::Operation, &mut executor)
        .await
        .unwrap();

    identity.state.lock().unwrap().active = false;
    assert!(matches!(
        service.require(&actor(), &internal::CONTRACT_IMPORT, AccessTarget::Operation, &mut executor).await,
        Err(Error::Forbidden(_))
    ));

    identity.state.lock().unwrap().active = true;
    identity.state.lock().unwrap().fail = true;
    assert!(matches!(
        service.require(&actor(), &internal::CONTRACT_IMPORT, AccessTarget::Operation, &mut executor).await,
        Err(Error::Internal(_))
    ));
    assert_eq!(facts.calls.load(Ordering::SeqCst), 0);
}

// 识别只接受快照中的无对象操作，其资格不能替代合同归档候选授权。
#[tokio::test]
async fn import_entry_rejects_non_snapshot_and_cannot_replace_contract_candidate_authorization() {
    let identity = Arc::new(Identity::default());
    let service = service(identity.clone(), Arc::new(Facts::default()));
    identity.state.lock().unwrap().grants = vec![("role".into(), vec![contract::CREATE])];
    for executor in [&mut NoTransaction as &mut dyn Executor, &mut ReadExecutor::default()] {
        assert!(matches!(
            service.require(&actor(), &internal::CONTRACT_IMPORT, AccessTarget::Operation, executor).await,
            Err(Error::ValidationError(_))
        ));
    }
    assert!(matches!(
        service
            .require(&actor(), &contract::CREATE, AccessTarget::Operation, &mut SnapshotExecutor::default())
            .await,
        Err(Error::ValidationError(_))
    ));
    assert!(matches!(
        service
            .require(
                &actor(),
                &internal::CONTRACT_IMPORT,
                AccessTarget::record("task").unwrap(),
                &mut SnapshotExecutor::default()
            )
            .await,
        Err(Error::ValidationError(_))
    ));
}

// 供应商下单候选和幂等回放均要求原下单资格，并按各自当前归属判断。
#[tokio::test]
async fn supplier_place_candidates_and_replays_share_qualification_and_current_owner_rules() {
    let identity = Arc::new(Identity::default());
    let facts = Arc::new(Facts::new(vec![object("supplier_fulfillment_order", "order", "user")]));
    let mut builder = complete();
    builder.facilities = identity.clone();
    builder.facts.insert("supplier_fulfillment_order".into(), facts.clone());
    let service = builder.build().unwrap();
    identity.state.lock().unwrap().grants = vec![("role".into(), vec![supplier_fulfillment::ORDER_SUBMIT])];
    let permission = &supplier_fulfillment::ORDER_SUBMIT;
    let mut executor = SnapshotExecutor::default();
    service
        .require(
            &actor(),
            permission,
            AccessTarget::Candidate(candidate(permission.resource(), "user")),
            &mut executor,
        )
        .await
        .unwrap();
    assert_eq!(facts.calls.load(Ordering::SeqCst), 0);
    service
        .require(&actor(), permission, AccessTarget::record("order").unwrap(), &mut executor)
        .await
        .unwrap();

    identity.state.lock().unwrap().grants.clear();
    for target in [
        AccessTarget::Candidate(candidate(permission.resource(), "user")),
        AccessTarget::record("order").unwrap(),
    ] {
        assert!(matches!(
            service.require(&actor(), permission, target, &mut executor).await,
            Err(Error::Forbidden(_))
        ));
    }
}

// 销售分享要求独立固定资格，并按合同写入范围判断实际对象。
#[tokio::test]
async fn sales_sharing_uses_fixed_independent_qualification_and_the_contract_write_scope() {
    let identity = Arc::new(Identity::default());
    let facts = Arc::new(Facts::new(vec![object("contract", "contract", "user")]));
    let mut builder = complete();
    builder.facilities = identity.clone();
    builder.facts.insert("contract".into(), facts.clone());
    let service = builder.build().unwrap();
    let target = AccessTarget::record("contract").unwrap();
    let permission = &internal::CONTRACT_SALES_ACCESS;
    identity.state.lock().unwrap().grants = vec![
        ("contract-writer".into(), vec![contract::TERMINATE]),
        ("assignment-manager".into(), vec![customer::ASSIGNMENT_APPLY]),
    ];
    service.require(&actor(), permission, target.clone(), &mut SnapshotExecutor::default()).await.unwrap();
    for granted in [vec![contract::TERMINATE], vec![customer::ASSIGNMENT_APPLY], vec![permission.clone()]] {
        identity.state.lock().unwrap().grants = vec![("incomplete".into(), granted)];
        assert!(matches!(
            service.require(&actor(), permission, target.clone(), &mut SnapshotExecutor::default()).await,
            Err(Error::Forbidden(_))
        ));
    }

    identity.state.lock().unwrap().grants = vec![("writer".into(), vec![contract::TERMINATE])];
    service
        .require(&actor(), &contract::TERMINATE, target.clone(), &mut SnapshotExecutor::default())
        .await
        .unwrap();

    identity.state.lock().unwrap().grants =
        vec![("writer".into(), vec![contract::TERMINATE, customer::ASSIGNMENT_APPLY])];
    facts.objects.lock().unwrap()[0].owner_id = FactValue::Known(Some("other".into()));
    assert!(matches!(
        service.require(&actor(), permission, target.clone(), &mut SnapshotExecutor::default()).await,
        Err(Error::Forbidden(_))
    ));

    facts.objects.lock().unwrap()[0].owner_id = FactValue::Known(Some("user".into()));
    assert!(matches!(
        service.require(&actor(), permission, target, &mut ReadExecutor::default()).await,
        Err(Error::ValidationError(_))
    ));
    assert!(matches!(
        service
            .require(&actor(), permission, AccessTarget::Operation, &mut SnapshotExecutor::default())
            .await,
        Err(Error::ValidationError(_))
    ));
}
