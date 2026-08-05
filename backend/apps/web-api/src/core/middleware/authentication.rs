use axum::{
    extract::{Request, State},
    http::{header::AUTHORIZATION, HeaderMap},
    middleware::Next,
    response::{IntoResponse, Response},
};
use services::{audit::AuditActor, auth::BackofficeAuthService, iam};
use tracing::{error, info, warn};

use crate::{
    app_state::AppState,
    core::{
        auth::jwt::TokenPayload,
        extractor::{Account, UserID},
        response::ApiResponse,
    },
};

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
        }
    };
    let Some(token) = bearer_token(request.headers()) else {
        return ApiResponse::<()>::unauthorized().into_response();
    };
    let Ok(payload) = engine.verify_token(token) else {
        warn!("Authorization failed: invalid token");
        return ApiResponse::<()>::unauthorized().into_response();
    };
    if let Err(response) = validate_current_identity(&state, &payload).await {
        return response.into_response();
    }
    if let Err(response) = attach_identity(&mut request, payload) {
        return response.into_response();
    }

    info!("Authorization success");
    next.run(request).await
}

/// 校验 token 中的后台身份仍与当前账号记录一致且处于可用状态。
async fn validate_current_identity(state: &AppState, payload: &TokenPayload) -> Result<(), ApiResponse<()>> {
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
        Ok(_) => Ok(()),
        Err(services::Error::Unauthenticated(_)) => {
            warn!("Authorization failed: backoffice account is no longer active");
            Err(ApiResponse::unauthorized())
        }
        Err(error) => {
            error!(error = %error, "Failed to validate current backoffice account");
            Err(ApiResponse::system_error())
        }
    }
}

/// 从标准 Authorization 头提取非空 Bearer token。
fn bearer_token(headers: &HeaderMap) -> Option<&str> {
    let value = headers.get(AUTHORIZATION)?.to_str().ok()?;
    let (scheme, token) = value.split_once(' ')?;
    (scheme.eq_ignore_ascii_case("Bearer")
        && !token.is_empty()
        && !token.bytes().any(|byte| byte.is_ascii_whitespace()))
    .then_some(token)
}

/// 校验 token 身份边界并写入后续 Handler 所需的扩展。
fn attach_identity(request: &mut Request, payload: TokenPayload) -> Result<(), ApiResponse<()>> {
    let TokenPayload {
        id: user_id,
        account,
        subject_kind,
        account_kind,
        ..
    } = payload;
    let Some(account_kind) = account_kind else {
        warn!("Authorization failed: missing account kind for backoffice token");
        return Err(ApiResponse::unauthorized());
    };
    request
        .extensions_mut()
        .insert(AuditActor::new(user_id.clone(), account.clone(), account_kind));
    request.extensions_mut().insert(account_kind);
    let rbac_subject = RbacSubject(iam::subject(account_kind, &user_id));
    request.extensions_mut().insert(UserID(user_id));
    request.extensions_mut().insert(Account(account));
    request.extensions_mut().insert(subject_kind);
    request.extensions_mut().insert(rbac_subject);

    Ok(())
}

#[cfg(test)]
mod tests {
    use axum::body::Body;
    use axum::extract::Request;
    use axum::http::{header::AUTHORIZATION, HeaderMap, HeaderValue};
    use entities::AccountKind;
    use services::audit::AuditActor;

    use super::{attach_identity, bearer_token, RbacSubject};
    use crate::core::{
        auth::jwt::{SubjectKind, TokenPayload},
        extractor::{Account, UserID},
    };

    #[test]
    fn rbac_subject_should_include_account_kind_and_id() {
        let subject = RbacSubject(services::iam::subject(AccountKind::Admin, "admin-1"));
        assert_eq!(subject.0, "user:admin:admin-1");
    }

    #[test]
    fn attach_identity_inserts_backoffice_context() {
        let mut request = Request::builder()
            .uri("/admin/roles")
            .body(Body::empty())
            .expect("request should be valid");
        let payload =
            TokenPayload::backoffice("admin-1".to_string(), "alice".to_string(), AccountKind::Admin, 1);

        assert!(attach_identity(&mut request, payload).is_ok());
        assert_eq!(
            request.extensions().get::<UserID>().map(|value| value.0.as_str()),
            Some("admin-1")
        );
        assert_eq!(
            request
                .extensions()
                .get::<Account>()
                .map(|value| value.0.as_str()),
            Some("alice")
        );
        assert_eq!(
            request.extensions().get::<AccountKind>(),
            Some(&AccountKind::Admin)
        );
        assert_eq!(
            request
                .extensions()
                .get::<RbacSubject>()
                .map(|value| value.0.as_str()),
            Some("user:admin:admin-1")
        );
        assert_eq!(
            request.extensions().get::<AuditActor>(),
            Some(&AuditActor::new(
                "admin-1".to_string(),
                "alice".to_string(),
                AccountKind::Admin,
            ))
        );
    }

    #[test]
    fn attach_identity_rejects_backoffice_token_without_account_kind() {
        let mut request = Request::builder()
            .uri("/admin/roles")
            .body(Body::empty())
            .expect("request should be valid");
        let payload = TokenPayload {
            id: "admin-1".to_string(),
            account: "alice".to_string(),
            subject_kind: SubjectKind::Backoffice,
            account_kind: None,
            account_version: None,
        };

        assert!(attach_identity(&mut request, payload).is_err());
        assert!(request.extensions().get::<UserID>().is_none());
        assert!(request.extensions().get::<Account>().is_none());
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
