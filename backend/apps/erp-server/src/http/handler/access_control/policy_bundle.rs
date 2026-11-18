//! 授权 JSON 配置入口，领域用例负责全部授权与事务。
use application_core::AuditActor;
use axum::Extension;
use axum::extract::State;
use erp_identity::Result as IdentityResult;
use erp_identity::dto::authorization_bundle::{
    ApplyPolicyRequest, ExportPolicyRequest, PolicyApplyResult, PolicyExport, PolicyPreview,
};
use erp_identity::entity::authorization_bundle::PolicyDocument;
use erp_identity::service::access_control::policy_bundle::PolicyBundleService;
use erp_permissions::Permission;

use crate::http::errors::Result;
use crate::http::extractor::Json;
use crate::http::response::ApiResponse;
use crate::http::state::HttpState;
use crate::wiring::scope_configuration;

include!(concat!(env!("OUT_DIR"), "/policy_permissions.rs"));

// 为权限配置文件读写接入生成目录中的可配置权限，以及解析与应用所需的领域端口。
fn service(state: &HttpState) -> IdentityResult<PolicyBundleService> {
    let catalog =
        POLICY_PERMISSION_CODES.iter().map(Permission::parse).collect::<std::result::Result<Vec<_>, _>>()?;
    Ok(PolicyBundleService::new(
        scope_configuration(
            state.application().db(),
            state.application().rbac(),
            state.application().authorization(),
        ),
        catalog,
    ))
}

/// 只读校验结构、当前目标和授权能力。
/// 规范化文件及完整预览。
/// 无效输入、越权或读取失败使用统一错误信封。
pub async fn validate(
    State(state): State<HttpState>,
    Extension(actor): Extension<AuditActor>,
    Json(document): Json<PolicyDocument>,
) -> Result<PolicyPreview> {
    Ok(ApiResponse::ok_with_data(service(&state)?.preview(document, actor).await?))
}

/// 只读生成绑定当前事实和操作人的审核计划。
/// 原始差异、版本和审核摘要。
/// 无效输入、越权或读取失败使用统一错误信封。
pub async fn preview(
    State(state): State<HttpState>,
    Extension(actor): Extension<AuditActor>,
    Json(document): Json<PolicyDocument>,
) -> Result<PolicyPreview> {
    Ok(ApiResponse::ok_with_data(service(&state)?.preview(document, actor).await?))
}

/// 应用已审核文件，重验事实后在同一授权事务提交。
/// 原子应用结果或原命令回执。
/// 越权、冲突或写入失败使用统一错误信封。
pub async fn apply(
    State(state): State<HttpState>,
    Extension(actor): Extension<AuditActor>,
    Json(request): Json<ApplyPolicyRequest>,
) -> Result<PolicyApplyResult> {
    Ok(ApiResponse::ok_with_data(service(&state)?.apply(request, actor).await?))
}

/// 导出明确选择的人员及角色。
/// 配置文件、授权版本及系统政策说明。
/// 越权、无效选择或无法无损导出时拒绝。
pub async fn export(
    State(state): State<HttpState>,
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

    // 用生产 JSON 提取器检验权限配置文件，断言未知授权字段与不合法版本不能进入业务流程。
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

#[cfg(test)]
mod catalog_tests {
    use super::POLICY_PERMISSION_CODES;

    #[test]
    fn generated_policy_whitelist_matches_the_frozen_baseline() {
        let expected: Vec<String> =
            serde_json::from_str(include_str!("../../../../testdata/policy-permissions.json")).unwrap();
        assert_eq!(POLICY_PERMISSION_CODES, expected);
    }
}
