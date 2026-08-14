//! D03 人工任务责任路由。

use axum::{
    routing::{get, post},
    Router,
};
use services::iam::SharedRbacService;

use crate::{
    app_state::AppState,
    core::{handler::work_item, middleware::with_permission},
};

/// 返回只暴露稳定责任接口的管理端路由。
///
/// # 返回
/// 返回列表、详情、开始处理、退回团队、转交和关闭路由；不暴露公开创建、
/// 暂挂或通用完成接口。
pub fn routes(rbac: &SharedRbacService) -> Router<AppState> {
    Router::new()
        .route(
            "/work-items",
            with_permission(
                get(work_item::work_item_list),
                rbac,
                work_item::work_item_list_permission_key(),
            ),
        )
        .route(
            "/work-items/stats",
            with_permission(
                get(work_item::work_item_stats),
                rbac,
                work_item::work_item_stats_permission_key(),
            ),
        )
        .route(
            "/work-items/{id}",
            with_permission(
                get(work_item::work_item_detail),
                rbac,
                work_item::work_item_detail_permission_key(),
            ),
        )
        .route(
            "/work-items/{id}/start-processing",
            with_permission(
                post(work_item::work_item_start_processing),
                rbac,
                work_item::work_item_start_processing_permission_key(),
            ),
        )
        .route(
            "/work-items/{id}/release-to-team",
            with_permission(
                post(work_item::work_item_release_to_team),
                rbac,
                work_item::work_item_release_to_team_permission_key(),
            ),
        )
        .route(
            "/work-items/{id}/reassign",
            with_permission(
                post(work_item::work_item_reassign),
                rbac,
                work_item::work_item_reassign_permission_key(),
            ),
        )
        .route(
            "/work-items/{id}/close",
            with_permission(
                post(work_item::work_item_close),
                rbac,
                work_item::work_item_close_permission_key(),
            ),
        )
}
