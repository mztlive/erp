use application_core::AuditActor;
use axum::extract::{Request, State};
use axum::http::HeaderMap;
use axum::http::header::AUTHORIZATION;
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};
use erp_core::AccountKind;
use erp_identity::{BackofficeAuthResult, BackofficeAuthService};
use tracing::{error, info, warn};

use crate::app_state::AppState;
use crate::core::auth::jwt::{SubjectKind, TokenPayload};
use crate::core::extractor::{Account, UserID};
use crate::core::response::ApiResponse;
use crate::core::tracing::RequestId;

/// 已认证后台账号对应的 Casbin 主体。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RbacSubject(pub String);

/// 验证 JWT，并向请求扩展写入账号身份与 Casbin 主体。
///
/// # 返回值
/// 认证成功时继续执行后续处理器，否则返回统一错误响应。
pub async fn authenticate(State(state): State<AppState>, mut request: Request, next: Next) -> Response {
    let engine = match state.jwt_engine().await {
        Ok(engine) => engine,
        Err(err) => {
            error!(error = %err, "Failed to get JWT engine");
            return ApiResponse::<()>::system_error().into_response();
        },
    };
    let Some(token) = bearer_token(request.headers()) else {
        return ApiResponse::<()>::unauthorized().into_response();
    };
    let Ok(payload) = engine.verify_token(token) else {
        warn!("Authorization failed: invalid token");
        return ApiResponse::<()>::unauthorized().into_response();
    };
    let identity = match validate_current_identity(&state, &payload).await {
        Ok(identity) => identity,
        Err(response) => return response.into_response(),
    };
    if let Err(response) = attach_identity(&mut request, payload, Some(identity.name().to_string())) {
        return response.into_response();
    }

    info!("Authorization success");
    next.run(request).await
}

/// 校验 token 中的后台身份仍与当前账号记录一致且处于可用状态。
async fn validate_current_identity(
    state: &AppState,
    payload: &TokenPayload,
) -> Result<BackofficeAuthResult, ApiResponse<()>> {
    if payload.subject_kind != SubjectKind::Backoffice || payload.account_kind != Some(AccountKind::Admin) {
        return Err(ApiResponse::unauthorized());
    }
    let Some(account_kind) = payload.account_kind else {
        warn!("Authorization failed: missing account kind for backoffice token");
        return Err(ApiResponse::unauthorized());
    };
    let Some(account_version) = payload.account_version else {
        warn!("Authorization failed: missing account version for backoffice token");
        return Err(ApiResponse::unauthorized());
    };

    match BackofficeAuthService::new(state.db())
        .validate_session(&payload.id, &payload.account, account_kind, account_version)
        .await
    {
        Ok(identity) => Ok(identity),
        Err(erp_identity::Error::Unauthenticated(_)) => {
            warn!("Authorization failed: backoffice account is no longer active");
            Err(ApiResponse::unauthorized())
        },
        Err(error) => {
            error!(error = %error, "Failed to validate current backoffice account");
            Err(ApiResponse::system_error())
        },
    }
}

/// 从标准 Authorization 头提取非空 Bearer token。
pub(super) fn bearer_token(headers: &HeaderMap) -> Option<&str> {
    let value = headers.get(AUTHORIZATION)?.to_str().ok()?;
    let (scheme, token) = value.split_once(' ')?;
    (scheme.eq_ignore_ascii_case("Bearer")
        && !token.is_empty()
        && !token.bytes().any(|byte| byte.is_ascii_whitespace()))
    .then_some(token)
}

/// 校验 token 身份边界并写入后续 Handler 所需的扩展。
fn attach_identity(
    request: &mut Request,
    payload: TokenPayload,
    actor_name_snapshot: Option<String>,
) -> Result<(), ApiResponse<()>> {
    let TokenPayload { id: user_id, account, subject_kind, account_kind, .. } = payload;
    if subject_kind != SubjectKind::Backoffice || account_kind != Some(AccountKind::Admin) {
        return Err(ApiResponse::unauthorized());
    }
    let Some(account_kind) = account_kind else {
        warn!("Authorization failed: missing account kind for backoffice token");
        return Err(ApiResponse::unauthorized());
    };
    let actor = AuditActor::new(user_id.clone(), account.clone(), account_kind)
        .with_actor_name_snapshot(actor_name_snapshot)
        .and_then(|actor| {
            actor.with_request_id(
                request.extensions().get::<RequestId>().map(|request_id| request_id.0.clone()),
            )
        })
        .map_err(|error| {
            error!(error = %error, "Failed to capture authenticated audit context");
            ApiResponse::system_error()
        })?;
    request.extensions_mut().insert(actor);
    request.extensions_mut().insert(account_kind);
    let rbac_subject = RbacSubject(erp_identity::subject(account_kind, &user_id));
    request.extensions_mut().insert(UserID(user_id));
    request.extensions_mut().insert(Account(account));
    request.extensions_mut().insert(subject_kind);
    request.extensions_mut().insert(rbac_subject);

    Ok(())
}

#[cfg(test)]
mod tests {
    use application_core::AuditActor;
    use axum::body::Body;
    use axum::extract::Request;
    use axum::http::header::AUTHORIZATION;
    use axum::http::{HeaderMap, HeaderValue};
    use erp_core::AccountKind;

    use super::{RbacSubject, attach_identity, bearer_token};
    use crate::core::auth::jwt::{SubjectKind, TokenPayload};
    use crate::core::extractor::{Account, UserID};

    #[test]
    fn rbac_subject_should_include_account_kind_and_id() {
        let subject = RbacSubject(erp_identity::subject(AccountKind::Admin, "admin-1"));
        assert_eq!(subject.0, "user:admin:admin-1");
    }

    #[test]
    fn supplier_token_cannot_attach_internal_permissions_context() {
        let mut request = Request::builder().uri("/admin/products").body(Body::empty()).unwrap();
        let payload = TokenPayload::supplier_portal("supplier-user".into(), "supplier01".into(), 1, 1);
        assert!(attach_identity(&mut request, payload, None).is_err());
        assert!(request.extensions().get::<RbacSubject>().is_none());
        assert!(request.extensions().get::<AuditActor>().is_none());
    }

    #[test]
    fn attach_identity_inserts_backoffice_context() {
        let mut request =
            Request::builder().uri("/admin/roles").body(Body::empty()).expect("request should be valid");
        let payload =
            TokenPayload::backoffice("admin-1".to_string(), "alice".to_string(), AccountKind::Admin, 1);

        assert!(attach_identity(&mut request, payload, None).is_ok());
        assert_eq!(request.extensions().get::<UserID>().map(|value| value.0.as_str()), Some("admin-1"));
        assert_eq!(request.extensions().get::<Account>().map(|value| value.0.as_str()), Some("alice"));
        assert_eq!(request.extensions().get::<AccountKind>(), Some(&AccountKind::Admin));
        assert_eq!(
            request.extensions().get::<RbacSubject>().map(|value| value.0.as_str()),
            Some("user:admin:admin-1")
        );
        assert_eq!(
            request.extensions().get::<AuditActor>(),
            Some(&AuditActor::new("admin-1".to_string(), "alice".to_string(), AccountKind::Admin,))
        );
    }

    #[test]
    fn attach_identity_rejects_backoffice_token_without_account_kind() {
        let mut request =
            Request::builder().uri("/admin/roles").body(Body::empty()).expect("request should be valid");
        let payload = TokenPayload {
            id: "admin-1".to_string(),
            account: "alice".to_string(),
            subject_kind: SubjectKind::Backoffice,
            account_kind: None,
            account_version: None,
            binding_version: None,
        };

        assert!(attach_identity(&mut request, payload, None).is_err());
        assert!(request.extensions().get::<UserID>().is_none());
        assert!(request.extensions().get::<Account>().is_none());
    }

    /// 本次会话名称进入真实工厂并冻结，不依赖后续账号查询或名称变化。
    #[test]
    fn attached_current_name_is_frozen_in_real_audit_factory() {
        use erp_audit::{AuditActorLogs, prepare_business_log};

        let mut request = Request::builder().uri("/admin/customer").body(Body::empty()).unwrap();
        let payload =
            TokenPayload::backoffice("actor-1".into(), "sales-account".into(), AccountKind::Admin, 1);
        let mut verified_name = "  周晓彤  ".to_string();
        attach_identity(&mut request, payload, Some(verified_name.clone())).unwrap();
        verified_name = "之后的新名称".into();
        let actor = request.extensions().get::<AuditActor>().unwrap().clone();
        assert_eq!(actor.actor_name_snapshot(), Some("周晓彤"));
        let log = actor.resource_log("customer.create", "customer", "customer-1".into()).unwrap();
        let frozen = prepare_business_log(&log).unwrap();
        assert_eq!(frozen.structured_event.as_ref().unwrap().actor_name_snapshot.as_deref(), Some("周晓彤"));
        assert!(!frozen.message.as_deref().unwrap().contains(&verified_name));
        assert!(frozen.message.as_deref().unwrap().contains("周晓彤"));
    }

    /// 原 HTTP 中间件关联经真实身份附加和事件工厂进入事件、尝试，并冻结为同一值。
    #[tokio::test]
    async fn traced_request_id_is_frozen_in_events_and_attempts() {
        use axum::body::to_bytes;
        use axum::routing::get;
        use axum::{Json, Router, middleware};
        use erp_audit::{AuditActorLogs, AuditAttempt, AuditAttemptResult, AuditLog, attempt_context};
        use tower::ServiceExt;

        use crate::core::tracing::{RequestId, trace_middleware};

        let app = Router::new()
            .route(
                "/",
                get(|mut request: Request| async move {
                    let request_id = request.extensions().get::<RequestId>().unwrap().0.clone();
                    assert_eq!(request.headers().get("X-Trace-Id").unwrap(), request_id.as_str());
                    let payload =
                        TokenPayload::backoffice("actor-1".into(), "sales".into(), AccountKind::Admin, 1);
                    attach_identity(&mut request, payload, None).unwrap();
                    let actor = request.extensions().get::<AuditActor>().unwrap().clone();
                    request.extensions_mut().insert(RequestId("later-context".into()));
                    assert_eq!(actor.request_id(), Some(request_id.as_str()));
                    let log = actor.resource_log("customer.create", "customer", "customer-1".into()).unwrap();
                    let attempt = attempt_context(&log).unwrap().attempt(AuditAttemptResult::Unknown);
                    Json((log, attempt))
                }),
            )
            .layer(middleware::from_fn(trace_middleware));
        let request =
            Request::builder().uri("/").header("X-Trace-Id", "original-request").body(Body::empty()).unwrap();
        let response = app.oneshot(request).await.unwrap();
        assert_eq!(response.headers().get("X-Trace-Id").unwrap(), "original-request");
        let body = to_bytes(response.into_body(), 8192).await.unwrap();
        let (log, attempt): (AuditLog, AuditAttempt) = serde_json::from_slice(&body).unwrap();
        assert_eq!(log.structured_event.unwrap().request_id.as_deref(), Some("original-request"));
        assert_eq!(attempt.request_id.as_deref(), Some("original-request"));
    }

    #[test]
    fn bearer_token_requires_exact_non_empty_scheme() {
        let mut headers = HeaderMap::new();
        headers.insert(AUTHORIZATION, HeaderValue::from_static("token"));
        assert_eq!(bearer_token(&headers), None);

        headers.insert(AUTHORIZATION, HeaderValue::from_static("Bearer "));
        assert_eq!(bearer_token(&headers), None);

        headers.insert(AUTHORIZATION, HeaderValue::from_static("Bearer token"));
        assert_eq!(bearer_token(&headers), Some("token"));

        headers.insert(AUTHORIZATION, HeaderValue::from_static("bearer token"));
        assert_eq!(bearer_token(&headers), Some("token"));

        headers.insert(AUTHORIZATION, HeaderValue::from_static("Bearer token extra"));
        assert_eq!(bearer_token(&headers), None);
    }
}
