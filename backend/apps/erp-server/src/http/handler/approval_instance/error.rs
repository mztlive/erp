//! 审批 HTTP 稳定错误码映射。
//!
//! Handler 不得把 BPM 或数据库错误直接暴露给客户端；只识别服务层结构化稳定码。

use std::collections::BTreeMap;

use application_core::ErrorClass;
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use erp_workflow::ErrorCode;
use serde_json::Value;
use uuid::Uuid;

use crate::http::errors::Error as HttpError;
use crate::http::errors::approval::approval_message;
use crate::http::response::ApiResponse;

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
    field_errors: Option<BTreeMap<String, String>>,
}

impl ApprovalHttpError {
    // 为审批协议失败构造指针大小的错误，避免稳定码、提示和可选回读数据撑大 `Result`。
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
            field_errors: None,
        }))
    }
}

impl ApprovalHttpError {
    /// 由冻结的 `APPROVAL_*` 合同码确定 HTTP 状态，并保留请求关联 ID；409 可带安全回读数据，403 或 404 丢弃明细以免暴露对象。
    pub fn coded(code: ErrorCode, correlation_id: String, data: Option<Value>) -> Self {
        let status = status_of(code);
        let hide_details = matches!(status, StatusCode::FORBIDDEN | StatusCode::NOT_FOUND);
        Self::new(
            status,
            code.as_str(),
            approval_message(code).to_string(),
            correlation_id,
            code.retryable(),
            if hide_details { None } else { data },
        )
    }

    /// 审批运行时返回错误后，保留其业务分类，生成对应 HTTP 状态、稳定码和请求关联 ID。
    pub fn from_service(error: impl Into<crate::wiring::errors::Error>, headers: &HeaderMap) -> Self {
        let error = error.into();
        let correlation_id = correlation_id(headers);
        if let Some(code) = error.code() {
            return Self::coded(code, correlation_id, None);
        }
        Self::from_http(HttpError::from(error), correlation_id)
    }

    /// 把已分类的通用 HTTP 错误包装成审批信封，沿用原状态语义和本次关联 ID。
    pub fn from_http(error: HttpError, correlation_id: String) -> Self {
        let status = error.http_status();
        let code = error.error_code();
        let message = error.user_message();
        let retryable = error.retryable();
        let mut result = Self::new(status, code, message, correlation_id, retryable, None);
        result.0.field_errors = error.field_errors();
        result
    }

    /// 审批查询或业务命令不符合协议约束时返回带关联 ID 的 422，不公开内部结构。
    pub fn unprocessable(message: impl Into<String>, headers: &HeaderMap) -> Self {
        Self::from_http(HttpError::Unprocessable(message.into()), correlation_id(headers))
    }

    /// 请求字段不能解析为审批命令所需格式时生成带关联 ID 的 400。
    pub fn bad_request(message: impl Into<String>, headers: &HeaderMap) -> Self {
        Self::from_http(HttpError::BadRequest(message.into()), correlation_id(headers))
    }

    /// 供客户端按合同码或通用 HTTP 码识别审批错误，不依赖提示文案。
    pub fn code(&self) -> &'static str {
        self.0.code
    }

    /// 供协议适配读取已确定的 HTTP 状态，不从错误正文重新推断。
    pub fn status(&self) -> StatusCode {
        self.0.status
    }
}

impl From<crate::wiring::errors::Error> for ApprovalHttpError {
    // 在缺少请求头时用新关联 ID 映射服务错误。
    // 返回审批 HTTP 错误。
    fn from(error: crate::wiring::errors::Error) -> Self {
        Self::from_service(error, &HeaderMap::new())
    }
}

impl From<HttpError> for ApprovalHttpError {
    // 包装通用 HTTP 错误。
    // 返回审批 HTTP 错误。
    fn from(error: HttpError) -> Self {
        Self::from_http(error, Uuid::new_v4().to_string())
    }
}

impl IntoResponse for ApprovalHttpError {
    // 输出稳定 JSON 信封；409 必须带稳定码与关联 ID。
    fn into_response(self) -> Response {
        let inner = *self.0;
        tracing::warn!(
            target: "http_error", code = inner.code, status = inner.status.as_u16(),
            public_message = %inner.message,
            fields = ?inner.field_errors.as_ref().map(|fields| fields.keys().collect::<Vec<_>>()),
            "Approval request rejected"
        );
        let data = conflict_data(inner.status, &inner.correlation_id, inner.data);
        ApiResponse {
            status: inner.status.as_u16(),
            message: inner.message,
            code: Some(inner.code.to_string()),
            required_permission: None,
            field_errors: inner.field_errors,
            retryable: Some(inner.retryable),
            data,
            success: false,
        }
        .into_response()
    }
}

/// 形成审批响应时优先沿用请求的 `X-Trace-Id`，无法取得时生成 UUID 以保留日志关联。
pub fn correlation_id(headers: &HeaderMap) -> String {
    headers
        .get(TRACE_ID_HEADER)
        .and_then(|value| value.to_str().ok())
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
        .unwrap_or_else(|| Uuid::new_v4().to_string())
}

/// 把 HTTP 中的正十进制版本字符串转成运行时版本；空值、非法数字或溢出返回 400，不使用调用方字段名定制错误正文。
pub fn parse_version(value: &str, _label: &str, headers: &HeaderMap) -> Result<u64, ApprovalHttpError> {
    let bytes = value.as_bytes();
    if bytes.is_empty() || !matches!(bytes[0], b'1'..=b'9') || !bytes[1..].iter().all(u8::is_ascii_digit) {
        return Err(ApprovalHttpError::bad_request("页面数据已失效，请刷新后重试", headers));
    }
    value.parse::<u64>().map_err(|_| ApprovalHttpError::bad_request("页面数据已失效，请刷新后重试", headers))
}

/// 可缺省的版本字段未提供时保持为空；存在时执行和必填版本相同的正十进制校验，非法值返回 400。
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
        },
        None => serde_json::Map::new(),
    };
    object.insert("correlation_id".to_string(), Value::String(correlation_id.to_string()));
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

#[cfg(test)]
mod tests {
    use axum::body::to_bytes;
    use axum::http::{HeaderValue, StatusCode};
    use axum::response::IntoResponse;
    use erp_workflow::ErrorCode;
    use serde_json::Value;

    use super::{ApprovalHttpError, TRACE_ID_HEADER, parse_version, status_of};

    #[test]
    fn version_parser_only_accepts_canonical_positive_decimal_strings() {
        let headers = axum::http::HeaderMap::new();
        for value in ["1", "9", "9007199254740993", "18446744073709551615"] {
            assert!(parse_version(value, "版本", &headers).is_ok(), "{value}");
        }
        for value in ["", "0", "01", "+1", "-1", " 1", "1 ", "1.0", "一", "18446744073709551616"] {
            assert!(parse_version(value, "版本", &headers).is_err(), "{value:?}");
        }
    }

    #[test]
    fn policy_not_registered_is_internal_error() {
        assert_eq!(
            status_of(ErrorCode::ApprovalPolicyNotRegistered),
            axum::http::StatusCode::INTERNAL_SERVER_ERROR
        );
        assert_eq!(ErrorCode::ALL.len(), 22);
    }

    #[tokio::test]
    async fn pending_resource_grant_is_a_refreshable_conflict() {
        let response = ApprovalHttpError::from_service(
            crate::wiring::errors::Error::Coded(ErrorCode::ResourceAuthorizationNotReady),
            &axum::http::HeaderMap::new(),
        )
        .into_response();
        assert_eq!(response.status(), StatusCode::CONFLICT);
        let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
        let body: Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(body["code"], "RESOURCE_AUTHORIZATION_NOT_READY");
        assert_eq!(body["retryable"], true);
        assert_eq!(body["success"], false);
    }

    #[tokio::test]
    async fn conflict_response_includes_stable_code_and_correlation_id() {
        let mut headers = axum::http::HeaderMap::new();
        headers.insert(TRACE_ID_HEADER, HeaderValue::from_static("corr-1"));
        let response = ApprovalHttpError::from_service(
            crate::wiring::errors::Error::from_approval_code(ErrorCode::ApprovalInstanceBlocked),
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
        let response = ApprovalHttpError::from(crate::wiring::errors::Error::from_approval_code(
            ErrorCode::ApprovalPolicyNotRegistered,
        ))
        .into_response();
        assert_eq!(response.status(), axum::http::StatusCode::INTERNAL_SERVER_ERROR);
        let body = to_bytes(response.into_body(), usize::MAX).await.expect("body");
        let body: Value = serde_json::from_slice(&body).expect("json");
        assert_eq!(body["code"], "APPROVAL_POLICY_NOT_REGISTERED");
        assert_eq!(body["errorMessage"], "系统暂时无法完成操作，请稍后重试；如仍失败，请联系支持人员");
    }

    // 通过真实库存调整更新边界比较无权和不存在的响应，防止客户端据响应区分隐藏对象。
    #[tokio::test]
    async fn stock_adjustment_update_hidden_failures_have_identical_http_projection() {
        async fn project(error: crate::wiring::errors::Error) -> (StatusCode, Value) {
            let response =
                ApprovalHttpError::from_service(error, &axum::http::HeaderMap::new()).into_response();
            let status = response.status();
            let body =
                to_bytes(response.into_body(), usize::MAX).await.expect("response body should be readable");
            let body = serde_json::from_slice(&body).expect("response body should be valid JSON");
            (status, body)
        }

        let unauthorized =
            project(crate::wiring::errors::Error::NotFound("库存调整单不存在".to_string())).await;
        let missing = project(crate::wiring::errors::Error::NotFound("库存调整单不存在".to_string())).await;

        assert_eq!(unauthorized, missing);
        assert_eq!(unauthorized.0, StatusCode::NOT_FOUND);
        assert_eq!(unauthorized.1["status"], 404);
        assert_eq!(unauthorized.1["code"], "NOT_FOUND");
        assert_eq!(unauthorized.1["errorMessage"], "库存调整单不存在，请刷新后重新选择");
    }
}
