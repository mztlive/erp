use axum::{
    middleware,
    routing::{delete, get, post, put},
    Router,
};
use services::iam::SharedRbacService;

use crate::{
    app_state::AppState,
    core::{
        handler::admin,
        middleware::{authenticate, with_permission},
    },
};

/// 管理后台路由入口。
///
/// # 返回值
/// 返回统一使用 Casbin RBAC 的管理后台路由。
pub fn routes(state: AppState) -> Router<AppState> {
    let rbac_service = state.rbac();

    Router::new()
        .merge(account_routes(&rbac_service))
        .merge(consumer_routes(&rbac_service))
        .merge(area_routes(&rbac_service))
        .merge(role_routes(&rbac_service))
        .merge(audit_log_routes(&rbac_service))
        .route_layer(middleware::from_fn_with_state(state.clone(), authenticate))
        .with_state(state)
}

/// 管理后台账号相关路由。
///
/// # 参数
/// * `rbac_service` - 授权引擎
///
/// # 返回值
/// 返回账号相关路由集合
fn account_routes(rbac_service: &SharedRbacService) -> Router<AppState> {
    Router::new()
        .route(
            "/admins",
            with_permission(
                get(admin::account::list_admins),
                rbac_service,
                admin::account::list_admins_permission_key(),
            ),
        )
        .route(
            "/admins",
            with_permission(
                post(admin::account::create_admin),
                rbac_service,
                admin::account::create_admin_permission_key(),
            ),
        )
        .route(
            "/admins/{id}",
            with_permission(
                put(admin::account::update_admin),
                rbac_service,
                admin::account::update_admin_permission_key(),
            ),
        )
        .route(
            "/admins/{id}",
            with_permission(
                delete(admin::account::delete_admin),
                rbac_service,
                admin::account::delete_admin_permission_key(),
            ),
        )
        .route(
            "/admins/{id}/role",
            with_permission(
                put(admin::account::update_admin_role),
                rbac_service,
                admin::account::update_admin_role_permission_key(),
            ),
        )
}

/// 管理后台消费者相关路由。
///
/// # 参数
/// * `rbac_service` - 授权引擎
///
/// # 返回值
/// 返回消费者相关路由集合
fn consumer_routes(rbac_service: &SharedRbacService) -> Router<AppState> {
    Router::new()
        .route(
            "/consumers",
            with_permission(
                get(admin::consumer::list_consumers),
                rbac_service,
                admin::consumer::list_consumers_permission_key(),
            ),
        )
        .route(
            "/consumers",
            with_permission(
                post(admin::consumer::create_consumer),
                rbac_service,
                admin::consumer::create_consumer_permission_key(),
            ),
        )
        .route(
            "/consumers/{id}",
            with_permission(
                put(admin::consumer::update_consumer),
                rbac_service,
                admin::consumer::update_consumer_permission_key(),
            ),
        )
}

/// 管理后台角色相关路由。
///
/// # 参数
/// * `rbac_service` - 授权引擎
///
/// # 返回值
/// 返回角色相关路由集合
fn role_routes(rbac_service: &SharedRbacService) -> Router<AppState> {
    Router::new()
        .route(
            "/roles",
            with_permission(
                get(admin::role::list_roles),
                rbac_service,
                admin::role::list_roles_permission_key(),
            ),
        )
        .route(
            "/roles/assignable",
            with_permission(
                get(admin::role::list_assignable_roles),
                rbac_service,
                admin::role::list_assignable_roles_permission_key(),
            ),
        )
        .route(
            "/roles",
            with_permission(
                post(admin::role::create_role),
                rbac_service,
                admin::role::create_role_permission_key(),
            ),
        )
        .route(
            "/roles/{id}",
            with_permission(
                put(admin::role::update_role),
                rbac_service,
                admin::role::update_role_permission_key(),
            ),
        )
        .route(
            "/roles/{id}",
            with_permission(
                delete(admin::role::delete_role),
                rbac_service,
                admin::role::delete_role_permission_key(),
            ),
        )
}

/// 管理后台行政区数据路由。
///
/// # 参数
/// * `rbac_service` - 授权引擎
///
/// # 返回值
/// 返回行政区数据路由集合
fn area_routes(rbac_service: &SharedRbacService) -> Router<AppState> {
    Router::new().route(
        "/shared/area-tree",
        with_permission(
            get(admin::area::area_tree),
            rbac_service,
            admin::area::area_tree_permission_key(),
        ),
    )
}

/// 管理后台审计日志相关路由。
///
/// # 参数
/// * `rbac_service` - 授权引擎
///
/// # 返回值
/// 返回审计日志相关路由集合
fn audit_log_routes(rbac_service: &SharedRbacService) -> Router<AppState> {
    Router::new().route(
        "/audit-logs",
        with_permission(
            get(admin::audit_log::list_audit_logs),
            rbac_service,
            admin::audit_log::list_audit_logs_permission_key(),
        ),
    )
}
