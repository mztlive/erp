//! 人工任务责任 HTTP 适配层。
//!
//! 已删除 start-processing / release-to-team / claim。通用写接口拒绝审批任务。

use axum::{
    extract::{Path, Query, State},
    http::HeaderMap,
    response::{IntoResponse, Response},
    Extension, Json,
};
use entities::work_item::WorkItemType;
use serde::Serialize;
use services::{
    audit::AuditActor,
    work_item::{
        CloseWorkItemRequest, ReassignWorkItemRequest, WorkItemConflict, WorkItemListParams,
        WorkItemMutationOutcome, WorkItemPageView, WorkItemReassignCandidateView, WorkItemService,
        WorkItemStatsParams, WorkItemStatsView, WorkItemView,
    },
};

use crate::{
    app_state::AppState,
    core::{
        errors::{Error as HttpError, Result},
        handler::approval_instance::error::ApprovalHttpError,
        response::ApiResponse,
    },
};

pub mod finance_responsibility;

/// 线协议责任类型。
#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ResponsibilityKind {
    /// 单据审批个人责任。
    PersonalApproval,
    /// 非审批个人业务任务。
    PersonalBusinessTask,
}

/// 带 `responsibility_kind` 的任务投影。
#[derive(Debug, Clone, Serialize)]
pub struct WorkItemHttpView {
    /// 服务层安全投影。
    #[serde(flatten)]
    pub inner: WorkItemView,
    /// 合同冻结的责任类型。
    pub responsibility_kind: ResponsibilityKind,
}

/// 带责任类型的分页投影。
#[derive(Debug, Clone, Serialize)]
pub struct WorkItemHttpPageView {
    /// 当前页任务。
    pub items: Vec<WorkItemHttpView>,
    /// 授权范围内总数。
    pub total: i64,
    /// 当前页码。
    pub page: u64,
    /// 单页条数。
    pub page_size: u32,
    /// 服务端形成的稳定队列上下文。
    pub queue_context_id: String,
}

/// 责任命令 HTTP 边界错误。
#[derive(Debug)]
pub enum WorkItemActionError {
    /// 并发版本或当前责任发生变化。
    Conflict(Box<WorkItemConflict>),
    /// 审批任务保护。
    ApprovalProtected(ApprovalHttpError),
    /// 其余错误沿用统一 HTTP 错误合同。
    Other(HttpError),
}

impl From<services::Error> for WorkItemActionError {
    /// 将服务错误映射为责任命令错误。
    ///
    /// # 参数
    /// * `error` - 服务层错误
    ///
    /// # 返回
    /// 审批任务保护使用稳定码，其余沿用统一映射。
    fn from(error: services::Error) -> Self {
        let message = error.to_string();
        if message.contains("APPROVAL_GENERIC_WORK_ITEM_MUTATION_FORBIDDEN") {
            return Self::ApprovalProtected(ApprovalHttpError::coded(
                "APPROVAL_GENERIC_WORK_ITEM_MUTATION_FORBIDDEN",
                uuid::Uuid::new_v4().to_string(),
                None,
            ));
        }
        Self::Other(HttpError::from(error))
    }
}

impl IntoResponse for WorkItemActionError {
    /// 将责任命令错误转换为真实 HTTP 状态与稳定 JSON 信封。
    ///
    /// # 返回
    /// 冲突返回 409 和权限安全的最新任务摘要；审批保护返回稳定码。
    fn into_response(self) -> Response {
        match self {
            Self::Conflict(conflict) => {
                let kind = conflict.kind();
                ApiResponse {
                    status: 409,
                    message: kind.message().to_string(),
                    code: Some(kind.code().to_string()),
                    field_errors: None,
                    retryable: Some(true),
                    data: Some(*conflict),
                    success: false,
                }
                .into_response()
            }
            Self::ApprovalProtected(error) => error.into_response(),
            Self::Other(error) => error.into_response(),
        }
    }
}

/// 责任命令 HTTP 结果。
pub type WorkItemActionResult = std::result::Result<ApiResponse<WorkItemHttpView>, WorkItemActionError>;

/// 由任务类型计算责任类型。
///
/// # 参数
/// * `work_item_type` - 服务层任务类型
///
/// # 返回
/// `DocumentApproval` 对应 `PERSONAL_APPROVAL`。
pub fn responsibility_kind_of(work_item_type: WorkItemType) -> ResponsibilityKind {
    if work_item_type == WorkItemType::DocumentApproval {
        ResponsibilityKind::PersonalApproval
    } else {
        ResponsibilityKind::PersonalBusinessTask
    }
}

/// 包装单条任务投影。
///
/// # 参数
/// * `view` - 服务层投影
///
/// # 返回
/// 返回带责任类型的 HTTP 投影。
fn wrap_view(view: WorkItemView) -> WorkItemHttpView {
    let responsibility_kind = responsibility_kind_of(view.work_item_type);
    WorkItemHttpView {
        inner: view,
        responsibility_kind,
    }
}

/// 包装分页投影。
///
/// # 参数
/// * `page` - 服务层分页
///
/// # 返回
/// 返回带责任类型的分页。
fn wrap_page(page: WorkItemPageView) -> WorkItemHttpPageView {
    WorkItemHttpPageView {
        items: page.items.into_iter().map(wrap_view).collect(),
        total: page.total,
        page: page.page,
        page_size: page.page_size,
        queue_context_id: page.queue_context_id,
    }
}

/// 拒绝审批任务的通用写命令。
///
/// # 错误
/// `DocumentApproval` 返回稳定 409。
fn reject_approval_task(
    view: &WorkItemView,
    headers: &HeaderMap,
) -> std::result::Result<(), WorkItemActionError> {
    if view.work_item_type != WorkItemType::DocumentApproval && view.approval_node_execution_id.is_none() {
        return Ok(());
    }
    Err(WorkItemActionError::ApprovalProtected(ApprovalHttpError::coded(
        "APPROVAL_GENERIC_WORK_ITEM_MUTATION_FORBIDDEN",
        crate::core::handler::approval_instance::error::correlation_id(headers),
        None,
    )))
}

fn work_item_action_response(outcome: WorkItemMutationOutcome, headers: &HeaderMap) -> WorkItemActionResult {
    match outcome {
        WorkItemMutationOutcome::Applied(view) => {
            reject_approval_task(&view, headers)?;
            Ok(ApiResponse::ok_with_data(wrap_view(view)))
        }
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
/// 返回携带 `responsibility_kind` 的分页投影。
pub async fn work_item_list(
    State(state): State<AppState>,
    Extension(actor): Extension<AuditActor>,
    Query(params): Query<WorkItemListParams>,
) -> Result<WorkItemHttpPageView> {
    let page = WorkItemService::new(state.db(), state.rbac())
        .work_item_list(&params, &actor)
        .await?;
    Ok(ApiResponse::ok_with_data(wrap_page(page)))
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
/// 返回个人、到期、超期、异常计数及服务端统计时点。
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
/// 返回带 `responsibility_kind` 的任务投影。
pub async fn work_item_detail(
    State(state): State<AppState>,
    Extension(actor): Extension<AuditActor>,
    Path(id): Path<String>,
) -> Result<WorkItemHttpView> {
    let view = WorkItemService::new(state.db(), state.rbac())
        .work_item_detail(&id, &actor)
        .await?;
    Ok(ApiResponse::ok_with_data(wrap_view(view)))
}

#[permission_macros::permission(
    group = "统一待办",
    group_desc = "待办队列与责任处理",
    desc = "查询非审批任务可转交人员",
    resource = "work_item",
    action = "reassign"
)]
/// 查询开放非审批任务当前合格的转交候选人。
///
/// # 返回
/// 返回经账号状态、完整操作权限、管理范围和采购级联约束过滤后的具体账号。
pub async fn work_item_reassign_candidates(
    State(state): State<AppState>,
    Extension(actor): Extension<AuditActor>,
    Path(id): Path<String>,
) -> Result<Vec<WorkItemReassignCandidateView>> {
    let candidates = WorkItemService::new(state.db(), state.rbac())
        .reassign_candidates(&id, &actor)
        .await?;
    Ok(ApiResponse::ok_with_data(candidates))
}

#[permission_macros::permission(
    group = "统一待办",
    group_desc = "待办队列与责任处理",
    desc = "在授权范围内受控转交非审批任务",
    resource = "work_item",
    action = "reassign"
)]
/// 转交开放非审批任务给重新校验合格的用户。
///
/// 审批任务必须失败关闭。
///
/// # 返回
/// 返回责任已更新的同一任务。
pub async fn work_item_reassign(
    State(state): State<AppState>,
    Extension(actor): Extension<AuditActor>,
    headers: HeaderMap,
    Path(id): Path<String>,
    Json(req): Json<ReassignWorkItemRequest>,
) -> WorkItemActionResult {
    let current = WorkItemService::new(state.db(), state.rbac())
        .work_item_detail(&id, &actor)
        .await?;
    reject_approval_task(&current, &headers)?;
    let outcome = WorkItemService::new(state.db(), state.rbac())
        .reassign(&id, req, &actor)
        .await?;
    work_item_action_response(outcome, &headers)
}

#[permission_macros::permission(
    group = "统一待办",
    group_desc = "待办队列与责任处理",
    desc = "关闭重复、误派或已有替代的非审批任务",
    resource = "work_item",
    action = "close"
)]
/// 受控关闭允许人工关闭的无效非审批任务。
///
/// 审批任务必须失败关闭。
///
/// # 返回
/// 返回已关闭任务的只读事实。
pub async fn work_item_close(
    State(state): State<AppState>,
    Extension(actor): Extension<AuditActor>,
    headers: HeaderMap,
    Path(id): Path<String>,
    Json(req): Json<CloseWorkItemRequest>,
) -> WorkItemActionResult {
    let current = WorkItemService::new(state.db(), state.rbac())
        .work_item_detail(&id, &actor)
        .await?;
    reject_approval_task(&current, &headers)?;
    let outcome = WorkItemService::new(state.db(), state.rbac())
        .close(&id, req, &actor)
        .await?;
    work_item_action_response(outcome, &headers)
}

#[cfg(test)]
mod tests {
    use axum::{body::to_bytes, http::StatusCode, response::IntoResponse};
    use entities::work_item::WorkItemType;
    use serde_json::{json, Value};
    use services::work_item::{WorkItemConflict, WorkItemConflictKind};

    use super::{responsibility_kind_of, ResponsibilityKind, WorkItemActionError};

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

    #[tokio::test]
    async fn approval_generic_mutation_maps_to_stable_409() {
        let response = WorkItemActionError::from(services::Error::ConflictError(
            "APPROVAL_GENERIC_WORK_ITEM_MUTATION_FORBIDDEN".to_string(),
        ))
        .into_response();
        assert_eq!(response.status(), StatusCode::CONFLICT);
        let body = to_bytes(response.into_body(), usize::MAX).await.expect("body");
        let body: Value = serde_json::from_slice(&body).expect("json");
        assert_eq!(body["code"], "APPROVAL_GENERIC_WORK_ITEM_MUTATION_FORBIDDEN");
    }

    #[test]
    fn document_approval_is_personal_approval() {
        assert_eq!(
            responsibility_kind_of(WorkItemType::DocumentApproval),
            ResponsibilityKind::PersonalApproval
        );
        assert_eq!(
            responsibility_kind_of(WorkItemType::BusinessException),
            ResponsibilityKind::PersonalBusinessTask
        );
    }
}
