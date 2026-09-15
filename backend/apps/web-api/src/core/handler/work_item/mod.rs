//! 人工任务责任 HTTP 适配层。
//!
//! 已删除 start-processing / release-to-team / claim。通用写接口拒绝审批任务。

use application_core::AuditActor;
use axum::{
    extract::{Path, Query, State},
    http::HeaderMap,
    response::{IntoResponse, Response},
    Extension, Json,
};
use std::future::Future;
use std::pin::Pin;
use std::task::{Context, Poll};

/// Wrap a future so axum can treat it as `Send`.
///
/// Nested `async fn` captures of `&T` trip rustc HRTB even when `T: Sync`.
/// The wrapped work still runs on the request worker; this does not spawn
/// another task or change business behavior.
///
/// # Parameters
/// * `fut` - handler body future
///
/// # Returns
/// The same output, with a `Send` future type.
///
/// # Errors
/// None; errors come from `fut`.
pub(super) fn assume_send<F>(fut: F) -> impl Future<Output = F::Output> + Send
where
    F: Future,
{
    struct SendFut<F>(F);
    unsafe impl<F> Send for SendFut<F> {}
    impl<F: Future> Future for SendFut<F> {
        type Output = F::Output;
        fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
            unsafe { Pin::new_unchecked(&mut self.get_unchecked_mut().0) }.poll(cx)
        }
    }
    SendFut(fut)
}

use erp_processes::adapters::workflow::{work_item_service, workflow_auth};
use erp_read_models::{
    FulfillmentQueueListParams, FulfillmentQueuePageView, WorkItemListParams, WorkItemPageView,
    WorkItemStatsParams, WorkItemStatsView, WorkItemView, WorkbenchReadService,
};
use erp_workflow::entity::work_item::WorkItemType;
use erp_workflow::service::work_item::{
    CloseWorkItemRequest, ReassignWorkItemRequest, WorkItemConflictKind, WorkItemMutationOutcome,
    WorkItemReassignCandidateView,
};
use serde::Serialize;

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
    pub inner: serde_json::Value,
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
    /// 跨页授权及结果集版本，后续页必须回传。
    pub scope_version: String,
}

/// 责任命令 HTTP 边界错误。
#[derive(Debug)]
pub enum WorkItemActionError {
    /// 并发版本或当前责任发生变化。
    Conflict(Box<HttpWorkItemConflict>),
    /// 审批任务保护。
    ApprovalProtected(ApprovalHttpError),
    /// 其余错误沿用统一 HTTP 错误合同。
    Other(HttpError),
}

impl From<erp_workflow::Error> for WorkItemActionError {
    fn from(error: erp_workflow::Error) -> Self {
        erp_processes::Error::from(error).into()
    }
}

impl From<erp_read_models::Error> for WorkItemActionError {
    fn from(error: erp_read_models::Error) -> Self {
        erp_processes::Error::from(error).into()
    }
}

impl From<erp_processes::Error> for WorkItemActionError {
    /// 将服务错误映射为责任命令错误。
    ///
    /// # 参数
    /// * `error` - 服务层错误
    ///
    /// # 返回
    /// 审批任务保护使用稳定码，其余沿用统一映射。
    fn from(error: erp_processes::Error) -> Self {
        if error.code() == Some(erp_workflow::ErrorCode::ApprovalGenericWorkItemMutationForbidden) {
            return Self::ApprovalProtected(ApprovalHttpError::coded(
                erp_workflow::ErrorCode::ApprovalGenericWorkItemMutationForbidden,
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
fn wrap_view<V: serde::Serialize>(view: V) -> WorkItemHttpView {
    let inner = serde_json::to_value(&view).expect("任务投影可序列化");
    let work_item_type = inner
        .get("work_item_type")
        .cloned()
        .and_then(|value| serde_json::from_value(value).ok())
        .expect("任务类型存在");
    WorkItemHttpView {
        inner,
        responsibility_kind: responsibility_kind_of(work_item_type),
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
        scope_version: page.scope_version,
    }
}

/// 拒绝审批任务的通用写命令。
///
/// # 错误
/// `DocumentApproval` 返回稳定 409。
fn reject_approval_task(
    work_item_type: WorkItemType,
    approval_node_execution_id: Option<&str>,
    headers: &HeaderMap,
) -> std::result::Result<(), WorkItemActionError> {
    if work_item_type != WorkItemType::DocumentApproval && approval_node_execution_id.is_none() {
        return Ok(());
    }
    Err(WorkItemActionError::ApprovalProtected(ApprovalHttpError::coded(
        erp_workflow::ErrorCode::ApprovalGenericWorkItemMutationForbidden,
        crate::core::handler::approval_instance::error::correlation_id(headers),
        None,
    )))
}

async fn work_item_action_response(
    state: AppState,
    actor: AuditActor,
    outcome: WorkItemMutationOutcome,
    headers: HeaderMap,
) -> WorkItemActionResult {
    match outcome {
        WorkItemMutationOutcome::Applied { work_item_id } => {
            let view = WorkbenchReadService::new(state.db(), workflow_auth(state.db(), state.rbac()))
                .work_item_detail(work_item_id, actor)
                .await?;
            reject_approval_task(
                view.work_item_type,
                view.approval_node_execution_id.as_deref(),
                &headers,
            )?;
            Ok(ApiResponse::ok_with_data(wrap_view(view)))
        }
        WorkItemMutationOutcome::Conflict(conflict) => {
            let current = match conflict.work_item_id() {
                Some(id) => WorkbenchReadService::new(state.db(), workflow_auth(state.db(), state.rbac()))
                    .work_item_detail(id.to_string(), actor)
                    .await
                    .ok(),
                None => None,
            };
            Err(WorkItemActionError::Conflict(Box::new(HttpWorkItemConflict {
                kind: conflict.kind(),
                current_work_item: current,
            })))
        }
    }
}

/// HTTP 409 payload. Query view is assembled by read-models.
#[derive(Debug, Clone, serde::Serialize)]
pub struct HttpWorkItemConflict {
    #[serde(skip)]
    kind: WorkItemConflictKind,
    current_work_item: Option<WorkItemView>,
}

impl HttpWorkItemConflict {
    fn kind(&self) -> WorkItemConflictKind {
        self.kind
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
    let page = WorkbenchReadService::new(state.db(), workflow_auth(state.db(), state.rbac()))
        .work_item_list(params, actor)
        .await?;
    Ok(ApiResponse::ok_with_data(wrap_page(page)))
}

#[permission_macros::permission(
    group = "统一待办",
    group_desc = "待办队列与责任处理",
    desc = "查询本人授权范围内的履约责任队列",
    resource = "work_item",
    action = "list"
)]
/// 查询 W09 服务端分页履约责任读模型。
///
/// # 返回
/// 返回当前账号开放履约责任的分页、指标和仓库筛选项。
pub async fn fulfillment_queue_list(
    State(state): State<AppState>,
    Extension(actor): Extension<AuditActor>,
    Query(params): Query<FulfillmentQueueListParams>,
) -> Result<FulfillmentQueuePageView> {
    let page = WorkbenchReadService::new(state.db(), workflow_auth(state.db(), state.rbac()))
        .fulfillment_queue_list(params, actor)
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
/// 返回个人、到期、超期、异常、任务族计数及服务端统计时点。
pub async fn work_item_stats(
    State(state): State<AppState>,
    Extension(actor): Extension<AuditActor>,
    Query(params): Query<WorkItemStatsParams>,
) -> Result<WorkItemStatsView> {
    let stats = WorkbenchReadService::new(state.db(), workflow_auth(state.db(), state.rbac()))
        .work_item_stats(params, actor)
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
    let view = WorkbenchReadService::new(state.db(), workflow_auth(state.db(), state.rbac()))
        .work_item_detail(id, actor)
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
    let candidates = work_item_service(state.db(), state.rbac())
        .reassign_candidates(id, actor)
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
pub fn work_item_reassign(
    State(state): State<AppState>,
    Extension(actor): Extension<AuditActor>,
    headers: HeaderMap,
    Path(id): Path<String>,
    Json(req): Json<ReassignWorkItemRequest>,
) -> impl Future<Output = WorkItemActionResult> + Send {
    assume_send(async move {
        let outcome = work_item_service(state.db(), state.rbac())
            .reassign(id, req, actor.clone())
            .await?;
        work_item_action_response(state, actor, outcome, headers).await
    })
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
pub fn work_item_close(
    State(state): State<AppState>,
    Extension(actor): Extension<AuditActor>,
    headers: HeaderMap,
    Path(id): Path<String>,
    Json(req): Json<CloseWorkItemRequest>,
) -> impl Future<Output = WorkItemActionResult> + Send {
    assume_send(async move {
        let outcome = work_item_service(state.db(), state.rbac())
            .close(id, req, actor.clone())
            .await?;
        work_item_action_response(state, actor, outcome, headers).await
    })
}

#[cfg(test)]
mod tests {
    use axum::{body::to_bytes, http::StatusCode, response::IntoResponse};
    use erp_workflow::entity::work_item::WorkItemType;
    use erp_workflow::service::work_item::WorkItemConflictKind;
    use serde_json::{json, Value};

    use super::{responsibility_kind_of, HttpWorkItemConflict, ResponsibilityKind, WorkItemActionError};

    #[tokio::test]
    async fn version_conflict_uses_409_stable_code_and_safe_tombstone() {
        let response = WorkItemActionError::Conflict(Box::new(HttpWorkItemConflict {
            kind: WorkItemConflictKind::Version,
            current_work_item: None,
        }))
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
        let response = WorkItemActionError::Conflict(Box::new(HttpWorkItemConflict {
            kind: WorkItemConflictKind::Responsibility,
            current_work_item: None,
        }))
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
        let response = WorkItemActionError::from(erp_processes::Error::from_approval_code(
            erp_workflow::ErrorCode::ApprovalGenericWorkItemMutationForbidden,
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
