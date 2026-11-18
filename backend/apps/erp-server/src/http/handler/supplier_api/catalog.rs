//! 供应商商品目录 HTTP 入口。
use application_core::AuditActor;
use axum::Extension;
use axum::extract::{Path, State};
use erp_read_models::supplier_center::supplier_api::{CatalogQuery, CatalogView};
use erp_supply::dto::supplier_catalog::{CatalogIdentityReviewView, ReviewCatalogIdentityRequest};

use crate::http::errors::Result;
use crate::http::extractor::{Json, Query};
use crate::http::response::ApiResponse;
use crate::http::state::HttpState;

/// 分页查看连接已同步的供应商商品。
/// 目录观察、同步进度和运行记录。
/// 无权限、连接不存在、非法分页或存储失败时返回统一错误。
pub async fn list(
    State(state): State<HttpState>,
    Extension(actor): Extension<AuditActor>,
    Path(id): Path<String>,
    Query(query): Query<CatalogQuery>,
) -> Result<CatalogView> {
    Ok(ApiResponse::ok_with_data(
        state.application().supplier_api_read_service().catalog(&id, query, &actor).await?,
    ))
}

/// 读取来源关键规格的人工核对资料。
/// 来源与公司规格和单位，不含采购成本。
/// 越权、越范围或关联缺失时拒绝。
pub async fn identity_review(
    State(state): State<HttpState>,
    Extension(actor): Extension<AuditActor>,
    Path((id, item)): Path<(String, String)>,
) -> Result<CatalogIdentityReviewView> {
    Ok(ApiResponse::ok_with_data(
        state
            .application()
            .supplier_api_governance_process()
            .catalog_identity_review(&id, &item, &actor)
            .await?,
    ))
}

/// 提交同一销售项及单位的人工作业确认。
/// 当前认证账号、来源路径和明确核对请求。
/// 保留现有公司 SKU 身份的核对结果。
/// 未确认、版本变化、单位不同或权限不足时拒绝。
pub async fn confirm_identity_review(
    State(state): State<HttpState>,
    Extension(actor): Extension<AuditActor>,
    Path((id, item)): Path<(String, String)>,
    Json(request): Json<ReviewCatalogIdentityRequest>,
) -> Result<CatalogIdentityReviewView> {
    Ok(ApiResponse::ok_with_data(
        state
            .application()
            .supplier_api_governance_process()
            .review_catalog_identity(&id, &item, request, &actor)
            .await?,
    ))
}
