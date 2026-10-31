//! 财务对账结案与风险释放的薄HTTP边界。

use application_core::AuditActor;
use axum::Extension;
use axum::extract::{Path, State};
use erp_finance::entity::supplier_deposit::{SupplierDepositReconciliation, SupplierDepositReservation};
use erp_processes::supplier_deposits::{ReleaseDepositReservation, ResolveDepositReconciliation};

use super::process;
use crate::app_state::AppState;
use crate::core::errors::Result;
use crate::core::extractor::Json;
use crate::core::response::ApiResponse;

/// 核实并结案原账户差异。
/// # 参数
/// 当前认证人员、原账户与对账路径以及完整核验命令。
/// # 返回
/// 返回保留原差异的结案记录。
/// # 错误
/// 越范围、版本变化或证据不足时拒绝。
pub async fn resolve(
    State(state): State<AppState>,
    Extension(actor): Extension<AuditActor>,
    Path((id, reconciliation)): Path<(String, String)>,
    Json(command): Json<ResolveDepositReconciliation>,
) -> Result<SupplierDepositReconciliation> {
    Ok(ApiResponse::ok_with_data(
        process(&state).resolve_reconciliation(&id, &reconciliation, command, &actor).await?,
    ))
}

/// 核实原执行风险后释放未使用尾款。
/// # 参数
/// 当前认证人员、原账户与占用路径及完整核验命令。
/// # 返回
/// 返回带不可变释放回执的原占用。
/// # 错误
/// 未知结果、越范围、版本变化或证据不足时拒绝。
pub async fn release(
    State(state): State<AppState>,
    Extension(actor): Extension<AuditActor>,
    Path((id, reservation)): Path<(String, String)>,
    Json(command): Json<ReleaseDepositReservation>,
) -> Result<SupplierDepositReservation> {
    Ok(ApiResponse::ok_with_data(
        process(&state).release_reservation(&id, &reservation, command, &actor).await?,
    ))
}

#[cfg(test)]
mod tests {
    use axum::body::Body;
    use axum::http::{Request, StatusCode};
    use config::{Config, SafeConfig};
    use mongodb::Client;
    use mongodb::options::ClientOptions;
    use storage::{S3Storage, S3StorageConfig};
    use tower::ServiceExt;

    use super::*;
    use crate::core::routes;

    // 只构造本地客户端；请求在认证边界结束，不访问数据库或对象存储。
    fn state() -> AppState {
        let settings = Config::from_toml_str(
            r#"
            [xxl_job]
admin_url = "http://admin.invalid/xxl-job-admin"
executor_url = "http://worker.invalid:10002"
app_name = "erp-test"
access_token = "test-token-123456"

[app]
            port=10001
            secret="unit-test-only-secret-at-least-32-bytes"
            [database]
            uri="mongodb://127.0.0.1:1/unit_test"
            db_name="unit_test"
            [s3]
            bucket="unit-test"
            region="unit-test"
            access_key_id="unit-test"
            secret_access_key="unit-test"
            public_base_url="http://127.0.0.1:1"
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
        AppState::new(db, SafeConfig::new(settings), storage)
    }

    #[tokio::test]
    async fn funds_resolution_commands_require_authentication_before_any_financial_access() {
        let state = state();
        let app = routes::create(state);
        for path in [
            "/admin/supplier-deposits/accounts/account/reconciliations/review/resolve",
            "/admin/supplier-deposits/accounts/account/reservations/reservation/release",
        ] {
            let response = app
                .clone()
                .oneshot(
                    Request::post(path)
                        .header("content-type", "application/json")
                        .body(Body::from("{}"))
                        .unwrap(),
                )
                .await
                .unwrap();
            assert_eq!(response.status(), StatusCode::UNAUTHORIZED, "{path}");
        }
    }
}
