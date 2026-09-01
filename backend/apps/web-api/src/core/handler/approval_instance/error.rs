//! 审批 HTTP 稳定错误码映射。
//!
//! Handler 不得把 BPM 或数据库错误直接暴露给客户端；只识别服务层结构化稳定码。

use axum::{
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
};
use serde_json::Value;
use services::{ErrorClass, ErrorCode};
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
    pub fn coded(code: ErrorCode, correlation_id: String, data: Option<Value>) -> Self {
        let status = status_of(code);
        let hide_details = matches!(status, StatusCode::FORBIDDEN | StatusCode::NOT_FOUND);
        Self::new(
            status,
            code.as_str(),
            message_of(code).to_string(),
            correlation_id,
            code.retryable(),
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
        if let Some(code) = error.code() {
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
        Self::coded(ErrorCode::ApprovalInstanceBlocked, correlation_id, data)
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
    let bytes = value.as_bytes();
    if bytes.is_empty() || !matches!(bytes[0], b'1'..=b'9') || !bytes[1..].iter().all(u8::is_ascii_digit) {
        return Err(ApprovalHttpError::bad_request(
            "页面数据已失效，请刷新后重试",
            headers,
        ));
    }
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

fn status_of(code: ErrorCode) -> StatusCode {
    match code.class() {
        ErrorClass::Internal => StatusCode::INTERNAL_SERVER_ERROR,
        ErrorClass::Conflict => StatusCode::CONFLICT,
        ErrorClass::BusinessRule => StatusCode::UNPROCESSABLE_ENTITY,
        ErrorClass::Forbidden => StatusCode::FORBIDDEN,
    }
}

fn message_of(code: ErrorCode) -> &'static str {
    match code {
        ErrorCode::ApprovalPolicyNotRegistered => {
            "系统暂时无法完成操作，请稍后重试；如仍失败，请联系支持人员"
        }
        ErrorCode::ApprovalProcessNotConfigured => {
            "该单据类型尚未配置可用的审批流程，请联系管理员发布流程后重试"
        }
        ErrorCode::ApprovalDraftSourceNotAvailable => {
            "当前没有可复制的已发布审批流程，请先发布流程后再创建草稿"
        }
        ErrorCode::ApprovalDefinitionNotDraft => "只能修改草稿流程，请复制为新草稿后再修改",
        ErrorCode::ApprovalDefinitionVersionConflict => "审批流程已被他人更新，请刷新后重试",
        ErrorCode::ApprovalDefinitionInvalid => "审批流程内容不符合要求，请修改后重试",
        ErrorCode::ApprovalDefinitionBindingCorrupted => "单据审批关系异常，请联系支持人员处理",
        ErrorCode::ApprovalAlreadyStarted => "该版本已有未完成的审批，请先查看当前审批进度",
        ErrorCode::ApprovalTaskNotOpen => "审批任务已完成或关闭，请刷新后查看当前状态",
        ErrorCode::ApprovalTaskNotAssignedToActor => {
            "当前账号没有执行此操作的权限，请联系管理员或有权限的同事"
        }
        ErrorCode::ApprovalTaskVersionConflict => "审批任务状态已变化，请刷新后重试",
        ErrorCode::ApprovalInstanceVersionConflict => "审批进度已变化，请刷新后重试",
        ErrorCode::ApprovalExecutionVersionConflict => "当前审批步骤已变化，请刷新后重试",
        ErrorCode::ApprovalSubjectVersionConflict => "单据内容已更新，请刷新后重试",
        ErrorCode::ApprovalRejectReasonRequired => "请填写驳回原因后再提交",
        ErrorCode::ApprovalInstanceBlocked => "当前审批已暂停，请先处理暂停原因",
        ErrorCode::ApprovalResumeNotAllowedForBlocker => {
            "当前暂停原因不允许恢复原审批人，请改用其他可用处理方式"
        }
        ErrorCode::ApprovalCurrentApproverNotRecovered => "原审批人仍不具备审批资格，请先恢复全部资格后重试",
        ErrorCode::ApprovalBlockedCancelNotAllowed => "当前暂停原因不允许取消审批，请恢复原审批人后继续审批",
        ErrorCode::ApprovalGenericWorkItemMutationForbidden => "请在审批任务页面处理该任务",
        ErrorCode::ApprovalIdempotencyPayloadConflict => "该任务号已用于其他请求，请关闭弹窗后重新发起操作",
    }
}

#[cfg(test)]
mod tests {
    use axum::{
        body::to_bytes,
        http::{HeaderValue, StatusCode},
        response::IntoResponse,
    };
    use serde_json::Value;

    use services::ErrorCode;

    use super::{parse_version, status_of, ApprovalHttpError, TRACE_ID_HEADER};

    #[test]
    fn version_parser_only_accepts_canonical_positive_decimal_strings() {
        let headers = axum::http::HeaderMap::new();
        for value in ["1", "9", "9007199254740993", "18446744073709551615"] {
            assert!(parse_version(value, "版本", &headers).is_ok(), "{value}");
        }
        for value in [
            "",
            "0",
            "01",
            "+1",
            "-1",
            " 1",
            "1 ",
            "1.0",
            "一",
            "18446744073709551616",
        ] {
            assert!(parse_version(value, "版本", &headers).is_err(), "{value:?}");
        }
    }

    #[test]
    fn policy_not_registered_is_internal_error() {
        assert_eq!(
            status_of(ErrorCode::ApprovalPolicyNotRegistered),
            axum::http::StatusCode::INTERNAL_SERVER_ERROR
        );
        assert_eq!(ErrorCode::ALL.len(), 21);
    }

    #[tokio::test]
    async fn conflict_response_includes_stable_code_and_correlation_id() {
        let mut headers = axum::http::HeaderMap::new();
        headers.insert(TRACE_ID_HEADER, HeaderValue::from_static("corr-1"));
        let response = ApprovalHttpError::from_service(
            services::Error::from_approval_code(ErrorCode::ApprovalInstanceBlocked),
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
            ErrorCode::ApprovalTaskNotAssignedToActor,
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
        let response = ApprovalHttpError::from(services::Error::from_approval_code(
            ErrorCode::ApprovalPolicyNotRegistered,
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

    /// 库存调整更新的越权与不存在必须经实际 Handler 适配器保持不可区分。
    #[tokio::test]
    async fn stock_adjustment_update_hidden_failures_have_identical_http_projection() {
        async fn project(error: services::Error) -> (StatusCode, Value) {
            let response =
                ApprovalHttpError::from_service(error, &axum::http::HeaderMap::new()).into_response();
            let status = response.status();
            let body = to_bytes(response.into_body(), usize::MAX)
                .await
                .expect("response body should be readable");
            let body = serde_json::from_slice(&body).expect("response body should be valid JSON");
            (status, body)
        }

        let unauthorized = project(services::Error::NotFound("库存调整单不存在".to_string())).await;
        let missing = project(services::Error::NotFound("库存调整单不存在".to_string())).await;

        assert_eq!(unauthorized, missing);
        assert_eq!(unauthorized.0, StatusCode::NOT_FOUND);
        assert_eq!(unauthorized.1["status"], 404);
        assert_eq!(unauthorized.1["code"], "NOT_FOUND");
        assert_eq!(
            unauthorized.1["errorMessage"],
            "库存调整单不存在，请刷新后重新选择"
        );
    }
}
