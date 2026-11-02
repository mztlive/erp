//! 选品册复制协议入口。

use application_core::AuditActor;
use axum::Extension;
use axum::extract::{Path, State};
use erp_sales::dto::sales_selection::{CopySalesSelectionBookletRequest, SalesSelectionBookletView};

use super::{access, process};
use crate::app_state::AppState;
use crate::core::errors::Result;
use crate::core::extractor::Json;
use crate::core::handler::customer::ensure_customer_access;
use crate::core::response::ApiResponse;

/// 为重新选择的客户复制选品册。
///
/// # 参数
/// * `state` - 应用状态
/// * `actor` - 已认证操作人
/// * `id` - 原册身份
/// * `req` - 新客户与原册版本
///
/// # 返回
/// 返回新册详情。
///
/// # 错误
/// 原册或新客户越权、版本冲突、客户停用或复制失败时拒绝。
pub async fn booklet_copy(
    State(state): State<AppState>,
    Extension(actor): Extension<AuditActor>,
    Path(id): Path<String>,
    Json(req): Json<CopySalesSelectionBookletRequest>,
) -> Result<SalesSelectionBookletView> {
    access::booklet(&state, &actor, &id).await?;
    ensure_customer_access(&state, &actor, "detail", &req.customer_id).await?;
    Ok(ApiResponse::ok_with_data(process(&state).copy(id, req, actor).await?))
}

#[cfg(test)]
mod tests {
    use axum::body::Body;
    use axum::extract::FromRequest;
    use axum::http::{Request, StatusCode};
    use serde_json::json;

    use super::*;

    #[tokio::test]
    async fn copy_http_accepts_only_new_customer_and_source_version() {
        let payload = json!({
            "expected_version": 3, "customer_id": "new-customer", "idempotency_key": "copy-1"
        });
        let request = Request::builder()
            .header("content-type", "application/json")
            .body(Body::from(payload.to_string()))
            .unwrap();
        let Json(decoded) =
            Json::<CopySalesSelectionBookletRequest>::from_request(request, &()).await.unwrap();
        assert_eq!(decoded.customer_id, "new-customer");
        assert_eq!(decoded.expected_version, 3);
        assert_eq!(decoded.idempotency_key, "copy-1");

        for extra in ["sales_owner_user_id", "pool_filter", "items", "access_password_hash"] {
            let mut forged = payload.clone();
            forged[extra] = json!("forged");
            let request = Request::builder()
                .header("content-type", "application/json")
                .body(Body::from(forged.to_string()))
                .unwrap();
            let error =
                Json::<CopySalesSelectionBookletRequest>::from_request(request, &()).await.unwrap_err();
            assert_eq!(error.status(), StatusCode::UNPROCESSABLE_ENTITY);
        }
        for field in ["customer_id", "expected_version", "idempotency_key"] {
            let mut missing = payload.clone();
            missing.as_object_mut().unwrap().remove(field);
            let request = Request::builder()
                .header("content-type", "application/json")
                .body(Body::from(missing.to_string()))
                .unwrap();
            assert!(Json::<CopySalesSelectionBookletRequest>::from_request(request, &()).await.is_err());
        }
    }
}
