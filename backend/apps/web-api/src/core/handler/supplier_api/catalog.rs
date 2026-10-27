//! 供应商商品目录 HTTP 入口。
use application_core::AuditActor;
use axum::Extension;
use axum::extract::{Path, State};
use erp_read_models::supplier_center::supplier_api::{CatalogQuery, CatalogView};
use erp_supply::dto::supplier_catalog::{CatalogIdentityReviewView, ReviewCatalogIdentityRequest};

use crate::app_state::AppState;
use crate::core::errors::Result;
use crate::core::extractor::{Json, Query};
use crate::core::response::ApiResponse;

#[permission_macros::permission(
    group = "API 供应商连接",
    group_desc = "供应商 API 连接与能力治理（W20）",
    desc = "查看供应商 API 连接详情",
    resource = "supplier_api_connection",
    action = "detail"
)]
/// 分页查看连接已同步的供应商商品。
/// # 参数
/// 状态为组合根，路径为连接身份，查询为分页。
/// # 返回
/// 目录观察、同步进度和运行记录。
/// # 错误
/// 无权限、连接不存在、非法分页或存储失败时返回统一错误。
pub async fn list(
    State(state): State<AppState>,
    Extension(actor): Extension<AuditActor>,
    Path(id): Path<String>,
    Query(query): Query<CatalogQuery>,
) -> Result<CatalogView> {
    Ok(ApiResponse::ok_with_data(state.supplier_api_read_service().catalog(&id, query, &actor).await?))
}

#[permission_macros::permission(
    group = "API 供应商连接",
    group_desc = "供应商 API 连接与能力治理（W20）",
    desc = "查看供应商 API 连接详情",
    resource = "supplier_api_connection",
    action = "detail"
)]
/// 读取来源关键规格的人工核对资料。
/// # 参数
/// 已认证操作人及连接、目录身份。
/// # 返回
/// 来源与公司规格和单位，不含采购成本。
/// # 错误
/// 越权、越范围或关联缺失时拒绝。
pub async fn identity_review(
    State(state): State<AppState>,
    Extension(actor): Extension<AuditActor>,
    Path((id, item)): Path<(String, String)>,
) -> Result<CatalogIdentityReviewView> {
    Ok(ApiResponse::ok_with_data(
        state.supplier_api_governance_process().catalog_identity_review(&id, &item, &actor).await?,
    ))
}

#[permission_macros::permission(
    group = "API 供应商连接",
    group_desc = "供应商 API 连接与能力治理（W20）",
    desc = "同步供应商商品目录",
    resource = "supplier_api_connection",
    action = "catalog_sync"
)]
/// 提交同一销售项及单位的人工作业确认。
/// # 参数
/// 当前认证账号、来源路径和明确核对请求。
/// # 返回
/// 保留现有公司 SKU 身份的核对结果。
/// # 错误
/// 未确认、版本变化、单位不同或权限不足时拒绝。
pub async fn confirm_identity_review(
    State(state): State<AppState>,
    Extension(actor): Extension<AuditActor>,
    Path((id, item)): Path<(String, String)>,
    Json(request): Json<ReviewCatalogIdentityRequest>,
) -> Result<CatalogIdentityReviewView> {
    Ok(ApiResponse::ok_with_data(
        state.supplier_api_governance_process().review_catalog_identity(&id, &item, request, &actor).await?,
    ))
}
