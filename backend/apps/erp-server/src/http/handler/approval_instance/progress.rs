//! 发起人进展计数和已读确认协议适配。

use application_core::AuditActor;
use axum::Extension;
use axum::extract::{Path, State};
use axum::http::HeaderMap;
use erp_workflow::dto::approval_progress::{ApprovalProgressCount, ReadApprovalProgressRequest};

use super::error::{ApprovalHttpError, parse_version};
use super::{ApprovalResult, runtime_service};
use crate::http::extractor::Json;
use crate::http::response::ApiResponse;
use crate::http::state::HttpState;

/// 返回本人发起审批的未读进展数量。
/// 按审批单计数的未读数量。
/// 没有读取权限、账号失效或存储失败时返回对应 HTTP 错误。
pub async fn count(
    State(state): State<HttpState>,
    Extension(actor): Extension<AuditActor>,
    headers: HeaderMap,
) -> ApprovalResult<ApprovalProgressCount> {
    let unread_count = runtime_service(&state)
        .started_unread_count(&actor)
        .await
        .map_err(|error| ApprovalHttpError::from_service(error, &headers))?;
    Ok(ApiResponse::ok_with_data(ApprovalProgressCount { unread_count }))
}

/// 确认当前用户已经展示的审批进展。
/// 成功返回空；旧版本确认不清除更新的进展。
/// 非法版本返回 400，无权或实例缺失返回 404，存储错误按统一目录映射。
pub async fn mark_read(
    State(state): State<HttpState>,
    Extension(actor): Extension<AuditActor>,
    Path(id): Path<String>,
    headers: HeaderMap,
    Json(request): Json<ReadApprovalProgressRequest>,
) -> ApprovalResult<()> {
    let version = parse_version(&request.observed_instance_version, "查看版本", &headers)?;
    runtime_service(&state)
        .mark_progress_read(&actor, &id, version)
        .await
        .map_err(|error| ApprovalHttpError::from_service(error, &headers))?;
    Ok(ApiResponse::ok_with_data(()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn read_request_rejects_missing_version_or_forged_account() {
        assert!(serde_json::from_value::<ReadApprovalProgressRequest>(serde_json::json!({})).is_err());
        assert!(
            serde_json::from_value::<ReadApprovalProgressRequest>(serde_json::json!({
                "observed_instance_version": "4", "actor_id": "other"
            }))
            .is_err()
        );
        let request: ReadApprovalProgressRequest = serde_json::from_value(serde_json::json!({
            "observed_instance_version": "4"
        }))
        .unwrap();
        assert_eq!(parse_version(&request.observed_instance_version, "版本", &HeaderMap::new()).unwrap(), 4);
        for version in ["0", "-1", "1.5", "", "9007199254740993x"] {
            assert!(parse_version(version, "版本", &HeaderMap::new()).is_err());
        }
    }
}
