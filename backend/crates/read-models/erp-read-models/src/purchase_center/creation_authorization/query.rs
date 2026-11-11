//! 采购创建任务的授权编排；查询与复验沿同一读取事务。

use std::sync::Arc;

use application_core::AuditActor;
use erp_identity::ports::AccessMapping;
use erp_identity::{
    AccessMode, AccessRegistration, AuthorizationRegistry, AuthorizationService, Permission,
    SharedRbacService,
};
use erp_workflow::entity::work_item::WorkItem;
use mongodb::Database;
use persistence_core::{Executor, Transactional};

use super::data;
use crate::Result;
use crate::purchase_center::repository::creation_tasks::{
    CreationTaskFilter, CreationTaskRoot, read_creation_tasks,
};

/// 在同一读取事务中查询 identity 允许的开放采购创建任务。
/// # 参数
/// db、rbac 为身份与业务设施，actor 为真实账号；两个可选身份只收窄查询。
/// # 返回
/// 经完整分配政策授权的任务，按创建时间排序。
/// # 错误
/// 身份失效、缺少采购创建权限、授权凭证变化或数据库失败时拒绝。
pub(crate) async fn creation_tasks(
    db: &Database,
    rbac: &SharedRbacService,
    actor: &AuditActor,
    sales_order_id: Option<&str>,
    work_item_id: Option<&str>,
) -> Result<Vec<WorkItem>> {
    let db = db.clone();
    let rbac = rbac.clone();
    let actor = actor.clone();
    let filter = CreationTaskFilter {
        sales_order_id: sales_order_id.map(str::to_owned),
        work_item_id: work_item_id.map(str::to_owned),
    };
    db.client()
        .clone()
        .with_transaction(move |executor| {
            Box::pin(async move { query_tasks(&db, &rbac, &actor, filter, executor).await })
        })
        .await
}

async fn query_tasks(
    db: &Database,
    rbac: &SharedRbacService,
    actor: &AuditActor,
    filter: CreationTaskFilter,
    executor: &mut dyn Executor,
) -> Result<Vec<WorkItem>> {
    let permission = Permission::parse("purchase_order:create")?;
    let mapping: Arc<dyn AccessMapping> = Arc::new(CreationTaskRoot::new()?);
    let registry = AuthorizationRegistry::new(vec![AccessRegistration {
        permission: permission.clone(),
        required_permissions: vec![],
        access: AccessMode::Query { data: data(), mapping: mapping.clone() },
    }])?;
    let service = AuthorizationService::new(db.clone(), rbac.clone(), registry);
    let access = service
        .query(actor, &permission, data().subject, executor)
        .await?
        .compile(mapping.as_ref(), executor)
        .await?;
    service.validate(&access, executor).await?;
    let tasks = read_creation_tasks(db, &filter, &access, executor).await?;
    service.validate(&access, executor).await?;
    Ok(tasks)
}
