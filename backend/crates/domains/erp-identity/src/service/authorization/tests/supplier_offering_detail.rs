//! 供给详情独立功能资格、完整查询绑定与交付有效期。

use std::sync::Arc;
use std::sync::atomic::Ordering;

use erp_core::common::time::Instant;
use erp_permissions::supplier_offering;
use persistence_core::testing::SnapshotExecutor;

use super::fixtures::{Facts, Identity, actor, complete, object};
use crate::{AccessTarget, Error};

#[tokio::test]
async fn offering_detail_requires_its_own_qualification_for_record_and_query() {
    let identity = Arc::new(Identity::default());
    let facts = Arc::new(Facts::new(vec![object("supplier_offering", "offering", "user")]));
    let mut builder = complete();
    builder.facilities = identity.clone();
    builder.facts.insert("supplier_offering".into(), facts.clone());
    let service = builder.build().unwrap();
    let mut executor = SnapshotExecutor::default();
    identity.state.lock().unwrap().grants = vec![("list-reader".into(), vec![supplier_offering::LIST])];
    assert!(matches!(
        service
            .require(
                &actor(),
                &supplier_offering::DETAIL,
                AccessTarget::record("offering").unwrap(),
                &mut executor,
            )
            .await,
        Err(Error::Forbidden(_))
    ));
    assert!(matches!(
        service.query(&actor(), &supplier_offering::DETAIL, &mut executor).await,
        Err(Error::Forbidden(_))
    ));
    assert_eq!(facts.calls.load(Ordering::SeqCst), 0);

    identity.state.lock().unwrap().grants = vec![("detail-reader".into(), vec![supplier_offering::DETAIL])];
    service
        .require(
            &actor(),
            &supplier_offering::DETAIL,
            AccessTarget::record("offering").unwrap(),
            &mut executor,
        )
        .await
        .unwrap();
    let access = service.query(&actor(), &supplier_offering::DETAIL, &mut executor).await.unwrap();
    access.ensure_current(&mut executor).unwrap();
    service.validate(&access, &mut executor).await.unwrap();
    assert_eq!(facts.calls.load(Ordering::SeqCst), 1);
    assert!(matches!(
        service.query(&actor(), &supplier_offering::LIST, &mut executor).await,
        Err(Error::Forbidden(_))
    ));
}

#[tokio::test]
async fn offering_detail_query_requires_mapping_and_rejects_expired_delivery() {
    let mut missing = complete();
    missing.mappings.remove(&supplier_offering::DETAIL);
    assert!(matches!(
        missing.build(),
        Err(Error::ValidationError(reason)) if reason == "缺少查询 Mapping：supplier_offering:detail"
    ));
    let service = complete().build().unwrap();
    let mut executor = SnapshotExecutor::default();
    let mut access = service.query(&actor(), &supplier_offering::DETAIL, &mut executor).await.unwrap();
    access.proof.valid_until = Some(Instant::from_unix_secs(1));
    assert!(matches!(
        service.validate(&access, &mut executor).await,
        Err(Error::ConflictError(reason)) if reason.starts_with("DATA_SCOPE_CHANGED")
    ));
}
