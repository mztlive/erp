//! 选品册客户售价 HTTP 适配。

use application_core::AuditActor;
use axum::Extension;
use axum::extract::{Path, State};
use erp_sales::dto::sales_selection::{SalesSelectionBookletView, UpdateSelectionPricesRequest};

use super::{access, process};
use crate::app_state::AppState;
use crate::core::errors::Result;
use crate::core::extractor::Json;
use crate::core::response::ApiResponse;

/// 调整当前准备批次的客户售价。
///
/// # 参数
/// `state` 为应用状态，`actor` 为已认证人，`id` 为册，`req` 为改价命令。
///
/// # 返回
/// 返回更新详情。
///
/// # 错误
/// 权限、范围、状态、版本、批次或金额无效时拒绝。
pub async fn booklet_update_prices(
    State(state): State<AppState>,
    Extension(actor): Extension<AuditActor>,
    Path(id): Path<String>,
    Json(req): Json<UpdateSelectionPricesRequest>,
) -> Result<SalesSelectionBookletView> {
    access::booklet(&state, &actor, &id).await?;
    Ok(ApiResponse::ok_with_data(process(&state).update_prices(id, req, actor).await?))
}

#[cfg(test)]
mod tests {
    use erp_sales::entity::sales_selection::SelectionPriceChange;

    use super::*;

    #[test]
    fn price_commands_require_explicit_decimal_strings_and_one_change() {
        let mut payload = serde_json::json!({
            "expected_version": 3, "batch_id": "batch", "idempotency_key": "price-key",
            "change": { "kind": "MARKUP", "percent": "10.25" }
        });
        let req: UpdateSelectionPricesRequest = serde_json::from_value(payload.clone()).unwrap();
        assert!(matches!(req.change, SelectionPriceChange::Markup { .. }));
        payload["change"]["percent"] = serde_json::json!(10.25);
        assert!(serde_json::from_value::<UpdateSelectionPricesRequest>(payload.clone()).is_err());
        payload["change"] = serde_json::json!({ "kind": "SINGLE", "sku_id": "sku", "price": "12.50" });
        assert!(serde_json::from_value::<UpdateSelectionPricesRequest>(payload.clone()).is_ok());
        payload["change"]["percent"] = serde_json::json!("10");
        assert!(serde_json::from_value::<UpdateSelectionPricesRequest>(payload.clone()).is_err());
        payload["change"] = serde_json::json!({ "kind": "MARKUP", "percent": "1.234" });
        assert!(serde_json::from_value::<UpdateSelectionPricesRequest>(payload).is_err());
    }
}
