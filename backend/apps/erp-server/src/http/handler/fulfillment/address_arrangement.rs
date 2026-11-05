//! 收货安排 HTTP 适配；销售范围和当前任务身份在根事务内重验。
use application_core::AuditActor;
use axum::Extension;
use axum::extract::{Path, State};
use erp_fulfillment_execution::fulfillment_execution::address_arrangement::{
    AddressArrangementView, SaveAddressArrangement,
};

use super::process;
use crate::http::errors::Result;
use crate::http::extractor::Json;
use crate::http::response::ApiResponse;
use crate::http::state::HttpState;

/// 查询销售收货安排。
/// # 参数
/// 应用状态、认证身份和销售单身份。
/// # 返回
/// 授权编辑上下文。
/// # 错误
/// 无销售范围权限或来源无效时拒绝。
pub async fn sales_get(
    State(state): State<HttpState>,
    Extension(actor): Extension<AuditActor>,
    Path(id): Path<String>,
) -> Result<AddressArrangementView> {
    Ok(ApiResponse::ok_with_data(process(&state).sales_address_arrangement(&id, &actor).await?))
}
/// 保存销售收货安排。
/// # 参数
/// 应用状态、认证身份、销售单身份和版本化输入。
/// # 返回
/// 成功响应。
/// # 错误
/// 版本、已发保护或权限校验失败时拒绝。
pub async fn sales_save(
    State(state): State<HttpState>,
    Extension(actor): Extension<AuditActor>,
    Path(id): Path<String>,
    Json(req): Json<SaveAddressArrangement>,
) -> Result<()> {
    process(&state).save_sales_address_arrangement(&id, req, &actor).await?;
    Ok(ApiResponse::ok_with_data(()))
}
/// 查询当前发货任务收货安排。
/// # 参数
/// 应用状态、认证身份和发货身份。
/// # 返回
/// 限定当前任务数量的编辑上下文。
/// # 错误
/// 非当前处理人或已发货时拒绝。
pub async fn delivery_get(
    State(state): State<HttpState>,
    Extension(actor): Extension<AuditActor>,
    Path(id): Path<String>,
) -> Result<AddressArrangementView> {
    Ok(ApiResponse::ok_with_data(process(&state).delivery_address_arrangement(&id, &actor).await?))
}
/// 保存当前任务收货安排。
/// # 参数
/// 应用状态、认证身份、发货身份和版本化输入。
/// # 返回
/// 成功响应。
/// # 错误
/// 身份、版本、数量或包裹重核确认不满足时拒绝。
pub async fn delivery_save(
    State(state): State<HttpState>,
    Extension(actor): Extension<AuditActor>,
    Path(id): Path<String>,
    Json(req): Json<SaveAddressArrangement>,
) -> Result<()> {
    process(&state).save_delivery_address_arrangement(&id, req, &actor).await?;
    Ok(ApiResponse::ok_with_data(()))
}

#[cfg(test)]
mod tests {
    use axum::body::Body;
    use axum::extract::{FromRequest, Request};
    use serde_json::{Value, json};

    use super::*;

    fn body() -> Value {
        json!({
            "expected_version": "revision:1:0:1",
            "lines": [{"line_id": "sales-line", "allocations": [{
                "recipient": "收货人", "phone": "13800000000",
                "region_code": "440305", "address": "测试路1号", "quantity": "2.500000"
            }]}],
            "reason": "", "reset_packages": false
        })
    }
    fn request(value: Value) -> Request {
        Request::builder()
            .header("content-type", "application/json")
            .body(Body::from(value.to_string()))
            .unwrap()
    }
    #[tokio::test]
    async fn accepts_decimal_allocations_and_empty_pending_addresses() {
        let Json(decoded) = Json::<SaveAddressArrangement>::from_request(request(body()), &()).await.unwrap();
        assert_eq!(decoded.lines[0].allocations[0].quantity.to_string(), "2.500000");
        let mut pending = body();
        pending["lines"][0]["allocations"] = json!([]);
        let Json(decoded) =
            Json::<SaveAddressArrangement>::from_request(request(pending), &()).await.unwrap();
        assert!(decoded.lines[0].allocations.is_empty());
    }
    #[tokio::test]
    async fn rejects_overridden_capacity_and_commercial_fields() {
        for field in ["quantity", "price", "shipped_addresses"] {
            let mut forged = body();
            forged["lines"][0][field] = json!("999");
            assert!(Json::<SaveAddressArrangement>::from_request(request(forged), &()).await.is_err());
        }
        let mut forged = body();
        forged["sales_order_id"] = json!("another-order");
        assert!(Json::<SaveAddressArrangement>::from_request(request(forged), &()).await.is_err());
    }
    #[tokio::test]
    async fn requires_version_and_explicit_package_confirmation() {
        for field in ["expected_version", "reset_packages"] {
            let mut incomplete = body();
            incomplete.as_object_mut().unwrap().remove(field);
            assert!(Json::<SaveAddressArrangement>::from_request(request(incomplete), &()).await.is_err());
        }
    }
}
