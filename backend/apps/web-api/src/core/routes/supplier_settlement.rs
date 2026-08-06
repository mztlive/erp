//! 域 D33 `supplier_settlement` 管理端路由。
//!
//! 经 `admin.rs` 的 `/admin` nest 后，最终路径为 `/admin/supplier-settlement-statements`、
//! `/admin/supplier-settlement-items`、`/admin/supplier-settlement-differences`；
//! 每条路由统一走 JWT + RBAC（`with_permission`），handler 标注
//! `#[permission_macros::permission]`。

use axum::{
    routing::{get, post},
    Router,
};
use services::iam::SharedRbacService;

use crate::{
    app_state::AppState,
    core::{handler::supplier_settlement, middleware::with_permission},
};

/// 返回本域管理端路由集合。
///
/// # 参数
/// * `rbac` - 共享 Casbin RBAC 服务
///
/// # 返回
/// 返回挂载了权限校验层的路由集合。
pub fn routes(rbac: &SharedRbacService) -> Router<AppState> {
    Router::new()
        .route(
            "/supplier-settlement-statements",
            with_permission(
                get(supplier_settlement::supplier_settlement_statement_list),
                rbac,
                supplier_settlement::supplier_settlement_statement_list_permission_key(),
            ),
        )
        .route(
            "/supplier-settlement-statements",
            with_permission(
                post(supplier_settlement::supplier_settlement_statement_create),
                rbac,
                supplier_settlement::supplier_settlement_statement_create_permission_key(),
            ),
        )
        .route(
            "/supplier-settlement-statements/{id}",
            with_permission(
                get(supplier_settlement::supplier_settlement_statement_detail),
                rbac,
                supplier_settlement::supplier_settlement_statement_detail_permission_key(),
            ),
        )
        .route(
            "/supplier-settlement-statements/{id}/submit-review",
            with_permission(
                post(supplier_settlement::supplier_settlement_statement_submit_review),
                rbac,
                supplier_settlement::supplier_settlement_statement_submit_review_permission_key(),
            ),
        )
        .route(
            "/supplier-settlement-statements/{id}/confirm",
            with_permission(
                post(supplier_settlement::supplier_settlement_statement_confirm),
                rbac,
                supplier_settlement::supplier_settlement_statement_confirm_permission_key(),
            ),
        )
        .route(
            "/supplier-settlement-statements/{id}/void",
            with_permission(
                post(supplier_settlement::supplier_settlement_statement_void),
                rbac,
                supplier_settlement::supplier_settlement_statement_void_permission_key(),
            ),
        )
        .route(
            "/supplier-settlement-items",
            with_permission(
                get(supplier_settlement::supplier_settlement_item_list),
                rbac,
                supplier_settlement::supplier_settlement_item_list_permission_key(),
            ),
        )
        .route(
            "/supplier-settlement-differences",
            with_permission(
                get(supplier_settlement::supplier_settlement_difference_list),
                rbac,
                supplier_settlement::supplier_settlement_difference_list_permission_key(),
            ),
        )
        .route(
            "/supplier-settlement-differences/{id}/resolve",
            with_permission(
                post(supplier_settlement::supplier_settlement_difference_resolve),
                rbac,
                supplier_settlement::supplier_settlement_difference_resolve_permission_key(),
            ),
        )
}
