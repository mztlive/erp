//! 域 D07 `party` 管理端路由。
//!
//! 经 `admin.rs` 的 `/admin` nest 后，最终路径为 `/admin/parties`、
//! `/admin/party-contacts/{id}` 等。结算主体目录只要求后台登录；
//! 主体档案及维护入口继续使用 JWT + RBAC（`with_permission`）。

use axum::Router;
use axum::routing::{delete, get, post, put};
use erp_identity::SharedRbacService;

use crate::http::handler::party;
use crate::http::middleware::with_permission;
use crate::http::state::HttpState;
use crate::permissions;

/// 返回本域管理端路由集合。
///
/// # 参数
/// * `rbac` - 共享 Casbin RBAC 服务
///
/// # 返回
/// 返回挂载了权限校验层的路由集合。
pub fn routes(rbac: &SharedRbacService) -> Router<HttpState> {
    Router::new()
        // 上层 admin 路由统一认证；结算主体候选与回显不设置独立权限。
        .route("/settlement-parties", get(party::directory::list))
        .route("/settlement-parties/selected", get(party::directory::selected))
        .route(
            "/companies",
            with_permission(
                get(party::company::company_list),
                rbac,
                permissions::party::COMPANY_COMPANY_LIST,
            ),
        )
        .route(
            "/companies",
            with_permission(
                post(party::company::company_create),
                rbac,
                permissions::party::COMPANY_COMPANY_CREATE,
            ),
        )
        .route(
            "/companies/{id}",
            with_permission(
                get(party::company::company_detail),
                rbac,
                permissions::party::COMPANY_COMPANY_DETAIL,
            ),
        )
        .route(
            "/companies/{id}",
            with_permission(
                put(party::company::company_update),
                rbac,
                permissions::party::COMPANY_COMPANY_UPDATE,
            ),
        )
        .route("/parties", with_permission(get(party::party_list), rbac, permissions::party::LIST))
        .route("/parties", with_permission(post(party::party_create), rbac, permissions::party::CREATE))
        .route("/parties/{id}", with_permission(get(party::party_detail), rbac, permissions::party::DETAIL))
        .route("/parties/{id}", with_permission(put(party::party_update), rbac, permissions::party::UPDATE))
        .route(
            "/parties/{id}",
            with_permission(delete(party::party_delete), rbac, permissions::party::DELETE),
        )
        .route(
            "/parties/{id}/revisions",
            with_permission(get(party::party_revision_list), rbac, permissions::party::REVISION_LIST),
        )
        .route(
            "/parties/{id}/contacts",
            with_permission(get(party::party_contact_list), rbac, permissions::party::CONTACT_LIST),
        )
        .route(
            "/parties/{id}/contacts",
            with_permission(post(party::party_contact_create), rbac, permissions::party::CONTACT_CREATE),
        )
        .route(
            "/party-contacts/{id}",
            with_permission(put(party::party_contact_update), rbac, permissions::party::CONTACT_UPDATE),
        )
        .route(
            "/parties/{id}/addresses",
            with_permission(get(party::party_address_list), rbac, permissions::party::ADDRESS_LIST),
        )
        .route(
            "/parties/{id}/addresses",
            with_permission(post(party::party_address_create), rbac, permissions::party::ADDRESS_CREATE),
        )
        .route(
            "/party-addresses/{id}",
            with_permission(put(party::party_address_update), rbac, permissions::party::ADDRESS_UPDATE),
        )
        .route(
            "/parties/{id}/tax-profiles",
            with_permission(get(party::party_tax_profile_list), rbac, permissions::party::TAX_PROFILE_LIST),
        )
        .route(
            "/parties/{id}/tax-profiles",
            with_permission(
                post(party::party_tax_profile_create),
                rbac,
                permissions::party::TAX_PROFILE_CREATE,
            ),
        )
        .route(
            "/party-tax-profiles/{id}",
            with_permission(
                put(party::party_tax_profile_update),
                rbac,
                permissions::party::TAX_PROFILE_UPDATE,
            ),
        )
        .route(
            "/parties/{id}/bank-accounts",
            with_permission(get(party::party_bank_account_list), rbac, permissions::party::BANK_ACCOUNT_LIST),
        )
        .route(
            "/parties/{id}/bank-accounts",
            with_permission(
                post(party::party_bank_account_create),
                rbac,
                permissions::party::BANK_ACCOUNT_CREATE,
            ),
        )
        .route(
            "/party-bank-accounts/{id}",
            with_permission(
                put(party::party_bank_account_update),
                rbac,
                permissions::party::BANK_ACCOUNT_UPDATE,
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
    use crate::http::routes::admin;

    /// 只构造客户端；用例在认证或参数边界返回，不访问 MongoDB 或 S3。
    fn state() -> HttpState {
        let config = Config::from_toml_str(
            r#"
            [xxl_job]
admin_url = "http://admin.invalid/xxl-job-admin"
executor_url = "http://worker.invalid:10002"
app_name = "erp-test"
access_token = "test-token-123456"

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
        HttpState::new(db, SafeConfig::new(config), storage)
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
        let app = routes(&state.application().rbac())
            .layer(Extension(AuditActor::new("user".into(), "user".into(), AccountKind::Admin)))
            .with_state(state);
        for path in ["/settlement-parties?page=0", "/settlement-parties/selected"] {
            let response =
                app.clone().oneshot(Request::builder().uri(path).body(Body::empty()).unwrap()).await.unwrap();
            assert_eq!(response.status(), StatusCode::BAD_REQUEST, "{path}");
        }
    }
}
