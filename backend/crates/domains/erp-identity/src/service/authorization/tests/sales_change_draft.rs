//! 销售变更草稿读取保留修改范围，并在完整结果交付前核对阶段和有效期。
use std::sync::Arc;

use erp_core::common::time::Instant;
use erp_permissions::{internal, sales_order, sales_review};
use persistence_core::testing::SnapshotExecutor;

use super::fixtures::{Facts, Identity, actor, complete, inquiry_members, object};
use crate::entity::authorization::{AccessTarget, FactValue};
use crate::entity::resource_access::ResourceKind;
use crate::{AuthorizationService, Error};

// 销售变更草稿读取要求两项独立入口资格，只沿用当前修改范围，不接纳历史补权。
#[tokio::test]
async fn draft_read_requires_both_independent_entries_and_never_adds_historical_read_supplements() {
    let identity = Arc::new(Identity::default());
    let mut historical = object("sales_order", "historical", "other");
    historical.history_participants = FactValue::Known(["user".into()].into());
    identity.record_grants.lock().unwrap().insert((ResourceKind::SalesOrder, "historical".into()));
    let mut builder = complete();
    builder.facilities = identity.clone();
    builder.facts.insert("sales_order".into(), Arc::new(Facts::new(vec![historical])));
    let service: AuthorizationService = builder.build().unwrap();
    let permission = internal::SALES_CHANGE_DRAFT_READ;
    for incomplete in [sales_order::HANDOVER, sales_review::SALES_CHANGE_DRAFT] {
        identity.state.lock().unwrap().grants = vec![("incomplete".into(), vec![incomplete])];
        assert!(matches!(
            service.query(&actor(), &permission, &mut SnapshotExecutor::default()).await,
            Err(Error::Forbidden(_))
        ));
    }

    identity.state.lock().unwrap().grants = vec![
        ("source".into(), vec![sales_order::HANDOVER]),
        ("entry".into(), vec![sales_review::SALES_CHANGE_DRAFT]),
    ];
    let mut executor = SnapshotExecutor::default();
    let access = service.query(&actor(), &permission, &mut executor).await.unwrap();
    assert_eq!(inquiry_members(&access), ["one"]);
    service.validate(&access, &mut executor).await.unwrap();
    assert!(matches!(access.ensure_current(&mut executor), Err(Error::ValidationError(_))));

    assert!(matches!(
        service
            .require(&actor(), &permission, AccessTarget::record("historical").unwrap(), &mut executor)
            .await,
        Err(Error::Forbidden(_))
    ));

    let mut expiring = service.query(&actor(), &permission, &mut executor).await.unwrap();
    expiring.validity.valid_until = Some(Instant::from_unix_secs(1));
    assert!(matches!(service.validate(&expiring, &mut executor).await, Err(Error::ConflictError(_))));
}
