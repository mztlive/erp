//! 询价对象事实、字段映射与 identity 授权登记。
use std::sync::Arc;

use application_core::AuditActor;
use erp_identity::entity::authorization::{FactKind, QueryModel, QueryModelKey, QueryOutput, ReadAccess};
use erp_identity::ports::AccessMapping;
use erp_identity::{
    AccessMode, AccessPolicy, AccessRegistration, AccessSubject, AccessTarget, AuthorizationRegistry,
    AuthorizationService, CompiledAccess, DataAccess, Permission, ResourceTargets, SharedRbacService,
};
use erp_sales::entity::sales_inquiry::SalesInquiry;
use mongodb::Database;
use persistence_core::Executor;

use crate::repository::sales_inquiry_access::{InquiryFacts, InquiryMapping, TaskFacts, TaskInquiryMapping};
use crate::{Error, Result};

pub(super) fn service(
    db: Database,
    rbac: SharedRbacService,
    action: &str,
    extra: &[Permission],
    candidate: Option<SalesInquiry>,
) -> Result<(AuthorizationService, Arc<dyn AccessMapping>, Permission)> {
    let permission = permission(action)?;
    let mapping: Arc<dyn AccessMapping> = Arc::new(InquiryMapping(QueryModel {
        key: QueryModelKey::parse("sales_inquiries")?,
        resource: "sales_inquiry".into(),
        collection: "sales_inquiries".into(),
        id_field: "id".into(),
        output: QueryOutput::Object,
        facts: [FactKind::Owner, FactKind::Organization, FactKind::Collaborators].into(),
    }));
    let entry = AccessRegistration {
        permission: permission.clone(),
        required_permissions: extra.to_vec(),
        access: AccessMode::QueryAndObject {
            data: DataAccess {
                subject: AccessSubject::Direct,
                required_facts: [FactKind::Owner, FactKind::Organization].into(),
                policy: AccessPolicy::Scope { permission: permission.clone(), read: ReadAccess::default() },
            },
            mapping: mapping.clone(),
            facts: Arc::new(InquiryFacts { db: db.clone(), candidate }),
        },
    };
    Ok((AuthorizationService::new(db, rbac, AuthorizationRegistry::new(vec![entry])?), mapping, permission))
}

fn permission(action: &str) -> Result<Permission> {
    if !matches!(
        action,
        "list"
            | "detail"
            | "create"
            | "update"
            | "submit"
            | "accept"
            | "reinquire"
            | "close"
            | "import"
            | "convert_sales"
            | "recover"
    ) {
        return Err(Error::ValidationError("未登记的销售询价动作".into()));
    }
    Ok(Permission::parse(format!("sales_inquiry:{action}"))?)
}

pub(super) async fn require(
    db: Database,
    rbac: SharedRbacService,
    actor: &AuditActor,
    action: &str,
    inquiry: &SalesInquiry,
    extra: &[Permission],
    executor: &mut dyn Executor,
) -> Result<()> {
    let (authorization, _, permission) = service(db, rbac, action, extra, Some(inquiry.clone()))?;
    authorization
        .require(
            actor,
            &permission,
            AccessTarget::Records {
                subject: AccessSubject::Direct,
                ids: ResourceTargets::new(&[inquiry.base.id.as_str()])?,
            },
            executor,
        )
        .await?;
    Ok(())
}

pub(super) async fn query(
    db: Database,
    rbac: SharedRbacService,
    actor: &AuditActor,
    action: &str,
    executor: &mut dyn Executor,
) -> Result<(Arc<CompiledAccess>, AuthorizationService)> {
    let (authorization, mapping, permission) = service(db, rbac, action, &[], None)?;
    let query = authorization.query(actor, &permission, AccessSubject::Direct, executor).await?;
    let compiled = Arc::new(query.compile(mapping.as_ref(), executor).await?);
    authorization.validate(&compiled, executor).await?;
    Ok((compiled, authorization))
}

pub(super) async fn task_query(
    db: Database,
    rbac: SharedRbacService,
    actor: &AuditActor,
    tasks: &[erp_workflow::entity::work_item::WorkItem],
    executor: &mut dyn Executor,
) -> Result<(Arc<CompiledAccess>, AuthorizationService)> {
    let permission = Permission::parse("procurement_inquiry:list")?;
    let mut parts = tasks
        .iter()
        .map(|task| format!("{}:{}:{}", task.base.id, task.base.version, task.subject_version))
        .collect::<Vec<_>>();
    parts.sort();
    let version = application_core::CommandFingerprint::from_parts(parts).as_str().to_owned();
    let mapping: Arc<dyn AccessMapping> = Arc::new(TaskInquiryMapping {
        model: QueryModel {
            key: QueryModelKey::parse("procurement_inquiries")?,
            resource: "procurement_inquiry".into(),
            collection: "sales_inquiries".into(),
            id_field: "id".into(),
            output: QueryOutput::Object,
            facts: [FactKind::Assignee].into(),
        },
        ids: tasks
            .iter()
            .filter(|task| task.owner_user_id.as_deref() == Some(actor.id()))
            .map(|task| task.business_object_id.clone())
            .collect(),
        version,
    });
    let entry = AccessRegistration {
        permission: permission.clone(),
        required_permissions: vec![],
        access: AccessMode::Query {
            data: DataAccess {
                subject: AccessSubject::Direct,
                required_facts: [FactKind::Assignee].into(),
                policy: AccessPolicy::Assigned,
            },
            mapping: mapping.clone(),
        },
    };
    let authorization = AuthorizationService::new(db, rbac, AuthorizationRegistry::new(vec![entry])?);
    let query = authorization.query(actor, &permission, AccessSubject::Direct, executor).await?;
    let compiled = Arc::new(query.compile(mapping.as_ref(), executor).await?);
    authorization.validate(&compiled, executor).await?;
    Ok((compiled, authorization))
}

pub(super) async fn require_task_owner(
    db: Database,
    rbac: SharedRbacService,
    actor: &AuditActor,
    action: &str,
    task: &erp_workflow::entity::work_item::WorkItem,
    executor: &mut dyn Executor,
) -> Result<()> {
    let permission = Permission::parse(format!("procurement_inquiry:{action}"))?;
    let entry = AccessRegistration {
        permission: permission.clone(),
        required_permissions: vec![],
        access: AccessMode::Object {
            data: DataAccess {
                subject: AccessSubject::Direct,
                required_facts: [FactKind::Assignee].into(),
                policy: AccessPolicy::Assigned,
            },
            facts: Arc::new(TaskFacts(task.clone())),
        },
    };
    let service = AuthorizationService::new(db, rbac, AuthorizationRegistry::new(vec![entry])?);
    service
        .require(
            actor,
            &permission,
            AccessTarget::Records {
                subject: AccessSubject::Direct,
                ids: ResourceTargets::new(&[task.base.id.as_str()])?,
            },
            executor,
        )
        .await?;
    Ok(())
}
