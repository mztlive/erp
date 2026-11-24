//! 人工任务责任路由。
//!
//! 已删除 `/work-items/{id}/start-processing`、`release-to-team` 与 `claim`。

use axum::Router;
use axum::routing::{get, post, put};
use erp_permissions as permissions;

use crate::http::handler::work_item;
use crate::http::routes::permission_route;
use crate::http::state::HttpState;

/// 返回只暴露稳定责任接口的管理端路由。
/// 返回列表、统计、详情、转交和关闭路由；仅支持指定责任人的任务合同。
pub fn routes() -> Router<HttpState> {
    Router::new()
        .route(
            "/work-items/{id}/timeline",
            permission_route(get(work_item::timeline::task_timeline), &permissions::work_item::DETAIL),
        )
        .route("/work-items", permission_route(get(work_item::work_item_list), &permissions::work_item::LIST))
        .route(
            "/work-items/fulfillment-queue",
            permission_route(get(work_item::fulfillment_queue_list), &permissions::work_item::LIST),
        )
        .route(
            "/work-items/stats",
            permission_route(get(work_item::work_item_stats), &permissions::work_item::LIST),
        )
        .route(
            "/work-items/{id}",
            permission_route(get(work_item::work_item_detail), &permissions::work_item::DETAIL),
        )
        .route(
            "/work-items/{id}/reassign",
            permission_route(post(work_item::work_item_reassign), &permissions::work_item::MANAGE),
        )
        .route(
            "/work-items/{id}/reassign-candidates",
            permission_route(get(work_item::work_item_reassign_candidates), &permissions::work_item::MANAGE),
        )
        .route(
            "/work-items/{id}/close",
            permission_route(post(work_item::work_item_close), &permissions::work_item::MANAGE),
        )
        .route(
            "/work-items/{id}/abandon-rework",
            permission_route(post(work_item::abandon_rework), &permissions::work_item::DETAIL),
        )
        .merge(finance_responsibility_routes())
}

fn finance_responsibility_routes() -> Router<HttpState> {
    Router::new()
        .route(
            "/finance-responsibility-rules",
            permission_route(
                get(work_item::finance_responsibility::finance_responsibility_rule_list),
                &permissions::work_item::FINANCE_RESPONSIBILITY_FINANCE_RESPONSIBILITY_RULE_LIST,
            ),
        )
        .route(
            "/finance-responsibility-rules",
            permission_route(
                post(work_item::finance_responsibility::finance_responsibility_rule_create),
                &permissions::work_item::FINANCE_RESPONSIBILITY_FINANCE_RESPONSIBILITY_RULE_CREATE,
            ),
        )
        .route(
            "/finance-responsibility-rules/{id}",
            permission_route(
                put(work_item::finance_responsibility::finance_responsibility_rule_update),
                &permissions::work_item::FINANCE_RESPONSIBILITY_FINANCE_RESPONSIBILITY_RULE_CREATE,
            ),
        )
        .route(
            "/finance-responsibility-owner-options",
            permission_route(
                get(work_item::finance_responsibility::finance_responsibility_owner_options),
                &permissions::work_item::FINANCE_RESPONSIBILITY_FINANCE_RESPONSIBILITY_RULE_LIST,
            ),
        )
}
