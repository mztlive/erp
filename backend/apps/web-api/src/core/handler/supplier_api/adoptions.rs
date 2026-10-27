//! 供应商目录选品的冻结预览与逐项确认。

use application_core::AuditActor;
use axum::Extension;
use axum::extract::{Path, State};
use erp_processes::supplier_catalog_adoption::{
    AdoptionBatchResult, AdoptionPreviewView, AdoptionResolution,
};
use erp_supply::catalog_adoption::{ConfirmAdoptionRequest, PreviewAdoptionRequest, ResolveAdoptionRequest};

use crate::app_state::AppState;
use crate::core::errors::Result;
use crate::core::extractor::Json;
use crate::core::response::ApiResponse;

#[permission_macros::permission(
    group = "API 供应商连接",
    group_desc = "供应商 API 连接与能力治理（W20）",
    desc = "同步供应商商品目录",
    resource = "supplier_api_connection",
    action = "catalog_sync"
)]
/// 冻结本次采购明确勾选或完整筛选的来源规格。
/// # 参数
/// 路径为连接，载荷为选择模式，actor 为认证采购人员。
/// # 返回
/// 最多五百行的固定预览，可按每批五十行确认。
/// # 错误
/// 无权限、来源不存在、选择重复或超限时拒绝。
pub async fn preview(
    State(state): State<AppState>,
    Extension(actor): Extension<AuditActor>,
    Path(id): Path<String>,
    Json(request): Json<PreviewAdoptionRequest>,
) -> Result<AdoptionPreviewView> {
    Ok(ApiResponse::ok_with_data(
        state.supplier_catalog_adoption_process()?.preview(&id, request, &actor).await?,
    ))
}

#[permission_macros::permission(
    group = "API 供应商连接",
    group_desc = "供应商 API 连接与能力治理（W20）",
    desc = "同步供应商商品目录",
    resource = "supplier_api_connection",
    action = "catalog_sync"
)]
/// 按明确映射及条款逐项建立正式公司商品、供给和来源关系。
/// # 参数
/// 路径为连接及预览；载荷为预览版本、原始操作号和确认行。
/// # 返回
/// 每行真实成功结果或失败原因，预览版本保持冻结。
/// # 错误
/// 越权、过期、异载荷或请求不属于冻结范围时整体拒绝。
pub async fn confirm(
    State(state): State<AppState>,
    Extension(actor): Extension<AuditActor>,
    Path((id, preview)): Path<(String, String)>,
    Json(request): Json<ConfirmAdoptionRequest>,
) -> Result<AdoptionBatchResult> {
    Ok(ApiResponse::ok_with_data(
        state.supplier_catalog_adoption_process()?.confirm(&id, &preview, request, &actor).await?,
    ))
}

#[permission_macros::permission(
    group = "API 供应商连接",
    group_desc = "供应商 API 连接与能力治理（W20）",
    desc = "同步供应商商品目录",
    resource = "supplier_api_connection",
    action = "catalog_sync"
)]
/// 解析冻结来源商品的自动匹配结果。
/// # 参数
/// 路径为连接与预览；请求只包含建档字典和展示内容。
/// # 返回
/// 自动关联或新建的结果及提交重验摘要。
/// # 错误
/// 越权、过期、重复候选或身份冲突时拒绝。
pub async fn resolve(
    State(state): State<AppState>,
    Extension(actor): Extension<AuditActor>,
    Path((id, preview)): Path<(String, String)>,
    Json(request): Json<ResolveAdoptionRequest>,
) -> Result<AdoptionResolution> {
    Ok(ApiResponse::ok_with_data(
        state.supplier_catalog_adoption_process()?.resolve(&id, &preview, request, &actor).await?,
    ))
}

#[cfg(test)]
mod tests {
    use axum::body::Body;
    use axum::extract::FromRequest;
    use axum::http::Request;
    use erp_supply::dto::supplier_offering::SupplierOfferingTermsWrite;
    use serde_json::{Value, json};

    use super::*;

    fn request(value: Value) -> Request<Body> {
        Request::builder()
            .header("content-type", "application/json")
            .body(Body::from(value.to_string()))
            .unwrap()
    }

    #[tokio::test]
    async fn resolution_accepts_mapping_and_rejects_client_selected_sku() {
        let mut body = json!({"item_id":"source-item","mapping":{"category_id":"category","unit_id":"unit","product_kind":"PHYSICAL"}});
        let Json(decoded) =
            Json::<ResolveAdoptionRequest>::from_request(request(body.clone()), &()).await.unwrap();
        assert_eq!(decoded.item_id, "source-item");
        assert_eq!(decoded.mapping.unit_id, "unit");
        body["sku_id"] = json!("forced-sku");
        assert!(Json::<ResolveAdoptionRequest>::from_request(request(body), &()).await.is_err());
    }

    #[tokio::test]
    async fn confirmation_rejects_legacy_manual_target_at_http_boundary() {
        let terms = SupplierOfferingTermsWrite::new(
            "10".into(),
            "9".into(),
            "0.13".into(),
            "10".into(),
            "2026-10-27".into(),
        );
        let mut body = json!({"expected_version":1,"idempotency_key":"command","rows":[{
            "item_id":"source-item","target":{"kind":"AUTO","mapping":{"category_id":"cat","unit_id":"unit","product_kind":"PHYSICAL"},"expected_match":"reviewed"},
            "terms":terms,"delivery_template_id":null
        }]});
        assert!(Json::<ConfirmAdoptionRequest>::from_request(request(body.clone()), &()).await.is_ok());
        body["rows"][0]["target"] = json!({"kind":"EXISTING","sku_id":"forced-sku","expected_sku_version":1,"expected_product_version":1,"unit_id":"unit"});
        assert!(Json::<ConfirmAdoptionRequest>::from_request(request(body), &()).await.is_err());
    }
}
