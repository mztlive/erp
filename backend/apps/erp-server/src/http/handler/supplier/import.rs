//! 导入沿用供应商创建权限，每行仍由服务端执行全部根命令校验。
use application_core::AuditActor;
use axum::Extension;
use axum::extract::State;
use erp_supplier::dto::import::{SupplierImportRequest, SupplierImportResult};

use crate::http::errors::Result;
use crate::http::extractor::Json;
use crate::http::response::ApiResponse;
use crate::http::state::HttpState;

/// 返回各行成功、重复、失败或结果待确认状态。
///
/// # Errors
///
/// 批次不符合大小限制时拒绝请求；各行业务错误留在逐行响应中。
pub async fn supplier_import(
    State(state): State<HttpState>,
    Extension(actor): Extension<AuditActor>,
    Json(request): Json<SupplierImportRequest>,
) -> Result<Vec<SupplierImportResult>> {
    Ok(ApiResponse::ok_with_data(
        state.application().supplier_profile_process().import(request, &actor).await?,
    ))
}

/// 登记供应商文件导入并返回持久化任务，供后台继续处理；参数、重用命令身份但载荷不同、存储或数据库失败返回统一错误。
pub async fn supplier_import_submit(
    State(state): State<HttpState>,
    Extension(actor): Extension<AuditActor>,
    Json(request): Json<erp_supplier::dto::import_job::SupplierImportJobRequest>,
) -> Result<erp_support::BackgroundJobView> {
    Ok(ApiResponse::ok_with_data(
        state.application().supplier_import_process().submit(request, &actor).await?,
    ))
}

/// 原提交人可在导入结束后下载待处理行，供修正后重导；越权、任务未结束或读取源文件失败时拒绝。
pub async fn supplier_import_failures(
    State(state): State<HttpState>,
    Extension(actor): Extension<AuditActor>,
    axum::extract::Path(id): axum::extract::Path<String>,
) -> Result<erp_supplier::dto::import_job::SupplierImportFailures> {
    Ok(ApiResponse::ok_with_data(state.application().supplier_import_process().failures(&id, &actor).await?))
}
