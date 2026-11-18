//! 品牌后台导入边界，统一任务可在页面关闭后继续执行。
use application_core::AuditActor;
use axum::Extension;
use axum::extract::{Path, State};
use erp_catalog::dto::{BrandImportFailures, BrandImportJobRequest};
use erp_support::BackgroundJobView;

use crate::http::errors::Result;
use crate::http::extractor::Json;
use crate::http::response::ApiResponse;
use crate::http::state::HttpState;

/// 持久化品牌导入任务，返回已登记的任务。
/// 统一后台任务视图。
/// 参数错误、权限不足、异载荷冲突、持久化失败。
pub async fn submit(
    State(state): State<HttpState>,
    Extension(actor): Extension<AuditActor>,
    Json(request): Json<BrandImportJobRequest>,
) -> Result<BackgroundJobView> {
    Ok(ApiResponse::ok_with_data(state.application().brand_import_process().submit(request, &actor).await?))
}

/// 读取原提交人已结束任务的待处理行。
/// 可供修正重导的源行与失败说明。
/// 任务不存在、非原提交人、权限失效、任务未结束或源内容不可读。
pub async fn failures(
    State(state): State<HttpState>,
    Extension(actor): Extension<AuditActor>,
    Path(id): Path<String>,
) -> Result<BrandImportFailures> {
    Ok(ApiResponse::ok_with_data(state.application().brand_import_process().failures(&id, &actor).await?))
}

#[cfg(test)]
mod tests {
    use erp_catalog::dto::BrandImportJobRequest;
    #[test]
    fn background_request_keeps_file_name_original_rows_and_optional_code() {
        let request: BrandImportJobRequest = serde_json::from_value(serde_json::json!({
            "request_id":"12345678-1234-4234-8234-123456789abc", "file_name":"品牌.xlsx",
            "rows":[{"row_number":2,"name":"品牌"}]
        }))
        .unwrap();
        assert!(request.key().is_ok());
        assert!(request.rows[0].brand_code.is_empty());
        assert!(serde_json::from_value::<BrandImportJobRequest>(serde_json::json!({"rows":[]})).is_err());
    }
}
