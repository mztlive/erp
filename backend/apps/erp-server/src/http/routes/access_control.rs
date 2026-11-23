//! 域 D06 `access_control` 管理端路由。
//!
//! 经 `admin.rs` 的 `/admin` nest 后，最终路径为 `/admin/person-data-scopes`、
//! `/admin/authorization-policies`、`/admin/audit-events` 等；
//! 每条路由统一走 JWT 认证；业务授权由用例执行，目录由 `permission_route` 提供，权限由显式目录统一定义。

use axum::Router;
use axum::extract::DefaultBodyLimit;
use axum::routing::{get, post, put};
use erp_permissions as permissions;

use crate::http::handler::{access_control, organization};
use crate::http::routes::permission_route;
use crate::http::state::HttpState;

/// 为管理端挂载人员与访问控制接口，按操作登记公共权限目录信息；文件入口保留对应的上传限制。
pub fn routes() -> Router<HttpState> {
    Router::new()
        .merge(organization_routes())
        .merge(person_scope_routes())
        .merge(policy_bundle_routes())
        .merge(inspection_routes())
        .merge(people_routes())
        .merge(resource_grant_routes())
}

// 挂载权限配置文件入口并复用后台认证；应用配置时还需通过角色及范围维护资格校验。
fn policy_bundle_routes() -> Router<HttpState> {
    Router::new()
        .route(
            "/authorization-policies/validate",
            permission_route(
                post(access_control::policy_bundle::validate),
                &permissions::access_control::POLICY_BUNDLE_PREVIEW,
            ),
        )
        .route(
            "/authorization-policies/preview",
            permission_route(
                post(access_control::policy_bundle::preview),
                &permissions::access_control::POLICY_BUNDLE_PREVIEW,
            ),
        )
        .route(
            "/authorization-policies/apply",
            permission_route(
                post(access_control::policy_bundle::apply),
                &permissions::access_control::POLICY_BUNDLE_APPLY,
            ),
        )
        .route(
            "/authorization-policies/export",
            permission_route(
                post(access_control::policy_bundle::export),
                &permissions::access_control::POLICY_BUNDLE_EXPORT,
            ),
        )
        .layer(DefaultBodyLimit::max(2 * 1024 * 1024))
}

// 为父路由挂载组织结构及人员查询资格路由。
fn organization_routes() -> Router<HttpState> {
    Router::new()
        .route(
            "/person-query-qualifications/{category}/{account_id}",
            permission_route(
                get(access_control::person_directory::get_qualification),
                &permissions::access_control::PERSON_DIRECTORY_GET_QUALIFICATION,
            ),
        )
        .route(
            "/person-query-qualifications/{category}/{account_id}",
            permission_route(
                put(access_control::person_directory::change_qualification),
                &permissions::access_control::PERSON_DIRECTORY_GET_QUALIFICATION,
            ),
        )
        .route("/org-units", permission_route(get(organization::list), &permissions::organization::LIST))
        .route(
            "/org-units/preview",
            permission_route(post(organization::preview), &permissions::organization::CHANGE),
        )
        .route(
            "/org-units/change",
            permission_route(post(organization::change), &permissions::organization::CHANGE),
        )
}

// 为父路由挂载访问检查与审计查询路由。
fn inspection_routes() -> Router<HttpState> {
    Router::new()
        .route(
            "/access-check",
            permission_route(
                post(access_control::inspection::inspect),
                &permissions::access_control::PERSON_SCOPE_LIST,
            ),
        )
        .route(
            "/audit-events",
            permission_route(
                get(access_control::audit_event_list),
                &permissions::access_control::AUDIT_EVENT_LIST,
            ),
        )
}

// 为父路由挂载按类别读取业务人员的目录路由。
fn people_routes() -> Router<HttpState> {
    Router::new()
        .route(
            "/salespeople",
            permission_route(
                get(access_control::person_directory::list_salespeople),
                &permissions::access_control::PERSON_DIRECTORY_LIST_SALESPEOPLE,
            ),
        )
        .route(
            "/salespeople/selected",
            permission_route(
                get(access_control::person_directory::list_selected_salespeople),
                &permissions::access_control::PERSON_DIRECTORY_LIST_SALESPEOPLE,
            ),
        )
        .route(
            "/procurement-people",
            permission_route(
                get(access_control::person_directory::list_procurement_people),
                &permissions::access_control::PERSON_DIRECTORY_LIST_PROCUREMENT_PEOPLE,
            ),
        )
        .route(
            "/procurement-people/selected",
            permission_route(
                get(access_control::person_directory::list_selected_procurement_people),
                &permissions::access_control::PERSON_DIRECTORY_LIST_PROCUREMENT_PEOPLE,
            ),
        )
        .route(
            "/business-people",
            permission_route(
                get(access_control::person_directory::list_business_people),
                &permissions::access_control::PERSON_DIRECTORY_LIST_BUSINESS_PEOPLE,
            ),
        )
        .route(
            "/business-people/selected",
            permission_route(
                get(access_control::person_directory::list_selected_business_people),
                &permissions::access_control::PERSON_DIRECTORY_LIST_BUSINESS_PEOPLE,
            ),
        )
}

// 挂载人员业务扩展授权入口，复用范围配置动作；领域层在命令中重验公司边界。
fn person_scope_routes() -> Router<HttpState> {
    Router::new()
        .route(
            "/person-data-scopes/{user_id}",
            permission_route(
                get(access_control::person_scope::list),
                &permissions::access_control::PERSON_SCOPE_LIST,
            ),
        )
        .route(
            "/person-data-scopes/{user_id}",
            permission_route(
                put(access_control::person_scope::save),
                &permissions::access_control::PERSON_SCOPE_SAVE,
            ),
        )
}

// 把指定记录授权挂到范围维护入口；业务服务确认当前人具备全公司管理资格后才执行。
fn resource_grant_routes() -> Router<HttpState> {
    Router::new()
        .route(
            "/resource-grants",
            permission_route(
                post(access_control::resource_grants::grant),
                &permissions::access_control::PERSON_SCOPE_SAVE,
            ),
        )
        .route(
            "/resource-grants/sources",
            permission_route(
                get(access_control::resource_grants::sources),
                &permissions::access_control::PERSON_SCOPE_SAVE,
            ),
        )
        .route(
            "/resource-grants/revoke",
            permission_route(
                post(access_control::resource_grants::revoke),
                &permissions::access_control::PERSON_SCOPE_SAVE,
            ),
        )
        .route(
            "/resource-grants/recompute",
            permission_route(
                post(access_control::resource_grants::recompute),
                &permissions::access_control::PERSON_SCOPE_SAVE,
            ),
        )
        .route(
            "/resource-grants/backfill",
            permission_route(
                post(access_control::resource_grants::backfill),
                &permissions::access_control::PERSON_SCOPE_SAVE,
            ),
        )
        .layer(DefaultBodyLimit::max(256 * 1024))
}
