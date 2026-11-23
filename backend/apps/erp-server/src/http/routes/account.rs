use axum::extract::DefaultBodyLimit;
use axum::routing::{get, post};
use axum::{Router, middleware};

use crate::http::handler::approval_instance::copies;
use crate::http::handler::{auth, data_transfer as transfers, notification, region};
use crate::http::middleware::authenticate;
use crate::http::state::HttpState;

/// 挂载已登录账号的个人资料、审批进展与收件接口；个人业务范围仍由各用例校验。
pub fn routes(state: HttpState) -> Router<HttpState> {
    Router::new()
        // 本人收件箱沿用 account 路由认证；所有读写强制绑定会话账号。
        .route("/notifications", get(notification::inbox))
        .route("/notifications/read-all", post(notification::mark_all_read))
        .route("/notifications/{id}/read", post(notification::mark_read))
        .route("/notifications/events", get(notification::events))
        .route("/profile", get(auth::profile::account_profile))
        .route("/approval-leader", get(auth::profile::own_approval_leader))
        .route("/regions", get(region::list_regions))
        .merge(copy_routes())
        .route(
            "/data-transfers",
            get(transfers::list).post(transfers::submit).layer(DefaultBodyLimit::max(76 * 1024 * 1024)),
        )
        .route("/data-transfers/{id}", get(transfers::status))
        .route("/data-transfers/{id}/context", get(transfers::context))
        .route("/data-transfers/{id}/recover", post(transfers::recover))
        .route("/data-transfers/{id}/result", get(transfers::result))
        .route_layer(middleware::from_fn_with_state(state.clone(), authenticate))
        .with_state(state)
}

// 在个人账号路由下挂载审批收件接口，只要求后台登录；收件归属和冻结资料访问由领域服务逐条校验。
fn copy_routes() -> Router<HttpState> {
    Router::new()
        .route("/approval-copies", get(copies::list))
        .route("/approval-copies/read", post(copies::mark_read))
        .route("/approval-copies/{id}", get(copies::detail))
        .route("/approval-copies/{id}/materials/{file}/preview", get(copies::preview))
        .route("/approval-copies/{id}/materials/{file}/download", get(copies::download))
}
