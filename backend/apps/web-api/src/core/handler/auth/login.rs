use std::net::SocketAddr;

use axum::{
    extract::{ConnectInfo, Extension, State},
    Json,
};
use entities::AuditLogData;
use services::{
    audit::AuditLogService,
    auth::{AuthRequest, AuthResponse, BackofficeAuthResult, BackofficeAuthService},
};
use tracing::{info, instrument, warn};

use crate::app_state::AppState;
use crate::core::auth::jwt::TokenPayload;
use crate::core::errors::{Error, Result};
use crate::core::rate_limit::RateLimiter;
use crate::core::response::ApiResponse;

/// 完成后台账号认证并签发 JWT。
///
/// 密码、状态和账号类型规则由 `BackofficeAuthService` 负责；HTTP 层仅处理
/// 令牌签发、响应和审计。
#[instrument(skip(state, limiter, peer, request), fields(account_kind = ?request.account_kind))]
pub(crate) async fn login(
    State(state): State<AppState>,
    Extension(limiter): Extension<RateLimiter>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    Json(request): Json<AuthRequest>,
) -> Result<AuthResponse> {
    let (source_key, source_account_key) =
        super::login_rate_keys(super::BACKOFFICE_LOGIN_REALM, peer.ip(), &request.account);
    let _permit = limiter.admit_hierarchy(&[&source_key, &source_account_key])?;
    info!("Backoffice login attempt");

    let authentication = BackofficeAuthService::new(state.db())
        .authenticate(&request)
        .await;
    let identity = match authentication {
        Ok(identity) => identity,
        Err(services::Error::Unauthenticated(_)) => {
            return handle_login_failure(&state, &request).await;
        }
        Err(services::Error::ValidationError(message)) => {
            return Err(Error::BadRequest(message));
        }
        Err(error) => {
            record_login_audit(&state, &request, None, false, Some("登录处理失败".to_string())).await;
            return Err(Error::from(error));
        }
    };

    let jwt_engine = state
        .jwt_engine()
        .await
        .map_err(|error| Error::Internal(error.to_string()))?;
    let token = jwt_engine
        .create_token(token_payload_for_identity(&identity))
        .map_err(|error| Error::Internal(error.to_string()))?;
    handle_login_success(&state, &request, &identity, token).await
}

/// 为认证成功的后台身份构建 JWT 载荷。
fn token_payload_for_identity(identity: &BackofficeAuthResult) -> TokenPayload {
    TokenPayload::backoffice(
        identity.account_id().to_string(),
        identity.account().to_string(),
        identity.account_kind(),
        identity.account_version(),
    )
}

/// 处理登录成功流程。
async fn handle_login_success(
    state: &AppState,
    request: &AuthRequest,
    identity: &BackofficeAuthResult,
    token: String,
) -> Result<AuthResponse> {
    info!("Login successful for account: {}", identity.account());
    record_login_audit(state, request, Some(identity.account_id()), true, None).await;
    Ok(ApiResponse::ok_with_data(AuthResponse { token }))
}

/// 处理无效凭证登录失败流程。
async fn handle_login_failure(state: &AppState, request: &AuthRequest) -> Result<AuthResponse> {
    info!(account_kind = ?request.account_kind, "Backoffice login failed");
    record_login_audit(state, request, None, false, Some("用户名或密码错误".to_string())).await;
    Err(Error::Unauthorized("用户名或密码错误".to_string()))
}

/// 写入登录审计日志。
async fn record_login_audit(
    state: &AppState,
    request: &AuthRequest,
    actor_id: Option<&str>,
    success: bool,
    failure_reason: Option<String>,
) {
    let data = AuditLogData {
        actor_id: actor_id.unwrap_or("unknown").to_string(),
        actor_account: request.account.clone(),
        actor_type: request.account_kind,
        action: "auth.login".to_string(),
        resource_type: "auth".to_string(),
        resource_id: None,
        success,
        message: failure_reason,
    };
    if let Err(error) = AuditLogService::new(state.db()).create(data).await {
        warn!(%error, "Failed to record login audit");
    }
}

#[cfg(test)]
mod tests {
    use super::token_payload_for_identity;
    use entities::{AccountCore, AccountCoreData, AccountKind, AccountStatus, LoginAccount, Secret};
    use services::auth::BackofficeAuthResult;

    fn identity(kind: AccountKind) -> BackofficeAuthResult {
        let account = AccountCore::new(
            format!("{}-1", kind.as_str()),
            AccountCoreData {
                secret: Secret::new(
                    LoginAccount::new(format!("{}01", kind.as_str())).unwrap(),
                    "password123",
                )
                .unwrap(),
                name: "测试账号".to_string(),
                kind,
                status: AccountStatus::Active,
                email: None,
                phone: None,
                avatar: None,
            },
        )
        .unwrap();
        BackofficeAuthResult::from(&account)
    }

    #[test]
    fn token_payload_should_keep_account_kind() {
        let payload = token_payload_for_identity(&identity(AccountKind::Admin));
        assert_eq!(payload.account_kind, Some(AccountKind::Admin));
    }
}
