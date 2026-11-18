//! 创建提交候选使用真实资金来源，已有结果仍使用持久化根事实。
use std::sync::Arc;
use std::sync::atomic::Ordering;

use erp_permissions::{Permission, payable, policy_permission, receivable, returns};
use persistence_core::ReadExecutor;
use persistence_core::testing::SnapshotExecutor;

use super::fixtures::{Facts, Identity, actor, candidate, complete, object};
use crate::Error;
use crate::entity::authorization::{AccessTarget, FactValue, ObjectFacts, RelatedObjects, RelationKey};

fn origin(resource: &str) -> ObjectFacts {
    ObjectFacts {
        relations: ["sales_source", "purchase_source", "settlement_source"]
            .into_iter()
            .map(|key| (RelationKey::parse(key).unwrap(), RelatedObjects::default()))
            .collect(),
        ..object(resource, "funds", "user")
    }
}

fn commit_candidate(permission: &Permission, key: &str, other: Option<&str>) -> ObjectFacts {
    let mut value = candidate(permission.resource(), "user");
    value.relations.insert(
        RelationKey::parse(key).unwrap(),
        RelatedObjects { expected_ids: ["funds".into()].into(), objects: vec![] },
    );
    if let Some(other) = other {
        value.relations.insert(RelationKey::parse(other).unwrap(), RelatedObjects::default());
    }
    value
}

#[tokio::test]
async fn commit_candidates_require_current_submit_and_full_real_source_without_loading_a_new_root() {
    for (permission, source_permission, key, other) in [
        (
            returns::CUSTOMER_REFUND_CUSTOMER_REFUND_SUBMIT,
            receivable::CUSTOMER_RECEIPT_DETAIL,
            "receipt_source",
            Some("receivable_source"),
        ),
        (
            returns::SUPPLIER_REFUND_SUPPLIER_REFUND_SUBMIT,
            payable::SUPPLIER_PAYMENT_DETAIL,
            "payment_source",
            Some("payable_source"),
        ),
        (
            returns::RECEIPT_REVERSAL_RECEIPT_REVERSAL_SUBMIT,
            receivable::CUSTOMER_RECEIPT_DETAIL,
            "receipt_source",
            None,
        ),
        (
            returns::PAYMENT_REVERSAL_PAYMENT_REVERSAL_SUBMIT,
            payable::SUPPLIER_PAYMENT_DETAIL,
            "payment_source",
            None,
        ),
    ] {
        let identity = Arc::new(Identity::default());
        identity.state.lock().unwrap().grants = vec![
            ("submitter".into(), vec![permission.clone()]),
            ("funds-reader".into(), vec![source_permission.clone(), policy_permission::FINANCE_LEDGER_READ]),
        ];
        let source = Arc::new(Facts::new(vec![origin(source_permission.resource())]));
        let root = Arc::new(Facts::default());
        let mut builder = complete();
        builder.facilities = identity.clone();
        builder.facts.insert(source_permission.resource().into(), source.clone());
        builder.facts.insert(permission.resource().into(), root.clone());
        let service = builder.build().unwrap();
        let target = || AccessTarget::Candidate(commit_candidate(&permission, key, other));
        let mut executor = SnapshotExecutor::default();

        service.require(&actor(), &permission, target(), &mut executor).await.unwrap();
        assert_eq!(root.calls.load(Ordering::SeqCst), 0);
        assert_eq!(source.calls.load(Ordering::SeqCst), 1);

        assert!(matches!(
            service.require(&actor(), &permission, target(), &mut ReadExecutor::default()).await,
            Err(Error::ValidationError(_))
        ));

        let calls = source.calls.load(Ordering::SeqCst);
        identity.state.lock().unwrap().grants.remove(0);
        assert!(matches!(
            service.require(&actor(), &permission, target(), &mut executor).await,
            Err(Error::Forbidden(_))
        ));
        assert_eq!(source.calls.load(Ordering::SeqCst), calls);

        identity.state.lock().unwrap().grants.push(("submitter".into(), vec![permission.clone()]));
        source.objects.lock().unwrap().clear();
        assert!(matches!(
            service.require(&actor(), &permission, target(), &mut executor).await,
            Err(Error::Forbidden(_))
        ));

        source.objects.lock().unwrap().push(origin(source_permission.resource()));
        identity.state.lock().unwrap().grants = vec![("submitter".into(), vec![permission.clone()])];
        assert!(matches!(
            service.require(&actor(), &permission, target(), &mut executor).await,
            Err(Error::Forbidden(_))
        ));

        identity.state.lock().unwrap().grants.push((
            "funds-reader".into(),
            vec![source_permission.clone(), policy_permission::FINANCE_LEDGER_READ],
        ));
        let mut wrong_handler = commit_candidate(&permission, key, other);
        wrong_handler.owner_id = FactValue::Known(Some("other".into()));
        assert!(matches!(
            service
                .require(&actor(), &permission, AccessTarget::Candidate(wrong_handler), &mut executor)
                .await,
            Err(Error::Forbidden(_))
        ));

        let mut fabricated = commit_candidate(&permission, key, other);
        fabricated
            .relations
            .get_mut(&RelationKey::parse(key).unwrap())
            .unwrap()
            .objects
            .push(origin(source_permission.resource()));
        assert!(matches!(
            service.require(&actor(), &permission, AccessTarget::Candidate(fabricated), &mut executor).await,
            Err(Error::ValidationError(_))
        ));

        assert!(matches!(
            service.require(&actor(), &permission, AccessTarget::Operation, &mut executor).await,
            Err(Error::ValidationError(_))
        ));
        assert!(matches!(
            service
                .require(&actor(), &permission, AccessTarget::record("missing").unwrap(), &mut executor)
                .await,
            Err(Error::NotFound(_))
        ));
    }
}
