//! 人工任务责任路由。
//!
//! 已删除 `/work-items/{id}/start-processing`、`release-to-team` 与 `claim`。

use axum::Router;
use axum::routing::{get, post, put};
use erp_identity::SharedRbacService;

use crate::app_state::AppState;
use crate::core::handler::work_item;
use crate::core::middleware::with_permission;
use crate::permissions;

/// 返回只暴露稳定责任接口的管理端路由。
///
/// # 返回
/// 返回列表、统计、详情、转交和关闭路由；仅支持指定责任人的任务合同。
pub fn routes(rbac: &SharedRbacService) -> Router<AppState> {
    Router::new()
        .route(
            "/work-items/{id}/timeline",
            with_permission(
                get(work_item::timeline::task_timeline),
                rbac,
                permissions::work_item::TIMELINE_TASK_TIMELINE,
            ),
        )
        .route(
            "/work-items",
            with_permission(get(work_item::work_item_list), rbac, permissions::work_item::LIST),
        )
        .route(
            "/work-items/fulfillment-queue",
            with_permission(
                get(work_item::fulfillment_queue_list),
                rbac,
                permissions::work_item::FULFILLMENT_QUEUE_LIST,
            ),
        )
        .route(
            "/work-items/stats",
            with_permission(get(work_item::work_item_stats), rbac, permissions::work_item::STATS),
        )
        .route(
            "/work-items/{id}",
            with_permission(get(work_item::work_item_detail), rbac, permissions::work_item::DETAIL),
        )
        .route(
            "/work-items/{id}/reassign",
            with_permission(post(work_item::work_item_reassign), rbac, permissions::work_item::REASSIGN),
        )
        .route(
            "/work-items/{id}/reassign-candidates",
            with_permission(
                get(work_item::work_item_reassign_candidates),
                rbac,
                permissions::work_item::REASSIGN_CANDIDATES,
            ),
        )
        .route(
            "/work-items/{id}/close",
            with_permission(post(work_item::work_item_close), rbac, permissions::work_item::CLOSE),
        )
        .route(
            "/finance-responsibility-rules",
            with_permission(
                get(work_item::finance_responsibility::finance_responsibility_rule_list),
                rbac,
                permissions::work_item::FINANCE_RESPONSIBILITY_FINANCE_RESPONSIBILITY_RULE_LIST,
            ),
        )
        .route(
            "/finance-responsibility-rules",
            with_permission(
                post(work_item::finance_responsibility::finance_responsibility_rule_create),
                rbac,
                permissions::work_item::FINANCE_RESPONSIBILITY_FINANCE_RESPONSIBILITY_RULE_CREATE,
            ),
        )
        .route(
            "/finance-responsibility-rules/{id}",
            with_permission(
                put(work_item::finance_responsibility::finance_responsibility_rule_update),
                rbac,
                permissions::work_item::FINANCE_RESPONSIBILITY_FINANCE_RESPONSIBILITY_RULE_UPDATE,
            ),
        )
        .route(
            "/finance-responsibility-owner-options",
            with_permission(
                get(work_item::finance_responsibility::finance_responsibility_owner_options),
                rbac,
                permissions::work_item::FINANCE_RESPONSIBILITY_FINANCE_RESPONSIBILITY_OWNER_OPTIONS,
            ),
        )
}
