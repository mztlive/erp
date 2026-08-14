use std::time::Duration;

use axum::{
    extract::DefaultBodyLimit,
    http::StatusCode,
    middleware,
    routing::{get, post},
    Router,
};
use tower::ServiceBuilder;
use tower_http::{
    cors::{Any, CorsLayer},
    timeout::TimeoutLayer,
};

use crate::{
    app_state::AppState,
    core::{
        errors::Result,
        handler::upload as upload_handler,
        middleware::authenticate,
        response::ApiResponse,
        tracing::trace_middleware,
        upload::{self, enforce_admission, MAX_MULTIPART_REQUEST_BYTES},
    },
};

mod account;
mod admin;
mod public;

mod access_control;
mod approval_instance;
mod bulk_job;
mod card_instance;
mod catalog;
mod contract;
mod cost;
mod customer;
mod document_registry;
mod file_asset;
mod fulfillment;
mod integration_ops;
mod inventory;
mod legacy_import;
mod mall_after_sales;
mod mall_backfill;
mod mall_order;
mod mall_sync;
mod party;
mod payable;
mod projection;
mod publication;
mod purchase_order;
mod receivable;
mod returns;
mod sales_order;
mod sales_review;
mod source_registry;
mod supplier;
mod supplier_api;
mod supplier_fulfillment;
mod supplier_offering;
mod supplier_settlement;
mod warehouse;
mod work_item;

/// 覆盖 MongoDB 最长 120 秒提交确认窗口，避免服务端先返回 408 而事务随后提交。
const REQUEST_TIMEOUT: Duration = Duration::from_secs(180);

/// 构建应用主路由。
///
/// # 参数
/// * `app_state` - 应用状态
///
/// # 返回
/// 返回 `Router` 实例。
pub fn create(app_state: AppState) -> Router {
    let upload = post(upload_handler::upload_file).route_layer(middleware::from_fn_with_state(
        upload::limiter(),
        enforce_admission,
    ));
    let upload_routes = Router::new()
        .route("/upload", upload)
        .route_layer(middleware::from_fn_with_state(app_state.clone(), authenticate))
        .layer(DefaultBodyLimit::max(MAX_MULTIPART_REQUEST_BYTES))
        .with_state(app_state.clone());
    let api_routes = Router::new()
        .route("/health", get(health))
        .merge(public::routes(app_state.clone()))
        .nest("/account", account::routes(app_state.clone()))
        .nest("/admin", admin::routes(app_state.clone()))
        .merge(upload_routes);

    Router::new().merge(api_routes).with_state(app_state).layer(
        ServiceBuilder::new()
            .layer(middleware::from_fn(trace_middleware))
            .layer(TimeoutLayer::with_status_code(
                StatusCode::REQUEST_TIMEOUT,
                REQUEST_TIMEOUT,
            ))
            .layer(
                CorsLayer::new()
                    .allow_origin(Any)
                    .allow_methods(Any)
                    .allow_headers(Any),
            ),
    )
}

/// 健康检查
///
/// # 返回
/// 返回执行结果，`Ok` 表示成功，`Err` 表示失败。
///
/// # 错误
/// 当内部逻辑或依赖操作失败时返回错误。
pub async fn health() -> Result<()> {
    Ok(ApiResponse::<()>::ok())
}
