//! 人工任务责任路由。
//!
//! 已删除 `/work-items/{id}/start-processing`、`release-to-team` 与 `claim`。

use axum::{
    routing::{get, post, put},
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
/// 返回列表、统计、详情、转交和关闭路由；仅支持指定责任人的任务合同。
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
            "/work-items/fulfillment-queue",
            with_permission(
                get(work_item::fulfillment_queue_list),
                rbac,
                work_item::fulfillment_queue_list_permission_key(),
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
            "/work-items/{id}/reassign",
            with_permission(
                post(work_item::work_item_reassign),
                rbac,
                work_item::work_item_reassign_permission_key(),
            ),
        )
        .route(
            "/work-items/{id}/reassign-candidates",
            with_permission(
                get(work_item::work_item_reassign_candidates),
                rbac,
                work_item::work_item_reassign_candidates_permission_key(),
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
        .route(
            "/finance-responsibility-rules",
            with_permission(
                get(work_item::finance_responsibility::finance_responsibility_rule_list),
                rbac,
                work_item::finance_responsibility::finance_responsibility_rule_list_permission_key(),
            ),
        )
        .route(
            "/finance-responsibility-rules",
            with_permission(
                post(work_item::finance_responsibility::finance_responsibility_rule_create),
                rbac,
                work_item::finance_responsibility::finance_responsibility_rule_create_permission_key(),
            ),
        )
        .route(
            "/finance-responsibility-rules/{id}",
            with_permission(
                put(work_item::finance_responsibility::finance_responsibility_rule_update),
                rbac,
                work_item::finance_responsibility::finance_responsibility_rule_update_permission_key(),
            ),
        )
        .route(
            "/finance-responsibility-owner-options",
            with_permission(
                get(work_item::finance_responsibility::finance_responsibility_owner_options),
                rbac,
                work_item::finance_responsibility::finance_responsibility_owner_options_permission_key(),
            ),
        )
}
