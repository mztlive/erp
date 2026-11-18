use erp_supplier_orders::supplier_credentials::authorization::JdVopTokenConfiguration;

use super::*;

/// 敏感令牌正文使用固定错误，禁止把解析值或字段路径送入响应及日志。
pub struct TokenJson(pub JdVopTokenConfiguration);

impl<S: Send + Sync> FromRequest<S> for TokenJson {
    type Rejection = Response;

    async fn from_request(request: Request, state: &S) -> std::result::Result<Self, Self::Rejection> {
        axum::Json::<JdVopTokenConfiguration>::from_request(request, state)
            .await
            .map(|value| Self(value.0))
            .map_err(|error| {
                if error.status() == StatusCode::PAYLOAD_TOO_LARGE {
                    rejection(
                        StatusCode::PAYLOAD_TOO_LARGE,
                        "REQUEST_BODY_TOO_LARGE",
                        "令牌资料超过48 KiB，请核对输入",
                    )
                } else {
                    AuthorizationHttpError::from(AuthorizationError::InvalidTokens).into_response()
                }
            })
    }
}

/// 保存同账号同应用的外部授权令牌，复用京东授权权限及供应商数据范围。
/// 脱敏状态和保存操作回执。
/// 格式、权限、绑定、版本或并发不符时返回固定错误。
pub async fn configure(
    State(state): State<HttpState>,
    Extension(actor): Extension<AuditActor>,
    Path(id): Path<String>,
    TokenJson(request): TokenJson,
) -> std::result::Result<ApiResponse<JdVopAuthorizationStatus>, AuthorizationHttpError> {
    let status = state.jd_vop_authorization_process().configure_tokens(&id, request, &actor).await?;
    Ok(ApiResponse::ok_with_data(status))
}

#[cfg(test)]
mod tests {
    use axum::Router;
    use axum::body::{Body, to_bytes};
    use axum::routing::post;
    use erp_core::AccountKind;
    use tower::ServiceExt;

    use super::*;

    async fn accept(TokenJson(_): TokenJson) -> StatusCode {
        StatusCode::NO_CONTENT
    }

    #[tokio::test]
    async fn malformed_tokens_are_redacted_and_admission_precedes_parsing() {
        for (body, authenticated, expected) in [
            (
                r#"{"access-token-sentinel":"refresh-token-sentinel"}"#.to_string(),
                true,
                StatusCode::BAD_REQUEST,
            ),
            (
                r#"{"access-token-sentinel":"refresh-token-sentinel"}"#.to_string(),
                false,
                StatusCode::UNAUTHORIZED,
            ),
            ("x".repeat(49 * 1024), true, StatusCode::PAYLOAD_TOO_LARGE),
        ] {
            let app = Router::new().route("/tokens", authorize_route(post(accept)));
            let mut request = Request::builder()
                .method("POST")
                .uri("/tokens")
                .header("content-type", "application/json")
                .body(Body::from(body))
                .unwrap();
            if authenticated {
                request.extensions_mut().insert(AuditActor::new(
                    "operator".into(),
                    "operator".into(),
                    AccountKind::Admin,
                ));
            }
            let response = app.oneshot(request).await.unwrap();
            assert_eq!(response.status(), expected);
            let bytes = to_bytes(response.into_body(), usize::MAX).await.unwrap();
            let text = std::str::from_utf8(&bytes).unwrap();
            assert!(!text.contains("access-token-sentinel"));
            assert!(!text.contains("refresh-token-sentinel"));
        }
    }
}
