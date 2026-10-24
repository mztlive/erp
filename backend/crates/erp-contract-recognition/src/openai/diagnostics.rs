//! 关联 HTTP 响应、原始错误正文与合同识别任务日志。
use std::sync::{Arc, Mutex};
use std::time::Instant;

use rig_agent::agent::{AgentHook, HookContext, OutcomeAction, OutcomeEvent};
use rig_agent::completion::{PromptError, StructuredOutputError};
use rig_core::ProviderError;
use rig_core::completion::CompletionResponse;
use rig_core::http_client::HeaderMap;
use serde_json::Value as JsonValue;

use super::Result;
use super::output::{DecodeError, validate_response};
use super::rejection::Rejection;
use super::transport::Failure;

#[derive(Clone, Default)]
pub(super) struct Diagnostics(Arc<Mutex<ResponseMetadata>>);

#[derive(Default)]
struct ResponseMetadata {
    status: Option<u16>,
    transport_failure: Option<Failure>,
    request_id: Option<String>,
    headers: Vec<(String, String)>,
    sdk_error: Option<String>,
    response_body: Option<String>,
    decoded_response: Option<JsonValue>,
    finish_reason: Option<String>,
    validation_error: Option<String>,
    rejection: Rejection,
}

impl AgentHook for Diagnostics {
    async fn on_outcome(&self, _: &HookContext, event: OutcomeEvent<'_>) -> OutcomeAction {
        if let Some(response) = event.completion() {
            self.output(response);
            if let Err(error) = validate_response(response) {
                self.output_error(&error);
                return OutcomeAction::stop(error.to_string());
            }
        }
        OutcomeAction::proceed()
    }
}

impl Diagnostics {
    /// 记录传输失败。锁中毒时仍写回内部记录。
    ///
    /// # 参数
    /// * `failure` - 传输层失败分类。
    ///
    /// # 返回
    /// 无返回值。
    ///
    /// # 错误
    /// 不返回错误。
    pub(super) fn transport_failure(&self, failure: Failure) {
        self.0.lock().unwrap_or_else(|error| error.into_inner()).transport_failure = Some(failure);
    }

    /// 读取已记录的传输失败。锁中毒时仍返回内部记录。
    ///
    /// # 参数
    /// 无。
    ///
    /// # 返回
    /// 已记录的 `Failure`；尚未记录时为 `None`。
    ///
    /// # 错误
    /// 不返回错误。
    pub(super) fn failure(&self) -> Option<Failure> {
        self.0.lock().unwrap_or_else(|error| error.into_inner()).transport_failure
    }

    /// 记录提取失败。完成错误转入 `error`；已有校验原因时不覆盖。
    ///
    /// # 参数
    /// * `error` - Rig 结构化输出错误。
    ///
    /// # 返回
    /// 无返回值。
    ///
    /// # 错误
    /// 不返回错误。
    pub(super) fn extraction_error(&self, error: &StructuredOutputError) {
        if let StructuredOutputError::PromptError(PromptError::CompletionError(error)) = error {
            self.error(error);
            return;
        }
        let mut metadata = self.0.lock().unwrap_or_else(|error| error.into_inner());
        metadata.sdk_error = Some(format!("{error:?}"));
        if let Some(status) = error.provider_response_status() {
            metadata.status = Some(status.as_u16());
        }
        if let Some(headers) = error.provider_response_headers() {
            if metadata.headers.is_empty() {
                metadata.headers = response_headers(headers);
            }
            if metadata.request_id.is_none() {
                metadata.request_id = request_id(headers);
            }
        }
        if metadata.rejection.body_state.is_none()
            && let Some(body) = error.provider_response_body()
        {
            metadata.rejection = Rejection::parse(body.to_owned());
        }
        // hook 已记录完成状态等具体拒绝原因时，不用取消错误覆盖。
        if metadata.validation_error.is_none() {
            metadata.validation_error = Some(error.to_string());
        }
    }

    /// 按损失编码保存原始响应正文。锁中毒时仍写回内部记录。
    ///
    /// # 参数
    /// * `body` - 已读完的响应字节。
    ///
    /// # 返回
    /// 无返回值。
    ///
    /// # 错误
    /// 不返回错误。
    pub(super) fn body(&self, body: &[u8]) {
        self.0.lock().unwrap_or_else(|error| error.into_inner()).response_body =
            Some(String::from_utf8_lossy(body).into_owned());
    }

    /// 保存解码后的原始响应和结束原因。
    ///
    /// # 参数
    /// * `response` - Rig 完成响应。
    ///
    /// # 返回
    /// 无返回值。
    ///
    /// # 错误
    /// 不返回错误。
    pub(super) fn output(&self, response: &CompletionResponse) {
        let mut metadata = self.0.lock().unwrap_or_else(|error| error.into_inner());
        metadata.decoded_response = Some(response.raw.clone());
        metadata.finish_reason = response.finish_reason().map(|reason| format!("{reason:?}"));
    }

    /// 把输出校验失败记入 `validation_error`。
    ///
    /// # 参数
    /// * `error` - 本地输出解码错误。
    ///
    /// # 返回
    /// 无返回值。
    ///
    /// # 错误
    /// 不返回错误。
    pub(super) fn output_error(&self, error: &DecodeError) {
        self.0.lock().unwrap_or_else(|error| error.into_inner()).validation_error = Some(error.to_string());
    }

    /// 记录 HTTP 状态、请求标识和响应头。非成功状态把拒绝正文标为待读取。
    ///
    /// # 参数
    /// * `status` - HTTP 状态码。
    /// * `headers` - 原始响应头。
    ///
    /// # 返回
    /// 无返回值。
    ///
    /// # 错误
    /// 不返回错误。
    pub(super) fn response(&self, status: u16, headers: &HeaderMap) {
        let mut metadata = self.0.lock().unwrap_or_else(|error| error.into_inner());
        metadata.status = Some(status);
        metadata.request_id = request_id(headers);
        metadata.headers = response_headers(headers);
        if !(200..300).contains(&status) {
            metadata.rejection = Rejection::pending();
        }
    }

    /// 读取已记录的 HTTP 状态。锁中毒时仍返回内部记录。
    ///
    /// # 参数
    /// 无。
    ///
    /// # 返回
    /// 已记录的状态码；尚未记录时为 `None`。
    ///
    /// # 错误
    /// 不返回错误。
    pub(super) fn status(&self) -> Option<u16> {
        self.0.lock().unwrap_or_else(|error| error.into_inner()).status
    }

    /// 用已解析的供应商拒绝替换当前拒绝记录。
    ///
    /// # 参数
    /// * `rejection` - 错误正文的解析结果。
    ///
    /// # 返回
    /// 无返回值。
    ///
    /// # 错误
    /// 不返回错误。
    pub(super) fn rejection(&self, rejection: Rejection) {
        self.0.lock().unwrap_or_else(|error| error.into_inner()).rejection = rejection;
    }

    /// 记录供应商错误的调试文本、状态、响应头和正文。已有拒绝正文时不覆盖。
    ///
    /// # 参数
    /// * `error` - Rig 供应商错误。
    ///
    /// # 返回
    /// 无返回值。
    ///
    /// # 错误
    /// 不返回错误。
    pub(super) fn error(&self, error: &ProviderError) {
        let mut metadata = self.0.lock().unwrap_or_else(|error| error.into_inner());
        metadata.sdk_error = Some(format!("{error:?}"));
        if let Some(status) = error.provider_response_status() {
            metadata.status = Some(status.as_u16());
            if metadata.request_id.is_none() {
                metadata.request_id = error.provider_response_headers().and_then(request_id);
            }
        }
        if metadata.headers.is_empty()
            && let Some(headers) = error.provider_response_headers()
        {
            metadata.headers = response_headers(headers);
        }
        if metadata.rejection.body_state.is_none()
            && let Some(body) = error.provider_response_body()
        {
            metadata.rejection = Rejection::parse(body.to_owned());
        }
    }

    /// 按成功或失败写一条合同提取结束日志，不改变调用结果。
    ///
    /// # 参数
    /// * `start` - 提取开始时刻，用于计算耗时。
    /// * `result` - 调用方的提取结果；只读成功与失败码。
    /// * `timeout_source` - 超时所在阶段；没有超时时为 `None`。
    ///
    /// # 返回
    /// 无返回值。
    ///
    /// # 错误
    /// 不返回错误。
    pub(super) fn finish<T>(&self, start: Instant, result: &Result<T>, timeout_source: Option<&str>) {
        let metadata = self.0.lock().unwrap_or_else(|error| error.into_inner());
        let elapsed_ms = u64::try_from(start.elapsed().as_millis()).unwrap_or(u64::MAX);
        let response = metadata.decoded_response.as_ref().unwrap_or(&JsonValue::Null);
        match result {
            Ok(_) => tracing::info!(
                event = "contract_ai_finished",
                outcome = "succeeded",
                elapsed_ms,
                http_status = metadata.status,
                provider_request_id = metadata.request_id.as_deref(),
                "合同字段提取完成"
            ),
            Err(failure) => tracing::warn!(
                event = "contract_ai_finished", outcome = "failed", elapsed_ms,
                error_code = %failure.code, timeout_source,
                http_status = metadata.status, provider_request_id = metadata.request_id.as_deref(),
                provider_error_body_state = metadata.rejection.body_state,
                provider_error_body = metadata.rejection.body.as_deref(),
                provider_error_read_error = metadata.rejection.read_error.as_deref(),
                provider_error_type = metadata.rejection.kind.as_deref(),
                provider_error_code = metadata.rejection.code.as_deref(),
                provider_error_param = metadata.rejection.param.as_deref(),
                provider_error_message = metadata.rejection.message.as_deref(),
                provider_response_headers = ?metadata.headers, provider_sdk_error = metadata.sdk_error.as_deref(),
                provider_response_body = metadata.response_body.as_deref(),
                provider_decoded_response = metadata.decoded_response.as_ref().map(JsonValue::to_string).as_deref(),
                provider_finish_reason = metadata.finish_reason.as_deref(),
                provider_response_status = response["status"].as_str(),
                provider_incomplete_details = response.get("incomplete_details").map(JsonValue::to_string).as_deref(),
                provider_usage = response.get("usage").map(JsonValue::to_string).as_deref(),
                output_validation_error = metadata.validation_error.as_deref(),
                "合同字段提取失败"
            ),
        }
    }
}

fn request_id(headers: &HeaderMap) -> Option<String> {
    ["x-request-id", "x-dashscope-request-id", "x-acs-request-id", "x-ds-trace-id"].into_iter().find_map(
        |name| headers.get(name).map(|value| String::from_utf8_lossy(value.as_bytes()).into_owned()),
    )
}

fn response_headers(headers: &HeaderMap) -> Vec<(String, String)> {
    headers
        .iter()
        .map(|(name, value)| (name.to_string(), String::from_utf8_lossy(value.as_bytes()).into_owned()))
        .collect()
}

#[cfg(test)]
mod tests {
    use rig_core::http_client::HeaderValue;

    use super::*;

    #[test]
    fn retains_request_ids_and_all_response_headers_without_content_limits() {
        let mut headers = HeaderMap::new();
        headers.insert("authorization", HeaderValue::from_static("Bearer credential"));
        headers.insert("x-untrusted-id", HeaderValue::from_static("private-data"));
        assert_eq!(request_id(&headers), None);
        for value in ["".to_owned(), "x".repeat(4096), "vendor request id / 123".into()] {
            headers.insert("x-request-id", HeaderValue::from_str(&value).unwrap());
            assert_eq!(request_id(&headers).as_deref(), Some(value.as_str()));
        }
        headers.remove("x-request-id");
        headers.insert("x-dashscope-request-id", HeaderValue::from_static("req_123-abc.4"));
        assert_eq!(request_id(&headers).as_deref(), Some("req_123-abc.4"));
        headers.insert("x-ds-trace-id", HeaderValue::from_static("deepseek-trace-123"));
        assert_eq!(request_id(&headers).as_deref(), Some("req_123-abc.4"));
        headers.remove("x-dashscope-request-id");
        assert_eq!(request_id(&headers).as_deref(), Some("deepseek-trace-123"));
        assert!(response_headers(&headers).contains(&("x-untrusted-id".into(), "private-data".into())));
    }
}
