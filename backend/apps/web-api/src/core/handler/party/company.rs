//! 我方公司专用权限边界。
use application_core::AuditActor;
use axum::extract::{Path, Query, State};
use axum::{Extension, Json};
use erp_party::PageView;
use erp_party::dto::company::{CompanyListParams, CompanyView, SaveCompanyRequest};

use crate::app_state::AppState;
use crate::core::errors::Result;
use crate::core::response::ApiResponse;

#[permission_macros::permission(
    group = "公司主体",
    group_desc = "我方公司及导入别名维护",
    desc = "查询公司主体",
    resource = "company",
    action = "list"
)]
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

#[permission_macros::permission(
    group = "公司主体",
    group_desc = "我方公司及导入别名维护",
    desc = "查看公司主体",
    resource = "company",
    action = "detail"
)]
/// 回显指定公司。
///
/// # Errors
/// 公司不存在或查询失败时返回统一错误。
pub async fn company_detail(State(state): State<AppState>, Path(id): Path<String>) -> Result<CompanyView> {
    Ok(ApiResponse::ok_with_data(state.party_service().company_detail(&id).await?))
}

#[permission_macros::permission(
    group = "公司主体",
    group_desc = "我方公司及导入别名维护",
    desc = "新建公司主体",
    resource = "company",
    action = "create"
)]
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

#[permission_macros::permission(
    group = "公司主体",
    group_desc = "我方公司及导入别名维护",
    desc = "维护公司主体",
    resource = "company",
    action = "update"
)]
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
