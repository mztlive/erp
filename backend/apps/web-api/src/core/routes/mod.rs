use std::{path::Path, time::Duration};

use axum::{
    extract::DefaultBodyLimit,
    http::StatusCode,
    middleware,
    routing::{get, post},
    Extension, Router,
};
use tower::ServiceBuilder;
use tower_http::{
    cors::{Any, CorsLayer},
    services::ServeDir,
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
mod supplier_catalog;
mod supplier_fulfillment;
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
    let upload_path = app_state.upload_path().to_path_buf();
    let upload = post(upload_handler::upload_file).route_layer(middleware::from_fn_with_state(
        upload::limiter(),
        enforce_admission,
    ));
    let upload_routes = Router::new()
        .route("/upload", upload)
        .route_layer(middleware::from_fn_with_state(app_state.clone(), authenticate))
        .layer(DefaultBodyLimit::max(MAX_MULTIPART_REQUEST_BYTES))
        .layer(Extension(upload::write_lock()))
        .with_state(app_state.clone());
    let api_routes = Router::new()
        .route("/health", get(health))
        .merge(public::routes(app_state.clone()))
        .nest("/account", account::routes(app_state.clone()))
        .nest("/admin", admin::routes(app_state.clone()))
        .merge(upload_routes)
        .nest_service("/uploads", upload_files(upload_path));

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

/// 构建不追加目录首页的只读上传文件服务。
///
/// `ServeDir` 只接受 GET/HEAD；关闭目录 `index.html` 追加可确保该挂载点仅按
/// 随机文件名读取，不提供目录入口。
fn upload_files(path: impl AsRef<Path>) -> ServeDir {
    ServeDir::new(path).append_index_html_on_directories(false)
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

#[cfg(test)]
mod tests {
    use axum::{
        body::Body,
        http::{Method, Request, StatusCode},
    };
    use tower::Service;

    use super::upload_files;

    #[tokio::test]
    async fn uploaded_files_are_readable_without_directory_entry_or_write_methods() {
        let directory =
            std::env::temp_dir().join(format!("rs-project-template-upload-{}", uuid::Uuid::new_v4()));
        tokio::fs::create_dir_all(&directory).await.unwrap();
        tokio::fs::write(directory.join("picture.png"), b"image")
            .await
            .unwrap();
        tokio::fs::write(directory.join("index.html"), b"index")
            .await
            .unwrap();

        let read = upload_files(&directory)
            .call(
                Request::builder()
                    .uri("/picture.png")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        let directory_entry = upload_files(&directory)
            .call(Request::builder().uri("/").body(Body::empty()).unwrap())
            .await
            .unwrap();
        let write = upload_files(&directory)
            .call(
                Request::builder()
                    .method(Method::POST)
                    .uri("/picture.png")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(read.status(), StatusCode::OK);
        assert_eq!(directory_entry.status(), StatusCode::NOT_FOUND);
        assert_eq!(write.status(), StatusCode::METHOD_NOT_ALLOWED);
        tokio::fs::remove_dir_all(directory).await.unwrap();
    }
}
