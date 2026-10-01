//! 供给资料与历史条款的只读 HTTP 适配。

use application_core::AuditActor;
use axum::Extension;
use axum::extract::{Path, Query, State};
use erp_processes::adapters::MongoOfferingDataScope;
use erp_read_models::supplier_center::offering::SupplierOfferingView;
use erp_read_models::supplier_center::{MongoOfferingProcurementOwners, SupplierOfferingReadService};
use erp_supply::SupplierOfferingService;
use erp_supply::dto::offering_history::{OfferingHistoryPage, OfferingHistoryParams};

use super::can_view_costs;
use crate::app_state::AppState;
use crate::core::errors::Result;
use crate::core::middleware::RbacSubject;
use crate::core::response::ApiResponse;

#[permission_macros::permission(
    group = "供应商供给",
    group_desc = "维护公司 SKU 的供应商供给、价格条款和可供状态",
    desc = "查询供应商供给列表",
    resource = "supplier_offering",
    action = "list"
)]
/// 查询供给资料，复用列表动作和供给自身的数据范围。
///
/// # 参数
/// * `state` - 应用状态
/// * `subject` - 成本权限主体
/// * `actor` - 范围授权操作人
/// * `id` - 供给稳定 ID
///
/// # 返回
/// 返回已按权限脱敏的当前供给资料。
///
/// # 错误
/// 无动作权限返回 Forbidden；越权对象与缺失对象返回 NotFound。
pub async fn detail(
    State(state): State<AppState>,
    Extension(subject): Extension<RbacSubject>,
    Extension(actor): Extension<AuditActor>,
    Path(id): Path<String>,
) -> Result<SupplierOfferingView> {
    let mut view = SupplierOfferingReadService::new(
        state.db(),
        MongoOfferingDataScope::shared(state.db(), state.rbac()),
        MongoOfferingProcurementOwners::shared(state.db()),
    )
    .detail(&id, &actor)
    .await?;
    if !can_view_costs(&state, &subject).await? {
        view.redact_costs();
    }
    Ok(ApiResponse::ok_with_data(view))
}

#[permission_macros::permission(
    group = "供应商供给",
    group_desc = "维护公司 SKU 的供应商供给、价格条款和可供状态",
    desc = "查询供应商供给列表",
    resource = "supplier_offering",
    action = "list"
)]
/// 查询供给历史条款，每次重新校验对象范围与成本权限。
///
/// # 参数
/// * `state` - 应用状态
/// * `subject` - 成本权限主体
/// * `actor` - 范围授权操作人
/// * `id` - 供给稳定 ID
/// * `params` - 历史版本游标
///
/// # 返回
/// 返回脱敏后的条款历史页，不包含实时可供数量。
///
/// # 错误
/// 游标非法、权限不足、对象不可见或存储失败时返回统一错误。
pub async fn history(
    State(state): State<AppState>,
    Extension(subject): Extension<RbacSubject>,
    Extension(actor): Extension<AuditActor>,
    Path(id): Path<String>,
    Query(params): Query<OfferingHistoryParams>,
) -> Result<OfferingHistoryPage> {
    let mut page = SupplierOfferingService::new(state.db())
        .with_data_scope(MongoOfferingDataScope::shared(state.db(), state.rbac()))
        .history(&id, params, &actor)
        .await?;
    if !can_view_costs(&state, &subject).await? {
        for item in &mut page.items {
            item.redact_costs();
        }
    }
    Ok(ApiResponse::ok_with_data(page))
}
