//! 指定记录授权管理；业务校验及事务归领域服务和授权编排。
use application_core::AuditActor;
use axum::Extension;
use axum::extract::State;
use erp_core::common::time::Instant;
use erp_identity::Error as IdentityError;
use erp_identity::dto::GrantSourcePage;
use erp_identity::entity::resource_access::GrantSource;
use erp_resource_grants::{
    BackfillReport, BackfillRequest, ManualGrantRequest, RecomputeRequest, ResourceGrantProcess,
    RevokeRequest, SourcesQuery,
};

use crate::http::errors::Result;
use crate::http::extractor::{Json, Query};
use crate::http::response::ApiResponse;
use crate::http::state::HttpState;

fn process(state: &HttpState) -> ResourceGrantProcess {
    ResourceGrantProcess::new(state.application().db(), state.application().authorization())
}

/// 按稳定操作号授予指定记录读取权。
/// state 为共享应用，actor 为管理员，request 为人工来源及完整目标。
/// 当前来源、目标与版本。
/// 越权、目标无效、操作号异载荷或已终止来源时拒绝。
pub async fn grant(
    State(state): State<HttpState>,
    Extension(actor): Extension<AuditActor>,
    Json(request): Json<ManualGrantRequest>,
) -> Result<GrantSource> {
    Ok(ApiResponse::ok_with_data(process(&state).management().grant(&actor, request.into_update()).await?))
}

/// 永久终止一个授权来源。
/// state、actor 为当前上下文，request 含来源身份、期望版本及原因。
/// 终止后的来源。
/// 越权、版本变化或来源缺失时拒绝。
pub async fn revoke(
    State(state): State<HttpState>,
    Extension(actor): Extension<AuditActor>,
    Json(request): Json<RevokeRequest>,
) -> Result<GrantSource> {
    Ok(ApiResponse::ok_with_data(
        process(&state)
            .management()
            .revoke(&actor, request.source, request.expected_version, request.reason)
            .await?,
    ))
}

/// 查看指定账号的来源及终止记录。
/// 最多 100 个来源及下一页游标。
/// 越权、参数非法或读取失败时拒绝。
pub async fn sources(
    State(state): State<HttpState>,
    Extension(actor): Extension<AuditActor>,
    Query(query): Query<SourcesQuery>,
) -> Result<GrantSourcePage> {
    query.validate()?;
    Ok(ApiResponse::ok_with_data(
        process(&state).management().sources(&actor, &query.recipient, query.after.as_deref()).await?,
    ))
}

/// 对一页现有正式事件执行受控重算。
/// 下一页游标；空页返回空值。
/// 越权、参数非法或事务失败时拒绝。
pub async fn recompute(
    State(state): State<HttpState>,
    Extension(actor): Extension<AuditActor>,
    Json(request): Json<RecomputeRequest>,
) -> Result<Option<String>> {
    request.validate()?;
    let now =
        u64::try_from(Instant::now().unix_secs()).map_err(|_| IdentityError::Internal("时间越界".into()))?;
    Ok(ApiResponse::ok_with_data(
        process(&state).management().recompute_managed(&actor, request.selection, request.after, now).await?,
    ))
}

/// 预览或追加一页历史正式证据对应的授权事件。
/// 候选数量、已有来源、终止来源、无效证据及下一页游标。
/// 越权或基础设施故障中止整页，不自动重放业务命令。
pub async fn backfill(
    State(state): State<HttpState>,
    Extension(actor): Extension<AuditActor>,
    Json(request): Json<BackfillRequest>,
) -> Result<BackfillReport> {
    Ok(ApiResponse::ok_with_data(process(&state).backfill(&actor, request).await?))
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;
    #[test]
    fn maintenance_http_contract_rejects_forged_evidence_and_unbounded_inputs() {
        let request = json!({"kind":"invoice_approvals", "dry_run":true, "recipient":"forged"});
        assert!(serde_json::from_value::<BackfillRequest>(request).is_err());
        let source: SourcesQuery =
            serde_json::from_value(json!({"recipient":"user", "after":"cursor"})).unwrap();
        source.validate().unwrap();
        let invalid: SourcesQuery = serde_json::from_value(json!({"recipient":" "})).unwrap();
        assert!(invalid.validate().is_err());
        assert!(
            serde_json::from_value::<RecomputeRequest>(
                json!({"selection":{"kind":"rule","value":"arbitrary"}})
            )
            .is_err()
        );
        let request: BackfillRequest = serde_json::from_value(json!({"kind":"invoice_approvals"})).unwrap();
        assert!(request.dry_run);
    }
}
