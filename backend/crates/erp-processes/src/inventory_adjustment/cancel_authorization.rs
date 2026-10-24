//! 库存调整普通撤回授权：原提交人和运行管理员沿调用方快照重新证明身份。

use application_core::AuditActor;
use bpm::model::ApprovalProcessInstance;
use erp_identity::SharedRbacService;
use erp_workflow::ApprovalIntegrationExt;
use erp_workflow::entity::approval_integration::ApprovalSubjectSnapshot;
use erp_workflow::entity::document_registry::DocumentType;
use erp_workflow::ports::WorkflowAuthorizationPort;
use erp_workflow::repository::prelude::*;
use erp_workflow::service::approval::{
    approval_action_roles_with_executor, approval_actor_is_active_with_executor,
    definition_management_visibility_with_executor,
};
use mongodb::Database;
use persistence_core::{Executor, NoTransaction};

use super::InventoryAdjustmentProcess;
use crate::adapters::workflow::workflow_auth;
use crate::{Error, Result};

/// 普通撤回的授权身份，用于审计应急代办路径。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum CancelAuthority {
    Submitter,
    RuntimeAdmin,
}

/// 事务内重新证明的普通撤回授权事实。
pub(crate) struct CancelAuthorization {
    pub(crate) authority: CancelAuthority,
    pub(crate) submitted_by: String,
    pub(crate) responsible_org_id: String,
}

impl CancelAuthority {
    /// 返回撤回授权身份的展示名称。
    ///
    /// # 参数
    /// 无。
    ///
    /// # 返回
    /// 返回原提交人或审批管理员的中文名称。
    ///
    /// # 错误
    /// 不返回错误。
    pub(crate) fn label(self) -> &'static str {
        match self {
            Self::Submitter => "原提交人",
            Self::RuntimeAdmin => "审批管理员",
        }
    }
}

/// 校验普通撤回的账号、动作权限、对象读取范围和提交人/运行管理员身份。
///
/// # 参数
/// * `service` - 库存调整流程服务。
/// * `instance` - 待撤回的审批实例。
/// * `actor` - 当前认证操作人。
///
/// # 返回
/// 返回原提交人或运行管理员的授权事实。
///
/// # 错误
/// 与 `ensure_cancel_authorized_with_executor` 相同；本函数使用非事务快照。
pub(crate) async fn ensure_cancel_authorized(
    service: &InventoryAdjustmentProcess,
    instance: &ApprovalProcessInstance,
    actor: &AuditActor,
) -> Result<CancelAuthorization> {
    ensure_cancel_authorized_with_executor(&service.db, &service.rbac, instance, actor, &mut NoTransaction)
        .await
}

/// 在调用方事务快照内重新校验普通撤回授权。
///
/// # 参数
/// * `db` - 快照与授权所在数据库。
/// * `rbac` - 动作权限与对象读取使用的 RBAC。
/// * `instance` - 待撤回的审批实例。
/// * `actor` - 当前认证操作人。
/// * `executor` - 调用方数据库快照。
///
/// # 返回
/// 原提交人返回 `Submitter`；具备运行管理员可见性且对象可读时返回 `RuntimeAdmin`。
///
/// # 错误
/// 账号未激活、缺少撤回或读取权限、不是原提交人且不是运行管理员或对象不可读时返回 `Forbidden`。缺少或对不上冻结快照时返回 `ConflictError`。授权或快照读取失败时返回对应错误。
pub(crate) async fn ensure_cancel_authorized_with_executor(
    db: &Database,
    rbac: &SharedRbacService,
    instance: &ApprovalProcessInstance,
    actor: &AuditActor,
    executor: &mut dyn Executor,
) -> Result<CancelAuthorization> {
    if !approval_actor_is_active_with_executor(&workflow_auth(db.clone(), rbac.clone()), actor, executor)
        .await?
    {
        return Err(Error::Forbidden("当前账号不可执行库存调整审批撤回".to_string()));
    }
    let snapshot = cancel_authorization_snapshot(db, instance, executor).await?;
    let auth = workflow_auth(db.clone(), rbac.clone());
    let roles =
        approval_action_roles_with_executor(&auth, actor, "approval_instance:cancel", executor).await?;
    let read_roles =
        approval_action_roles_with_executor(&auth, actor, "approval_instance:read", executor).await?;
    if roles.is_empty() || read_roles.is_empty() {
        return Err(Error::Forbidden("当前账号缺少撤回审批或读取权限".to_string()));
    }
    if snapshot.payload.submitted_by == actor.id() {
        return Ok(CancelAuthorization {
            authority: CancelAuthority::Submitter,
            submitted_by: snapshot.payload.submitted_by,
            responsible_org_id: snapshot.payload.responsible_org_id,
        });
    }
    let visibility = definition_management_visibility_with_executor(
        &workflow_auth(db.clone(), rbac.clone()),
        actor,
        executor,
    )
    .await?;
    if !visibility.runtime_admin_types().contains(&DocumentType::StockAdjustment)
        || !auth
            .approval_source_readable(
                actor,
                DocumentType::StockAdjustment,
                &snapshot.business_object_id,
                executor,
            )
            .await?
    {
        return Err(Error::Forbidden("只有原提交人或库存调整审批运行管理员可以撤回".to_string()));
    }
    Ok(CancelAuthorization {
        authority: CancelAuthority::RuntimeAdmin,
        submitted_by: snapshot.payload.submitted_by,
        responsible_org_id: snapshot.payload.responsible_org_id,
    })
}

/// 在原授权步骤的快照读取位置，验证冻结业务对象仍对应同一运行实例。
async fn cancel_authorization_snapshot(
    db: &Database,
    instance: &ApprovalProcessInstance,
    executor: &mut dyn Executor,
) -> Result<ApprovalSubjectSnapshot> {
    let snapshot = db
        .approval_subject_snapshots()
        .find_by_process_instance_id(&instance.base.id, executor)
        .await?
        .ok_or_else(|| Error::ConflictError("审批实例缺少冻结业务快照".to_string()))?;
    snapshot
        .ensure_matches_runtime_subject(
            DocumentType::StockAdjustment,
            instance.subject.subject_id(),
            instance.subject_version,
        )
        .map_err(|_| Error::ConflictError("审批实例与冻结业务快照不一致".to_string()))?;
    Ok(snapshot)
}
