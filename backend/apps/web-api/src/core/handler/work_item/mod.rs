//! 人工任务责任 HTTP 适配层。
//!
//! 已删除 start-processing / release-to-team / claim。通用写接口拒绝审批任务。

use std::result::Result as StdResult;

use application_core::AuditActor;
use axum::extract::{Path, Query, State};
use axum::http::HeaderMap;
use axum::response::{IntoResponse, Response};
use axum::{Extension, Json};
use erp_processes::Error as ProcessError;
use erp_processes::adapters::workflow::{work_item_service, workflow_auth};
use erp_read_models::{
    Error as ReadModelError, FulfillmentQueueListParams, FulfillmentQueuePageView, WorkItemListParams,
    WorkItemPageView, WorkItemStatsParams, WorkItemStatsView, WorkItemView, WorkbenchReadService,
};
use erp_workflow::entity::work_item::WorkItemType;
use erp_workflow::service::work_item::{
    CloseWorkItemRequest, ReassignWorkItemRequest, WorkItemConflictKind, WorkItemMutationOutcome,
    WorkItemReassignCandidateView,
};
use erp_workflow::{Error as WorkflowError, ErrorCode};
use serde::Serialize;
use uuid::Uuid;

use crate::app_state::AppState;
use crate::core::errors::{Error as HttpError, Result};
use crate::core::handler::approval_instance::error::{ApprovalHttpError, correlation_id};
use crate::core::response::ApiResponse;

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

/// 为读模型增加 HTTP 合同要求的 `responsibility_kind`，其余字段沿用安全投影。
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

impl From<WorkflowError> for WorkItemActionError {
    /// 沿用流程命令的 HTTP 错误分类。
    ///
    /// # 参数
    /// * `error` - 流程领域错误
    ///
    /// # 返回
    /// 返回责任命令错误。
    ///
    /// # 错误
    /// 转换本身不失败。
    fn from(error: WorkflowError) -> Self {
        ProcessError::from(error).into()
    }
}

impl From<ReadModelError> for WorkItemActionError {
    /// 沿用查询错误的统一 HTTP 分类。
    ///
    /// # 参数
    /// * `error` - 已授权任务查询的错误
    ///
    /// # 返回
    /// 返回责任命令错误。
    ///
    /// # 错误
    /// 转换本身不失败。
    fn from(error: ReadModelError) -> Self {
        ProcessError::from(error).into()
    }
}

impl From<ProcessError> for WorkItemActionError {
    /// 将服务错误映射为责任命令错误。
    ///
    /// # 参数
    /// * `error` - 服务层错误
    ///
    /// # 返回
    /// 审批任务保护使用稳定码，其余沿用统一映射。
    ///
    /// # 错误
    /// 转换本身不失败，保留原服务错误分类。
    fn from(error: ProcessError) -> Self {
        if error.code() == Some(ErrorCode::ApprovalGenericWorkItemMutationForbidden) {
            return Self::ApprovalProtected(ApprovalHttpError::coded(
                ErrorCode::ApprovalGenericWorkItemMutationForbidden,
                Uuid::new_v4().to_string(),
                None,
            ));
        }
        Self::Other(HttpError::from(error))
    }
}

impl IntoResponse for WorkItemActionError {
    /// 将责任命令错误转换为真实 HTTP 状态与稳定 JSON 信封。
    ///
    /// # 参数
    /// * `self` - 已分类的责任命令错误
    ///
    /// # 返回
    /// 冲突返回 409 和权限安全的最新任务摘要；审批保护返回稳定码。
    ///
    /// # 错误
    /// 响应构造不失败。
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
            },
            Self::ApprovalProtected(error) => error.into_response(),
            Self::Other(error) => error.into_response(),
        }
    }
}

/// 责任命令 HTTP 结果。
pub type WorkItemActionResult = StdResult<ApiResponse<WorkItemHttpView>, WorkItemActionError>;

/// 由任务类型计算责任类型。
///
/// # 参数
/// * `work_item_type` - 服务层任务类型
///
/// # 返回
/// `DocumentApproval` 对应 `PERSONAL_APPROVAL`。
///
/// # 错误
/// 无。
pub fn responsibility_kind_of(work_item_type: WorkItemType) -> ResponsibilityKind {
    if work_item_type == WorkItemType::DocumentApproval {
        ResponsibilityKind::PersonalApproval
    } else {
        ResponsibilityKind::PersonalBusinessTask
    }
}

impl From<WorkItemView> for WorkItemHttpView {
    /// 保持服务层安全字段的平铺形状，并追加 HTTP 责任类型。
    ///
    /// # 参数
    /// * `inner` - 已授权的强类型任务投影
    ///
    /// # 返回
    /// 返回包含原投影全部字段和责任类型的 HTTP 投影。
    ///
    /// # 错误
    /// 无。
    fn from(inner: WorkItemView) -> Self {
        let responsibility_kind = responsibility_kind_of(inner.work_item_type);
        Self { inner, responsibility_kind }
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
        items: page.items.into_iter().map(WorkItemHttpView::from).collect(),
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
) -> StdResult<(), WorkItemActionError> {
    if work_item_type != WorkItemType::DocumentApproval && approval_node_execution_id.is_none() {
        return Ok(());
    }
    Err(WorkItemActionError::ApprovalProtected(ApprovalHttpError::coded(
        ErrorCode::ApprovalGenericWorkItemMutationForbidden,
        correlation_id(headers),
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
            reject_approval_task(view.work_item_type, view.approval_node_execution_id.as_deref(), &headers)?;
            Ok(ApiResponse::ok_with_data(view.into()))
        },
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
        },
    }
}

/// HTTP 409 payload. Query view is assembled by read-models.
#[derive(Debug, Clone, Serialize)]
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
/// # 参数
/// * `state` - 应用状态
/// * `actor` - 当前认证账号
/// * `params` - 责任范围、过滤与分页参数
///
/// # 返回
/// 返回携带 `responsibility_kind` 的分页投影。
///
/// # 错误
/// 查询参数无效、授权版本变化或查询失败时返回错误。
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
/// # 参数
/// * `state` - 应用状态
/// * `actor` - 当前认证账号
/// * `params` - 履约责任过滤与分页参数
///
/// # 返回
/// 返回当前账号开放履约责任的分页、指标和仓库筛选项。
///
/// # 错误
/// 查询参数无效、授权版本变化或查询失败时返回错误。
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
/// # 参数
/// * `state` - 应用状态
/// * `actor` - 当前认证账号
/// * `params` - 责任范围和统计过滤参数
///
/// # 返回
/// 返回个人、到期、超期、异常、任务族计数及服务端统计时点。
///
/// # 错误
/// 查询参数无效或授权统计失败时返回错误。
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
/// # 参数
/// * `state` - 应用状态
/// * `actor` - 当前认证账号
/// * `id` - 任务 ID
///
/// # 返回
/// 返回带 `responsibility_kind` 的任务投影。
///
/// # 错误
/// 任务不存在、当前账号不可见或查询失败时返回错误。
pub async fn work_item_detail(
    State(state): State<AppState>,
    Extension(actor): Extension<AuditActor>,
    Path(id): Path<String>,
) -> Result<WorkItemHttpView> {
    let view = WorkbenchReadService::new(state.db(), workflow_auth(state.db(), state.rbac()))
        .work_item_detail(id, actor)
        .await?;
    Ok(ApiResponse::ok_with_data(view.into()))
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
/// # 参数
/// * `state` - 应用状态
/// * `actor` - 当前认证账号
/// * `id` - 任务 ID
///
/// # 返回
/// 返回经账号状态、完整操作权限、管理范围和采购级联约束过滤后的具体账号。
///
/// # 错误
/// 任务不可见、不允许转交或候选查询失败时返回错误。
pub async fn work_item_reassign_candidates(
    State(state): State<AppState>,
    Extension(actor): Extension<AuditActor>,
    Path(id): Path<String>,
) -> Result<Vec<WorkItemReassignCandidateView>> {
    let candidates = work_item_service(state.db(), state.rbac()).reassign_candidates(id, actor).await?;
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
/// # 参数
/// * `state` - 应用状态
/// * `actor` - 当前操作人
/// * `headers` - 请求追踪等响应上下文
/// * `id` - 任务 ID
/// * `req` - 责任动作请求
///
/// # 返回
/// 返回责任已更新的同一任务。
///
/// # 错误
/// 授权不足、目标资格不符、版本冲突或任务转交失败时返回错误。
pub async fn work_item_reassign(
    State(state): State<AppState>,
    Extension(actor): Extension<AuditActor>,
    headers: HeaderMap,
    Path(id): Path<String>,
    Json(req): Json<ReassignWorkItemRequest>,
) -> WorkItemActionResult {
    let outcome = work_item_service(state.db(), state.rbac()).reassign(id, req, actor.clone()).await?;
    work_item_action_response(state, actor, outcome, headers).await
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
/// # 参数
/// * `state` - 应用状态
/// * `actor` - 当前操作人
/// * `headers` - 请求追踪等响应上下文
/// * `id` - 任务 ID
/// * `req` - 责任动作请求
///
/// # 返回
/// 返回已关闭任务的只读事实。
///
/// # 错误
/// 授权不足、任务不允许关闭、版本冲突或任务关闭失败时返回错误。
pub async fn work_item_close(
    State(state): State<AppState>,
    Extension(actor): Extension<AuditActor>,
    headers: HeaderMap,
    Path(id): Path<String>,
    Json(req): Json<CloseWorkItemRequest>,
) -> WorkItemActionResult {
    let outcome = work_item_service(state.db(), state.rbac()).close(id, req, actor.clone()).await?;
    work_item_action_response(state, actor, outcome, headers).await
}

#[cfg(test)]
mod tests {
    use axum::body::to_bytes;
    use axum::http::StatusCode;
    use axum::response::IntoResponse;
    use erp_workflow::dto::work_item::{ProcessingState, WorkItemAllowedAction, WorkItemPartyView};
    use erp_workflow::entity::work_item::{AssignmentSource, WorkItemPriority, WorkItemStatus, WorkItemType};
    use erp_workflow::service::work_item::WorkItemConflictKind;
    use serde_json::{Value, json, to_value};

    use super::{
        ErrorCode, HttpWorkItemConflict, ProcessError, ResponsibilityKind, WorkItemActionError,
        WorkItemHttpView, WorkItemPageView, WorkItemView, responsibility_kind_of, wrap_page,
    };

    fn task_view(work_item_type: WorkItemType) -> WorkItemView {
        WorkItemView {
            id: "task-id".to_string(),
            work_item_type,
            handler_key: "task-handler".to_string(),
            destination_workspace_id: "workspace".to_string(),
            route_context: None,
            approval_node_execution_id: (work_item_type == WorkItemType::DocumentApproval)
                .then(|| "node-id".to_string()),
            approval_context: None,
            status: WorkItemStatus::Open,
            assignment_source: AssignmentSource::SystemRule,
            owner_role: "finance".to_string(),
            owner_role_label: "财务".to_string(),
            owner_organization_id: "org-id".to_string(),
            owner_organization: WorkItemPartyView {
                id: "org-id".to_string(),
                display_name: "财务组织".to_string(),
            },
            owner_user_id: None,
            owner_user: None,
            processing_state: ProcessingState::Ready,
            processing_blocker: None,
            business_object_type: "invoice".to_string(),
            business_object_id: "invoice-id".to_string(),
            root_business_object_id: "invoice-id".to_string(),
            business_object_label: "INV-001".to_string(),
            counterparty_label: Some("客户".to_string()),
            next_action_hint: "处理".to_string(),
            summary_sections: Vec::new(),
            brief_lines: Vec::new(),
            brief_more_count: None,
            list_summary: None,
            subject_version: "2".to_string(),
            task_version: "3".to_string(),
            allowed_actions: vec![WorkItemAllowedAction::View],
            action_blockers: Vec::new(),
            priority: WorkItemPriority::Normal,
            due_at: Some(123),
            reason_code: None,
            reason_label: "待处理".to_string(),
            impact_summary: "待处理发票".to_string(),
            assigned_at: None,
            started_at: None,
            current_assignment_at: None,
            last_activity_at: None,
            completed_at: None,
            completed_by: None,
            closed_at: None,
            closed_by: None,
            close_reason: None,
            created_at: 100,
            queue_context_id: "queue-id".to_string(),
        }
    }

    #[test]
    fn typed_task_wrapper_keeps_flattened_fields_for_approval_and_business_tasks() {
        for (task_type, responsibility) in [
            (WorkItemType::DocumentApproval, "PERSONAL_APPROVAL"),
            (WorkItemType::BusinessException, "PERSONAL_BUSINESS_TASK"),
        ] {
            let view = task_view(task_type);
            let mut expected = to_value(&view).expect("task view serializes");
            expected["responsibility_kind"] = json!(responsibility);
            let actual = to_value(WorkItemHttpView::from(view)).expect("HTTP view serializes");

            assert_eq!(actual, expected);
            assert!(actual.get("inner").is_none());
            assert!(actual.get("route_context").is_none());
            assert_eq!(actual["owner_user"], Value::Null);
        }
    }

    #[test]
    fn typed_task_page_keeps_pagination_and_empty_page_contract() {
        for items in [Vec::new(), vec![task_view(WorkItemType::BusinessException)]] {
            let expected_items = items.iter().cloned().map(WorkItemHttpView::from).collect::<Vec<_>>();
            let page = WorkItemPageView {
                items,
                total: 42,
                page: 3,
                page_size: 20,
                queue_context_id: "queue-id".to_string(),
                scope_version: "scope-v2".to_string(),
            };

            assert_eq!(
                to_value(wrap_page(page)).expect("page serializes"),
                json!({
                    "items": expected_items,
                    "total": 42,
                    "page": 3,
                    "page_size": 20,
                    "queue_context_id": "queue-id",
                    "scope_version": "scope-v2",
                })
            );
        }
    }

    #[tokio::test]
    async fn version_conflict_uses_409_stable_code_and_safe_tombstone() {
        let response = WorkItemActionError::Conflict(Box::new(HttpWorkItemConflict {
            kind: WorkItemConflictKind::Version,
            current_work_item: None,
        }))
        .into_response();

        assert_eq!(response.status(), StatusCode::CONFLICT);
        let body =
            to_bytes(response.into_body(), usize::MAX).await.expect("conflict body should be readable");
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
        let body =
            to_bytes(response.into_body(), usize::MAX).await.expect("conflict body should be readable");
        let body: Value = serde_json::from_slice(&body).expect("conflict body should be JSON");

        assert_eq!(body["code"], "WORK_ITEM_RESPONSIBILITY_CONFLICT");
        assert_eq!(body["data"], json!({ "current_work_item": null }));
    }

    #[tokio::test]
    async fn approval_generic_mutation_maps_to_stable_409() {
        let response = WorkItemActionError::from(ProcessError::from_approval_code(
            ErrorCode::ApprovalGenericWorkItemMutationForbidden,
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
