use axum::routing::{delete, get, post, put};
use axum::{Router, middleware};
use erp_permissions as permissions;

use crate::http::handler::admin;
use crate::http::middleware::authenticate;
use crate::http::routes::{
    access_control, approval_instance, bulk_job, catalog, contract, cost, customer, customer_quality,
    demo_master_data, document_registry, file_asset, financial_files, fulfillment, integration_ops,
    inventory, legacy_import, party, payable, permission_route, purchase_order, purchase_payment_request,
    receivable, returns, sales_inquiry, sales_order, sales_review, sales_selection, source_registry,
    supplier, supplier_api, supplier_deposit, supplier_fulfillment, supplier_offering, supplier_portal,
    supplier_settlement, warehouse, work_item,
};
use crate::http::state::HttpState;

/// 把各领域路由合并到 `/admin` 并统一验证后台 JWT。路由权限标记供目录生成，操作与对象访问仍由业务用例授权。
pub fn routes(state: HttpState) -> Router<HttpState> {
    Router::new()
        .merge(account_routes())
        .merge(role_routes())
        .merge(audit_log_routes())
        .merge(procurement_responsibility_routes())
        .merge(access_control::routes())
        .merge(approval_instance::routes())
        .merge(bulk_job::routes())
        .merge(catalog::routes())
        .merge(contract::routes())
        .merge(cost::routes())
        .merge(customer::routes())
        .merge(customer_quality::routes())
        .merge(document_registry::routes())
        .merge(file_asset::routes())
        .merge(financial_files::routes())
        .merge(fulfillment::routes())
        .merge(integration_ops::routes())
        .merge(inventory::routes())
        .merge(legacy_import::routes())
        .merge(party::routes())
        .merge(payable::routes())
        .merge(purchase_order::routes())
        .merge(purchase_payment_request::routes())
        .merge(receivable::routes())
        .merge(returns::routes())
        .merge(sales_order::routes())
        .merge(sales_inquiry::routes())
        .merge(sales_review::routes())
        .merge(sales_selection::routes())
        .merge(source_registry::routes())
        .merge(supplier::routes())
        .merge(supplier_deposit::routes())
        .merge(supplier_api::routes())
        .merge(supplier_offering::routes())
        .merge(supplier_portal::admin_routes())
        .merge(supplier_fulfillment::routes())
        .merge(supplier_settlement::routes())
        .merge(warehouse::routes())
        .merge(work_item::routes())
        .merge(demo_master_data::routes())
        .route_layer(middleware::from_fn_with_state(state.clone(), authenticate))
        .with_state(state)
}

// 为父路由挂载管理后台账号相关路由。
// 返回账号相关路由集合。
fn account_routes() -> Router<HttpState> {
    Router::new()
        .route(
            "/admins",
            permission_route(get(admin::account::list_admins), &permissions::admin::ACCOUNT_LIST_ADMINS),
        )
        .route(
            "/admins",
            permission_route(post(admin::account::create_admin), &permissions::admin::ACCOUNT_CREATE_ADMIN),
        )
        .route(
            "/admins/{id}",
            permission_route(put(admin::account::update_admin), &permissions::admin::ACCOUNT_UPDATE_ADMIN),
        )
        .route(
            "/admins/{id}",
            permission_route(delete(admin::account::delete_admin), &permissions::admin::ACCOUNT_DELETE_ADMIN),
        )
        .route(
            "/admins/{id}/role",
            permission_route(
                put(admin::account::update_admin_role),
                &permissions::admin::ACCOUNT_UPDATE_ADMIN_ROLE,
            ),
        )
}

// 为父路由挂载管理后台角色相关路由。
// 返回角色相关路由集合。
fn role_routes() -> Router<HttpState> {
    role_template_routes().merge(role_crud_routes())
}

// 在角色管理下挂载内建岗位模板预览和生成，分别沿用普通角色查看与创建资格。
fn role_template_routes() -> Router<HttpState> {
    Router::new()
        .route(
            "/role-templates",
            permission_route(
                get(admin::role_template::list_role_templates),
                &permissions::admin::ROLE_LIST_ROLES,
            ),
        )
        .route(
            "/role-templates/generate",
            permission_route(
                post(admin::role_template::generate_builtin_roles),
                &permissions::admin::ROLE_CREATE_ROLE,
            ),
        )
}

// 为父路由挂载普通角色的列表、创建、修改和删除入口。
fn role_crud_routes() -> Router<HttpState> {
    Router::new()
        .route("/roles", permission_route(get(admin::role::list_roles), &permissions::admin::ROLE_LIST_ROLES))
        .route(
            "/roles/assignable",
            permission_route(
                get(admin::role::list_assignable_roles),
                &permissions::admin::ROLE_LIST_ASSIGNABLE_ROLES,
            ),
        )
        .route(
            "/roles",
            permission_route(post(admin::role::create_role), &permissions::admin::ROLE_CREATE_ROLE),
        )
        .route(
            "/roles/{id}",
            permission_route(put(admin::role::update_role), &permissions::admin::ROLE_UPDATE_ROLE),
        )
        .route(
            "/roles/{id}",
            permission_route(delete(admin::role::delete_role), &permissions::admin::ROLE_DELETE_ROLE),
        )
}

// 为父路由挂载管理后台审计日志相关路由。
// 返回审计日志相关路由集合。
fn audit_log_routes() -> Router<HttpState> {
    Router::new().route(
        "/audit-logs",
        permission_route(
            get(admin::audit_log::list_audit_logs),
            &permissions::admin::AUDIT_LOG_LIST_AUDIT_LOGS,
        ),
    )
}

// 为父路由挂载采购责任规则维护与解析预览路由。
// 返回规则查询、创建、更新和逐行解析路由集合。
fn procurement_responsibility_routes() -> Router<HttpState> {
    Router::new()
        .route(
            "/procurement-responsibility-rules",
            permission_route(
                get(admin::procurement_responsibility::list_rules),
                &permissions::admin::PROCUREMENT_RESPONSIBILITY_RESOLVE,
            ),
        )
        .route(
            "/procurement-responsibility-rules",
            permission_route(
                post(admin::procurement_responsibility::create_rule),
                &permissions::admin::PROCUREMENT_RESPONSIBILITY_CREATE_RULE,
            ),
        )
        .route(
            "/procurement-responsibility-rules/{id}",
            permission_route(
                put(admin::procurement_responsibility::update_rule),
                &permissions::admin::PROCUREMENT_RESPONSIBILITY_CREATE_RULE,
            ),
        )
        .route(
            "/procurement-responsibility/resolve",
            permission_route(
                post(admin::procurement_responsibility::resolve),
                &permissions::admin::PROCUREMENT_RESPONSIBILITY_RESOLVE,
            ),
        )
}
