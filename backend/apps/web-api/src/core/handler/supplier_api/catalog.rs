//! 供应商商品目录 HTTP 入口。
use application_core::AuditActor;
use axum::Extension;
use axum::extract::{Path, State};
use erp_read_models::supplier_center::supplier_api::{CatalogQuery, CatalogView};
use erp_supply::dto::supplier_catalog::MatchCatalogItemRequest;
use erp_supply::entity::supplier_catalog::SupplierCatalogItem;

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
    Path(id): Path<String>,
    Query(query): Query<CatalogQuery>,
) -> Result<CatalogView> {
    Ok(ApiResponse::ok_with_data(state.supplier_api_read_service().catalog(&id, query).await?))
}

#[permission_macros::permission(
    group = "API 供应商连接",
    group_desc = "供应商 API 连接与能力治理（W20）",
    desc = "同步供应商商品目录",
    resource = "supplier_api_connection",
    action = "catalog_sync"
)]
/// 保存人工商品匹配。
/// # 参数
/// 路径为连接和目录记录；请求为公司 SKU 与预期版本；actor 为认证操作人。
/// # 返回
/// 已保存的匹配记录。
/// # 错误
/// 权限、范围、版本或商品状态不满足时拒绝。
pub async fn match_item(
    State(state): State<AppState>,
    Extension(actor): Extension<AuditActor>,
    Path((id, item)): Path<(String, String)>,
    Json(request): Json<MatchCatalogItemRequest>,
) -> Result<SupplierCatalogItem> {
    Ok(ApiResponse::ok_with_data(
        state.supplier_api_governance_process().match_catalog_item(&id, &item, request, &actor).await?,
    ))
}
