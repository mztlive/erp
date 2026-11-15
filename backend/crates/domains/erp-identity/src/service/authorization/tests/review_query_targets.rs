//! 修复业务接入审查发现的功能查询交付和精确任务目标缺口。
use std::sync::Arc;
use std::sync::atomic::Ordering;

use erp_permissions::{
    customer, demo_master_data, internal, sales_inquiry, supplier, supplier_api, supplier_offering,
};
use persistence_core::ReadExecutor;
use persistence_core::testing::SnapshotExecutor;

use super::fixtures::{Facts, Identity, actor, candidate, complete, object};
use crate::Error;
use crate::entity::access_control::{ResolvedScope, ScopeClause};
use crate::entity::authorization::{AccessTarget, FactValue, ObjectFacts, RelatedObjects, RelationKey};

#[tokio::test]
async fn function_reads_check_snapshot_qualification_without_query_bindings() {
    for permission in [
        customer::ASSIGNMENT_LIST,
        demo_master_data::STATUS,
        supplier_api::CONNECTION_LIST,
        internal::CONNECTION_BIND_ENDPOINT,
        supplier_api::DANGAOSHUSHU_REFERENCE_TICKETS,
    ] {
        let identity = Arc::new(Identity::default());
        let mut builder = complete();
        builder.facilities = identity.clone();
        let service = builder.build().unwrap();
        let mut executor = SnapshotExecutor::default();
        identity.state.lock().unwrap().grants.clear();
        assert!(matches!(
            service.require(&actor(), &permission, AccessTarget::Operation, &mut executor).await,
            Err(Error::Forbidden(_))
        ));
        identity.state.lock().unwrap().grants = vec![("reader".into(), vec![permission.clone()])];
        service.require(&actor(), &permission, AccessTarget::Operation, &mut executor).await.unwrap();
        assert!(matches!(
            service.query(&actor(), &permission, &mut executor).await,
            Err(Error::ValidationError(_))
        ));
        identity.state.lock().unwrap().active = false;
        assert!(matches!(
            service.require(&actor(), &permission, AccessTarget::Operation, &mut executor).await,
            Err(Error::Forbidden(_))
        ));
    }
}

#[test]
fn function_reads_build_without_query_mappings_and_reject_mapping_registration() {
    let builder = complete();
    let mapping = builder.mappings[&customer::LIST].clone();
    assert!(builder.build().is_ok());
    for permission in [
        customer::ASSIGNMENT_LIST,
        demo_master_data::STATUS,
        supplier_api::CONNECTION_LIST,
        internal::CONNECTION_BIND_ENDPOINT,
        supplier_api::DANGAOSHUSHU_REFERENCE_TICKETS,
    ] {
        assert!(matches!(complete().mapping(&permission, mapping.clone()), Err(Error::ValidationError(_))));
    }
}

#[tokio::test]
async fn exception_task_completion_cannot_replace_an_exact_task_with_an_operation() {
    let permission = supplier_offering::COMPLETE_SUPPLY_EXCEPTION_TASK;
    let task = Facts::new(vec![ObjectFacts {
        assignee_id: FactValue::Known(Some("user".into())),
        ..object(permission.resource(), "task", "user")
    }]);
    let task = Arc::new(task);
    let mut builder = complete();
    builder.facts.insert("work_item".into(), task.clone());
    let service = builder.build().unwrap();
    let mut executor = SnapshotExecutor::default();
    assert!(matches!(
        service.require(&actor(), &permission, AccessTarget::Operation, &mut executor).await,
        Err(Error::ValidationError(_))
    ));
    assert_eq!(task.calls.load(Ordering::SeqCst), 0);
    let target = |version: &str| AccessTarget::Task { id: "task".into(), version: version.into() };
    service.require(&actor(), &permission, target("v1"), &mut executor).await.unwrap();
    assert!(matches!(
        service.require(&actor(), &permission, target("stale"), &mut executor).await,
        Err(Error::ConflictError(_))
    ));
    task.objects.lock().unwrap()[0].assignee_id = FactValue::Known(Some("other".into()));
    assert!(matches!(
        service.require(&actor(), &permission, target("v1"), &mut executor).await,
        Err(Error::Forbidden(_))
    ));
}

#[tokio::test]
async fn connection_detail_needs_supplier_qualification_and_catalog_keeps_its_independent_scope() {
    let identity = Arc::new(Identity::default());
    let mut builder = complete();
    builder.facilities = identity.clone();
    let service = builder.build().unwrap();
    let mut executor = ReadExecutor::default();
    identity.state.lock().unwrap().grants =
        vec![("connection".into(), vec![supplier_api::CONNECTION_DETAIL])];
    assert!(matches!(
        service.query(&actor(), &supplier_api::CONNECTION_DETAIL, &mut executor).await,
        Err(Error::Forbidden(_))
    ));
    identity.state.lock().unwrap().grants.push(("supplier".into(), vec![supplier::DETAIL]));
    let access = service.query(&actor(), &supplier_api::CONNECTION_DETAIL, &mut executor).await.unwrap();
    service.validate(&access, &mut executor).await.unwrap();
    let supplier = object("supplier", "supplier", "user");
    let connection = ObjectFacts {
        relations: [(
            RelationKey::parse("supplier").unwrap(),
            RelatedObjects { expected_ids: ["supplier".into()].into(), objects: vec![supplier] },
        )]
        .into(),
        ..object("supplier_api_connection", "connection", "user")
    };
    let facts = Arc::new(Facts::new(vec![connection]));
    let mut builder = complete();
    builder.facilities = identity.clone();
    builder.facts.insert("supplier_api_connection".into(), facts);
    let service = builder.build().unwrap();
    let mut executor = SnapshotExecutor::default();
    let check = |id| AccessTarget::record(id).unwrap();
    service
        .require(&actor(), &internal::CONNECTION_CATALOG_READ, check("connection"), &mut executor)
        .await
        .unwrap();
    identity.scopes.lock().unwrap().insert(
        (supplier::DETAIL, supplier::DETAIL),
        ResolvedScope { role_clauses: vec![ScopeClause::default()], user_limit: None },
    );
    assert!(matches!(
        service
            .require(&actor(), &internal::CONNECTION_CATALOG_READ, check("connection"), &mut executor)
            .await,
        Err(Error::Forbidden(_))
    ));
}

#[tokio::test]
async fn inquiry_write_qualification_previews_require_the_callers_snapshot() {
    let mut builder = complete();
    let facts = Arc::new(Facts::new(vec![object("sales_inquiry", "inquiry", "user")]));
    builder.facts.insert("sales_inquiry".into(), facts.clone());
    let service = builder.build().unwrap();
    for (permission, target) in [
        (sales_inquiry::CONVERSION_SALES_BASIS, AccessTarget::record("inquiry").unwrap()),
        (sales_inquiry::IMPORT_SALES_IMPORT_PREVIEW, AccessTarget::record("inquiry").unwrap()),
        (internal::SALES_INQUIRY_IMPORT_CREATE, AccessTarget::Candidate(candidate("sales_inquiry", "user"))),
    ] {
        let calls = facts.calls.load(Ordering::SeqCst);
        assert!(matches!(
            service.require(&actor(), &permission, target.clone(), &mut ReadExecutor::default()).await,
            Err(Error::ValidationError(message)) if message == "写入授权必须使用快照事务"
        ));
        assert_eq!(facts.calls.load(Ordering::SeqCst), calls);
        service.require(&actor(), &permission, target, &mut SnapshotExecutor::default()).await.unwrap();
    }
}
