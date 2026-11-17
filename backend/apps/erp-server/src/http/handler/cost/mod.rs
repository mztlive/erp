//! 域 D20 `cost` 的 HTTP handler。
//!
//! Handler 只做协议适配：`Validate`（DTO 内联）→ Service 调用 → `ApiResponse`，
//! 直接复用 `erp_finance::dto::cost` 的 DTO。

pub mod extra_cost;
pub mod profit_loss;

use application_core::AuditActor;
use axum::Extension;
use axum::extract::{Path, State};
use erp_finance::dto::cost::{
    CostAllocationView, CostEntryView, CreateCostEntryRequest, PageView, ScopedCostEntryView,
};
use erp_read_models::finance::cost::{AllocationReadParams, CostReadModel, CostReadParams, CostReadResult};

use crate::http::errors::Result;
use crate::http::extractor::{Json, Query};
use crate::http::response::ApiResponse;
use crate::http::state::HttpState;

/// 查询成本事实列表。
///
/// # 参数
/// * `state` - 应用状态
/// * `query` - 分页与筛选参数（扁平传递）
///
/// # 返回
/// 返回契约形状的分页视图。
pub async fn cost_entry_list(
    State(state): State<HttpState>,
    Extension(actor): Extension<AuditActor>,
    Query(params): Query<CostReadParams>,
) -> Result<CostReadResult<PageView<ScopedCostEntryView>>> {
    let page = CostReadModel::new(state.application().db(), state.application().authorization())
        .list(params, &actor)
        .await?;

    Ok(ApiResponse::ok_with_data(page))
}

/// 查询成本事实详情（事实 + 分配行）。
///
/// # 参数
/// * `state` - 应用状态
/// * `id` - 成本事实 ID
///
/// # 返回
/// 返回完整成本视图。
pub async fn cost_entry_detail(
    State(state): State<HttpState>,
    Extension(actor): Extension<AuditActor>,
    Path(id): Path<String>,
) -> Result<CostReadResult<ScopedCostEntryView>> {
    let view = CostReadModel::new(state.application().db(), state.application().authorization())
        .detail(&id, &actor)
        .await?;

    Ok(ApiResponse::ok_with_data(view))
}

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
    State(state): State<HttpState>,
    Extension(actor): Extension<AuditActor>,
    Json(req): Json<CreateCostEntryRequest>,
) -> Result<CostEntryView> {
    let view = erp_finance_posting::finance_posting::cost::CostProcess::new(
        state.application().db(),
        state.application().authorization(),
    )
    .create_cost_entry(req, &actor)
    .await?;

    Ok(ApiResponse::ok_with_data(view))
}

/// 查询成本分配列表。
///
/// # 参数
/// * `state` - 应用状态
/// * `query` - 分页与筛选参数（`cost_entry_id`/`sales_order_id`）
///
/// # 返回
/// 返回契约形状的分页视图。
pub async fn cost_allocation_list(
    State(state): State<HttpState>,
    Extension(actor): Extension<AuditActor>,
    Query(params): Query<AllocationReadParams>,
) -> Result<CostReadResult<PageView<CostAllocationView>>> {
    let page = CostReadModel::new(state.application().db(), state.application().authorization())
        .allocations(params, &actor)
        .await?;

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
