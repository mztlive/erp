//! 域 D07 `party` 管理端路由。
//!
//! 经 `admin.rs` 的 `/admin` nest 后，最终路径为 `/admin/parties`、
//! `/admin/party-contacts/{id}` 等。结算主体目录只要求后台登录；
//! 主体档案及维护入口继续使用 JWT + RBAC（`with_permission`）。

use axum::Router;
use axum::routing::{delete, get, post, put};
use erp_identity::SharedRbacService;

use crate::app_state::AppState;
use crate::core::handler::party;
use crate::core::middleware::with_permission;

/// 返回本域管理端路由集合。
///
/// # 参数
/// * `rbac` - 共享 Casbin RBAC 服务
///
/// # 返回
/// 返回挂载了权限校验层的路由集合。
pub fn routes(rbac: &SharedRbacService) -> Router<AppState> {
    Router::new()
        // 上层 admin 路由统一认证；结算主体候选与回显不设置独立权限。
        .route("/settlement-parties", get(party::directory::list))
        .route("/settlement-parties/selected", get(party::directory::selected))
        .route(
            "/companies",
            with_permission(
                get(party::company::company_list),
                rbac,
                party::company::company_list_permission_key(),
            ),
        )
        .route(
            "/companies",
            with_permission(
                post(party::company::company_create),
                rbac,
                party::company::company_create_permission_key(),
            ),
        )
        .route(
            "/companies/{id}",
            with_permission(
                get(party::company::company_detail),
                rbac,
                party::company::company_detail_permission_key(),
            ),
        )
        .route(
            "/companies/{id}",
            with_permission(
                put(party::company::company_update),
                rbac,
                party::company::company_update_permission_key(),
            ),
        )
        .route("/parties", with_permission(get(party::party_list), rbac, party::party_list_permission_key()))
        .route(
            "/parties",
            with_permission(post(party::party_create), rbac, party::party_create_permission_key()),
        )
        .route(
            "/parties/{id}",
            with_permission(get(party::party_detail), rbac, party::party_detail_permission_key()),
        )
        .route(
            "/parties/{id}",
            with_permission(put(party::party_update), rbac, party::party_update_permission_key()),
        )
        .route(
            "/parties/{id}",
            with_permission(delete(party::party_delete), rbac, party::party_delete_permission_key()),
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
            with_permission(get(party::party_contact_list), rbac, party::party_contact_list_permission_key()),
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
            with_permission(get(party::party_address_list), rbac, party::party_address_list_permission_key()),
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

#[cfg(test)]
mod tests {
    use application_core::AuditActor;
    use axum::Extension;
    use axum::body::Body;
    use axum::http::{Request, StatusCode};
    use config::{Config, SafeConfig};
    use erp_core::AccountKind;
    use mongodb::Client;
    use mongodb::options::ClientOptions;
    use storage::{S3Storage, S3StorageConfig};
    use tower::ServiceExt;

    use super::*;
    use crate::core::routes::admin;

    /// 只构造客户端；用例在认证或参数边界返回，不访问 MongoDB 或 S3。
    fn state() -> AppState {
        let config = Config::from_toml_str(
            r#"
            [app]
            port = 10001
            secret = "unit-test-only-secret-at-least-32-bytes"
            [database]
            uri = "mongodb://127.0.0.1:1/unit_test"
            db_name = "unit_test"
            [s3]
            bucket = "unit-test"
            region = "unit-test"
            access_key_id = "unit-test"
            secret_access_key = "unit-test"
            public_base_url = "http://127.0.0.1:1"
            "#,
        )
        .unwrap();
        let db = Client::with_options(ClientOptions::default()).unwrap().database("unit_test");
        let storage = S3Storage::new(S3StorageConfig::new(
            "unit-test",
            "unit-test",
            "unit-test",
            "unit-test",
            "http://127.0.0.1:1",
        ))
        .unwrap();
        AppState::new(db, SafeConfig::new(config), storage)
    }

    #[tokio::test]
    async fn settlement_directory_keeps_admin_authentication() {
        let state = state();
        let app = admin::routes(state.clone()).with_state(state);
        for path in ["/settlement-parties", "/settlement-parties/selected?ids=company"] {
            let response =
                app.clone().oneshot(Request::builder().uri(path).body(Body::empty()).unwrap()).await.unwrap();
            assert_eq!(response.status(), StatusCode::UNAUTHORIZED, "{path}");
        }
    }

    #[tokio::test]
    async fn authenticated_directory_reaches_validation_without_rbac_subject_or_roles() {
        let state = state();
        let app = routes(&state.rbac())
            .layer(Extension(AuditActor::new("user".into(), "user".into(), AccountKind::Admin)))
            .with_state(state);
        for path in ["/settlement-parties?page=0", "/settlement-parties/selected"] {
            let response =
                app.clone().oneshot(Request::builder().uri(path).body(Body::empty()).unwrap()).await.unwrap();
            assert_eq!(response.status(), StatusCode::BAD_REQUEST, "{path}");
        }
    }
}
