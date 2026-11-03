//! 实际盈亏协议入口；资源范围及同角色权限在报表读取事务内重验。
use axum::Extension;
use axum::extract::State;
use erp_read_models::finance::actual_profit_loss::dto::{
    PeriodBasisConfig, ProfitLossExport, ProfitLossQuery, ProfitLossView,
};
use erp_read_models::finance::actual_profit_loss::{ActualProfitLossReadModel, ProfitLossAccess};

use crate::http::errors::{Error, Result};
use crate::http::extractor::{Json, Query};
use crate::http::handler::customer::has_permission;
use crate::http::middleware::RbacSubject;
use crate::http::response::ApiResponse;
use crate::http::state::HttpState;

/// 复核协议入口的读取权限，具体数据范围由组合层在事务内解析。
async fn access(state: &HttpState, subject: &RbacSubject) -> std::result::Result<ProfitLossAccess, Error> {
    if !has_permission(state, subject, "sales_order:list").await? {
        return Err(Error::Forbidden("查看经营盈亏还需要销售单查询权限".into()));
    }
    // 组合层还须证明销售单 Company 范围及个人上限，才允许整笔费用下钻。
    let can_drill_cost = has_permission(state, subject, "cost_entry:detail").await?;
    Ok(ProfitLossAccess { can_drill_cost })
}
/// 期间口径元数据不包含经营金额，路由仍要求成本查询权限。
pub async fn period_basis() -> Result<PeriodBasisConfig> {
    Ok(ApiResponse::ok_with_data(ActualProfitLossReadModel::period_basis()))
}
/// 返回当前权限下的真实经营分析视图。
pub async fn view(
    State(state): State<HttpState>,
    Extension(subject): Extension<RbacSubject>,
    Extension(actor): Extension<application_core::AuditActor>,
    Query(query): Query<ProfitLossQuery>,
) -> Result<ProfitLossView> {
    let access = access(&state, &subject).await?;
    Ok(ApiResponse::ok_with_data(
        ActualProfitLossReadModel::new(state.application().db(), state.application().rbac())
            .view(query, access, &actor)
            .await?,
    ))
}
/// 全量筛选导出与查询采用相同授权、金额规则和来源快照。
pub async fn export(
    State(state): State<HttpState>,
    Extension(subject): Extension<RbacSubject>,
    Extension(actor): Extension<application_core::AuditActor>,
    Json(query): Json<ProfitLossQuery>,
) -> Result<ProfitLossExport> {
    let access = access(&state, &subject).await?;
    Ok(ApiResponse::ok_with_data(
        ActualProfitLossReadModel::new(state.application().db(), state.application().rbac())
            .export(query, access, &actor)
            .await?,
    ))
}

/// 独立历史候选；只接受期间与客户上下文，不接受报表结果条件。
pub async fn history_directory(
    State(state): State<HttpState>,
    Extension(subject): Extension<RbacSubject>,
    Extension(actor): Extension<application_core::AuditActor>,
    Query(query): Query<erp_read_models::historical_directory::ProfitLossDirectoryQuery>,
) -> Result<erp_read_models::historical_directory::HistoricalDirectoryView> {
    access(&state, &subject).await?;
    Ok(ApiResponse::ok_with_data(
        ActualProfitLossReadModel::new(state.application().db(), state.application().rbac())
            .history_directory(query, &actor)
            .await?,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::permissions;
    #[test]
    fn all_routes_require_existing_cost_read_permission() {
        assert_eq!(
            permissions::cost::PROFIT_LOSS_VIEW.permission(),
            permissions::cost::ENTRY_LIST.permission()
        );
        assert_eq!(
            permissions::cost::PROFIT_LOSS_PERIOD_BASIS.permission(),
            permissions::cost::PROFIT_LOSS_VIEW.permission()
        );
        assert_eq!(
            permissions::cost::PROFIT_LOSS_EXPORT.permission(),
            permissions::cost::PROFIT_LOSS_VIEW.permission()
        );
    }
    #[test]
    fn client_cannot_submit_authorized_customer_scope_or_export_amounts() {
        let json = serde_json::json!({"from":"2026-09-01","to":"2026-09-30","period_basis":"sales_order_effective_date","authorized_customer_ids":["other-customer"]});
        assert!(serde_json::from_value::<ProfitLossQuery>(json).is_err());
        let json = serde_json::json!({"from":"2026-09-01","to":"2026-09-30","period_basis":"sales_order_effective_date","row_count":999,"formula_version":"override"});
        assert!(serde_json::from_value::<ProfitLossQuery>(json).is_err());
    }
}
