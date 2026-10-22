//! 采购内部报价、费用依据和附件使用独立权限及当前任务访问。
use application_core::AuditActor;
use axum::Extension;
use axum::extract::{Path, State};
use axum::response::Response;
use erp_processes::sales_inquiry::dto::InquiryMutationView;
use erp_processes::sales_inquiry::internal_quote::{SaveInquiryFeeBasisCommand, SaveInquiryQuoteCommand};
use erp_processes::sales_inquiry::quote_import::ImportInquiryQuotesCommand;
use erp_processes::sales_inquiry::read::{InquiryFeeBasisParams, InquiryReadService};
use erp_supply::inquiry::InquiryFeeBasis;

use crate::app_state::AppState;
use crate::core::errors::{Error, Result};
use crate::core::extractor::{Json, Query};
use crate::core::handler::file_asset::{asset_download_response, read_asset, revalidate_asset};
use crate::core::response::ApiResponse;

#[permission_macros::permission(
    group = "采购询价",
    group_desc = "询价需求与采购反馈协作",
    desc = "导入供应商回填报价",
    resource = "procurement_inquiry",
    action = "internal_quote"
)]
/// 原子导入核对后的供应商回填报价。
/// # 参数
/// 当前应用、采购身份、询价与包含任务版本的批量报价。
/// # 返回
/// 返回可按原命令恢复的安全回执。
/// # 错误
/// 任一行数据、任务、供应资格或需求版本不符时整批拒绝。
pub async fn import_quotes(
    State(state): State<AppState>,
    Extension(actor): Extension<AuditActor>,
    Path(id): Path<String>,
    Json(request): Json<ImportInquiryQuotesCommand>,
) -> Result<InquiryMutationView> {
    Ok(ApiResponse::ok_with_data(
        state.sales_inquiry_process().import_internal_quotes(&actor, &id, request).await?,
    ))
}

#[permission_macros::permission(
    group = "采购询价",
    group_desc = "询价需求与采购反馈协作",
    desc = "保存询价内部报价",
    resource = "procurement_inquiry",
    action = "internal_quote"
)]
/// 追加当前责任商品的内部报价版本，不隐式修改通用供给。
/// # 参数
/// `state`、`actor` 为当前应用及采购，`id` 为询价，`request` 为带版本及任务的报价。
/// # 返回
/// 返回安全询价命令回执。
/// # 错误
/// 任务、权限、供应资格、报价范围或版本不符时拒绝。
pub async fn save_quote(
    State(state): State<AppState>,
    Extension(actor): Extension<AuditActor>,
    Path(id): Path<String>,
    Json(request): Json<SaveInquiryQuoteCommand>,
) -> Result<InquiryMutationView> {
    Ok(ApiResponse::ok_with_data(
        state.sales_inquiry_process().save_internal_quote(&actor, &id, request).await?,
    ))
}

#[permission_macros::permission(
    group = "采购询价",
    group_desc = "询价需求与采购反馈协作",
    desc = "保存询价费用内部依据",
    resource = "procurement_inquiry",
    action = "internal_quote"
)]
/// 保存明确费用的内部收款依据，公开反馈只保留对销售金额及说明。
/// # 参数
/// `state`、`actor` 为当前应用及采购，`id` 为询价，`request` 为责任任务和费用内部依据。
/// # 返回
/// 返回安全询价命令回执。
/// # 错误
/// 任务、费用范围、供应商或付款条件非法时拒绝。
pub async fn save_fee_basis(
    State(state): State<AppState>,
    Extension(actor): Extension<AuditActor>,
    Path(id): Path<String>,
    Json(request): Json<SaveInquiryFeeBasisCommand>,
) -> Result<InquiryMutationView> {
    Ok(ApiResponse::ok_with_data(state.sales_inquiry_process().save_fee_basis(&actor, &id, request).await?))
}

#[permission_macros::permission(
    group = "采购询价",
    group_desc = "询价需求与采购反馈协作",
    desc = "读取询价费用内部依据",
    resource = "procurement_inquiry",
    action = "internal_quote"
)]
/// 读取当前责任范围内明确轮次的内部费用依据。
/// # 参数
/// `state`、`actor` 为当前应用及采购，`id` 为询价，`params` 为当前任务及反馈轮次。
/// # 返回
/// 返回责任范围内费用收款资料。
/// # 错误
/// 权限、当前任务、轮次或范围失效时拒绝。
pub async fn fee_bases(
    State(state): State<AppState>,
    Extension(actor): Extension<AuditActor>,
    Path(id): Path<String>,
    Query(params): Query<InquiryFeeBasisParams>,
) -> Result<Vec<InquiryFeeBasis>> {
    Ok(ApiResponse::ok_with_data(
        InquiryReadService::new(state.db(), state.rbac()).fee_bases(&actor, &id, &params).await?,
    ))
}

#[permission_macros::permission(
    group = "采购询价",
    group_desc = "询价需求与采购反馈协作",
    desc = "下载当前责任费用依据附件",
    resource = "procurement_inquiry",
    action = "internal_quote"
)]
/// 在存储读取前后复核当前费用责任、反馈轮次及精确依据附件关系。
/// # 参数
/// `state`、`actor` 为当前应用及采购，路径含询价、依据及文件；查询含任务和反馈轮次。
/// # 返回
/// 返回禁止缓存及嗅探的附件下载，存储键不进入响应。
/// # 错误
/// 当前任务、费用范围、附件关系、文件治理或指纹失效时拒绝。
pub async fn fee_basis_attachment(
    State(state): State<AppState>,
    Extension(actor): Extension<AuditActor>,
    Path((id, basis_id, asset_id)): Path<(String, String, String)>,
    Query(params): Query<InquiryFeeBasisParams>,
) -> std::result::Result<Response, Error> {
    let service = InquiryReadService::new(state.db(), state.rbac());
    service.require_fee_basis_attachment(&actor, &id, &params, &basis_id, &asset_id).await?;
    let (view, bytes) = read_asset(&state, &actor, &asset_id).await?;
    service.require_fee_basis_attachment(&actor, &id, &params, &basis_id, &asset_id).await?;
    revalidate_asset(&state, &view).await?;
    asset_download_response(&view, bytes)
}

#[cfg(test)]
mod tests {
    use axum::body::Body;
    use axum::extract::FromRequest;
    use axum::http::Request;
    use serde_json::{Value, json};

    use super::*;

    fn payload() -> Value {
        json!({
            "command_key": "quote", "expected_version": 1, "task_id": "task", "task_version": 1,
            "quote": {"revision_no": 1, "line_id": "line", "supplier_id": "supplier",
                "supplier_offering_id": "offering", "internal_note": "",
                "terms": {"unit_cost_gross": "10", "input_tax_rate": "0.13", "available_quantity": "100",
                    "expected_delivery_date": "2026-10-30"}}
        })
    }

    async fn extract(value: Value) -> bool {
        let request = Request::builder()
            .header("content-type", "application/json")
            .body(Body::from(value.to_string()))
            .unwrap();
        Json::<SaveInquiryQuoteCommand>::from_request(request, &()).await.is_ok()
    }

    #[tokio::test]
    async fn internal_quote_accepts_optional_expiry_and_rejects_removed_fields() {
        assert!(extract(payload()).await);
        for expiry in [Value::Null, json!("2026-11-01")] {
            let mut value = payload();
            value["quote"]["terms"]["valid_to"] = expiry;
            assert!(extract(value).await);
        }
        for field in [
            "valid_from",
            "minimum_order_quantity",
            "payment_term_code",
            "allow_cumulative_batches",
            "address_limits",
        ] {
            let mut value = payload();
            value["quote"]["terms"][field] = json!("legacy");
            assert!(!extract(value).await, "{field}");
        }
        let mut value = payload();
        value["quote"]["attachment_ids"] = json!([]);
        assert!(!extract(value).await);
        let mut value = payload();
        value["quote"]["terms"]["address_limits"] = json!([]);
        assert!(!extract(value).await);
        let mut value = payload();
        value["quote"]["terms"]["unit_cost_gross"] = json!(1.23);
        assert!(!extract(value).await);
    }
    #[tokio::test]
    async fn quote_import_extracts_batch_and_rejects_injected_scope_and_numeric_money() {
        let mut value = payload();
        let quote = value.as_object_mut().unwrap().remove("quote").unwrap();
        value["requirement_version"] = json!(1);
        value["feedback_round"] = json!(1);
        value["rows"] = json!([{"source":"报价.xlsx 第5行", "sku_id":"sku", "quote":quote}]);
        for invalid in [false, true] {
            let mut body = value.clone();
            if invalid {
                body["rows"][0]["quote"]["terms"]["freight_gross"] = json!(10);
            }
            let request = Request::builder()
                .header("content-type", "application/json")
                .body(Body::from(body.to_string()))
                .unwrap();
            assert_eq!(
                Json::<ImportInquiryQuotesCommand>::from_request(request, &()).await.is_ok(),
                !invalid
            );
        }
        value["rows"][0]["quote"]["terms"]["address_limits"] = json!([]);
        let request = Request::builder()
            .header("content-type", "application/json")
            .body(Body::from(value.to_string()))
            .unwrap();
        assert!(Json::<ImportInquiryQuotesCommand>::from_request(request, &()).await.is_err());
    }
}
