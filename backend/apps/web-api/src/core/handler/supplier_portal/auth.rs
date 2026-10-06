//! 外部账号凭证签发，独立于后台登录及角色系统。
use std::net::SocketAddr;
use std::result::Result as StdResult;

use axum::extract::{ConnectInfo, State};
use axum::{Extension, Json};
use erp_audit::{AuditLogData, AuditLogService};
use erp_core::AccountKind;
use erp_identity::{PortalActor, PortalIdentityService, PortalLoginPayload, PortalPasswordUpdate};
use persistence_core::NoTransaction;
use serde::Serialize;
use tracing::warn;

use super::process;
use crate::app_state::AppState;
use crate::core::auth::jwt::TokenPayload;
use crate::core::errors::{Error, Result};
use crate::core::rate_limit::RateLimiter;
use crate::core::response::ApiResponse;

#[derive(Serialize)]
pub struct PortalLoginResponse {
    pub token: String,
    pub profile: PortalActor,
}

/// 验证外部账号密码、绑定和供应商启用状态，签发独立门户凭证。
/// # 参数
/// 请求仅包含账号密码；限流基于连接来源。
/// # 返回
/// 门户凭证和必要身份。
/// # 错误
/// 非供应商身份、密码错误、绑定或供应商失效时拒绝。
pub(crate) async fn login(
    State(state): State<AppState>,
    Extension(limiter): Extension<RateLimiter>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    Json(input): Json<PortalLoginPayload>,
) -> Result<PortalLoginResponse> {
    let _permit = limiter.admit(&format!("supplier_portal|{}", peer.ip()))?;
    let actor = match authenticate(&state, &input).await {
        Ok(actor) => actor,
        Err(error) => {
            record_login_audit(&state, &input.account, None, false).await;
            return Err(error);
        },
    };
    let engine = state.jwt_engine().await.map_err(|error| Error::Internal(error.to_string()))?;
    let token = engine
        .create_token(TokenPayload::supplier_portal(
            actor.account_id.clone(),
            actor.account.clone(),
            actor.account_version,
            actor.binding_version,
        ))
        .map_err(|error| Error::Internal(error.to_string()))?;
    record_login_audit(&state, &actor.account, Some(&actor.account_id), true).await;
    Ok(ApiResponse::ok_with_data(PortalLoginResponse { token, profile: actor }))
}

async fn authenticate(state: &AppState, input: &PortalLoginPayload) -> StdResult<PortalActor, Error> {
    let actor = PortalIdentityService::new(state.db()).authenticate(input).await?;
    Ok(process(state).session_validate(&actor, &mut NoTransaction).await?)
}

/// 登录独立留痕不记录密码，也不因审计失败改变凭证结果。
async fn record_login_audit(state: &AppState, account: &str, id: Option<&str>, success: bool) {
    let data = AuditLogData {
        actor_id: id.unwrap_or("unknown").into(),
        actor_account: account.into(),
        actor_type: AccountKind::Supplier,
        action: "auth.login".into(),
        resource_type: "auth".into(),
        resource_id: None,
        success,
        message: Some(if success { "供应商门户登录成功" } else { "供应商门户登录失败" }.into()),
    };
    if let Err(error) = AuditLogService::new(state.db()).create(data).await {
        warn!(%error, "Failed to record supplier portal login audit");
    }
}

/// 自助更换密码，成功后原凭证失效。
/// # 参数
/// 身份来自当前门户认证，请求只含原密码和新密码。
/// # 返回
/// 返回新账号版本，界面须重新登录。
/// # 错误
/// 原密码错误、身份失效或并发修改时拒绝。
pub async fn password(
    State(state): State<AppState>,
    Extension(actor): Extension<PortalActor>,
    Json(input): Json<PortalPasswordUpdate>,
) -> Result<PortalActor> {
    let updated = process(&state).password_update(&actor, &input).await?;
    Ok(ApiResponse::ok_with_data(updated))
}
