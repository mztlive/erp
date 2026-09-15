//! 内部组织协议适配；组织授权和变更校验由身份域执行。

use application_core::AuditActor;
use axum::extract::State;
use axum::{Extension, Json};
use erp_identity::OrganizationStateView;
use erp_identity::entity::organization_change::{OrganizationChangeReceipt, OrganizationChangeRequest};
use erp_processes::adapters::organization_service;

use crate::app_state::AppState;
use crate::core::errors::Result;
use crate::core::response::ApiResponse;

/// 查询组织管理范围内的组织和关系。
///
/// # 参数
/// * `state` - 应用状态
/// * `actor` - 已认证操作人
///
/// # 返回
/// 配置边界内的组织事实，以及范围版本、空集原因和人员/角色标签。
///
/// # 错误
/// 无权限及状态读取错误沿用统一响应。
///
/// # 关键业务约束
/// 缺范围返回空集并标记 `no_scope`；组织配置权不授予业务执行权。
#[permission_macros::permission(
    group = "组织管理",
    group_desc = "内部组织、成员与管理关系",
    desc = "查询组织",
    resource = "org_unit",
    action = "list"
)]
pub async fn list(
    State(state): State<AppState>,
    Extension(actor): Extension<AuditActor>,
) -> Result<OrganizationStateView> {
    Ok(ApiResponse::ok_with_data(organization_service(state.db(), state.rbac()).state(&actor).await?))
}

/// 预览组织变更影响，提交前不写入任何关系。
///
/// # 参数
/// * `state` - 应用状态
/// * `actor` - 已认证操作人
/// * `request` - 含 `expected_version`、幂等键、原因和固定 `change` 命令
///
/// # 返回
/// 变更前后组织事实；不写入关系、不改派任务。
///
/// # 错误
/// 版本、边界或业务约束失败时返回统一错误。
///
/// # 关键业务约束
/// 组织配置权不授予业务执行权；部门负责人身份不代替 `org_unit:manage`。
#[permission_macros::permission(
    group = "组织管理",
    group_desc = "内部组织、成员与管理关系",
    desc = "管理组织",
    resource = "org_unit",
    action = "manage"
)]
pub async fn preview(
    State(state): State<AppState>,
    Extension(actor): Extension<AuditActor>,
    Json(request): Json<OrganizationChangeRequest>,
) -> Result<OrganizationChangeReceipt> {
    Ok(ApiResponse::ok_with_data(
        organization_service(state.db(), state.rbac()).preview(&actor, request).await?,
    ))
}

/// 提交已预览的组织变更，事务内独立重验版本和权限。
///
/// # 参数
/// * `state` - 应用状态
/// * `actor` - 已认证操作人
/// * `request` - 与预览相同的命令，必须重验 `expected_version`
///
/// # 返回
/// 写入后的变更回执。
///
/// # 错误
/// 冲突及越权不产生部分变更。
///
/// # 关键业务约束
/// 事务内重验授权、版本、成员及管理关系资格；不自动改派任务。
#[permission_macros::permission(
    group = "组织管理",
    group_desc = "内部组织、成员与管理关系",
    desc = "管理组织",
    resource = "org_unit",
    action = "manage"
)]
pub async fn change(
    State(state): State<AppState>,
    Extension(actor): Extension<AuditActor>,
    Json(request): Json<OrganizationChangeRequest>,
) -> Result<OrganizationChangeReceipt> {
    Ok(ApiResponse::ok_with_data(
        organization_service(state.db(), state.rbac()).change(&actor, request).await?,
    ))
}
