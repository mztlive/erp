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
        routes::{
            access_control, approval_instance, bulk_job, card_instance, catalog, contract, cost, customer,
            document_registry, file_asset, fulfillment, integration_ops, inventory, legacy_import,
            mall_after_sales, mall_backfill, mall_order, mall_sync, party, payable, projection, publication,
            purchase_order, receivable, returns, sales_order, sales_review, source_registry, supplier,
            supplier_api, supplier_fulfillment, supplier_offering, supplier_settlement, warehouse, work_item,
        },
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
        .merge(role_routes(&rbac_service))
        .merge(audit_log_routes(&rbac_service))
        .merge(access_control::routes(&rbac_service))
        .merge(approval_instance::routes(&rbac_service))
        .merge(bulk_job::routes(&rbac_service))
        .merge(card_instance::routes(&rbac_service))
        .merge(catalog::routes(&rbac_service))
        .merge(contract::routes(&rbac_service))
        .merge(cost::routes(&rbac_service))
        .merge(customer::routes(&rbac_service))
        .merge(document_registry::routes(&rbac_service))
        .merge(file_asset::routes(&rbac_service))
        .merge(fulfillment::routes(&rbac_service))
        .merge(integration_ops::routes(&rbac_service))
        .merge(inventory::routes(&rbac_service))
        .merge(legacy_import::routes(&rbac_service))
        .merge(mall_after_sales::routes(&rbac_service))
        .merge(mall_backfill::routes(&rbac_service))
        .merge(mall_order::routes(&rbac_service))
        .merge(mall_sync::routes(&rbac_service))
        .merge(party::routes(&rbac_service))
        .merge(payable::routes(&rbac_service))
        .merge(projection::routes(&rbac_service))
        .merge(publication::routes(&rbac_service))
        .merge(purchase_order::routes(&rbac_service))
        .merge(receivable::routes(&rbac_service))
        .merge(returns::routes(&rbac_service))
        .merge(sales_order::routes(&rbac_service))
        .merge(sales_review::routes(&rbac_service))
        .merge(source_registry::routes(&rbac_service))
        .merge(supplier::routes(&rbac_service))
        .merge(supplier_api::routes(&rbac_service))
        .merge(supplier_offering::routes(&rbac_service))
        .merge(supplier_fulfillment::routes(&rbac_service))
        .merge(supplier_settlement::routes(&rbac_service))
        .merge(warehouse::routes(&rbac_service))
        .merge(work_item::routes(&rbac_service))
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
