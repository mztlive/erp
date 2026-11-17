//! 实际盈亏协议入口；资源范围及同角色权限在报表读取事务内重验。
use application_core::AuditActor;
use axum::Extension;
use axum::extract::State;
use erp_permissions::cost;
use erp_read_models::finance::actual_profit_loss::dto::{
    PeriodBasisConfig, ProfitLossExport, ProfitLossQuery, ProfitLossView,
};
use erp_read_models::finance::actual_profit_loss::{ActualProfitLossReadModel, ProfitLossAccess};
use persistence_core::NoTransaction;

use crate::http::errors::{Error, Result};
use crate::http::extractor::{Json, Query};
use crate::http::response::ApiResponse;
use crate::http::state::HttpState;

/// 复核协议入口的读取权限，具体数据范围由组合层在事务内解析。
async fn access(state: &HttpState, actor: &AuditActor) -> std::result::Result<ProfitLossAccess, Error> {
    let can_drill_cost = state
        .application()
        .authorization()
        .has_permission(actor, &cost::ENTRY_DETAIL, &mut NoTransaction)
        .await?;
    Ok(ProfitLossAccess { can_drill_cost })
}
/// 期间口径元数据不包含经营金额，路由仍要求成本查询权限。
pub async fn period_basis(
    State(state): State<HttpState>,
    Extension(actor): Extension<AuditActor>,
) -> Result<PeriodBasisConfig> {
    let view = ActualProfitLossReadModel::new(state.application().db(), state.application().authorization())
        .period_basis(&actor)
        .await?;
    Ok(ApiResponse::ok_with_data(view))
}
/// 返回当前权限下的真实经营分析视图。
pub async fn view(
    State(state): State<HttpState>,
    Extension(actor): Extension<AuditActor>,
    Query(query): Query<ProfitLossQuery>,
) -> Result<ProfitLossView> {
    let access = access(&state, &actor).await?;
    Ok(ApiResponse::ok_with_data(
        ActualProfitLossReadModel::new(state.application().db(), state.application().authorization())
            .view(query, access, &actor)
            .await?,
    ))
}
/// 全量筛选导出与查询采用相同授权、金额规则和来源快照。
pub async fn export(
    State(state): State<HttpState>,
    Extension(actor): Extension<AuditActor>,
    Json(query): Json<ProfitLossQuery>,
) -> Result<ProfitLossExport> {
    let access = access(&state, &actor).await?;
    Ok(ApiResponse::ok_with_data(
        ActualProfitLossReadModel::new(state.application().db(), state.application().authorization())
            .export(query, access, &actor)
            .await?,
    ))
}

/// 独立历史候选；只接受期间与客户上下文，不接受报表结果条件。
pub async fn history_directory(
    State(state): State<HttpState>,
    Extension(actor): Extension<AuditActor>,
    Query(query): Query<erp_read_models::historical_directory::ProfitLossDirectoryQuery>,
) -> Result<erp_read_models::historical_directory::HistoricalDirectoryView> {
    access(&state, &actor).await?;
    Ok(ApiResponse::ok_with_data(
        ActualProfitLossReadModel::new(state.application().db(), state.application().authorization())
            .history_directory(query, &actor)
            .await?,
    ))
}

#[cfg(test)]
mod tests {
    use erp_permissions as permissions;

    use super::*;
    #[test]
    fn all_routes_require_existing_cost_read_permission() {
        assert_eq!(permissions::cost::PROFIT_LOSS_VIEW, permissions::cost::ENTRY_LIST);
        assert_eq!(permissions::cost::PROFIT_LOSS_PERIOD_BASIS, permissions::cost::PROFIT_LOSS_VIEW);
        assert_eq!(permissions::cost::PROFIT_LOSS_EXPORT, permissions::cost::PROFIT_LOSS_VIEW);
    }
    #[test]
    fn client_cannot_submit_authorized_customer_scope_or_export_amounts() {
        let json = serde_json::json!({"from":"2026-09-01","to":"2026-09-30","period_basis":"sales_order_effective_date","authorized_customer_ids":["other-customer"]});
        assert!(serde_json::from_value::<ProfitLossQuery>(json).is_err());
        let json = serde_json::json!({"from":"2026-09-01","to":"2026-09-30","period_basis":"sales_order_effective_date","row_count":999,"formula_version":"override"});
        assert!(serde_json::from_value::<ProfitLossQuery>(json).is_err());
    }
}
