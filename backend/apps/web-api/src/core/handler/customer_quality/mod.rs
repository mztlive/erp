//! S3-05 M10 客户经营质量双口径协议入口。
//!
//! 当前负责与历史贡献各自独立路由、独立授权重验；响应只含服务端投影，
//! 不返回内部授权证明或不可见人员集合。

use axum::extract::{Query, State};
use axum::{Extension, Json};
use erp_read_models::customer_quality::{
    CurrentQualityQuery, CurrentQualityView, CustomerQualityReadModel, HistoryQualityQuery,
    HistoryQualityView, QualityExport,
};

use crate::app_state::AppState;
use crate::core::errors::{Error, Result};
use crate::core::handler::customer::has_permission;
use crate::core::middleware::RbacSubject;
use crate::core::response::ApiResponse;

/// 双口径共用的读取门槛：客户查询与销售单查询动作缺一不可。
async fn ensure_list_access(state: &AppState, subject: &RbacSubject) -> std::result::Result<(), Error> {
    if !has_permission(state, subject, "customer:list").await? {
        return Err(Error::Forbidden("查看客户经营质量需要客户查询权限".into()));
    }
    if !has_permission(state, subject, "sales_order:list").await? {
        return Err(Error::Forbidden("查看客户经营质量需要销售单查询权限".into()));
    }
    Ok(())
}

/// 当前负责口径需要客户读模型 Port；未注入组合层 adapter 时失败关闭。
fn read_model(state: &AppState) -> CustomerQualityReadModel {
    CustomerQualityReadModel::new(
        state.db(),
        state.rbac(),
        erp_processes::adapters::MongoCustomerDataScope::shared(state.db(), state.rbac()),
    )
}

/// 当前负责客户的经营情况：现任主责分组与汇总。
#[permission_macros::permission(
    group = "客户经营质量",
    group_desc = "当前负责与历史贡献双口径（M10）",
    desc = "查询当前负责客户经营情况",
    resource = "customer",
    action = "list"
)]
pub async fn quality_current(
    State(state): State<AppState>,
    Extension(subject): Extension<RbacSubject>,
    Extension(actor): Extension<application_core::AuditActor>,
    Query(query): Query<CurrentQualityQuery>,
) -> Result<CurrentQualityView> {
    ensure_list_access(&state, &subject).await?;
    Ok(ApiResponse::ok_with_data(read_model(&state).view_current(query, &actor).await?))
}

/// 历史负责订单的贡献：冻结归属分组与汇总，不用现任回填。
#[permission_macros::permission(
    group = "客户经营质量",
    group_desc = "当前负责与历史贡献双口径（M10）",
    desc = "查询历史负责订单贡献",
    resource = "sales_order",
    action = "list"
)]
pub async fn quality_history(
    State(state): State<AppState>,
    Extension(subject): Extension<RbacSubject>,
    Extension(actor): Extension<application_core::AuditActor>,
    Query(query): Query<HistoryQualityQuery>,
) -> Result<HistoryQualityView> {
    ensure_list_access(&state, &subject).await?;
    Ok(ApiResponse::ok_with_data(read_model(&state).view_history(query, &actor).await?))
}

/// 当前口径全量导出：版本绑定首个响应，返回前重验授权。
#[permission_macros::permission(
    group = "客户经营质量",
    group_desc = "当前负责与历史贡献双口径（M10）",
    desc = "导出当前负责客户经营情况",
    resource = "customer",
    action = "list"
)]
pub async fn quality_current_export(
    State(state): State<AppState>,
    Extension(subject): Extension<RbacSubject>,
    Extension(actor): Extension<application_core::AuditActor>,
    Json(query): Json<CurrentQualityQuery>,
) -> Result<QualityExport> {
    ensure_list_access(&state, &subject).await?;
    Ok(ApiResponse::ok_with_data(read_model(&state).export_current(query, &actor).await?))
}

/// 历史口径全量导出：冻结归属列随行导出，不使用客户端金额。
#[permission_macros::permission(
    group = "客户经营质量",
    group_desc = "当前负责与历史贡献双口径（M10）",
    desc = "导出历史负责订单贡献",
    resource = "sales_order",
    action = "list"
)]
pub async fn quality_history_export(
    State(state): State<AppState>,
    Extension(subject): Extension<RbacSubject>,
    Extension(actor): Extension<application_core::AuditActor>,
    Json(query): Json<HistoryQualityQuery>,
) -> Result<QualityExport> {
    ensure_list_access(&state, &subject).await?;
    Ok(ApiResponse::ok_with_data(read_model(&state).export_history(query, &actor).await?))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn caliber_routes_do_not_share_ranking_or_permission() {
        assert_ne!(quality_current_permission_key(), quality_history_permission_key());
        let current = quality_current_permission_key();
        let history = quality_history_permission_key();
        assert_eq!(current.resource(), "customer");
        assert_eq!(history.resource(), "sales_order");
    }

    #[test]
    fn client_cannot_submit_cross_caliber_or_authorized_scope() {
        let json = serde_json::json!({"from":"2026-09-01","to":"2026-09-30","attribution_user_ids":"u-1"});
        assert!(serde_json::from_value::<CurrentQualityQuery>(json).is_err());
        let json = serde_json::json!({"from":"2026-09-01","to":"2026-09-30","owner_user_ids":"u-1"});
        assert!(serde_json::from_value::<HistoryQualityQuery>(json).is_err());
        let json = serde_json::json!({"from":"2026-09-01","to":"2026-09-30","row_count":999});
        assert!(serde_json::from_value::<CurrentQualityQuery>(json).is_err());
    }
}

/// 独立历史候选；只接受期间与客户上下文，不接受报表结果条件。
#[permission_macros::permission(
    group = "历史归属目录",
    group_desc = "冻结归属查询",
    desc = "查询历史归属候选",
    resource = "sales_order",
    action = "list"
)]
pub async fn quality_history_directory(
    State(state): State<AppState>,
    Extension(subject): Extension<RbacSubject>,
    Extension(actor): Extension<application_core::AuditActor>,
    Query(query): Query<erp_read_models::historical_directory::HistoricalDirectoryQuery>,
) -> Result<erp_read_models::historical_directory::HistoricalDirectoryView> {
    ensure_list_access(&state, &subject).await?;
    Ok(ApiResponse::ok_with_data(read_model(&state).history_directory(query, &actor).await?))
}
