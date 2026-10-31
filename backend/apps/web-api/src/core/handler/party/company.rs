//! 我方公司专用权限边界。
use application_core::AuditActor;
use axum::Extension;
use axum::extract::{Path, State};
use erp_party::PageView;
use erp_party::dto::company::{CompanyListParams, CompanyView, SaveCompanyRequest};

use crate::app_state::AppState;
use crate::core::errors::Result;
use crate::core::extractor::{Json, Query};
use crate::core::response::ApiResponse;

/// 查询公司列表。
///
/// # Errors
/// 查询失败时返回统一错误。
pub async fn company_list(
    State(state): State<AppState>,
    Query(params): Query<CompanyListParams>,
) -> Result<PageView<CompanyView>> {
    Ok(ApiResponse::ok_with_data(state.party_service().company_list(&params).await?))
}

/// 回显指定公司。
///
/// # Errors
/// 公司不存在或查询失败时返回统一错误。
pub async fn company_detail(State(state): State<AppState>, Path(id): Path<String>) -> Result<CompanyView> {
    Ok(ApiResponse::ok_with_data(state.party_service().company_detail(&id).await?))
}

/// 创建公司身份及名称修订。
///
/// # Errors
/// 校验、冲突或事务错误按标准合同返回。
pub async fn company_create(
    State(state): State<AppState>,
    Extension(actor): Extension<AuditActor>,
    Json(req): Json<SaveCompanyRequest>,
) -> Result<CompanyView> {
    Ok(ApiResponse::ok_with_data(state.party_service().create_company(req, &actor).await?))
}

/// 更新公司身份与启停状态。
///
/// # Errors
/// 校验、版本冲突或事务错误按标准合同返回。
pub async fn company_update(
    State(state): State<AppState>,
    Extension(actor): Extension<AuditActor>,
    Path(id): Path<String>,
    Json(req): Json<SaveCompanyRequest>,
) -> Result<CompanyView> {
    Ok(ApiResponse::ok_with_data(state.party_service().update_company(&id, req, &actor).await?))
}
