//! 京东页面授权边界；敏感正文使用固定错误，不进入通用字段路径诊断。
pub mod tokens;
use std::time::Duration;

use application_core::AuditActor;
use axum::Extension;
use axum::extract::{DefaultBodyLimit, FromRequest, Path, Request, State};
use axum::http::StatusCode;
use axum::middleware::{self, Next};
use axum::response::{IntoResponse, Response};
use axum::routing::MethodRouter;
use erp_supplier_orders::supplier_credentials::authorization::{
    AuthorizationError, JdVopAuthorizationRequest, JdVopAuthorizationStatus, JdVopAuthorizationStatusQuery,
};

use crate::http::errors::Error;
use crate::http::extractor::Query;
use crate::http::rate_limit::RateLimiter;
use crate::http::response::ApiResponse;
use crate::http::state::HttpState;

/// 仅供京东授权使用的敏感JSON提取器；不格式化底层错误或动态字段名。
pub struct AuthorizationJson(pub JdVopAuthorizationRequest);

impl<S: Send + Sync> FromRequest<S> for AuthorizationJson {
    type Rejection = Response;
    async fn from_request(request: Request, state: &S) -> std::result::Result<Self, Self::Rejection> {
        axum::Json::<JdVopAuthorizationRequest>::from_request(request, state)
            .await
            .map(|value| Self(value.0))
            .map_err(|error| invalid_body(error.status()))
    }
}

fn invalid_body(status: StatusCode) -> Response {
    let (status, code, message) = match status {
        StatusCode::PAYLOAD_TOO_LARGE => (
            StatusCode::PAYLOAD_TOO_LARGE,
            "REQUEST_BODY_TOO_LARGE",
            "授权资料超过48 KiB，请核对私钥文件大小",
        ),
        StatusCode::UNSUPPORTED_MEDIA_TYPE => (
            StatusCode::UNSUPPORTED_MEDIA_TYPE,
            "REQUEST_CONTENT_TYPE_INVALID",
            "授权资料格式不正确，请重新提交",
        ),
        _ => (
            StatusCode::BAD_REQUEST,
            "INVALID_REQUEST",
            "授权资料格式不正确，请核对账号、密码、回调地址和私钥文件",
        ),
    };
    rejection(status, code, message)
}

/// 页面授权专用静态错误，不承载账号、密码、私钥或供应商正文。
#[derive(Debug)]
pub struct AuthorizationHttpError(AuthorizationError);

impl From<AuthorizationError> for AuthorizationHttpError {
    // 保留授权分类，不复制任何底层错误正文。
    // `error` 为只有固定分类的授权用例错误。
    // 可转换为固定HTTP状态及错误码的错误。
    fn from(error: AuthorizationError) -> Self {
        Self(error)
    }
}

impl IntoResponse for AuthorizationHttpError {
    // 将授权错误转换为脱敏业务信封。
    // 固定HTTP状态、错误码及静态提示。
    fn into_response(self) -> Response {
        let (status, code) = match self.0 {
            AuthorizationError::InvalidInput => (StatusCode::BAD_REQUEST, "JD_VOP_AUTH_INPUT_INVALID"),
            AuthorizationError::InvalidTokens => (StatusCode::BAD_REQUEST, "JD_VOP_TOKEN_INPUT_INVALID"),
            AuthorizationError::Forbidden => (StatusCode::FORBIDDEN, "JD_VOP_AUTH_FORBIDDEN"),
            AuthorizationError::Conflict => (StatusCode::CONFLICT, "JD_VOP_AUTH_CONFLICT"),
            AuthorizationError::Busy => (StatusCode::CONFLICT, "JD_VOP_AUTH_BUSY"),
            AuthorizationError::IdentityMismatch => {
                (StatusCode::UNPROCESSABLE_ENTITY, "JD_VOP_AUTH_IDENTITY_MISMATCH")
            },
            AuthorizationError::BindingMismatch => {
                (StatusCode::UNPROCESSABLE_ENTITY, "JD_VOP_AUTH_BINDING_MISMATCH")
            },
            AuthorizationError::Unavailable => (StatusCode::SERVICE_UNAVAILABLE, "JD_VOP_AUTH_UNAVAILABLE"),
            AuthorizationError::Unknown => (StatusCode::UNPROCESSABLE_ENTITY, "JD_VOP_AUTH_RESULT_UNKNOWN"),
            AuthorizationError::Rejected => (StatusCode::UNPROCESSABLE_ENTITY, "JD_VOP_AUTH_REJECTED"),
        };
        rejection(status, code, &self.0.to_string())
    }
}

fn rejection(status: StatusCode, code: &str, message: &str) -> Response {
    ApiResponse::<()> {
        status: status.as_u16(),
        message: message.into(),
        code: Some(code.into()),
        required_permission: None,
        field_errors: None,
        retryable: Some(false),
        data: None,
        success: false,
    }
    .into_response()
}

// 在读取敏感 JSON 前实施授权准入：每人 5 次/分钟、进程总 20 次/分钟、并发 4、累计正文最多 48 KiB。缺少认证、配额耗尽或正文超限时返回固定错误。
pub(crate) fn authorize_route<S: Clone + Send + Sync + 'static>(route: MethodRouter<S>) -> MethodRouter<S> {
    route.layer(DefaultBodyLimit::max(48 * 1024)).route_layer(middleware::from_fn_with_state(
        RateLimiter::new(5, 20, Duration::from_secs(60), 4),
        enforce_admission,
    ))
}

// 为授权状态查询设置独立配额，在解析查询参数前按账号准入：每人 60 次/分钟、进程总 1000 次/分钟、并发 20；无身份或超额时拒绝。
pub(crate) fn status_route<S: Clone + Send + Sync + 'static>(route: MethodRouter<S>) -> MethodRouter<S> {
    route.route_layer(middleware::from_fn_with_state(
        RateLimiter::new(60, 1000, Duration::from_secs(60), 20),
        enforce_admission,
    ))
}

async fn enforce_admission(State(limiter): State<RateLimiter>, request: Request, next: Next) -> Response {
    let Some(actor) = request.extensions().get::<AuditActor>() else {
        return ApiResponse::<()>::unauthorized().into_response();
    };
    let _permit = match limiter.admit(actor.id()) {
        Ok(permit) => permit,
        Err(error) => return Error::from(error).into_response(),
    };
    next.run(request).await
}

/// 查询指定京东连接的脱敏凭据状态，可用 `operation_id` 核对原授权回执；只读，不发起授权或健康检查。读取资格、对象范围、格式或配额不符时拒绝。
pub async fn status(
    State(state): State<HttpState>,
    Extension(actor): Extension<AuditActor>,
    Path(id): Path<String>,
    Query(query): Query<JdVopAuthorizationStatusQuery>,
) -> std::result::Result<ApiResponse<JdVopAuthorizationStatus>, AuthorizationHttpError> {
    query.validate()?;
    let status =
        state.jd_vop_authorization_process().status(&id, &actor, query.operation_id.as_deref()).await?;
    Ok(ApiResponse::ok_with_data(status))
}

/// 当操作人发起京东页面授权时，将连接、原操作身份和本次所需凭据交给授权流程。保存由后台任务完成，响应只返回脱敏状态；权限、范围或准入不符时使用固定错误。
pub async fn authorize(
    State(state): State<HttpState>,
    Extension(actor): Extension<AuditActor>,
    Path(id): Path<String>,
    AuthorizationJson(request): AuthorizationJson,
) -> std::result::Result<ApiResponse<JdVopAuthorizationStatus>, AuthorizationHttpError> {
    let status = state.jd_vop_authorization_process().authorize(&id, request, &actor).await?;
    Ok(ApiResponse::ok_with_data(status))
}

#[cfg(test)]
mod tests {
    use axum::Router;
    use axum::body::{Body, to_bytes};
    use axum::http::StatusCode;
    use axum::http::header::RETRY_AFTER;
    use axum::response::IntoResponse;
    use axum::routing::{get, post};
    use erp_core::AccountKind;
    use serde_json::{Value, json};
    use tower::ServiceExt;

    use super::*;

    const OPERATION_ID: &str = "26c4dfba-fda5-490d-9d46-d88da680f51d";

    fn valid_body() -> String {
        json!({
            "operation_id": OPERATION_ID,
            "username": "offline-account",
            "password": "private-password-sentinel",
            "private_key": "private-key-sentinel",
            "redirect_uri": "https://callback.example/verified"
        })
        .to_string()
    }

    fn request(method: &str, uri: &str, body: impl Into<Body>, actor: Option<&str>) -> Request {
        let mut request = Request::builder()
            .method(method)
            .uri(uri)
            .header("content-type", "application/json")
            .body(body.into())
            .unwrap();
        if let Some(actor) = actor {
            request.extensions_mut().insert(AuditActor::new(actor.into(), actor.into(), AccountKind::Admin));
        }
        request
    }

    async fn accept(AuthorizationJson(request): AuthorizationJson) -> StatusCode {
        assert_eq!(request.operation_id, OPERATION_ID);
        assert_eq!(request.username.as_str(), "offline-account");
        StatusCode::NO_CONTENT
    }

    async fn query_status(
        Query(query): Query<JdVopAuthorizationStatusQuery>,
    ) -> std::result::Result<ApiResponse<()>, AuthorizationHttpError> {
        query.validate()?;
        Ok(ApiResponse::ok())
    }

    fn routes() -> Router {
        Router::new()
            .route("/connections/{id}", status_route(get(query_status)).merge(authorize_route(post(accept))))
    }

    async fn assert_safe_error(response: Response, expected: StatusCode) -> Value {
        assert_eq!(response.status(), expected);
        let body = to_bytes(response.into_body(), 8192).await.unwrap();
        let text = String::from_utf8(body.to_vec()).unwrap();
        assert!(!text.contains("private-password-sentinel"));
        assert!(!text.contains("private-key-sentinel"));
        let body: Value = serde_json::from_str(&text).unwrap();
        assert_eq!(body["status"], expected.as_u16());
        assert_eq!(body["success"], false);
        assert!(body.get("fieldErrors").is_none());
        body
    }

    #[tokio::test]
    async fn malformed_authorization_json_never_exposes_field_names_values_or_parse_diagnostics() {
        for body in [
            r#"{"private-password-sentinel":"private-key-sentinel"}"#,
            r#"{"operation_id":"x","password":{"private-password-sentinel":true}}"#,
            "private-key-sentinel",
        ] {
            let request = Request::builder()
                .method("POST")
                .header("content-type", "application/json")
                .body(Body::from(body))
                .unwrap();
            let error = AuthorizationJson::from_request(request, &()).await.err().unwrap();
            let response = error.into_response();
            assert_eq!(response.status(), StatusCode::BAD_REQUEST);
            let body = to_bytes(response.into_body(), 8192).await.unwrap();
            let body = String::from_utf8(body.to_vec()).unwrap();
            assert!(!body.contains("private-password-sentinel") && !body.contains("private-key-sentinel"));
            assert!(!body.contains("fieldErrors"));
        }
    }

    #[tokio::test]
    async fn http_boundary_accepts_valid_sensitive_json_and_rejects_unsafe_shapes() {
        let router = routes();
        let response = router
            .clone()
            .oneshot(request("POST", "/connections/one", valid_body(), Some("operator")))
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::NO_CONTENT);
        for body in [
            "private-password-sentinel".into(),
            r#"{"private-password-sentinel":{"private-key-sentinel":true}}"#.into(),
            valid_body().replace(
                "\"password\":\"private-password-sentinel\"",
                "\"password\":{\"private-key-sentinel\":true}",
            ),
        ] {
            let response = router
                .clone()
                .oneshot(request("POST", "/connections/one", body, Some("operator")))
                .await
                .unwrap();
            assert_safe_error(response, StatusCode::BAD_REQUEST).await;
        }
        let mut request = request("POST", "/connections/two", valid_body(), Some("operator"));
        request.headers_mut().remove("content-type");
        assert_safe_error(router.oneshot(request).await.unwrap(), StatusCode::UNSUPPORTED_MEDIA_TYPE).await;
    }

    #[tokio::test]
    async fn http_boundary_enforces_exact_body_limit_without_echoing_input() {
        let mut body = valid_body().into_bytes();
        body.resize(48 * 1024, b' ');
        let response = routes()
            .oneshot(request("POST", "/connections/one", body.clone(), Some("operator")))
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::NO_CONTENT);
        body.push(b' ');
        let response =
            routes().oneshot(request("POST", "/connections/one", body, Some("operator"))).await.unwrap();
        let body = assert_safe_error(response, StatusCode::PAYLOAD_TOO_LARGE).await;
        assert_eq!(body["code"], "REQUEST_BODY_TOO_LARGE");
    }

    #[tokio::test]
    async fn unauthenticated_request_is_rejected_before_sensitive_body_parsing() {
        let response = routes()
            .oneshot(request("POST", "/connections/one", "private-password-sentinel", None))
            .await
            .unwrap();
        let body = assert_safe_error(response, StatusCode::UNAUTHORIZED).await;
        assert_eq!(body["code"], "UNAUTHENTICATED");
    }

    #[tokio::test]
    async fn authorization_rate_limit_counts_rejected_bodies_and_rotating_connections() {
        let router = routes();
        for index in 0..5 {
            let response = router
                .clone()
                .oneshot(request("POST", &format!("/connections/{index}"), "{", Some("operator")))
                .await
                .unwrap();
            assert_safe_error(response, StatusCode::BAD_REQUEST).await;
        }
        let response = router
            .oneshot(request("POST", "/connections/another", valid_body(), Some("operator")))
            .await
            .unwrap();
        assert!(response.headers().get(RETRY_AFTER).is_some());
        assert_safe_error(response, StatusCode::TOO_MANY_REQUESTS).await;
    }

    #[tokio::test]
    async fn authorization_global_limit_counts_distinct_actors() {
        let router = routes();
        for index in 0..20 {
            let response = router
                .clone()
                .oneshot(request("POST", "/connections/one", "{", Some(&format!("operator-{}", index / 5))))
                .await
                .unwrap();
            assert_safe_error(response, StatusCode::BAD_REQUEST).await;
        }
        let response = router
            .oneshot(request("POST", "/connections/one", valid_body(), Some("fresh-operator")))
            .await
            .unwrap();
        assert_safe_error(response, StatusCode::TOO_MANY_REQUESTS).await;
    }

    #[tokio::test]
    async fn status_limit_is_independent_and_counts_invalid_query_before_extraction() {
        let router = routes();
        for _ in 0..5 {
            let response = router
                .clone()
                .oneshot(request("POST", "/connections/one", "{", Some("operator")))
                .await
                .unwrap();
            assert_safe_error(response, StatusCode::BAD_REQUEST).await;
        }
        for _ in 0..60 {
            let response = router
                .clone()
                .oneshot(request(
                    "GET",
                    "/connections/one?private-password-sentinel=private-key-sentinel",
                    Body::empty(),
                    Some("operator"),
                ))
                .await
                .unwrap();
            assert_safe_error(response, StatusCode::BAD_REQUEST).await;
        }
        let response = router
            .clone()
            .oneshot(request("GET", "/connections/two", Body::empty(), Some("operator")))
            .await
            .unwrap();
        assert_safe_error(response, StatusCode::TOO_MANY_REQUESTS).await;
        let response = router
            .oneshot(request(
                "GET",
                &format!("/connections/two?operation_id={OPERATION_ID}"),
                Body::empty(),
                Some("fresh-operator"),
            ))
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn status_global_limit_counts_distinct_actors() {
        let router = routes();
        for index in 0..1000 {
            let response = router
                .clone()
                .oneshot(request(
                    "GET",
                    "/connections/one",
                    Body::empty(),
                    Some(&format!("operator-{}", index / 60)),
                ))
                .await
                .unwrap();
            assert_eq!(response.status(), StatusCode::OK);
        }
        let response = router
            .oneshot(request("GET", "/connections/one", Body::empty(), Some("fresh-operator")))
            .await
            .unwrap();
        assert_safe_error(response, StatusCode::TOO_MANY_REQUESTS).await;
    }

    #[tokio::test]
    async fn authorization_error_responses_keep_identity_mismatch_code_and_real_http_statuses() {
        for (error, expected, code) in [
            (AuthorizationError::InvalidInput, StatusCode::BAD_REQUEST, "JD_VOP_AUTH_INPUT_INVALID"),
            (AuthorizationError::Forbidden, StatusCode::FORBIDDEN, "JD_VOP_AUTH_FORBIDDEN"),
            (AuthorizationError::Conflict, StatusCode::CONFLICT, "JD_VOP_AUTH_CONFLICT"),
            (AuthorizationError::Busy, StatusCode::CONFLICT, "JD_VOP_AUTH_BUSY"),
            (
                AuthorizationError::IdentityMismatch,
                StatusCode::UNPROCESSABLE_ENTITY,
                "JD_VOP_AUTH_IDENTITY_MISMATCH",
            ),
            (
                AuthorizationError::BindingMismatch,
                StatusCode::UNPROCESSABLE_ENTITY,
                "JD_VOP_AUTH_BINDING_MISMATCH",
            ),
            (AuthorizationError::Unavailable, StatusCode::SERVICE_UNAVAILABLE, "JD_VOP_AUTH_UNAVAILABLE"),
            (AuthorizationError::Unknown, StatusCode::UNPROCESSABLE_ENTITY, "JD_VOP_AUTH_RESULT_UNKNOWN"),
            (AuthorizationError::Rejected, StatusCode::UNPROCESSABLE_ENTITY, "JD_VOP_AUTH_REJECTED"),
        ] {
            let router = Router::new().route(
                "/",
                post(move || async move { Err::<ApiResponse<()>, _>(AuthorizationHttpError::from(error)) }),
            );
            let response = router.oneshot(request("POST", "/", Body::empty(), None)).await.unwrap();
            let body = assert_safe_error(response, expected).await;
            assert_eq!(body["code"], code);
        }
    }
}
