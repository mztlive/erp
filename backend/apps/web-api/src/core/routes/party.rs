//! 域 D07 `party` 管理端路由。
//!
//! 经 `admin.rs` 的 `/admin` nest 后，最终路径为 `/admin/parties`、
//! `/admin/party-contacts/{id}` 等；每条路由统一走
//! JWT + RBAC（`with_permission`），handler 标注 `#[permission_macros::permission]`。

use axum::{
    routing::{delete, get, post, put},
    Router,
};
use services::iam::SharedRbacService;

use crate::{
    app_state::AppState,
    core::{handler::party, middleware::with_permission},
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
            "/parties",
            with_permission(get(party::party_list), rbac, party::party_list_permission_key()),
        )
        .route(
            "/parties",
            with_permission(
                post(party::party_create),
                rbac,
                party::party_create_permission_key(),
            ),
        )
        .route(
            "/parties/{id}",
            with_permission(
                get(party::party_detail),
                rbac,
                party::party_detail_permission_key(),
            ),
        )
        .route(
            "/parties/{id}",
            with_permission(
                put(party::party_update),
                rbac,
                party::party_update_permission_key(),
            ),
        )
        .route(
            "/parties/{id}",
            with_permission(
                delete(party::party_delete),
                rbac,
                party::party_delete_permission_key(),
            ),
        )
        .route(
            "/parties/{id}/revisions",
            with_permission(
                get(party::party_revision_list),
                rbac,
                party::party_revision_list_permission_key(),
            ),
        )
        .route(
            "/parties/{id}/contacts",
            with_permission(
                get(party::party_contact_list),
                rbac,
                party::party_contact_list_permission_key(),
            ),
        )
        .route(
            "/parties/{id}/contacts",
            with_permission(
                post(party::party_contact_create),
                rbac,
                party::party_contact_create_permission_key(),
            ),
        )
        .route(
            "/party-contacts/{id}",
            with_permission(
                put(party::party_contact_update),
                rbac,
                party::party_contact_update_permission_key(),
            ),
        )
        .route(
            "/parties/{id}/addresses",
            with_permission(
                get(party::party_address_list),
                rbac,
                party::party_address_list_permission_key(),
            ),
        )
        .route(
            "/parties/{id}/addresses",
            with_permission(
                post(party::party_address_create),
                rbac,
                party::party_address_create_permission_key(),
            ),
        )
        .route(
            "/party-addresses/{id}",
            with_permission(
                put(party::party_address_update),
                rbac,
                party::party_address_update_permission_key(),
            ),
        )
        .route(
            "/parties/{id}/tax-profiles",
            with_permission(
                get(party::party_tax_profile_list),
                rbac,
                party::party_tax_profile_list_permission_key(),
            ),
        )
        .route(
            "/parties/{id}/tax-profiles",
            with_permission(
                post(party::party_tax_profile_create),
                rbac,
                party::party_tax_profile_create_permission_key(),
            ),
        )
        .route(
            "/party-tax-profiles/{id}",
            with_permission(
                put(party::party_tax_profile_update),
                rbac,
                party::party_tax_profile_update_permission_key(),
            ),
        )
        .route(
            "/parties/{id}/bank-accounts",
            with_permission(
                get(party::party_bank_account_list),
                rbac,
                party::party_bank_account_list_permission_key(),
            ),
        )
        .route(
            "/parties/{id}/bank-accounts",
            with_permission(
                post(party::party_bank_account_create),
                rbac,
                party::party_bank_account_create_permission_key(),
            ),
        )
        .route(
            "/party-bank-accounts/{id}",
            with_permission(
                put(party::party_bank_account_update),
                rbac,
                party::party_bank_account_update_permission_key(),
            ),
        )
}
