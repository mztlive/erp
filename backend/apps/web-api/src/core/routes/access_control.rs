//! 域 D06 `access_control` 管理端路由。
//!
//! 经 `admin.rs` 的 `/admin` nest 后，最终路径为 `/admin/permissions`、
//! `/admin/data-scopes`、`/admin/user-roles`、`/admin/audit-events`；
//! 每条路由统一走 JWT + RBAC（`with_permission`），权限由显式目录统一定义。

use axum::Router;
use axum::extract::DefaultBodyLimit;
use axum::routing::{delete, get, post, put};
use erp_identity::SharedRbacService;

use crate::app_state::AppState;
use crate::core::handler::{access_control, organization};
use crate::core::middleware::with_permission;
use crate::permissions;

/// 返回本域管理端路由集合。
///
/// # 参数
/// * `rbac` - 共享 Casbin RBAC 服务
///
/// # 返回
/// 返回挂载了权限校验层的路由集合。
pub fn routes(rbac: &SharedRbacService) -> Router<AppState> {
    Router::new()
        .merge(organization_routes(rbac))
        .merge(permission_routes(rbac))
        .merge(person_scope_routes(rbac))
        .merge(policy_bundle_routes(rbac))
        .merge(binding_routes(rbac))
        .merge(inspection_routes(rbac))
        .merge(people_routes(rbac))
}

/// 文件入口复用认证链路，应用仍要求底层角色和范围配置资格。
fn policy_bundle_routes(rbac: &SharedRbacService) -> Router<AppState> {
    Router::new()
        .route(
            "/authorization-policies/validate",
            with_permission(
                post(access_control::policy_bundle::validate),
                rbac,
                permissions::access_control::POLICY_BUNDLE_VALIDATE,
            ),
        )
        .route(
            "/authorization-policies/preview",
            with_permission(
                post(access_control::policy_bundle::preview),
                rbac,
                permissions::access_control::POLICY_BUNDLE_PREVIEW,
            ),
        )
        .route(
            "/authorization-policies/apply",
            with_permission(
                post(access_control::policy_bundle::apply),
                rbac,
                permissions::access_control::POLICY_BUNDLE_APPLY,
            ),
        )
        .route(
            "/authorization-policies/export",
            with_permission(
                post(access_control::policy_bundle::export),
                rbac,
                permissions::access_control::POLICY_BUNDLE_EXPORT,
            ),
        )
        .layer(DefaultBodyLimit::max(2 * 1024 * 1024))
}

/// 组织结构及人员查询资格路由。
fn organization_routes(rbac: &SharedRbacService) -> Router<AppState> {
    Router::new()
        .route(
            "/person-query-qualifications/{category}/{account_id}",
            with_permission(
                get(access_control::person_directory::get_qualification),
                rbac,
                permissions::access_control::PERSON_DIRECTORY_GET_QUALIFICATION,
            ),
        )
        .route(
            "/person-query-qualifications/{category}/{account_id}",
            with_permission(
                put(access_control::person_directory::change_qualification),
                rbac,
                permissions::access_control::PERSON_DIRECTORY_CHANGE_QUALIFICATION,
            ),
        )
        .route("/org-units", with_permission(get(organization::list), rbac, permissions::organization::LIST))
        .route(
            "/org-units/preview",
            with_permission(post(organization::preview), rbac, permissions::organization::PREVIEW),
        )
        .route(
            "/org-units/change",
            with_permission(post(organization::change), rbac, permissions::organization::CHANGE),
        )
}

/// 权限目录管理路由。
fn permission_routes(rbac: &SharedRbacService) -> Router<AppState> {
    Router::new()
        .route(
            "/permissions",
            with_permission(
                get(access_control::permission_list),
                rbac,
                permissions::access_control::PERMISSION_LIST,
            ),
        )
        .route(
            "/permissions",
            with_permission(
                post(access_control::permission_create),
                rbac,
                permissions::access_control::PERMISSION_CREATE,
            ),
        )
        .route(
            "/permissions/{id}",
            with_permission(
                put(access_control::permission_update),
                rbac,
                permissions::access_control::PERMISSION_UPDATE,
            ),
        )
        .route(
            "/permissions/{id}",
            with_permission(
                delete(access_control::permission_delete),
                rbac,
                permissions::access_control::PERMISSION_DELETE,
            ),
        )
}

/// 用户角色绑定及撤销路由。
fn binding_routes(rbac: &SharedRbacService) -> Router<AppState> {
    Router::new()
        .route(
            "/user-roles",
            with_permission(
                get(access_control::user_role_list),
                rbac,
                permissions::access_control::USER_ROLE_LIST,
            ),
        )
        .route(
            "/user-roles",
            with_permission(
                post(access_control::user_role_create),
                rbac,
                permissions::access_control::USER_ROLE_CREATE,
            ),
        )
        .route(
            "/user-roles/{id}/revoke",
            with_permission(
                post(access_control::user_role_revoke),
                rbac,
                permissions::access_control::USER_ROLE_REVOKE,
            ),
        )
}

/// 访问检查与审计查询路由。
fn inspection_routes(rbac: &SharedRbacService) -> Router<AppState> {
    Router::new()
        .route(
            "/access-check",
            with_permission(
                post(access_control::inspection::inspect),
                rbac,
                permissions::access_control::INSPECTION_INSPECT,
            ),
        )
        .route(
            "/audit-events",
            with_permission(
                get(access_control::audit_event_list),
                rbac,
                permissions::access_control::AUDIT_EVENT_LIST,
            ),
        )
}

/// 按类别读取业务人员的目录路由。
fn people_routes(rbac: &SharedRbacService) -> Router<AppState> {
    Router::new()
        .route(
            "/salespeople",
            with_permission(
                get(access_control::person_directory::list_salespeople),
                rbac,
                permissions::access_control::PERSON_DIRECTORY_LIST_SALESPEOPLE,
            ),
        )
        .route(
            "/salespeople/selected",
            with_permission(
                get(access_control::person_directory::list_selected_salespeople),
                rbac,
                permissions::access_control::PERSON_DIRECTORY_LIST_SELECTED_SALESPEOPLE,
            ),
        )
        .route(
            "/procurement-people",
            with_permission(
                get(access_control::person_directory::list_procurement_people),
                rbac,
                permissions::access_control::PERSON_DIRECTORY_LIST_PROCUREMENT_PEOPLE,
            ),
        )
        .route(
            "/procurement-people/selected",
            with_permission(
                get(access_control::person_directory::list_selected_procurement_people),
                rbac,
                permissions::access_control::PERSON_DIRECTORY_LIST_SELECTED_PROCUREMENT_PEOPLE,
            ),
        )
        .route(
            "/business-people",
            with_permission(
                get(access_control::person_directory::list_business_people),
                rbac,
                permissions::access_control::PERSON_DIRECTORY_LIST_BUSINESS_PEOPLE,
            ),
        )
        .route(
            "/business-people/selected",
            with_permission(
                get(access_control::person_directory::list_selected_business_people),
                rbac,
                permissions::access_control::PERSON_DIRECTORY_LIST_SELECTED_BUSINESS_PEOPLE,
            ),
        )
}

/// 人员业务扩展授权使用现有范围配置动作，并在领域层重验公司配置边界。
fn person_scope_routes(rbac: &SharedRbacService) -> Router<AppState> {
    Router::new()
        .route(
            "/person-data-scopes/{user_id}",
            with_permission(
                get(access_control::person_scope::list),
                rbac,
                permissions::access_control::PERSON_SCOPE_LIST,
            ),
        )
        .route(
            "/person-data-scopes/{user_id}",
            with_permission(
                put(access_control::person_scope::save),
                rbac,
                permissions::access_control::PERSON_SCOPE_SAVE,
            ),
        )
}
