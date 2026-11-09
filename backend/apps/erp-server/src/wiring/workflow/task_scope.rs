//! 任务管理范围按当前具体负责人的内部组织关系编译，不改写责任组织。

use application_core::AuditActor;
use erp_identity::service::access_control::resolve::DataScopeService;
use erp_identity::{Error as IdentityError, Permission, SharedRbacService};
use erp_workflow::ports::WorkflowQueueAccessFact;
use erp_workflow::repository::prelude::*;
use erp_workflow::service::work_item::read::compile_task_owners;
use erp_workflow::{DocumentRegistryExt, Error, Result, WorkflowAuthorizationPort};
use mongodb::Database;
use persistence_core::Executor;

use super::map_service;

/// 首拍版本与管理范围共享同事务身份事实，权限和参与关系保留原查询及首错顺序。
///
/// # 参数
/// * `db` - 身份与参与集合所在数据库。
/// * `rbac` - 现有 RBAC 快照服务。
/// * `auth` - 工作流授权端口，用于角色与权限代码。
/// * `actor` - 当前操作人。
/// * `include_version` - 为真时读取授权版本。
/// * `executor` - 调用方执行器。
///
/// # 返回
/// 返回队列访问事实。无 `work_item:manage` 时 `managed_owner_ids` 为空向量。
/// 公司范围时该字段为 `None`。
///
/// # 错误
/// 授权版本、角色、参与单据或范围解析失败时返回映射后的工作流错误。负责人超过 20000 时返回校验错误。
///
/// # Panics
/// 固定权限码 `work_item:manage` 无法解析时 panic。
pub(super) async fn queue_access(
    db: &Database,
    rbac: &SharedRbacService,
    auth: &impl WorkflowAuthorizationPort,
    actor: &AuditActor,
    include_version: bool,
    executor: &mut dyn Executor,
) -> Result<WorkflowQueueAccessFact> {
    let service = DataScopeService::new(db.clone(), rbac.clone());
    let permission = Permission::parse("work_item:manage").expect("固定权限合法");
    let mut batch = service.batch(actor, &[permission], executor);
    let version = if include_version {
        Some(batch.authorization_version().await.map_err(|error| map_service(error.into()))?)
    } else {
        None
    };
    let mut facts = batch.read(|executor| Box::pin(access_inputs(db, auth, actor, executor))).await?;
    facts.identity_version = version;
    let scope = match batch.resolve_permissions("work_item", "manage", &[]).await {
        Ok(scope) => scope,
        Err(IdentityError::Forbidden(_)) => {
            facts.managed_owner_ids = Some(Vec::new());
            return Ok(facts);
        },
        Err(error) => return Err(map_service(error.into())),
    };
    facts.managed_owner_ids = compile_task_owners(&scope)?;
    Ok(facts)
}

/// 与原工作台相同，角色、权限代码、参与单据、逐角色管理证明依次读取。
///
/// # Panics
/// 已授予权限码或固定的 `work_item:manage` 无法解析时 panic。
async fn access_inputs(
    db: &Database,
    auth: &impl WorkflowAuthorizationPort,
    actor: &AuditActor,
    executor: &mut dyn Executor,
) -> Result<WorkflowQueueAccessFact> {
    let roles = auth.role_ids_with_executor(actor.kind(), actor.id(), executor).await?;
    let permission_codes = auth.permission_codes(actor.kind(), actor.id()).await?;
    let permissions = permission_codes
        .iter()
        .map(|code| Permission::parse(code).expect("granted permission must be valid"))
        .collect::<Vec<_>>();
    let participant_document_ids =
        db.document_participants().document_ids_by_user(actor.id(), executor).await?;
    let manage = Permission::parse("work_item:manage").expect("固定权限合法");
    let can_manage = if permissions.iter().any(|permission| permission.covers(&manage)) {
        !auth.roles_granting_permission(&roles, "work_item:manage").await?.is_empty()
    } else {
        false
    };
    Ok(WorkflowQueueAccessFact {
        permission_codes,
        participant_document_ids,
        managed_owner_ids: Some(Vec::new()),
        can_manage,
        identity_version: None,
    })
}

/// `None` 是公共解析器证明的公司范围；空集合保持失败关闭。
///
/// # 参数
/// * `db` - 身份集合所在数据库。
/// * `rbac` - 现有 RBAC 快照服务。
/// * `actor` - 当前操作人。
/// * `executor` - 调用方执行器。
///
/// # 返回
/// 公司范围返回 `None`；否则返回允许管理的负责人编号。无权限时返回空向量。
///
/// # 错误
/// 范围解析的非拒绝错误映射为工作流错误。负责人超过 20000 时返回校验错误。
pub(super) async fn managed_owners(
    db: &Database,
    rbac: &SharedRbacService,
    actor: &AuditActor,
    executor: &mut dyn Executor,
) -> Result<Option<Vec<String>>> {
    let scope = match DataScopeService::new(db.clone(), rbac.clone())
        .resolve(actor, "work_item", "manage", executor)
        .await
    {
        Ok(scope) => scope,
        Err(IdentityError::Forbidden(_)) => return Ok(Some(Vec::new())),
        Err(error) => return Err(map_service(error.into())),
    };
    compile_task_owners(&scope)
}

/// 统一解析 list、detail、manage；读取拒绝不得回退到本人或历史参与。
/// # 参数
/// 数据库、RBAC、真实账号、任务动作及当前执行器。
/// # 返回
/// 返回 DataScope 编译后的负责人条件。
/// # 错误
/// 未登记动作、权限或数据范围拒绝、基础设施故障均返回错误。
pub(super) async fn task_owners(
    db: &Database,
    rbac: &SharedRbacService,
    actor: &AuditActor,
    action: &str,
    executor: &mut dyn Executor,
) -> Result<Option<Vec<String>>> {
    if !matches!(action, "list" | "detail" | "manage") {
        return Err(Error::ValidationError("未登记的任务权限动作".into()));
    }
    let scope = DataScopeService::new(db.clone(), rbac.clone())
        .resolve(actor, "work_item", action, executor)
        .await
        .map_err(|error| map_service(error.into()))?;
    compile_task_owners(&scope)
}
