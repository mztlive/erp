use std::time::Duration;

use axum::{
    extract::{DefaultBodyLimit, State},
    http::{header::HeaderName, StatusCode},
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
    app_state::{AppState, ExternalConnectorReadiness},
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
mod party;
mod payable;
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
        .route("/ready", get(readiness))
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
                    .allow_headers(Any)
                    .expose_headers([HeaderName::from_static("x-trace-id")]),
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

/// 就绪检查；外部连接器未配置时返回 503，并公开各能力的失败关闭状态。
pub async fn readiness(State(state): State<AppState>) -> ApiResponse<ExternalConnectorReadiness> {
    readiness_response(state.external_connector_readiness())
}

fn readiness_response(readiness: ExternalConnectorReadiness) -> ApiResponse<ExternalConnectorReadiness> {
    let ready = readiness.is_ready();
    ApiResponse {
        status: if ready { 200 } else { 503 },
        message: if ready {
            "OK".to_string()
        } else {
            "外部连接器尚未全部配置，相关能力保持失败关闭".to_string()
        },
        code: (!ready).then(|| "EXTERNAL_CONNECTOR_NOT_READY".to_string()),
        field_errors: None,
        retryable: Some(false),
        data: Some(readiness),
        success: ready,
    }
}
