//! 实际盈亏协议入口，沿用成本读取权限并复核收入权限及当前客户范围。
use crate::core::handler::customer::has_permission;
use crate::{
    app_state::AppState,
    core::{
        errors::{Error, Result},
        extractor::UserID,
        middleware::RbacSubject,
        response::ApiResponse,
    },
};
use axum::{
    extract::{Query, State},
    Extension, Json,
};
use erp_read_models::finance::actual_profit_loss::{
    dto::{PeriodBasisConfig, ProfitLossExport, ProfitLossQuery, ProfitLossView},
    ActualProfitLossReadModel, ProfitLossAccess,
};

/// 服务端重验收入读取资格，并解析当前有效客户归属。
async fn access(
    state: &AppState,
    subject: &RbacSubject,
    user: &str,
) -> std::result::Result<ProfitLossAccess, Error> {
    if !has_permission(state, subject, "sales_order:list").await? {
        return Err(Error::Forbidden("查看经营盈亏还需要销售单查询权限".into()));
    }
    let all = has_permission(state, subject, "customer_scope:detail").await?;
    let scope = if all {
        erp_customer::CustomerScope::AllAuthorized
    } else {
        erp_customer::CustomerScope::Assigned
    };
    let customer_ids = state
        .customer_service()
        .customer_ids_for_scope(scope, user)
        .await?;
    // 成本详情是整笔费用，分摊跨客户时只对全量客户范围开放，避免泄露其他客户成本。
    let can_drill_cost = all && has_permission(state, subject, "cost_entry:detail").await?;
    Ok(ProfitLossAccess {
        customer_ids,
        can_drill_cost,
    })
}
/// 期间口径元数据不包含经营金额，路由仍要求成本查询权限。
#[permission_macros::permission(
    group = "实际经营盈亏",
    group_desc = "成本事实与成本分配管理（W16）",
    desc = "查询成本事实列表",
    resource = "cost_entry",
    action = "list"
)]
pub async fn period_basis() -> Result<PeriodBasisConfig> {
    Ok(ApiResponse::ok_with_data(
        ActualProfitLossReadModel::period_basis(),
    ))
}
/// 返回当前权限下的真实经营分析视图。
#[permission_macros::permission(
    group = "实际经营盈亏",
    group_desc = "成本事实与成本分配管理（W16）",
    desc = "查询成本事实列表",
    resource = "cost_entry",
    action = "list"
)]
pub async fn view(
    State(state): State<AppState>,
    Extension(subject): Extension<RbacSubject>,
    Extension(UserID(user)): Extension<UserID>,
    Query(query): Query<ProfitLossQuery>,
) -> Result<ProfitLossView> {
    let access = access(&state, &subject, &user).await?;
    Ok(ApiResponse::ok_with_data(
        ActualProfitLossReadModel::new(state.db())
            .view(query, access)
            .await?,
    ))
}
/// 全量筛选导出与查询采用相同授权、金额规则和来源快照。
#[permission_macros::permission(
    group = "实际经营盈亏",
    group_desc = "成本事实与成本分配管理（W16）",
    desc = "查询成本事实列表",
    resource = "cost_entry",
    action = "list"
)]
pub async fn export(
    State(state): State<AppState>,
    Extension(subject): Extension<RbacSubject>,
    Extension(UserID(user)): Extension<UserID>,
    Json(query): Json<ProfitLossQuery>,
) -> Result<ProfitLossExport> {
    let access = access(&state, &subject, &user).await?;
    Ok(ApiResponse::ok_with_data(
        ActualProfitLossReadModel::new(state.db())
            .export(query, access)
            .await?,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn all_routes_require_existing_cost_read_permission() {
        assert_eq!(
            view_permission_key(),
            super::super::cost_entry_list_permission_key()
        );
        assert_eq!(period_basis_permission_key(), view_permission_key());
        assert_eq!(export_permission_key(), view_permission_key());
    }
    #[test]
    fn client_cannot_submit_authorized_customer_scope_or_export_amounts() {
        let json = serde_json::json!({"from":"2026-09-01","to":"2026-09-30","period_basis":"sales_order_effective_date","authorized_customer_ids":["other-customer"]});
        assert!(serde_json::from_value::<ProfitLossQuery>(json).is_err());
        let json = serde_json::json!({"from":"2026-09-01","to":"2026-09-30","period_basis":"sales_order_effective_date","row_count":999,"formula_version":"override"});
        assert!(serde_json::from_value::<ProfitLossQuery>(json).is_err());
    }
}
