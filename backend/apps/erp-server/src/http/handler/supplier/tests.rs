use axum::body::Body;
use axum::http::{Method, Request, StatusCode};
use config::{Config, SafeConfig};
use mongodb::Client;
use mongodb::options::{ClientOptions, ServerAddress};
use storage::{S3Storage, S3StorageConfig};
use tower::ServiceExt;

use crate::http::routes;
use crate::http::state::HttpState;

// 为供应商上传认证测试构造本地 HTTP 状态；测试必须在触碰 MongoDB 和对象存储之前结束。
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
        secret = "supplier-assets-unit-test-secret-at-least-32-bytes"
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
    let options = ClientOptions::builder()
        .hosts(vec![ServerAddress::Tcp { host: "127.0.0.1".into(), port: Some(1) }])
        .build();
    let db = Client::with_options(options).unwrap().database("unit_test");
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

// 向生产供应商上传路由提交未认证请求，确认身份校验先于业务资格预检及文件处理。
#[tokio::test]
async fn asset_routes_require_authentication_before_upload_processing() {
    let app = routes::create(state());
    for (method, path) in [
        (Method::POST, "/admin/supplier-profiles/with-assets"),
        (Method::PUT, "/admin/supplier-profiles/supplier-1/with-assets"),
    ] {
        let request = Request::builder()
            .method(method)
            .uri(path)
            .header("content-type", "multipart/form-data; boundary=test-boundary")
            .body(Body::from("invalid multipart payload"))
            .unwrap();
        let response = app.clone().oneshot(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED, "{path}");
    }
}
