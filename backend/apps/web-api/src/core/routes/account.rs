use axum::routing::get;
use axum::{Router, middleware};

use crate::app_state::AppState;
use crate::core::handler::auth;
use crate::core::middleware::authenticate;

/// 当前账号相关路由入口。
///
/// # 参数
/// * `state` - 应用状态
///
/// # 返回值
/// 返回账号相关路由
pub fn routes(state: AppState) -> Router<AppState> {
    Router::new()
        .route("/profile", get(auth::profile::account_profile))
        .route_layer(middleware::from_fn_with_state(state.clone(), authenticate))
        .with_state(state)
}
