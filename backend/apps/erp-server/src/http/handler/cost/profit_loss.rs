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

// 在经营分析请求中确定是否允许成本明细下钻；数据范围仍由读模型在业务事务内解析。
async fn access(state: &HttpState, actor: &AuditActor) -> std::result::Result<ProfitLossAccess, Error> {
    let can_drill_cost = state
        .application()
        .authorization()
        .has_permission(actor, &cost::ENTRY_DETAIL, &mut NoTransaction)
        .await?;
    Ok(ProfitLossAccess { can_drill_cost })
}
/// 配置经营分析查询前，用成本查询资格读取可用的期间口径；该响应不包含实际经营金额。
pub async fn period_basis(
    State(state): State<HttpState>,
    Extension(actor): Extension<AuditActor>,
) -> Result<PeriodBasisConfig> {
    let view = ActualProfitLossReadModel::new(state.application().db(), state.application().authorization())
        .period_basis(&actor)
        .await?;
    Ok(ApiResponse::ok_with_data(view))
}
/// 按当前认证账号和查询条件加载实际经营分析视图；成本明细下钻资格作为单独能力传给读模型。
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
/// 导出全量筛选结果时调用同一经营分析读模型，沿用列表授权、金额计算规则及来源快照口径。
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

/// 为经营分析的历史人员筛选读取独立候选目录；仅以期间和客户为上下文，不接受报表结果条件。
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
