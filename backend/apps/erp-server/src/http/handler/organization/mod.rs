//! 内部组织协议适配；组织授权和变更校验由身份域执行。

use application_core::AuditActor;
use axum::Extension;
use axum::extract::State;
use erp_identity::OrganizationStateView;
use erp_identity::entity::organization_change::{OrganizationChangeReceipt, OrganizationChangeRequest};

use crate::http::errors::Result;
use crate::http::extractor::Json;
use crate::http::response::ApiResponse;
use crate::http::state::HttpState;
use crate::wiring::organization_service;

/// 查询组织管理范围内的组织和关系。
/// 配置边界内的组织事实，以及范围版本、空集原因和人员/角色标签。
/// 无权限及状态读取错误沿用统一响应。
///
/// 缺范围返回空集并标记 `no_scope`；组织配置权不授予业务执行权。
pub async fn list(
    State(state): State<HttpState>,
    Extension(actor): Extension<AuditActor>,
) -> Result<OrganizationStateView> {
    Ok(ApiResponse::ok_with_data(
        organization_service(
            state.application().db(),
            state.application().rbac(),
            state.application().authorization(),
        )
        .state(&actor)
        .await?,
    ))
}

/// 预览组织变更影响，提交前不写入任何关系。
/// `request` 为含 `expected_version`、幂等键、原因和固定 `change` 命令。
/// 变更前后组织事实；不写入关系、不改派任务。
/// 版本、边界或业务约束失败时返回统一错误。
///
/// 组织配置权不授予业务执行权；所属部门不代替 `org_unit:manage`。
pub async fn preview(
    State(state): State<HttpState>,
    Extension(actor): Extension<AuditActor>,
    Json(request): Json<OrganizationChangeRequest>,
) -> Result<OrganizationChangeReceipt> {
    Ok(ApiResponse::ok_with_data(
        organization_service(
            state.application().db(),
            state.application().rbac(),
            state.application().authorization(),
        )
        .preview(&actor, request)
        .await?,
    ))
}

/// 提交已预览的组织变更，事务内独立重验版本和权限。
/// `request` 为与预览相同的命令，必须重验 `expected_version`。
/// 写入后的变更回执。
/// 冲突及越权不产生部分变更。
///
/// 事务内重验授权、版本和成员资格；不自动改派任务。
pub async fn change(
    State(state): State<HttpState>,
    Extension(actor): Extension<AuditActor>,
    Json(request): Json<OrganizationChangeRequest>,
) -> Result<OrganizationChangeReceipt> {
    Ok(ApiResponse::ok_with_data(
        organization_service(
            state.application().db(),
            state.application().rbac(),
            state.application().authorization(),
        )
        .change(&actor, request)
        .await?,
    ))
}
