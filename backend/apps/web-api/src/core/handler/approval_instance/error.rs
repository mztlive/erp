//! 审批 HTTP 稳定错误码映射。
//!
//! Handler 不得把 BPM 或数据库错误直接暴露给客户端；只识别服务层稳定码与已知文案。

use axum::{
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
};
use serde_json::Value;
use uuid::Uuid;

use crate::core::{errors::Error as HttpError, response::ApiResponse};

/// 跟踪请求的关联 ID 头。
const TRACE_ID_HEADER: &str = "X-Trace-Id";

/// 审批 HTTP 边界错误。
#[derive(Debug)]
pub struct ApprovalHttpError(Box<ApprovalHttpErrorInner>);

#[derive(Debug)]
struct ApprovalHttpErrorInner {
    status: StatusCode,
    code: &'static str,
    message: String,
    correlation_id: String,
    retryable: bool,
    data: Option<Value>,
}

impl ApprovalHttpError {
    /// 装箱构造审批 HTTP 错误。
    ///
    /// # 返回
    /// 返回指针大小的错误，避免 clippy `result_large_err`。
    fn new(
        status: StatusCode,
        code: &'static str,
        message: String,
        correlation_id: String,
        retryable: bool,
        data: Option<Value>,
    ) -> Self {
        Self(Box::new(ApprovalHttpErrorInner {
            status,
            code,
            message,
            correlation_id,
            retryable,
            data,
        }))
    }
}

impl ApprovalHttpError {
    /// 由稳定码与可选冲突回读数据构造错误。
    ///
    /// # 参数
    /// * `code` - 合同冻结的 `APPROVAL_*` 码
    /// * `correlation_id` - 请求关联 ID
    /// * `data` - 409 可回读的安全数据；403/404 必须为空
    ///
    /// # 返回
    /// 返回带 HTTP 状态的审批错误。
    pub fn coded(code: &'static str, correlation_id: String, data: Option<Value>) -> Self {
        let status = status_of(code);
        let hide_details = matches!(status, StatusCode::FORBIDDEN | StatusCode::NOT_FOUND);
        Self::new(
            status,
            code,
            message_of(code).to_string(),
            correlation_id,
            retryable_of(code),
            if hide_details { None } else { data },
        )
    }

    /// 将服务错误映射为审批 HTTP 错误。
    ///
    /// # 参数
    /// * `error` - 服务层错误
    /// * `headers` - 用于提取关联 ID 的请求头
    ///
    /// # 返回
    /// 返回稳定码、状态与关联 ID。
    pub fn from_service(error: services::Error, headers: &HeaderMap) -> Self {
        let correlation_id = correlation_id(headers);
        if let Some(code) = extract_approval_code(&error.to_string()) {
            return Self::coded(code, correlation_id, None);
        }
        if let Some(code) = map_known_message(&error.to_string()) {
            return Self::coded(code, correlation_id, None);
        }
        Self::from_http(HttpError::from(error), correlation_id)
    }

    /// 将通用 HTTP 错误包装为审批边界错误。
    ///
    /// # 参数
    /// * `error` - 已分类的 HTTP 错误
    /// * `correlation_id` - 请求关联 ID
    ///
    /// # 返回
    /// 返回保留原状态语义的审批错误。
    pub fn from_http(error: HttpError, correlation_id: String) -> Self {
        let status = error.http_status();
        let code = error.error_code();
        let message = error.user_message();
        let retryable = error.retryable();
        Self::new(status, code, message, correlation_id, retryable, None)
    }

    /// 构造 422 协议校验错误。
    ///
    /// # 参数
    /// * `message` - 面向调用方的校验说明
    /// * `headers` - 请求头
    ///
    /// # 返回
    /// 返回不泄露内部结构的 422。
    pub fn unprocessable(message: impl Into<String>, headers: &HeaderMap) -> Self {
        Self::from_http(HttpError::Unprocessable(message.into()), correlation_id(headers))
    }

    /// 构造 400 请求错误。
    ///
    /// # 参数
    /// * `message` - 面向调用方的参数说明
    /// * `headers` - 请求头
    ///
    /// # 返回
    /// 返回 400。
    pub fn bad_request(message: impl Into<String>, headers: &HeaderMap) -> Self {
        Self::from_http(HttpError::BadRequest(message.into()), correlation_id(headers))
    }

    /// 将 `DecisionOutcome::Blocked` 映射为 409。
    ///
    /// # 参数
    /// * `correlation_id` - 请求关联 ID
    /// * `data` - 调用者有权查看的最新摘要
    ///
    /// # 返回
    /// 返回 `APPROVAL_INSTANCE_BLOCKED`。
    pub fn blocked(correlation_id: String, data: Option<Value>) -> Self {
        Self::coded("APPROVAL_INSTANCE_BLOCKED", correlation_id, data)
    }

    /// 返回稳定错误码。
    ///
    /// # 返回
    /// 返回合同码或通用 HTTP 码。
    pub fn code(&self) -> &'static str {
        self.0.code
    }

    /// 返回 HTTP 状态。
    ///
    /// # 返回
    /// 返回状态码。
    pub fn status(&self) -> StatusCode {
        self.0.status
    }
}

impl From<services::Error> for ApprovalHttpError {
    /// 在缺少请求头时用新关联 ID 映射服务错误。
    ///
    /// # 参数
    /// * `error` - 服务层错误
    ///
    /// # 返回
    /// 返回审批 HTTP 错误。
    fn from(error: services::Error) -> Self {
        Self::from_service(error, &HeaderMap::new())
    }
}

impl From<HttpError> for ApprovalHttpError {
    /// 包装通用 HTTP 错误。
    ///
    /// # 参数
    /// * `error` - 通用 HTTP 错误
    ///
    /// # 返回
    /// 返回审批 HTTP 错误。
    fn from(error: HttpError) -> Self {
        Self::from_http(error, Uuid::new_v4().to_string())
    }
}

impl IntoResponse for ApprovalHttpError {
    /// 输出稳定 JSON 信封；409 必须带稳定码与关联 ID。
    ///
    /// # 返回
    /// 返回 HTTP 响应。
    fn into_response(self) -> Response {
        let inner = *self.0;
        let data = conflict_data(inner.status, &inner.correlation_id, inner.data);
        ApiResponse {
            status: inner.status.as_u16(),
            message: inner.message,
            code: Some(inner.code.to_string()),
            field_errors: None,
            retryable: Some(inner.retryable),
            data,
            success: false,
        }
        .into_response()
    }
}

/// 从请求头提取或生成关联 ID。
///
/// # 参数
/// * `headers` - HTTP 请求头
///
/// # 返回
/// 返回 `X-Trace-Id` 或新 UUID。
pub fn correlation_id(headers: &HeaderMap) -> String {
    headers
        .get(TRACE_ID_HEADER)
        .and_then(|value| value.to_str().ok())
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
        .unwrap_or_else(|| Uuid::new_v4().to_string())
}

/// 解析十进制版本字符串。
///
/// # 参数
/// * `value` - 客户端提交的版本
/// * `_label` - 字段中文名（仅保留调用方语义，不返回给用户）
/// * `headers` - 请求头
///
/// # 错误
/// 非正整数字符串时返回 400。
pub fn parse_version(value: &str, _label: &str, headers: &HeaderMap) -> Result<u64, ApprovalHttpError> {
    let version = value
        .parse::<u64>()
        .map_err(|_| ApprovalHttpError::bad_request("页面数据已失效，请刷新后重试", headers))?;
    if version == 0 {
        return Err(ApprovalHttpError::bad_request(
            "页面数据已失效，请刷新后重试",
            headers,
        ));
    }
    Ok(version)
}

/// 解析可选版本。
///
/// # 参数
/// * `value` - 可空版本
/// * `label` - 字段中文名
/// * `headers` - 请求头
///
/// # 错误
/// 出现非法字符串时返回 400。
pub fn parse_optional_version(
    value: Option<&str>,
    label: &str,
    headers: &HeaderMap,
) -> Result<Option<u64>, ApprovalHttpError> {
    value.map(|item| parse_version(item, label, headers)).transpose()
}

fn conflict_data(status: StatusCode, correlation_id: &str, data: Option<Value>) -> Option<Value> {
    if status != StatusCode::CONFLICT {
        return data;
    }
    let mut object = match data {
        Some(Value::Object(map)) => map,
        Some(other) => {
            let mut map = serde_json::Map::new();
            map.insert("payload".to_string(), other);
            map
        }
        None => serde_json::Map::new(),
    };
    object.insert(
        "correlation_id".to_string(),
        Value::String(correlation_id.to_string()),
    );
    Some(Value::Object(object))
}

fn status_of(code: &str) -> StatusCode {
    match code {
        "APPROVAL_POLICY_NOT_REGISTERED" => StatusCode::INTERNAL_SERVER_ERROR,
        "APPROVAL_TASK_NOT_ASSIGNED_TO_ACTOR" => StatusCode::FORBIDDEN,
        "APPROVAL_DEFINITION_INVALID"
        | "APPROVAL_REJECT_REASON_REQUIRED"
        | "APPROVAL_REASSIGN_TARGET_INELIGIBLE" => StatusCode::UNPROCESSABLE_ENTITY,
        _ if code.starts_with("APPROVAL_") => StatusCode::CONFLICT,
        _ => StatusCode::INTERNAL_SERVER_ERROR,
    }
}

fn message_of(code: &str) -> &'static str {
    match code {
        "APPROVAL_POLICY_NOT_REGISTERED" => "系统暂时无法完成操作，请稍后重试；如仍失败，请联系支持人员",
        "APPROVAL_PROCESS_NOT_CONFIGURED" => "该单据类型尚未配置可用的审批流程，请联系管理员发布流程后重试",
        "APPROVAL_DRAFT_SOURCE_NOT_AVAILABLE" => "当前没有可复制的已发布审批流程，请先发布流程后再创建草稿",
        "APPROVAL_DEFINITION_NOT_DRAFT" => "只能修改草稿流程，请复制为新草稿后再修改",
        "APPROVAL_DEFINITION_VERSION_CONFLICT" => "审批流程已被他人更新，请刷新后重试",
        "APPROVAL_DEFINITION_INVALID" => "审批流程内容不符合要求，请修改后重试",
        "APPROVAL_DEFINITION_BINDING_CORRUPTED" => "单据审批关系异常，请联系支持人员处理",
        "APPROVAL_ALREADY_STARTED" => "该版本已有未完成的审批，请先查看当前审批进度",
        "APPROVAL_TASK_NOT_OPEN" => "审批任务已完成或关闭，请刷新后查看当前状态",
        "APPROVAL_TASK_NOT_ASSIGNED_TO_ACTOR" => "当前账号没有执行此操作的权限，请联系管理员或有权限的同事",
        "APPROVAL_TASK_VERSION_CONFLICT" => "审批任务状态已变化，请刷新后重试",
        "APPROVAL_INSTANCE_VERSION_CONFLICT" => "审批进度已变化，请刷新后重试",
        "APPROVAL_EXECUTION_VERSION_CONFLICT" => "当前审批步骤已变化，请刷新后重试",
        "APPROVAL_SUBJECT_VERSION_CONFLICT" => "单据内容已更新，请刷新后重试",
        "APPROVAL_REJECT_REASON_REQUIRED" => "请填写驳回原因后再提交",
        "APPROVAL_INSTANCE_BLOCKED" => "当前审批已暂停，请先处理暂停原因",
        "APPROVAL_RESUME_NOT_ALLOWED_FOR_BLOCKER" => "当前暂停原因不允许恢复原审批人，请改用其他可用处理方式",
        "APPROVAL_CURRENT_APPROVER_NOT_RECOVERED" => "原审批人仍不具备审批资格，请先恢复资格或改派",
        "APPROVAL_CURRENT_APPROVER_RECOVERED" => "原审批人已恢复资格，无需改派，请刷新后继续审批",
        "APPROVAL_REASSIGN_TARGET_INELIGIBLE" => "改派目标不具备审批资格，请重新选择审批人",
        "APPROVAL_REASSIGN_NOT_ALLOWED_FOR_BLOCKER" => "当前暂停原因不允许改派，请改用恢复或取消",
        "APPROVAL_BLOCKED_CANCEL_NOT_ALLOWED" => "当前暂停原因不允许取消审批，请改用恢复或改派",
        "APPROVAL_GENERIC_WORK_ITEM_MUTATION_FORBIDDEN" => "请在审批任务页面处理该任务",
        "APPROVAL_IDEMPOTENCY_PAYLOAD_CONFLICT" => "该任务号已用于其他请求，请关闭弹窗后重新发起操作",
        _ => "系统暂时无法完成操作，请稍后重试；如仍失败，请联系支持人员",
    }
}

fn retryable_of(code: &str) -> bool {
    matches!(
        code,
        "APPROVAL_DEFINITION_VERSION_CONFLICT"
            | "APPROVAL_TASK_VERSION_CONFLICT"
            | "APPROVAL_INSTANCE_VERSION_CONFLICT"
            | "APPROVAL_EXECUTION_VERSION_CONFLICT"
            | "APPROVAL_SUBJECT_VERSION_CONFLICT"
    )
}

const STABLE_CODES: &[&str] = &[
    "APPROVAL_POLICY_NOT_REGISTERED",
    "APPROVAL_PROCESS_NOT_CONFIGURED",
    "APPROVAL_DRAFT_SOURCE_NOT_AVAILABLE",
    "APPROVAL_DEFINITION_NOT_DRAFT",
    "APPROVAL_DEFINITION_VERSION_CONFLICT",
    "APPROVAL_DEFINITION_INVALID",
    "APPROVAL_DEFINITION_BINDING_CORRUPTED",
    "APPROVAL_ALREADY_STARTED",
    "APPROVAL_TASK_NOT_OPEN",
    "APPROVAL_TASK_NOT_ASSIGNED_TO_ACTOR",
    "APPROVAL_TASK_VERSION_CONFLICT",
    "APPROVAL_INSTANCE_VERSION_CONFLICT",
    "APPROVAL_EXECUTION_VERSION_CONFLICT",
    "APPROVAL_SUBJECT_VERSION_CONFLICT",
    "APPROVAL_REJECT_REASON_REQUIRED",
    "APPROVAL_INSTANCE_BLOCKED",
    "APPROVAL_RESUME_NOT_ALLOWED_FOR_BLOCKER",
    "APPROVAL_CURRENT_APPROVER_NOT_RECOVERED",
    "APPROVAL_CURRENT_APPROVER_RECOVERED",
    "APPROVAL_REASSIGN_TARGET_INELIGIBLE",
    "APPROVAL_REASSIGN_NOT_ALLOWED_FOR_BLOCKER",
    "APPROVAL_BLOCKED_CANCEL_NOT_ALLOWED",
    "APPROVAL_GENERIC_WORK_ITEM_MUTATION_FORBIDDEN",
    "APPROVAL_IDEMPOTENCY_PAYLOAD_CONFLICT",
];

fn extract_approval_code(message: &str) -> Option<&'static str> {
    STABLE_CODES.iter().copied().find(|code| message.contains(code))
}

fn map_known_message(message: &str) -> Option<&'static str> {
    if message.contains("当前没有可复制的已发布定义") {
        return Some("APPROVAL_DRAFT_SOURCE_NOT_AVAILABLE");
    }
    if message.contains("已发布或已退役的定义结构不可改") {
        return Some("APPROVAL_DEFINITION_NOT_DRAFT");
    }
    if message.contains("定义锁版本已过期") {
        return Some("APPROVAL_DEFINITION_VERSION_CONFLICT");
    }
    if message.contains("审批政策") && message.contains("不变量") {
        return Some("APPROVAL_POLICY_NOT_REGISTERED");
    }
    if message.contains("指定审批人")
        || message.contains("审批节点数量必须")
        || message.contains("节点顺序必须")
        || message.contains("不得包含节点用途")
    {
        return Some("APPROVAL_DEFINITION_INVALID");
    }
    None
}

#[cfg(test)]
mod tests {
    use axum::{body::to_bytes, http::HeaderValue, response::IntoResponse};
    use serde_json::Value;

    use super::{
        extract_approval_code, map_known_message, status_of, ApprovalHttpError, STABLE_CODES, TRACE_ID_HEADER,
    };

    #[test]
    fn policy_not_registered_is_internal_error() {
        assert_eq!(
            status_of("APPROVAL_POLICY_NOT_REGISTERED"),
            axum::http::StatusCode::INTERNAL_SERVER_ERROR
        );
        assert!(!STABLE_CODES.is_empty());
        assert_eq!(STABLE_CODES, services::approval_codes::ALL);
    }

    #[test]
    fn extracts_embedded_stable_code() {
        assert_eq!(
            extract_approval_code("数据冲突: APPROVAL_IDEMPOTENCY_PAYLOAD_CONFLICT"),
            Some("APPROVAL_IDEMPOTENCY_PAYLOAD_CONFLICT")
        );
    }

    #[test]
    fn maps_definition_lock_and_draft_messages() {
        assert_eq!(
            map_known_message("定义锁版本已过期，未写入任何节点"),
            Some("APPROVAL_DEFINITION_VERSION_CONFLICT")
        );
        assert_eq!(
            map_known_message("当前没有可复制的已发布定义"),
            Some("APPROVAL_DRAFT_SOURCE_NOT_AVAILABLE")
        );
    }

    #[tokio::test]
    async fn conflict_response_includes_stable_code_and_correlation_id() {
        let mut headers = axum::http::HeaderMap::new();
        headers.insert(TRACE_ID_HEADER, HeaderValue::from_static("corr-1"));
        let response = ApprovalHttpError::from_service(
            services::Error::ConflictError("APPROVAL_INSTANCE_BLOCKED".to_string()),
            &headers,
        )
        .into_response();
        assert_eq!(response.status(), axum::http::StatusCode::CONFLICT);
        let body = to_bytes(response.into_body(), usize::MAX).await.expect("body");
        let body: Value = serde_json::from_slice(&body).expect("json");
        assert_eq!(body["code"], "APPROVAL_INSTANCE_BLOCKED");
        assert_eq!(body["data"]["correlation_id"], "corr-1");
        assert_eq!(body["success"], false);
    }

    #[tokio::test]
    async fn forbidden_code_does_not_leak_versions() {
        let response = ApprovalHttpError::coded(
            "APPROVAL_TASK_NOT_ASSIGNED_TO_ACTOR",
            "corr-2".to_string(),
            Some(serde_json::json!({ "latest_instance_version": "9" })),
        )
        .into_response();
        assert_eq!(response.status(), axum::http::StatusCode::FORBIDDEN);
        let body = to_bytes(response.into_body(), usize::MAX).await.expect("body");
        let body: Value = serde_json::from_slice(&body).expect("json");
        assert_eq!(body["code"], "APPROVAL_TASK_NOT_ASSIGNED_TO_ACTOR");
        assert!(body["data"].is_null());
    }

    #[tokio::test]
    async fn policy_not_registered_response_is_500() {
        let response = ApprovalHttpError::from(services::Error::Internal(
            "APPROVAL_POLICY_NOT_REGISTERED".to_string(),
        ))
        .into_response();
        assert_eq!(response.status(), axum::http::StatusCode::INTERNAL_SERVER_ERROR);
        let body = to_bytes(response.into_body(), usize::MAX).await.expect("body");
        let body: Value = serde_json::from_slice(&body).expect("json");
        assert_eq!(body["code"], "APPROVAL_POLICY_NOT_REGISTERED");
        assert_eq!(
            body["errorMessage"],
            "系统暂时无法完成操作，请稍后重试；如仍失败，请联系支持人员"
        );
    }
}
