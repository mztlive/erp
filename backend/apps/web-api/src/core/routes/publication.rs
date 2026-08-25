//! 域 D26 `publication` 管理端路由。
//!
//! 经 `admin.rs` 的 `/admin` nest 后，只暴露已由内部身份策略建立的发布对象；
//! 当前禁止通过管理端 HTTP 创建稳定发布身份。其余路径为
//! `/admin/product-publications`、
//! `/admin/product-publication-revisions/{revision_id}/media`、
//! `/admin/product-publication-deliveries`；每条路由统一走
//! JWT + RBAC（`with_permission`），handler 标注 `#[permission_macros::permission]`。

use axum::{
    routing::{get, post, put},
    Router,
};
use services::iam::SharedRbacService;

use crate::{
    app_state::AppState,
    core::{handler::publication, middleware::with_permission},
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
            "/product-publications",
            with_permission(
                get(publication::product_publication_list),
                rbac,
                publication::product_publication_list_permission_key(),
            ),
        )
        .route(
            "/product-publications/{id}",
            with_permission(
                get(publication::product_publication_detail),
                rbac,
                publication::product_publication_detail_permission_key(),
            ),
        )
        .route(
            "/product-publications/{id}",
            with_permission(
                put(publication::product_publication_update),
                rbac,
                publication::product_publication_update_permission_key(),
            ),
        )
        .route(
            "/product-publications/{id}/revisions",
            with_permission(
                post(publication::product_publication_revision_create),
                rbac,
                publication::product_publication_revision_create_permission_key(),
            ),
        )
        .route(
            "/product-publications/{id}/revisions",
            with_permission(
                get(publication::product_publication_revision_list),
                rbac,
                publication::product_publication_revision_list_permission_key(),
            ),
        )
        .route(
            "/product-publication-revisions/{revision_id}/media",
            with_permission(
                get(publication::product_publication_revision_media_list),
                rbac,
                publication::product_publication_revision_media_list_permission_key(),
            ),
        )
        .route(
            "/product-publications/{id}/revisions/{revision_no}/deliver",
            with_permission(
                post(publication::product_publication_delivery_submit),
                rbac,
                publication::product_publication_delivery_submit_permission_key(),
            ),
        )
        .route(
            "/product-publication-deliveries",
            with_permission(
                get(publication::product_publication_delivery_list),
                rbac,
                publication::product_publication_delivery_list_permission_key(),
            ),
        )
        .route(
            "/product-publication-deliveries/{delivery_id}/retry",
            with_permission(
                post(publication::product_publication_delivery_retry),
                rbac,
                publication::product_publication_delivery_retry_permission_key(),
            ),
        )
        .route(
            "/product-publication-deliveries/{delivery_id}/actions",
            with_permission(
                post(publication::product_publication_delivery_action),
                rbac,
                publication::product_publication_delivery_action_permission_key(),
            ),
        )
        .route(
            "/product-publication-deliveries/process-pending",
            with_permission(
                post(publication::product_publication_delivery_process_pending),
                rbac,
                publication::product_publication_delivery_process_pending_permission_key(),
            ),
        )
        .route(
            "/product-publication-safety-pauses/{idempotency_key}",
            with_permission(
                get(publication::product_publication_safety_pause_detail),
                rbac,
                publication::product_publication_safety_pause_detail_permission_key(),
            ),
        )
}
