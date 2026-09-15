//! 导入沿用供应商创建权限，每行仍由服务端执行全部根命令校验。
use application_core::AuditActor;
use axum::extract::State;
use axum::{Extension, Json};
use erp_supplier::dto::import::{SupplierImportRequest, SupplierImportResult};

use crate::app_state::AppState;
use crate::core::errors::Result;
use crate::core::response::ApiResponse;

#[permission_macros::permission(
    group = "供应商",
    group_desc = "供应商角色、商务结算版本、能力与资质管理",
    desc = "导入供应商资料",
    resource = "supplier",
    action = "create"
)]
/// 返回各行成功、重复、失败或结果待确认状态。
///
/// # Errors
/// 批次不符合大小限制时拒绝请求；各行业务错误留在逐行响应中。
pub async fn supplier_import(
    State(state): State<AppState>,
    Extension(actor): Extension<AuditActor>,
    Json(request): Json<SupplierImportRequest>,
) -> Result<Vec<SupplierImportResult>> {
    Ok(ApiResponse::ok_with_data(state.supplier_profile_service().import(request, &actor).await?))
}

/// 提交供应商后台导入任务。
///
/// 返回持久化任务；参数、异载荷冲突、存储及数据库失败返回统一错误。
#[permission_macros::permission(
    group = "供应商",
    group_desc = "供应商角色、商务结算版本、能力与资质管理",
    desc = "提交供应商导入任务",
    resource = "supplier",
    action = "create"
)]
pub async fn supplier_import_submit(
    State(state): State<AppState>,
    Extension(actor): Extension<AuditActor>,
    Json(request): Json<erp_supplier::dto::import_job::SupplierImportJobRequest>,
) -> Result<erp_support::BackgroundJobView> {
    Ok(ApiResponse::ok_with_data(state.supplier_import_process().submit(request, &actor).await?))
}

/// 下载供应商导入待处理行，仅原提交人可读取。
///
/// 返回修正重导所需行；越权、任务未结束或源文件读取失败时拒绝。
#[permission_macros::permission(
    group = "供应商",
    group_desc = "供应商角色、商务结算版本、能力与资质管理",
    desc = "下载供应商导入待处理行",
    resource = "supplier",
    action = "create"
)]
pub async fn supplier_import_failures(
    State(state): State<AppState>,
    Extension(actor): Extension<AuditActor>,
    axum::extract::Path(id): axum::extract::Path<String>,
) -> Result<erp_supplier::dto::import_job::SupplierImportFailures> {
    Ok(ApiResponse::ok_with_data(state.supplier_import_process().failures(&id, &actor).await?))
}
