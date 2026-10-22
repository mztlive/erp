use axum::extract::DefaultBodyLimit;
use axum::routing::{get, post};
use axum::{Router, middleware};

use crate::app_state::AppState;
use crate::core::handler::approval_instance::copies;
use crate::core::handler::{auth, data_transfer as transfers, region};
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
        .route("/regions", get(region::list_regions))
        .merge(copy_routes())
        .route(
            "/data-transfers",
            get(transfers::list).post(transfers::submit).layer(DefaultBodyLimit::max(76 * 1024 * 1024)),
        )
        .route("/data-transfers/{id}", get(transfers::status))
        .route("/data-transfers/{id}/result", get(transfers::result))
        .route_layer(middleware::from_fn_with_state(state.clone(), authenticate))
        .with_state(state)
}

// 个人收件接口仅要求后台登录；领域服务逐条验证本人收件和冻结资料边界。
fn copy_routes() -> Router<AppState> {
    Router::new()
        .route("/approval-copies", get(copies::list))
        .route("/approval-copies/read", post(copies::mark_read))
        .route("/approval-copies/{id}", get(copies::detail))
        .route("/approval-copies/{id}/materials/{file}/preview", get(copies::preview))
        .route("/approval-copies/{id}/materials/{file}/download", get(copies::download))
}
