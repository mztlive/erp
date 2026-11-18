use std::net::SocketAddr;

use axum::extract::{ConnectInfo, Extension, State};
use erp_audit::{AuditLogData, AuditLogService};
use erp_identity::{AuthRequest, AuthResponse, BackofficeAuthResult, BackofficeAuthService};
use tracing::{info, instrument, warn};

use crate::http::auth::jwt::TokenPayload;
use crate::http::errors::{Error, Result};
use crate::http::extractor::Json;
use crate::http::rate_limit::RateLimiter;
use crate::http::response::ApiResponse;
use crate::http::state::HttpState;

// 处理后台登录：先按 TCP 连接来源检查准入，再由身份域校验密码、账号类型和状态；HTTP 层负责签发 JWT、生成响应及写登录审计。
#[instrument(skip(state, limiter, peer, request), fields(account_kind = ?request.account_kind))]
pub(crate) async fn login(
    State(state): State<HttpState>,
    Extension(limiter): Extension<RateLimiter>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    Json(request): Json<AuthRequest>,
) -> Result<AuthResponse> {
    let source_key = super::login_source_key(super::BACKOFFICE_LOGIN_REALM, peer.ip());
    let _permit = limiter.admit(&source_key)?;
    info!("Backoffice login attempt");

    let authentication = BackofficeAuthService::new(state.application().db()).authenticate(&request).await;
    let identity = match authentication {
        Ok(identity) => identity,
        Err(erp_identity::Error::Unauthenticated(_)) => {
            return handle_login_failure(&state, &request).await;
        },
        Err(erp_identity::Error::ValidationError(message)) => {
            return Err(Error::BadRequest(message));
        },
        Err(error) => {
            record_login_audit(&state, &request, None, false, Some("登录处理失败".to_string())).await;
            return Err(Error::from(error));
        },
    };

    let jwt_engine = state.jwt_engine().await.map_err(|error| Error::Internal(error.to_string()))?;
    let token = jwt_engine
        .create_token(token_payload_for_identity(&identity))
        .map_err(|error| Error::Internal(error.to_string()))?;
    handle_login_success(&state, &request, &identity, token).await
}

// 认证成功后用服务端身份构造后台 JWT 声明，保留账号类型及持久化版本供后续请求复验。
fn token_payload_for_identity(identity: &BackofficeAuthResult) -> TokenPayload {
    TokenPayload::backoffice(
        identity.account_id().to_string(),
        identity.account().to_string(),
        identity.account_kind(),
        identity.account_version(),
    )
}

// 签发 JWT 成功后记录已知账号的登录审计，再用统一响应交付令牌。
async fn handle_login_success(
    state: &HttpState,
    request: &AuthRequest,
    identity: &BackofficeAuthResult,
    token: String,
) -> Result<AuthResponse> {
    info!("Login successful for account: {}", identity.account());
    record_login_audit(state, request, Some(identity.account_id()), true, None).await;
    Ok(ApiResponse::ok_with_data(AuthResponse { token }))
}

// 凭证校验未通过时记录不关联账号 ID 的失败审计，并返回统一的用户名或密码错误，不区分账号是否存在。
async fn handle_login_failure(state: &HttpState, request: &AuthRequest) -> Result<AuthResponse> {
    info!(account_kind = ?request.account_kind, "Backoffice login failed");
    record_login_audit(state, request, None, false, Some("用户名或密码错误".to_string())).await;
    Err(Error::Unauthorized("用户名或密码错误".to_string()))
}

// 尽力保存登录成功或失败事件；审计写入异常只记警告，不改写原认证结果。
async fn record_login_audit(
    state: &HttpState,
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
    if let Err(error) = AuditLogService::new(state.application().db()).create(data).await {
        warn!(%error, "Failed to record login audit");
    }
}

#[cfg(test)]
mod tests {
    use erp_core::AccountKind;
    use erp_identity::{
        AccountCore, AccountCoreData, AccountStatus, BackofficeAuthResult, LoginAccount, Secret,
    };

    use super::token_payload_for_identity;

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
