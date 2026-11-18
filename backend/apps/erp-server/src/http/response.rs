use std::collections::BTreeMap;

use axum::Json;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use erp_identity::Permission;
use serde::Serialize;

#[derive(Debug, Serialize)]
pub struct ApiResponse<T> {
    pub status: u16,
    #[serde(rename = "errorMessage")]
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub code: Option<String>,
    #[serde(rename = "requiredPermission", skip_serializing_if = "Option::is_none")]
    pub required_permission: Option<String>,
    #[serde(rename = "fieldErrors", skip_serializing_if = "Option::is_none")]
    pub field_errors: Option<BTreeMap<String, String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub retryable: Option<bool>,
    pub data: Option<T>,
    pub success: bool,
}

impl<T> IntoResponse for ApiResponse<T>
where
    T: Serialize,
{
    // 把业务信封序列化为 JSON，并用其状态码作为真实 HTTP 状态；无法转换的状态回退到 HTTP 500。
    fn into_response(self) -> Response {
        let http_status = StatusCode::from_u16(self.status).unwrap_or(StatusCode::INTERNAL_SERVER_ERROR);
        (http_status, Json(self)).into_response()
    }
}

impl ApiResponse<()> {
    /// 用于已完成且没有业务数据的操作，返回 200 成功信封。
    pub fn ok() -> Self {
        Self {
            status: 200,
            message: "OK".to_string(),
            code: None,
            required_permission: None,
            field_errors: None,
            retryable: None,
            data: None,
            success: true,
        }
    }

    /// 在请求身份缺失或失效时返回 401 与 `UNAUTHENTICATED`，提示重新登录。
    pub fn unauthorized() -> Self {
        Self {
            status: 401,
            message: "登录状态已失效，请重新登录".to_string(),
            code: Some("UNAUTHENTICATED".to_string()),
            required_permission: None,
            field_errors: None,
            retryable: Some(false),
            data: None,
            success: false,
        }
    }

    /// 在 HTTP 层只能公开通用失败时返回 500 与固定提示，不附底层错误细节。
    pub fn system_error() -> Self {
        Self {
            status: 500,
            message: "系统暂时无法完成操作，请稍后重试；如仍失败，请联系支持人员".to_string(),
            code: Some("INTERNAL_ERROR".to_string()),
            required_permission: None,
            field_errors: None,
            retryable: Some(true),
            data: None,
            success: false,
        }
    }

    /// 请求身份有效但操作未获准时返回 403；在未知具体权限码时不填 `requiredPermission`。
    pub fn permission_denied() -> Self {
        Self {
            status: 403,
            message: "当前账号没有执行此操作的权限，请联系管理员或有权限的同事".to_string(),
            code: Some("PERMISSION_DENIED".to_string()),
            required_permission: None,
            field_errors: None,
            retryable: Some(false),
            data: None,
            success: false,
        }
    }
    /// 缺少可明确标识的功能权限时，在 403 `PERMISSION_DENIED` 上追加 `requiredPermission`，便于管理员定位所需权限。
    pub fn missing_permission(permission: &Permission) -> Self {
        Self {
            message: format!("缺少功能权限：{permission}，请联系管理员授权后重试"),
            required_permission: Some(permission.to_string()),
            ..Self::permission_denied()
        }
    }
}

impl<T> ApiResponse<T> {
    /// 把已成功计算的业务视图放入统一 200 响应的 `data` 字段。
    pub fn ok_with_data(data: T) -> Self {
        Self {
            status: 200,
            message: "OK".to_string(),
            code: None,
            required_permission: None,
            field_errors: None,
            retryable: None,
            data: Some(data),
            success: true,
        }
    }
}

#[cfg(test)]
mod tests {
    use axum::body::to_bytes;
    use axum::http::StatusCode;
    use axum::response::IntoResponse;
    use serde_json::{Value, json};

    use super::ApiResponse;

    #[tokio::test]
    async fn missing_permission_is_explicit_and_generic_denial_does_not_invent_one() {
        let permission = erp_identity::Permission::parse("sales_order:list").unwrap();
        let response = ApiResponse::missing_permission(&permission).into_response();
        assert_eq!(response.status(), StatusCode::FORBIDDEN);
        let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
        let body: Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(body["requiredPermission"], "sales_order:list");
        assert_eq!(body["code"], "PERMISSION_DENIED");
        assert_eq!(body["success"], false);
        let generic = serde_json::to_value(ApiResponse::permission_denied()).unwrap();
        assert!(generic.get("requiredPermission").is_none());
    }

    #[test]
    fn constructors_return_responses_directly() {
        let response = ApiResponse::ok_with_data("value");

        assert_eq!(response.status, 200);
        assert_eq!(response.data, Some("value"));
        assert!(response.success);
    }

    #[test]
    fn error_responses_use_real_http_statuses() {
        let cases = [
            (ApiResponse::<()>::unauthorized(), StatusCode::UNAUTHORIZED),
            (ApiResponse::<()>::permission_denied(), StatusCode::FORBIDDEN),
            (ApiResponse::<()>::system_error(), StatusCode::INTERNAL_SERVER_ERROR),
        ];

        for (response, expected_status) in cases {
            assert_eq!(response.into_response().status(), expected_status);
        }
    }

    #[tokio::test]
    async fn response_keeps_existing_json_fields() {
        let response = ApiResponse::<()>::permission_denied().into_response();
        let body =
            to_bytes(response.into_body(), usize::MAX).await.expect("response body should be readable");
        let body: Value = serde_json::from_slice(&body).expect("response body should be valid JSON");

        assert_eq!(
            body,
            json!({
                "status": 403,
                "errorMessage": "当前账号没有执行此操作的权限，请联系管理员或有权限的同事",
                "code": "PERMISSION_DENIED",
                "retryable": false,
                "data": null,
                "success": false
            })
        );
    }
}
