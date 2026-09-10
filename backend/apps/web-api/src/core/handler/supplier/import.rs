//! 导入沿用供应商创建权限，每行仍由服务端执行全部根命令校验。
use crate::{
    app_state::AppState,
    core::{errors::Result, response::ApiResponse},
};
use application_core::AuditActor;
use axum::{extract::State, Extension, Json};
use erp_supplier::dto::import::{SupplierImportRequest, SupplierImportResult};

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
    Ok(ApiResponse::ok_with_data(
        state.supplier_profile_service().import(request, &actor).await?,
    ))
}
