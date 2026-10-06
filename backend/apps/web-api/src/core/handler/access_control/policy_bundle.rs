//! 授权 JSON 配置入口，领域用例负责全部授权与事务。
use application_core::AuditActor;
use axum::extract::State;
use axum::{Extension, Json};
use erp_identity::dto::authorization_bundle::{
    ApplyPolicyRequest, ExportPolicyRequest, PolicyApplyResult, PolicyExport, PolicyPreview,
};
use erp_identity::entity::authorization_bundle::PolicyDocument;
use erp_identity::service::access_control::policy_bundle::PolicyBundleService;
use erp_identity::{Permission, Result as IdentityResult};
use erp_processes::adapters::scope_configuration;

use crate::app_state::AppState;
use crate::core::errors::Result;
use crate::core::response::ApiResponse;

include!(concat!(env!("OUT_DIR"), "/policy_permissions.rs"));

/// 从同一生成目录装配可配置权限及领域校验端口。
fn service(state: &AppState) -> IdentityResult<PolicyBundleService> {
    let catalog =
        POLICY_PERMISSION_CODES.iter().map(Permission::parse).collect::<std::result::Result<Vec<_>, _>>()?;
    Ok(PolicyBundleService::new(scope_configuration(state.db(), state.rbac()), catalog))
}

#[permission_macros::permission(
    group = "授权文件",
    group_desc = "声明式授权配置",
    desc = "校验授权文件",
    resource = "authorization_policy",
    action = "preview"
)]
/// 只读校验结构、当前目标和授权能力。
/// # 参数
/// state 为依赖，actor 为认证身份，document 为授权文件。
/// # 返回
/// 规范化文件及完整预览。
/// # 错误
/// 无效输入、越权或读取失败使用统一错误信封。
pub async fn validate(
    State(state): State<AppState>,
    Extension(actor): Extension<AuditActor>,
    Json(document): Json<PolicyDocument>,
) -> Result<PolicyPreview> {
    Ok(ApiResponse::ok_with_data(service(&state)?.preview(document, actor).await?))
}

#[permission_macros::permission(
    group = "授权文件",
    group_desc = "声明式授权配置",
    desc = "预览授权文件",
    resource = "authorization_policy",
    action = "preview"
)]
/// 只读生成绑定当前事实和操作人的审核计划。
/// # 参数
/// state 为依赖，actor 为认证身份，document 为授权文件。
/// # 返回
/// 原始差异、版本和审核摘要。
/// # 错误
/// 无效输入、越权或读取失败使用统一错误信封。
pub async fn preview(
    State(state): State<AppState>,
    Extension(actor): Extension<AuditActor>,
    Json(document): Json<PolicyDocument>,
) -> Result<PolicyPreview> {
    Ok(ApiResponse::ok_with_data(service(&state)?.preview(document, actor).await?))
}

#[permission_macros::permission(
    group = "授权文件",
    group_desc = "声明式授权配置",
    desc = "应用授权文件",
    resource = "authorization_policy",
    action = "apply"
)]
/// 应用已审核文件，重验事实后在同一授权事务提交。
/// # 参数
/// state 为依赖，actor 为认证身份，request 为原文件及预览和命令身份。
/// # 返回
/// 原子应用结果或原命令回执。
/// # 错误
/// 越权、冲突或写入失败使用统一错误信封。
pub async fn apply(
    State(state): State<AppState>,
    Extension(actor): Extension<AuditActor>,
    Json(request): Json<ApplyPolicyRequest>,
) -> Result<PolicyApplyResult> {
    Ok(ApiResponse::ok_with_data(service(&state)?.apply(request, actor).await?))
}

#[permission_macros::permission(
    group = "授权文件",
    group_desc = "声明式授权配置",
    desc = "导出授权文件",
    resource = "authorization_policy",
    action = "export"
)]
/// 导出明确选择的人员及角色。
/// # 参数
/// state 为依赖，actor 为认证身份，request 为有界选择。
/// # 返回
/// 配置文件、授权版本及系统政策说明。
/// # 错误
/// 越权、无效选择或无法无损导出时拒绝。
pub async fn export(
    State(state): State<AppState>,
    Extension(actor): Extension<AuditActor>,
    Json(request): Json<ExportPolicyRequest>,
) -> Result<PolicyExport> {
    Ok(ApiResponse::ok_with_data(service(&state)?.export(request, actor).await?))
}

#[cfg(test)]
mod tests {
    use axum::body::Body;
    use axum::extract::FromRequest;
    use axum::http::{Request, StatusCode};

    use super::*;

    /// 执行生产请求提取器，拒绝格式外的授权字段。
    #[tokio::test]
    async fn json_extractor_rejects_unknown_policy_fields_and_bad_versions() {
        for body in [
            r#"{"version":"2.0","roles":[],"bindings":[],"data_scopes":[]}"#,
            r#"{"version":"1.0","roles":[],"bindings":[],"data_scopes":[],"effect":"deny"}"#,
            r#"{"version":"1.0","roles":[],"bindings":[{"user_id":"u","mode":"merge","role_ids":[],"effective_to":9}],"data_scopes":[]}"#,
        ] {
            let request =
                Request::builder().header("content-type", "application/json").body(Body::from(body)).unwrap();
            let rejection = Json::<PolicyDocument>::from_request(request, &()).await.unwrap_err();
            assert_eq!(rejection.status(), StatusCode::UNPROCESSABLE_ENTITY);
        }
        let request = Request::builder()
            .header("content-type", "application/json")
            .body(Body::from(r#"{"version":"1.0","roles":[],"bindings":[],"data_scopes":[]}"#))
            .unwrap();
        assert!(Json::<PolicyDocument>::from_request(request, &()).await.is_ok());
    }
}
