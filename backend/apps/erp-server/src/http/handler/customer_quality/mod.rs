//! S3-05 M10 客户经营质量双口径协议入口。
//!
//! 当前负责与历史贡献各自独立路由、独立授权重验；响应只含服务端投影，
//! 不返回内部授权证明或不可见人员集合。

use application_core::AuditActor;
use axum::Extension;
use axum::extract::State;
use erp_read_models::customer_quality::{
    CurrentQualityQuery, CurrentQualityView, CustomerQualityReadModel, HistoryQualityQuery,
    HistoryQualityView, QualityExport,
};

use crate::http::errors::Result;
use crate::http::extractor::{Json, Query};
use crate::http::response::ApiResponse;
use crate::http::state::HttpState;
use crate::wiring::customer_access;

// 为客户经营质量查询注入共享客户授权适配器，确保当前归属的取数和交付前复验使用同一授权源。
fn read_model(state: &HttpState) -> CustomerQualityReadModel {
    CustomerQualityReadModel::new(
        state.application().db(),
        state.application().authorization(),
        customer_access(state.application().db(), state.application().authorization()),
    )
}

/// 按现任主责归集当前负责客户的经营数据，用于查看当前负责口径的分组和汇总。
pub async fn quality_current(
    State(state): State<HttpState>,
    Extension(actor): Extension<AuditActor>,
    Query(query): Query<CurrentQualityQuery>,
) -> Result<CurrentQualityView> {
    Ok(ApiResponse::ok_with_data(read_model(&state).view_current(query, &actor).await?))
}

/// 按订单冻结时的责任归属统计历史贡献，不能用现任客户负责人补填历史记录。
pub async fn quality_history(
    State(state): State<HttpState>,
    Extension(actor): Extension<AuditActor>,
    Query(query): Query<HistoryQualityQuery>,
) -> Result<HistoryQualityView> {
    Ok(ApiResponse::ok_with_data(read_model(&state).view_history(query, &actor).await?))
}

/// 从当前负责视图导出完整匹配数据；导出绑定首个响应版本，交付前重验授权。
pub async fn quality_current_export(
    State(state): State<HttpState>,
    Extension(actor): Extension<AuditActor>,
    Json(query): Json<CurrentQualityQuery>,
) -> Result<QualityExport> {
    Ok(ApiResponse::ok_with_data(read_model(&state).export_current(query, &actor).await?))
}

/// 导出按历史贡献口径筛选的全部明细，携带冻结归属并以服务端结果计算金额。
pub async fn quality_history_export(
    State(state): State<HttpState>,
    Extension(actor): Extension<AuditActor>,
    Json(query): Json<HistoryQualityQuery>,
) -> Result<QualityExport> {
    Ok(ApiResponse::ok_with_data(read_model(&state).export_history(query, &actor).await?))
}

/// 构造历史贡献筛选时查独立候选，只接受期间和客户条件，不用报表结果作为筛选输入。
pub async fn quality_history_directory(
    State(state): State<HttpState>,
    Extension(actor): Extension<AuditActor>,
    Query(query): Query<erp_read_models::historical_directory::HistoricalDirectoryQuery>,
) -> Result<erp_read_models::historical_directory::HistoricalDirectoryView> {
    Ok(ApiResponse::ok_with_data(read_model(&state).history_directory(query, &actor).await?))
}

#[cfg(test)]
mod tests {
    use erp_permissions as permissions;

    use super::*;

    #[test]
    fn caliber_routes_do_not_share_ranking_or_permission() {
        assert_ne!(
            permissions::customer_quality::QUALITY_CURRENT,
            permissions::customer_quality::QUALITY_HISTORY
        );
        let current = permissions::customer_quality::QUALITY_CURRENT;
        let history = permissions::customer_quality::QUALITY_HISTORY;
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
