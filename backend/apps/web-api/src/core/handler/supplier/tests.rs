use application_core::AuditActor;
use axum::Extension;
use axum::body::Body;
use axum::extract::{FromRequest, Multipart, Path, State};
use axum::http::Request;
use config::{Config, SafeConfig};
use erp_core::AccountKind;
use mongodb::Client;
use mongodb::options::{ClientOptions, ServerAddress};
use serde_json::json;
use storage::{S3Storage, S3StorageConfig};

use super::{supplier_profile_create_with_assets, supplier_profile_update_with_assets};
use crate::app_state::AppState;
use crate::core::errors::Error;

/// 只构造客户端；命令在输入校验阶段返回，不访问数据库或对象存储。
fn state() -> AppState {
    let config = Config::from_toml_str(
        r#"
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
    AppState::new(db, SafeConfig::new(config), storage)
}

async fn multipart() -> Multipart {
    let command = json!({
        "idempotency_key": "supplier-assets-unit-test", "legal_name": "测试供应商",
        "contact": {"contact_name": "测试联系人", "mobile": "13800000000"},
        "clear_contact": true,
        "settlement_mode": "monthly", "reconciliation_cycle": "none",
        "payment_term_snapshot": "PERIOD_MONTH_3", "invoice_type": "vat_special",
        "signing_entity_party_id": "party-1", "payment_entity_party_id": "party-1",
        "capability_codes": [], "qualifications": [],
        "effective_from": "2026-10-20", "change_reason": "测试"
    });
    let body = format!(
        "--test-boundary\r\nContent-Disposition: form-data; name=\"command\"\r\n\r\n{command}\r\n--test-boundary--\r\n"
    );
    let request = Request::builder()
        .method("POST")
        .header("content-type", "multipart/form-data; boundary=test-boundary")
        .body(Body::from(body))
        .unwrap();
    Multipart::from_request(request, &()).await.unwrap()
}

#[tokio::test]
async fn asset_handlers_inject_authorization_before_business_validation() {
    let state = state();
    let actor = AuditActor::new("buyer-1".into(), "buyer".into(), AccountKind::Admin);
    let created = supplier_profile_create_with_assets(
        State(state.clone()),
        Extension(actor.clone()),
        multipart().await,
    )
    .await;
    let updated = supplier_profile_update_with_assets(
        State(state),
        Extension(actor),
        Path("supplier-1".into()),
        multipart().await,
    )
    .await;
    for result in [created, updated] {
        assert!(matches!(
            result,
            Err(Error::BadRequest(message)) if message == "联系人不能同时替换和清空"
        ));
    }
}
