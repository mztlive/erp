//! 商品来源差异接口，复用商品授权与领域合同。
use application_core::AuditActor;
use axum::Extension;
use axum::extract::{Path, State};
use erp_catalog::source_sync::{ProductSourceDiffView, ResolveSourceDiffRequest};

use crate::app_state::AppState;
use crate::core::errors::Result;
use crate::core::extractor::Json;
use crate::core::response::ApiResponse;

#[permission_macros::permission(
    group = "商品与仓库",
    group_desc = "公司商品池、商品、类目、供应商与仓库基础资料",
    desc = "查看商品来源资料差异",
    resource = "product",
    action = "detail"
)]
/// 读取当前可见商品的供应商资料差异。
/// # 参数
/// 应用、已认证操作人和路径商品。
/// # 返回
/// 无来源时为 null；有来源时包含待处理字段与双方版本。
/// # 错误
/// 商品不可见或读取失败时返回统一错误。
pub async fn product_source_diff(
    State(state): State<AppState>,
    Extension(actor): Extension<AuditActor>,
    Path(id): Path<String>,
) -> Result<Option<ProductSourceDiffView>> {
    Ok(ApiResponse::ok_with_data(state.catalog_service().product_source_diff(&id, &actor).await?))
}

#[permission_macros::permission(
    group = "商品与仓库",
    group_desc = "公司商品池、商品、类目、供应商与仓库基础资料",
    desc = "确认商品来源资料差异",
    resource = "product",
    action = "update"
)]
/// 按用户看到的版本确认指定展示字段。
/// # 参数
/// 应用、操作人、路径商品及带幂等号的决策。
/// # 返回
/// 确认后的最新差异状态。
/// # 错误
/// 越权、版本过时、素材缺失或幂等载荷冲突时拒绝。
pub async fn resolve_product_source_diff(
    State(state): State<AppState>,
    Extension(actor): Extension<AuditActor>,
    Path(id): Path<String>,
    Json(req): Json<ResolveSourceDiffRequest>,
) -> Result<Option<ProductSourceDiffView>> {
    Ok(ApiResponse::ok_with_data(
        state.catalog_service().resolve_product_source_diff(&id, req, &actor).await?,
    ))
}
