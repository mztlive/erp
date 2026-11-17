//! 门户独立身份接口与内部采购确认接口；凭证和权限链分别装配。

use axum::extract::DefaultBodyLimit;
use axum::routing::{get, post, put};
use axum::{Extension, Router, middleware};
use erp_permissions as permissions;

use crate::http::handler::data_transfer as transfers;
use crate::http::handler::supplier_portal::{self, assets, auth, read, write};
use crate::http::middleware::supplier_portal::authenticate_portal;
use crate::http::routes::permission_route;
use crate::http::state::HttpState;
use crate::http::upload;

/// 装配公开登录及逐请求重验身份的门户业务接口。
///
/// # 参数
/// * `state` - 认证和业务组合根。
/// # 返回
/// 返回挂载于 `/supplier-portal` 的独立门户路由。
/// # 错误
/// 路由构建不执行外部读取；请求错误由认证及处理器返回。
pub fn routes(state: HttpState) -> Router<HttpState> {
    let login = Router::new()
        .route("/login", post(auth::login))
        .layer(Extension(super::public::login_limiter()))
        .layer(super::public::login_body_limit());
    let authenticated = read_routes()
        .merge(write_routes())
        .merge(asset_routes())
        .route(
            "/data-transfers",
            get(transfers::portal_list)
                .post(transfers::portal_submit)
                .layer(DefaultBodyLimit::max(76 * 1024 * 1024)),
        )
        .route("/data-transfers/{id}", get(transfers::portal_status))
        .route("/data-transfers/{id}/context", get(transfers::portal_context))
        .route("/data-transfers/{id}/recover", post(transfers::portal_recover))
        .route("/data-transfers/{id}/result", get(transfers::portal_result))
        .route("/password", post(auth::password))
        .route_layer(middleware::from_fn_with_state(state.clone(), authenticate_portal));
    Router::new().merge(login).merge(authenticated).with_state(state)
}

/// 只提供本供应商对象和定向开放目录的外部读取接口。
fn read_routes() -> Router<HttpState> {
    Router::new()
        .route("/session", get(read::session))
        .route("/offerings", get(read::offerings))
        .route("/offerings/{id}", get(read::offering))
        .route("/offerings/{id}/revisions", get(read::revisions))
        .route("/catalog", get(read::catalog))
        .route("/dictionaries/{kind}", get(read::dictionaries))
        .route("/category-mapping-suggestion", get(read::category_mapping_suggestion))
        .route("/applications", get(read::applications))
        .route("/applications/{id}", get(read::application))
        .route("/cooperation", get(read::cooperation))
}

/// 写请求由 Process 重验角色、来源、版本和操作号。
fn write_routes() -> Router<HttpState> {
    Router::new()
        .route("/offerings/{id}/availability", post(write::availability))
        .route("/applications", post(write::save_application))
        .route("/applications/{id}", put(write::update_application))
        .route("/applications/{id}/submit", post(write::submit))
        .route("/applications/{id}/withdraw", post(write::withdraw))
        .route("/new-products", post(write::save_new_product))
        .route("/cooperation/applications", post(write::save_cooperation))
        .route("/batch", post(write::batch).layer(DefaultBodyLimit::max(upload::MAX_UPLOAD_FILE_BYTES)))
}

/// 门户上传在认证后执行准入，文件解析与整请求上限分别约束。
fn asset_routes() -> Router<HttpState> {
    let upload = upload::multipart_route(post(assets::upload), upload::MAX_MULTIPART_REQUEST_BYTES);
    Router::new()
        .route("/applications/{id}/files", upload)
        .route("/files/{id}/download", get(assets::download))
}

/// 装配后台专项接口；统一后台认证由父级 `/admin` 路由执行。
///
/// # 参数
/// 无。
/// # 返回
/// 返回逐接口绑定专项权限的 `/supplier-portal` 子路由。
/// # 错误
/// 构建不执行授权读取；请求错误由权限层及处理器返回。
pub fn admin_routes() -> Router<HttpState> {
    Router::new().nest(
        "/supplier-portal",
        account_routes().merge(catalog_routes()).merge(application_routes()).merge(review_routes()),
    )
}

/// 账号管理入口不授予内部角色或组织身份。
fn account_routes() -> Router<HttpState> {
    Router::new()
        .route(
            "/accounts",
            permission_route(
                get(supplier_portal::admin::accounts),
                &permissions::supplier_portal::ADMIN_ACCOUNTS,
            ),
        )
        .route(
            "/accounts",
            permission_route(
                post(supplier_portal::admin::create_account),
                &permissions::supplier_portal::ADMIN_CREATE_ACCOUNT,
            ),
        )
        .route(
            "/accounts/{id}",
            permission_route(
                put(supplier_portal::admin::update_account),
                &permissions::supplier_portal::ADMIN_UPDATE_ACCOUNT,
            ),
        )
}

/// 定向目录开放使用独立目录权限。
fn catalog_routes() -> Router<HttpState> {
    Router::new()
        .route(
            "/catalog-grants",
            permission_route(
                get(supplier_portal::admin::grants),
                &permissions::supplier_portal::ADMIN_GRANTS,
            ),
        )
        .route(
            "/catalog-grants",
            permission_route(
                post(supplier_portal::admin::update_grant),
                &permissions::supplier_portal::ADMIN_UPDATE_GRANT,
            ),
        )
}

/// 申请列表、原稿读取和决定各自绑定对应专项权限。
fn application_routes() -> Router<HttpState> {
    Router::new()
        .route(
            "/applications",
            permission_route(
                get(supplier_portal::admin::applications),
                &permissions::supplier_portal::ADMIN_APPLICATIONS,
            ),
        )
        .route(
            "/applications/{id}",
            permission_route(
                get(supplier_portal::admin::application),
                &permissions::supplier_portal::ADMIN_IMPACTS,
            ),
        )
        .route(
            "/applications/{id}/review",
            permission_route(
                post(supplier_portal::admin::review),
                &permissions::supplier_portal::ADMIN_REVIEW,
            ),
        )
}

/// 审核辅助字典与重复候选仍受专项读取权限及领域范围约束。
fn review_routes() -> Router<HttpState> {
    Router::new()
        .route(
            "/offerings/{id}/impacts",
            permission_route(
                get(supplier_portal::admin::impacts),
                &permissions::supplier_portal::ADMIN_IMPACTS,
            ),
        )
        .route(
            "/applications/{id}/files/{file_id}/download",
            permission_route(
                get(supplier_portal::assets::review_download),
                &permissions::supplier_portal::ADMIN_IMPACTS,
            ),
        )
        .route(
            "/dictionaries/{kind}",
            permission_route(
                get(supplier_portal::admin::dictionaries),
                &permissions::supplier_portal::ADMIN_REVIEW,
            ),
        )
        .route(
            "/applications/{id}/duplicates",
            permission_route(
                get(supplier_portal::admin::duplicates),
                &permissions::supplier_portal::ADMIN_REVIEW,
            ),
        )
}
