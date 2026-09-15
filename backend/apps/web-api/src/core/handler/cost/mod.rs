//! 域 D20 `cost` 的 HTTP handler。
//!
//! Handler 只做协议适配：`Validate`（DTO 内联）→ Service 调用 → `ApiResponse`，
//! 直接复用 `erp_finance::dto::cost` 的 DTO。

pub mod profit_loss;

use application_core::AuditActor;
use axum::extract::{Path, Query, State};
use axum::{Extension, Json};
use erp_finance::dto::cost::{
    CostAllocationView, CostEntryView, CreateCostEntryRequest, PageView, ScopedCostEntryView,
};
use erp_read_models::finance::cost::{AllocationReadParams, CostReadModel, CostReadParams, CostReadResult};

use crate::app_state::AppState;
use crate::core::errors::Result;
use crate::core::response::ApiResponse;

#[permission_macros::permission(
    group = "实际经营盈亏",
    group_desc = "成本事实与成本分配管理（W16）",
    desc = "查询成本事实列表",
    resource = "cost_entry",
    action = "list"
)]
/// 查询成本事实列表。
///
/// # 参数
/// * `state` - 应用状态
/// * `query` - 分页与筛选参数（扁平传递）
///
/// # 返回
/// 返回契约形状的分页视图。
pub async fn cost_entry_list(
    State(state): State<AppState>,
    Extension(actor): Extension<AuditActor>,
    Query(params): Query<CostReadParams>,
) -> Result<CostReadResult<PageView<ScopedCostEntryView>>> {
    let page = CostReadModel::new(state.db(), state.rbac()).list(params, &actor).await?;

    Ok(ApiResponse::ok_with_data(page))
}

#[permission_macros::permission(
    group = "实际经营盈亏",
    group_desc = "成本事实与成本分配管理（W16）",
    desc = "查询成本事实详情",
    resource = "cost_entry",
    action = "detail"
)]
/// 查询成本事实详情（事实 + 分配行）。
///
/// # 参数
/// * `state` - 应用状态
/// * `id` - 成本事实 ID
///
/// # 返回
/// 返回完整成本视图。
pub async fn cost_entry_detail(
    State(state): State<AppState>,
    Extension(actor): Extension<AuditActor>,
    Path(id): Path<String>,
) -> Result<CostReadResult<ScopedCostEntryView>> {
    let view = CostReadModel::new(state.db(), state.rbac()).detail(&id, &actor).await?;

    Ok(ApiResponse::ok_with_data(view))
}

#[permission_macros::permission(
    group = "实际经营盈亏",
    group_desc = "成本事实与成本分配管理（W16）",
    desc = "手工登记成本事实",
    resource = "cost_entry",
    action = "create"
)]
/// 手工登记成本事实与分配行（跨集合事务：事实 + 分配行原子可见）。
///
/// # 参数
/// * `state` - 应用状态
/// * `actor` - 已通过鉴权的审计操作人
/// * `req` - 创建请求
///
/// # 返回
/// 返回新建成本事实视图。
pub async fn cost_entry_create(
    State(state): State<AppState>,
    Extension(actor): Extension<AuditActor>,
    Json(req): Json<CreateCostEntryRequest>,
) -> Result<CostEntryView> {
    let view = erp_processes::finance_posting::cost::CostService::new(state.db())
        .create_cost_entry(req, &actor)
        .await?;

    Ok(ApiResponse::ok_with_data(view))
}

#[permission_macros::permission(
    group = "实际经营盈亏",
    group_desc = "成本事实与成本分配管理（W16）",
    desc = "查询成本分配列表",
    resource = "cost_allocation",
    action = "list"
)]
/// 查询成本分配列表。
///
/// # 参数
/// * `state` - 应用状态
/// * `query` - 分页与筛选参数（`cost_entry_id`/`sales_order_id`）
///
/// # 返回
/// 返回契约形状的分页视图。
pub async fn cost_allocation_list(
    State(state): State<AppState>,
    Extension(actor): Extension<AuditActor>,
    Query(params): Query<AllocationReadParams>,
) -> Result<CostReadResult<PageView<CostAllocationView>>> {
    let page = CostReadModel::new(state.db(), state.rbac()).allocations(params, &actor).await?;

    Ok(ApiResponse::ok_with_data(page))
}

#[cfg(test)]
mod tests {
    use axum::http::Uri;

    use super::*;

    #[test]
    fn scope_query_decodes_real_url_numbers_and_rejects_unregistered_filters() {
        let uri: Uri = "/?page=2&page_size=25&scope_version=v1&cost_stage=actual".parse().unwrap();
        let Query(params) = Query::<CostReadParams>::try_from_uri(&uri).unwrap();
        assert_eq!(params.page, Some(2));
        assert_eq!(params.scope_version.as_deref(), Some("v1"));
        let unsupported: Uri = "/?owner_user_ids=someone".parse().unwrap();
        assert!(Query::<CostReadParams>::try_from_uri(&unsupported).is_err());
        assert!(Query::<AllocationReadParams>::try_from_uri(&unsupported).is_err());
    }
}
