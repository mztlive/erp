//! 人员资料页的业务部门授权入口。
use application_core::AuditActor;
use axum::extract::{Path, State};
use axum::{Extension, Json};
use erp_identity::dto::personal_grant::{
    CreatePersonalGrantRequest, PersonalGrantListView, RevokePersonalGrantRequest,
};
use erp_identity::entity::access_control::personal_grant::PersonalBusinessGrant;
use erp_processes::adapters::scope_configuration;

use crate::app_state::AppState;
use crate::core::errors::Result;
use crate::core::response::ApiResponse;

#[permission_macros::permission(
    group = "权限与审计",
    group_desc = "权限目录、数据范围、用户授权与审计查询",
    desc = "查看数据范围",
    resource = "data_scope",
    action = "list"
)]
/// 查询固定人员的部门扩展授权。
/// # 参数
/// 路径人员、当前身份和应用依赖。
/// # 返回
/// 授权及保存版本。
/// # 错误
/// 权限和读取错误使用统一信封。
pub async fn list(
    State(state): State<AppState>,
    Extension(actor): Extension<AuditActor>,
    Path(user_id): Path<String>,
) -> Result<PersonalGrantListView> {
    Ok(ApiResponse::ok_with_data(
        scope_configuration(state.db(), state.rbac()).personal_grants(&user_id, &actor).await?,
    ))
}

#[permission_macros::permission(
    group = "权限与审计",
    group_desc = "权限目录、数据范围、用户授权与审计查询",
    desc = "创建数据范围",
    resource = "data_scope",
    action = "create"
)]
/// 为固定人员扩大明确业务动作的部门范围。
/// # 参数
/// 人员、已选配置和期望策略版本。
/// # 返回
/// 保存后的个人授权。
/// # 错误
/// 失效角色、缺动作、非法目标和版本冲突拒绝。
pub async fn create(
    State(state): State<AppState>,
    Extension(actor): Extension<AuditActor>,
    Path(user_id): Path<String>,
    Json(req): Json<CreatePersonalGrantRequest>,
) -> Result<PersonalBusinessGrant> {
    Ok(ApiResponse::ok_with_data(
        scope_configuration(state.db(), state.rbac()).create_personal_grant(&user_id, req, &actor).await?,
    ))
}

#[permission_macros::permission(
    group = "权限与审计",
    group_desc = "权限目录、数据范围、用户授权与审计查询",
    desc = "删除数据范围",
    resource = "data_scope",
    action = "delete"
)]
/// 撤销固定人员的一条附加授权。
/// # 参数
/// 人员与授权身份、版本及当前身份。
/// # 返回
/// 撤销完成。
/// # 错误
/// 越权、冲突或重复撤销拒绝。
pub async fn revoke(
    State(state): State<AppState>,
    Extension(actor): Extension<AuditActor>,
    Path((user_id, id)): Path<(String, String)>,
    Json(req): Json<RevokePersonalGrantRequest>,
) -> Result<()> {
    scope_configuration(state.db(), state.rbac()).revoke_personal_grant(&user_id, &id, req, &actor).await?;
    Ok(ApiResponse::ok_with_data(()))
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;
    #[test]
    fn grant_http_commands_require_versions_and_reject_unknown_fields() {
        let mut request = json!({"grant": {"role_id":"sales", "resource":"sales_order", "actions":["list"], "org_unit_ids":["one"], "include_descendants":false}, "expected_policy_version":1});
        assert!(serde_json::from_value::<CreatePersonalGrantRequest>(request.clone()).is_ok());
        request["force"] = true.into();
        assert!(serde_json::from_value::<CreatePersonalGrantRequest>(request).is_err());
        assert!(serde_json::from_value::<RevokePersonalGrantRequest>(json!({"version":1})).is_err());
    }
}
