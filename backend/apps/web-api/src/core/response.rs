use axum::{
    http::StatusCode,
    response::{IntoResponse, Response},
    Json,
};
use serde::Serialize;

#[derive(Debug, Serialize)]
pub struct ApiResponse<T> {
    pub status: u16,
    #[serde(rename = "errorMessage")]
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub code: Option<String>,
    pub data: Option<T>,
    pub success: bool,
}

impl<T> IntoResponse for ApiResponse<T>
where
    T: Serialize,
{
    /// 转换为 HTTP 响应。
    ///
    /// # 返回
    /// 返回 `axum::response::Response` 实例。
    fn into_response(self) -> Response {
        let http_status = StatusCode::from_u16(self.status).unwrap_or(StatusCode::INTERNAL_SERVER_ERROR);
        (http_status, Json(self)).into_response()
    }
}

impl ApiResponse<()> {
    /// 构造成功响应。
    ///
    /// # 返回
    /// 返回成功响应。
    pub fn ok() -> Self {
        Self {
            status: 200,
            message: "OK".to_string(),
            code: None,
            data: None,
            success: true,
        }
    }

    /// 构造未授权响应。
    ///
    /// # 返回
    /// 返回未认证响应。
    pub fn unauthorized() -> Self {
        Self {
            status: 401,
            message: "登录状态已失效，请重新登录".to_string(),
            code: Some("UNAUTHENTICATED".to_string()),
            data: None,
            success: false,
        }
    }

    /// 构造系统错误响应。
    ///
    /// # 返回
    /// 返回不包含底层错误细节的系统错误响应。
    pub fn system_error() -> Self {
        Self {
            status: 500,
            message: "系统内部错误".to_string(),
            code: Some("INTERNAL_ERROR".to_string()),
            data: None,
            success: false,
        }
    }

    /// 构造无权限响应。
    ///
    /// # 返回
    /// 返回无权限响应。
    pub fn permission_denied() -> Self {
        Self {
            status: 403,
            message: "当前账号没有执行此操作的权限".to_string(),
            code: Some("PERMISSION_DENIED".to_string()),
            data: None,
            success: false,
        }
    }
}

impl<T> ApiResponse<T> {
    /// 构造携带数据的成功响应。
    ///
    /// # 参数
    /// * `data` - 数据内容
    ///
    /// # 返回
    /// 返回携带数据的成功响应。
    pub fn ok_with_data(data: T) -> Self {
        Self {
            status: 200,
            message: "OK".to_string(),
            code: None,
            data: Some(data),
            success: true,
        }
    }
}

#[cfg(test)]
mod tests {
    use axum::{body::to_bytes, http::StatusCode, response::IntoResponse};
    use serde_json::{json, Value};

    use super::ApiResponse;

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
            (
                ApiResponse::<()>::system_error(),
                StatusCode::INTERNAL_SERVER_ERROR,
            ),
        ];

        for (response, expected_status) in cases {
            assert_eq!(response.into_response().status(), expected_status);
        }
    }

    #[tokio::test]
    async fn response_keeps_existing_json_fields() {
        let response = ApiResponse::<()>::permission_denied().into_response();
        let body = to_bytes(response.into_body(), usize::MAX)
            .await
            .expect("response body should be readable");
        let body: Value = serde_json::from_slice(&body).expect("response body should be valid JSON");

        assert_eq!(
            body,
            json!({
                "status": 403,
                "errorMessage": "当前账号没有执行此操作的权限",
                "code": "PERMISSION_DENIED",
                "data": null,
                "success": false
            })
        );
    }
}
