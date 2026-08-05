use std::net::SocketAddr;

use axum::{
    extract::{ConnectInfo, Extension, State},
    Json,
};
use services::auth::{AuthResponse, PasswordLoginPayload};
use services::consumer::{ConsumerAuthResult, ConsumerAuthService};

use crate::{
    app_state::AppState,
    core::{
        auth::jwt::TokenPayload,
        errors::{Error, Result},
        rate_limit::RateLimiter,
        response::ApiResponse,
    },
};

/// 完成消费者账号认证并签发 JWT。
///
/// 进入密码校验前会按 TCP 来源及“来源 + 规范化账号”执行两层进程内速率限制，
/// 并施加全局应急窗口与并发限制。
///
/// # 错误
/// 凭证无效、请求超过登录限制或令牌签发失败时返回稳定 HTTP 错误。
pub(crate) async fn login(
    State(state): State<AppState>,
    Extension(limiter): Extension<RateLimiter>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    Json(payload): Json<PasswordLoginPayload>,
) -> Result<AuthResponse> {
    let (source_key, source_account_key) =
        super::login_rate_keys(super::CONSUMER_LOGIN_REALM, peer.ip(), &payload.account);
    let _permit = limiter.admit_hierarchy(&[&source_key, &source_account_key])?;
    let ConsumerAuthResult { user_id, account } = ConsumerAuthService::new(state.db())
        .login_with_password(payload)
        .await?;

    let jwt = state
        .jwt_engine()
        .await
        .map_err(|error| Error::Internal(error.to_string()))?;
    let token = jwt
        .create_token(TokenPayload::consumer(user_id, account))
        .map_err(|error| Error::Internal(error.to_string()))?;
    Ok(ApiResponse::ok_with_data(AuthResponse { token }))
}
