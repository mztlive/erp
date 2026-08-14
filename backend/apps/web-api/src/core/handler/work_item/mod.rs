//! D03 人工任务责任 HTTP 适配层。

use axum::{
    extract::{Path, Query, State},
    response::{IntoResponse, Response},
    Extension, Json,
};
use services::{
    audit::AuditActor,
    work_item::{
        CloseWorkItemRequest, ReassignWorkItemRequest, ReleaseToTeamRequest, StartProcessingRequest,
        WorkItemConflict, WorkItemListParams, WorkItemMutationOutcome, WorkItemPageView, WorkItemService,
        WorkItemStatsParams, WorkItemStatsView, WorkItemView,
    },
};

use crate::{
    app_state::AppState,
    core::{
        errors::{Error as HttpError, Result},
        response::ApiResponse,
    },
};

/// 责任命令 HTTP 边界错误。
#[derive(Debug)]
pub enum WorkItemActionError {
    /// 并发版本或当前责任发生变化。
    Conflict(Box<WorkItemConflict>),
    /// 其余错误沿用统一 HTTP 错误合同。
    Other(HttpError),
}

impl From<services::Error> for WorkItemActionError {
    fn from(error: services::Error) -> Self {
        Self::Other(HttpError::from(error))
    }
}

impl IntoResponse for WorkItemActionError {
    /// 将责任命令错误转换为真实 HTTP 状态与稳定 JSON 信封。
    ///
    /// # 返回
    /// 冲突返回 409 和权限安全的最新任务摘要；其他错误复用统一映射。
    fn into_response(self) -> Response {
        match self {
            Self::Conflict(conflict) => {
                let kind = conflict.kind();
                ApiResponse {
                    status: 409,
                    message: kind.message().to_string(),
                    code: Some(kind.code().to_string()),
                    data: Some(*conflict),
                    success: false,
                }
                .into_response()
            }
            Self::Other(error) => error.into_response(),
        }
    }
}

/// 责任命令 HTTP 结果。
pub type WorkItemActionResult = std::result::Result<ApiResponse<WorkItemView>, WorkItemActionError>;

fn work_item_action_response(outcome: WorkItemMutationOutcome) -> WorkItemActionResult {
    match outcome {
        WorkItemMutationOutcome::Applied(view) => Ok(ApiResponse::ok_with_data(view)),
        WorkItemMutationOutcome::Conflict(conflict) => Err(WorkItemActionError::Conflict(Box::new(conflict))),
    }
}

#[permission_macros::permission(
    group = "统一待办",
    group_desc = "待办队列与责任处理",
    desc = "查询本人授权范围内的待办",
    resource = "work_item",
    action = "list"
)]
/// 查询服务端责任过滤后的待办队列。
///
/// # 返回
/// 返回携带稳定队列上下文的分页投影。
pub async fn work_item_list(
    State(state): State<AppState>,
    Extension(actor): Extension<AuditActor>,
    Query(params): Query<WorkItemListParams>,
) -> Result<WorkItemPageView> {
    let page = WorkItemService::new(state.db(), state.rbac())
        .work_item_list(&params, &actor)
        .await?;
    Ok(ApiResponse::ok_with_data(page))
}

#[permission_macros::permission(
    group = "统一待办",
    group_desc = "待办队列与责任处理",
    desc = "查询本人授权范围内的待办统计",
    resource = "work_item",
    action = "list"
)]
/// 查询与正式待办列表复用授权快照的统计。
///
/// # 返回
/// 返回个人、团队、到期、超期、异常计数及服务端统计时点。
pub async fn work_item_stats(
    State(state): State<AppState>,
    Extension(actor): Extension<AuditActor>,
    Query(params): Query<WorkItemStatsParams>,
) -> Result<WorkItemStatsView> {
    let stats = WorkItemService::new(state.db(), state.rbac())
        .work_item_stats(&params, &actor)
        .await?;
    Ok(ApiResponse::ok_with_data(stats))
}

#[permission_macros::permission(
    group = "统一待办",
    group_desc = "待办队列与责任处理",
    desc = "查询本人有权查看的待办详情",
    resource = "work_item",
    action = "detail"
)]
/// 查询单条任务的安全详情。
///
/// # 返回
/// 返回服务端重新计算允许动作的任务投影。
pub async fn work_item_detail(
    State(state): State<AppState>,
    Extension(actor): Extension<AuditActor>,
    Path(id): Path<String>,
) -> Result<WorkItemView> {
    let view = WorkItemService::new(state.db(), state.rbac())
        .work_item_detail(&id, &actor)
        .await?;
    Ok(ApiResponse::ok_with_data(view))
}

#[permission_macros::permission(
    group = "统一待办",
    group_desc = "待办队列与责任处理",
    desc = "从团队待处理建立本人责任",
    resource = "work_item",
    action = "start_processing"
)]
/// 原子开始处理责任池任务。
///
/// # 返回
/// 返回已形成本人责任的同一任务。
pub async fn work_item_start_processing(
    State(state): State<AppState>,
    Extension(actor): Extension<AuditActor>,
    Path(id): Path<String>,
    Json(req): Json<StartProcessingRequest>,
) -> WorkItemActionResult {
    let outcome = WorkItemService::new(state.db(), state.rbac())
        .start_processing(&id, req, &actor)
        .await?;
    work_item_action_response(outcome)
}

#[permission_macros::permission(
    group = "统一待办",
    group_desc = "待办队列与责任处理",
    desc = "将本人负责的责任池任务退回团队",
    resource = "work_item",
    action = "release_to_team"
)]
/// 退回团队并保留首次分派和处理时间。
///
/// # 返回
/// 返回同一开放任务的最新责任事实。
pub async fn work_item_release_to_team(
    State(state): State<AppState>,
    Extension(actor): Extension<AuditActor>,
    Path(id): Path<String>,
    Json(req): Json<ReleaseToTeamRequest>,
) -> WorkItemActionResult {
    let outcome = WorkItemService::new(state.db(), state.rbac())
        .release_to_team(&id, req, &actor)
        .await?;
    work_item_action_response(outcome)
}

#[permission_macros::permission(
    group = "统一待办",
    group_desc = "待办队列与责任处理",
    desc = "在授权范围内受控转交任务",
    resource = "work_item",
    action = "reassign"
)]
/// 转交开放任务给重新校验合格的用户。
///
/// # 返回
/// 返回责任已更新的同一任务。
pub async fn work_item_reassign(
    State(state): State<AppState>,
    Extension(actor): Extension<AuditActor>,
    Path(id): Path<String>,
    Json(req): Json<ReassignWorkItemRequest>,
) -> WorkItemActionResult {
    let outcome = WorkItemService::new(state.db(), state.rbac())
        .reassign(&id, req, &actor)
        .await?;
    work_item_action_response(outcome)
}

#[permission_macros::permission(
    group = "统一待办",
    group_desc = "待办队列与责任处理",
    desc = "关闭重复、误派或已有替代的任务",
    resource = "work_item",
    action = "close"
)]
/// 受控关闭允许人工关闭的无效任务。
///
/// # 返回
/// 返回已关闭任务的只读事实。
pub async fn work_item_close(
    State(state): State<AppState>,
    Extension(actor): Extension<AuditActor>,
    Path(id): Path<String>,
    Json(req): Json<CloseWorkItemRequest>,
) -> WorkItemActionResult {
    let outcome = WorkItemService::new(state.db(), state.rbac())
        .close(&id, req, &actor)
        .await?;
    work_item_action_response(outcome)
}

#[cfg(test)]
mod tests {
    use axum::{body::to_bytes, http::StatusCode, response::IntoResponse};
    use serde_json::{json, Value};
    use services::work_item::{WorkItemConflict, WorkItemConflictKind};

    use super::WorkItemActionError;

    #[tokio::test]
    async fn version_conflict_uses_409_stable_code_and_safe_tombstone() {
        let response = WorkItemActionError::Conflict(Box::new(WorkItemConflict::new(
            WorkItemConflictKind::Version,
            None,
        )))
        .into_response();

        assert_eq!(response.status(), StatusCode::CONFLICT);
        let body = to_bytes(response.into_body(), usize::MAX)
            .await
            .expect("conflict body should be readable");
        let body: Value = serde_json::from_slice(&body).expect("conflict body should be JSON");
        assert_eq!(body["code"], "WORK_ITEM_VERSION_CONFLICT");
        assert_eq!(body["data"], json!({ "current_work_item": null }));
        assert_eq!(body["success"], false);
    }

    #[tokio::test]
    async fn responsibility_conflict_has_distinct_stable_code() {
        let response = WorkItemActionError::Conflict(Box::new(WorkItemConflict::new(
            WorkItemConflictKind::Responsibility,
            None,
        )))
        .into_response();
        let body = to_bytes(response.into_body(), usize::MAX)
            .await
            .expect("conflict body should be readable");
        let body: Value = serde_json::from_slice(&body).expect("conflict body should be JSON");

        assert_eq!(body["code"], "WORK_ITEM_RESPONSIBILITY_CONFLICT");
        assert_eq!(body["data"], json!({ "current_work_item": null }));
    }
}
