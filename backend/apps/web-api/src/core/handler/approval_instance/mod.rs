//! 审批实例、决定、恢复与受阻取消 HTTP 适配层。
//!
//! 删除 `recover` 与 `RETRY_CURRENT_STEP`。Handler 只做协议适配，不解释 ProcessKind。

/// P0-B 在 `handler/mod.rs` 正式声明本模块后应删除此 `#[path]`。
#[path = "../approval_process/mod.rs"]
pub mod approval_process;
pub mod error;
pub mod http;

use axum::{
    extract::{Path, Query, State},
    http::HeaderMap,
    Extension, Json,
};
use entities::document_registry::DocumentType;
use services::approval::execution::{ApprovalRuntimeService, UpgradeBindingCommand};
use services::approval::{ApprovalCancelBlockedCommand, ApprovalResumeCommand};
use services::audit::AuditActor;

use crate::{
    app_state::AppState,
    core::{
        handler::approval_instance::error::{parse_optional_version, parse_version, ApprovalHttpError},
        response::ApiResponse,
    },
};

use self::http::{
    CancelBlockedHttpRequest, DecisionValue, InstanceHistoryQuery, InstanceListCursor,
    PreparedInstanceListQuery, ResumeApproverHttpRequest, SubmitDecisionHttpRequest,
    UpgradeBindingHttpRequest,
};

/// 审批实例 Handler 结果。
type ApprovalResult<T> = std::result::Result<ApiResponse<T>, ApprovalHttpError>;

#[permission_macros::permission(
    group = "审批实例",
    group_desc = "审批运行、决定、恢复与取消",
    desc = "提交当前开放审批任务的通过或驳回",
    resource = "approval_instance",
    action = "decide"
)]
/// 对当前开放审批任务通过或驳回。
///
/// actor 只从认证上下文注入。`DecisionOutcome::Blocked` 必须映射为 409。
///
/// # 错误
/// 协议非法或运行端口未接入时返回稳定错误。
pub async fn submit_decision(
    State(state): State<AppState>,
    Extension(actor): Extension<AuditActor>,
    headers: HeaderMap,
    Json(request): Json<SubmitDecisionHttpRequest>,
) -> ApprovalResult<serde_json::Value> {
    let command = decision_command(request, actor.id(), &headers)?;
    let view = runtime_service(&state)
        .submit_decision(
            &actor,
            &command.work_item_id,
            command.decision.as_str(),
            command.reason.as_deref(),
            command.expected_task_version,
            &command.idempotency_key,
        )
        .await
        .map_err(|error| ApprovalHttpError::from_service(error, &headers))?;
    ok_json(view)
}

#[permission_macros::permission(
    group = "审批实例",
    group_desc = "审批运行、决定、恢复与取消",
    desc = "按固定 view 查询审批实例",
    resource = "approval_instance",
    action = "read"
)]
/// 按 `mine|started|managed|blocked` 查询实例摘要。
///
/// # 错误
/// view/status 组合非法时返回 422，不得返回伪空列表。
pub async fn instance_list(
    State(state): State<AppState>,
    Extension(actor): Extension<AuditActor>,
    headers: HeaderMap,
    PreparedInstanceListQuery { view, query }: PreparedInstanceListQuery,
) -> ApprovalResult<serde_json::Value> {
    let page = runtime_service(&state)
        .instance_list(&actor, query)
        .await
        .map_err(|error| ApprovalHttpError::from_service(error, &headers))?;
    let next_cursor = page
        .next_cursor
        .map(|cursor| InstanceListCursor::from_runtime(view, cursor).encode());
    ok_json(serde_json::json!({
        "items": page.items,
        "total": page.total,
        "next_cursor": next_cursor,
    }))
}

#[permission_macros::permission(
    group = "审批实例",
    group_desc = "审批运行、决定、恢复与取消",
    desc = "查询审批实例详情",
    resource = "approval_instance",
    action = "read"
)]
/// 返回实例、当前责任和最近受控条数历史。
///
/// 详情最多 20 条执行摘要并携带 `history_next_cursor`。
///
/// # 错误
/// 无权时不泄露存在性。
pub async fn instance_detail(
    State(state): State<AppState>,
    Extension(actor): Extension<AuditActor>,
    Path(id): Path<String>,
    headers: HeaderMap,
) -> ApprovalResult<serde_json::Value> {
    let _ = http::DETAIL_HISTORY_LIMIT;
    let view = runtime_service(&state)
        .instance_detail(&actor, &id)
        .await
        .map_err(|error| ApprovalHttpError::from_service(error, &headers))?;
    ok_json(view)
}

#[permission_macros::permission(
    group = "审批实例",
    group_desc = "审批运行、决定、恢复与取消",
    desc = "按轮次读取审批历史",
    resource = "approval_instance",
    action = "read"
)]
/// 按 `round_no asc, execution_no asc, id asc` 读取完整历史。
///
/// # 错误
/// 页大小非法时返回 422。
pub async fn instance_history(
    State(state): State<AppState>,
    Extension(actor): Extension<AuditActor>,
    Path(id): Path<String>,
    headers: HeaderMap,
    Query(query): Query<InstanceHistoryQuery>,
) -> ApprovalResult<serde_json::Value> {
    let limit = query
        .normalized_limit()
        .map_err(|message| ApprovalHttpError::unprocessable(message, &headers))?;
    let after_execution_no = query
        .normalized_cursor()
        .map_err(|message| ApprovalHttpError::unprocessable(message, &headers))?;
    let page = runtime_service(&state)
        .instance_history(&actor, &id, after_execution_no, limit)
        .await
        .map_err(|error| ApprovalHttpError::from_service(error, &headers))?;
    ok_json(page)
}

#[permission_macros::permission(
    group = "审批实例",
    group_desc = "审批运行、决定、恢复与取消",
    desc = "查询当前 blocker 的唯一合法恢复方式",
    resource = "approval_instance",
    action = "read"
)]
/// 返回当前 blocker 的唯一合法恢复方式。
///
/// 只对通过运行管理权与 DataScope 的 actor 返回结果。
///
/// # 错误
/// 无权时不泄露实例存在性。
pub async fn recovery_options(
    State(state): State<AppState>,
    Extension(actor): Extension<AuditActor>,
    Path(id): Path<String>,
    headers: HeaderMap,
) -> ApprovalResult<serde_json::Value> {
    let view = runtime_service(&state)
        .recovery_options(&actor, &id)
        .await
        .map_err(|error| ApprovalHttpError::from_service(error, &headers))?;
    ok_json(view)
}

#[permission_macros::permission(
    group = "审批实例",
    group_desc = "审批运行、决定、恢复与取消",
    desc = "原审批人重新合格后恢复当前节点",
    resource = "approval_instance",
    action = "resume"
)]
/// 恢复当前审批人并创建新执行和新任务。
///
/// 不接受目标用户、决定、节点或恢复动作枚举。
///
/// # 错误
/// 版本非法或端口未接入时返回稳定错误。
pub async fn resume_current_approver(
    State(state): State<AppState>,
    Extension(actor): Extension<AuditActor>,
    headers: HeaderMap,
    Path(id): Path<String>,
    Json(request): Json<ResumeApproverHttpRequest>,
) -> ApprovalResult<serde_json::Value> {
    let command = resume_command(id, request, actor.id(), &headers)?;
    let view = runtime_service(&state)
        .resume_current_approver(&actor, command)
        .await
        .map_err(|error| ApprovalHttpError::from_service(error, &headers))?;
    ok_json(view)
}

#[permission_macros::permission(
    group = "审批实例",
    group_desc = "审批运行、决定、恢复与取消",
    desc = "取消非人员一致性 blocker",
    resource = "approval_instance",
    action = "cancel_blocked"
)]
/// 取消非人员一致性 blocker。
///
/// 不得接受新定义、修复值、下一节点或目标用户。
///
/// # 错误
/// 人员失效 blocker 必须拒绝。
pub async fn cancel_blocked(
    State(state): State<AppState>,
    Extension(actor): Extension<AuditActor>,
    headers: HeaderMap,
    Path(id): Path<String>,
    Json(request): Json<CancelBlockedHttpRequest>,
) -> ApprovalResult<serde_json::Value> {
    let command = cancel_blocked_command(id, request, actor.id(), &headers)?;
    let view = runtime_service(&state)
        .cancel_blocked(&actor, command)
        .await
        .map_err(|error| ApprovalHttpError::from_service(error, &headers))?;
    ok_json(view)
}

#[permission_macros::permission(
    group = "审批实例",
    group_desc = "审批运行、决定、恢复与取消",
    desc = "升级未提交单据的审批定义绑定",
    resource = "approval_instance",
    action = "upgrade_binding"
)]
/// 升级未提交单据绑定到当前发布版本。
///
/// 目标定义不得由客户端提交。
///
/// # 错误
/// 请求夹带 definition ID 时在反序列化阶段拒绝。
pub async fn upgrade_binding(
    State(state): State<AppState>,
    Extension(actor): Extension<AuditActor>,
    headers: HeaderMap,
    Path((document_type, document_id)): Path<(DocumentType, String)>,
    Json(request): Json<UpgradeBindingHttpRequest>,
) -> ApprovalResult<serde_json::Value> {
    ensure_non_empty_reason(&request.reason, &headers)?;
    let expected_document_version = parse_version(&request.expected_document_version, "单据", &headers)?;
    let expected_approval_binding_version =
        parse_version(&request.expected_approval_binding_version, "审批绑定", &headers)?;
    let view = runtime_service(&state)
        .upgrade_binding(
            &actor,
            UpgradeBindingCommand {
                document_type,
                document_id,
                reason: request.reason,
                expected_document_version,
                expected_approval_binding_version,
                idempotency_key: request.idempotency_key,
            },
        )
        .await
        .map_err(|error| ApprovalHttpError::from_service(error, &headers))?;
    ok_json(view)
}

/// 构造运行服务。
fn runtime_service(state: &AppState) -> std::sync::Arc<ApprovalRuntimeService> {
    state.approval_runtime_service()
}

/// 将服务视图序列化为 JSON 响应。
fn ok_json<T: serde::Serialize>(data: T) -> ApprovalResult<serde_json::Value> {
    serde_json::to_value(data)
        .map(ApiResponse::ok_with_data)
        .map_err(|error| ApprovalHttpError::from(services::Error::Internal(error.to_string())))
}

/// 协议层已注入 actor 的决定命令。
#[derive(Debug, Clone, PartialEq, Eq)]
struct ValidatedDecision {
    work_item_id: String,
    decision: DecisionValue,
    reason: Option<String>,
    expected_task_version: u64,
    idempotency_key: String,
    actor_id: String,
}

/// 把决定 HTTP 请求转为协议命令并注入 actor。
///
/// # 错误
/// 版本非法或驳回原因为空时返回稳定错误。
fn decision_command(
    request: SubmitDecisionHttpRequest,
    actor_id: &str,
    headers: &HeaderMap,
) -> Result<ValidatedDecision, ApprovalHttpError> {
    let expected_task_version = parse_version(&request.expected_task_version, "审批任务", headers)?;
    if request.decision == DecisionValue::Reject && request.reason.as_deref().unwrap_or("").trim().is_empty()
    {
        return Err(ApprovalHttpError::coded(
            services::ErrorCode::ApprovalRejectReasonRequired,
            crate::core::handler::approval_instance::error::correlation_id(headers),
            None,
        ));
    }
    Ok(ValidatedDecision {
        work_item_id: request.work_item_id,
        decision: request.decision,
        reason: request.reason,
        expected_task_version,
        idempotency_key: request.idempotency_key,
        actor_id: actor_id.to_string(),
    })
}

/// 把恢复请求转为协议命令。
///
/// # 错误
/// 版本非法时返回 400。
fn resume_command(
    instance_id: String,
    request: ResumeApproverHttpRequest,
    actor_id: &str,
    headers: &HeaderMap,
) -> Result<ApprovalResumeCommand, ApprovalHttpError> {
    Ok(ApprovalResumeCommand {
        approval_process_instance_id: instance_id,
        expected_instance_version: parse_version(&request.expected_instance_version, "审批实例", headers)?,
        expected_execution_version: parse_version(&request.expected_execution_version, "节点执行", headers)?,
        expected_assignment_version: parse_version(
            &request.expected_assignment_version,
            "审批人绑定",
            headers,
        )?,
        expected_closed_task_version: parse_optional_version(
            request.expected_closed_task_version.as_deref(),
            "已关闭任务",
            headers,
        )?,
        idempotency_key: request.idempotency_key,
        actor_id: actor_id.to_string(),
    })
}

/// 把受阻取消请求转为协议命令。
///
/// blocker 由服务端从当前实例推导，请求体不得携带。
///
/// # 错误
/// 原因为空或版本非法时返回协议错误。
fn cancel_blocked_command(
    instance_id: String,
    request: CancelBlockedHttpRequest,
    actor_id: &str,
    headers: &HeaderMap,
) -> Result<ApprovalCancelBlockedCommand, ApprovalHttpError> {
    ensure_non_empty_reason(&request.reason, headers)?;
    Ok(ApprovalCancelBlockedCommand {
        approval_process_instance_id: instance_id,
        expected_instance_version: parse_version(&request.expected_instance_version, "审批实例", headers)?,
        expected_execution_version: parse_version(&request.expected_execution_version, "节点执行", headers)?,
        expected_task_version: parse_optional_version(
            request.expected_task_version.as_deref(),
            "审批任务",
            headers,
        )?,
        reason: request.reason,
        idempotency_key: request.idempotency_key,
        actor_id: actor_id.to_string(),
    })
}

/// 要求非空原因。
///
/// # 错误
/// 空白原因返回 422。
fn ensure_non_empty_reason(reason: &str, headers: &HeaderMap) -> Result<(), ApprovalHttpError> {
    if reason.trim().is_empty() {
        return Err(ApprovalHttpError::unprocessable("请填写原因后再提交", headers));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use axum::{
        body::{to_bytes, Body},
        http::{HeaderMap, Request, StatusCode},
        response::Response,
        routing::get,
        Router,
    };
    use serde_json::json;
    use tower::ServiceExt;

    use super::{decision_command, resume_command, PreparedInstanceListQuery, SubmitDecisionHttpRequest};
    use crate::core::handler::approval_instance::http::{DecisionValue, ResumeApproverHttpRequest};

    async fn instance_list_query_boundary(_: PreparedInstanceListQuery) -> StatusCode {
        StatusCode::OK
    }

    async fn instance_list_query_response(uri: &str) -> Response {
        Router::new()
            .route("/", get(instance_list_query_boundary))
            .oneshot(
                Request::builder()
                    .uri(uri)
                    .body(Body::empty())
                    .expect("构造查询请求"),
            )
            .await
            .expect("执行 Axum 查询提取")
    }

    async fn instance_list_query_status(uri: &str) -> StatusCode {
        instance_list_query_response(uri).await.status()
    }

    /// 生产 Handler 使用的查询提取器必须把反序列化与查询合同错误统一为 422。
    #[tokio::test]
    async fn instance_list_query_extractor_returns_422_for_all_invalid_boundaries() {
        let long_query = "a".repeat(129);
        let cases = [
            "/?view=mine&unexpected=true".to_string(),
            "/?view=unknown".to_string(),
            "/?view=managed&status=UNKNOWN".to_string(),
            "/?view=mine&status=APPROVED".to_string(),
            "/?view=mine&document_type=unknown".to_string(),
            "/?view=mine&document_type=".to_string(),
            "/?view=mine&document_type=%20%20".to_string(),
            "/?view=mine&limit=not-a-number".to_string(),
            "/?view=mine&limit=0".to_string(),
            "/?view=mine&limit=101".to_string(),
            "/?view=mine&cursor=".to_string(),
            "/?view=mine&cursor=mine%7Cnot-a-time%7Cwi-1".to_string(),
            "/?view=mine&cursor=mine%7C-1%7Cwi-1".to_string(),
            "/?view=started&cursor=managed%7C1%7Cinst-1".to_string(),
            format!("/?view=mine&q={long_query}"),
        ];

        for uri in cases {
            assert_eq!(
                instance_list_query_status(&uri).await,
                StatusCode::UNPROCESSABLE_ENTITY,
                "{uri} 必须由真实 Axum 提取边界映射为 422"
            );
        }
    }

    /// 查询提取失败必须沿用审批 API 的稳定 JSON 错误信封，而不是 Axum 文本拒绝。
    #[tokio::test]
    async fn instance_list_query_extractor_keeps_stable_error_envelope() {
        let response = instance_list_query_response("/?view=unknown").await;
        assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);
        let body = to_bytes(response.into_body(), usize::MAX)
            .await
            .expect("读取错误信封");
        let payload: serde_json::Value = serde_json::from_slice(&body).expect("解析错误信封");
        assert_eq!(payload["status"], 422);
        assert_eq!(payload["success"], false);
        assert_eq!(payload["code"], "BUSINESS_RULE_BLOCKED");
        assert_eq!(payload.get("retryable"), Some(&serde_json::Value::Bool(false)));
        assert_eq!(payload.get("data"), Some(&serde_json::Value::Null));
        assert!(payload["errorMessage"]
            .as_str()
            .is_some_and(|message| !message.is_empty()));
    }

    /// 合法查询必须通过同一生产提取器，且 Managed cursor 保留完整 i64 时间域。
    #[tokio::test]
    async fn instance_list_query_extractor_accepts_valid_values() {
        for uri in [
            "/?view=mine&status=RUNNING&cursor=mine%7C1%7Cwi-1&limit=1",
            "/?view=managed&status=APPROVED&cursor=managed%7C-1%7Cinst-1&limit=100&q=%20SO-1%20",
        ] {
            assert_eq!(instance_list_query_status(uri).await, StatusCode::OK, "{uri}");
        }
    }

    #[test]
    fn decision_injects_actor_and_rejects_empty_reject_reason() {
        let command = decision_command(
            SubmitDecisionHttpRequest {
                work_item_id: "wi-1".to_string(),
                decision: DecisionValue::Approve,
                reason: None,
                expected_task_version: "3".to_string(),
                idempotency_key: "k1".to_string(),
            },
            "user-1",
            &HeaderMap::new(),
        )
        .expect("合法决定");
        assert_eq!(command.actor_id, "user-1");
        assert_eq!(command.work_item_id, "wi-1");
        assert_eq!(command.expected_task_version, 3);

        let error = decision_command(
            SubmitDecisionHttpRequest {
                work_item_id: "wi-1".to_string(),
                decision: DecisionValue::Reject,
                reason: Some("  ".to_string()),
                expected_task_version: "3".to_string(),
                idempotency_key: "k1".to_string(),
            },
            "user-1",
            &HeaderMap::new(),
        )
        .expect_err("空驳回原因");
        assert_eq!(error.code(), "APPROVAL_REJECT_REASON_REQUIRED");
    }

    #[test]
    fn resume_injects_path_id_and_actor() {
        let command = resume_command(
            "inst-1".to_string(),
            ResumeApproverHttpRequest {
                expected_instance_version: "2".to_string(),
                expected_execution_version: "4".to_string(),
                expected_assignment_version: "1".to_string(),
                expected_closed_task_version: None,
                idempotency_key: "k4".to_string(),
            },
            "admin-1",
            &HeaderMap::new(),
        )
        .expect("合法恢复");
        assert_eq!(command.approval_process_instance_id, "inst-1");
        assert_eq!(command.actor_id, "admin-1");
        assert_eq!(command.expected_instance_version, 2);
    }

    #[test]
    fn recover_alias_payload_is_rejected() {
        assert!(serde_json::from_value::<ResumeApproverHttpRequest>(json!({
            "expected_instance_version": "1",
            "expected_execution_version": "1",
            "expected_assignment_version": "1",
            "idempotency_key": "k1",
            "recovery_action": "RETRY_CURRENT_STEP"
        }))
        .is_err());
    }
}
