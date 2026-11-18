//! 关联 HTTP 响应、原始错误正文与合同识别任务日志。
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::Instant;

use rig_agent::agent::{AgentHook, HookContext, OutcomeAction, OutcomeEvent};
use rig_agent::completion::{PromptError, StructuredOutputError};
use rig_core::ProviderError;
use rig_core::completion::CompletionResponse;
use rig_core::http_client::{HeaderMap, StatusCode};
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

impl ResponseMetadata {
    fn record_provider_error(
        &mut self,
        sdk_error: String,
        status: Option<StatusCode>,
        headers: Option<&HeaderMap>,
        body: Option<&str>,
    ) {
        self.sdk_error = Some(sdk_error);
        if let Some(status) = status {
            self.status = Some(status.as_u16());
        }
        if let Some(headers) = headers {
            if self.headers.is_empty() {
                self.headers = response_headers(headers);
            }
            if self.request_id.is_none() {
                self.request_id = request_id(headers);
            }
        }

        if self.rejection.body_state.is_none()
            && let Some(body) = body
        {
            self.rejection = Rejection::parse(body.to_owned());
        }
    }
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
    fn metadata(&self) -> MutexGuard<'_, ResponseMetadata> {
        self.0.lock().unwrap_or_else(|error| error.into_inner())
    }

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
        self.metadata().transport_failure = Some(failure);
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
        self.metadata().transport_failure
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

        let mut metadata = self.metadata();
        metadata.record_provider_error(
            format!("{error:?}"),
            error.provider_response_status(),
            error.provider_response_headers(),
            error.provider_response_body(),
        );

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
        self.metadata().response_body = Some(String::from_utf8_lossy(body).into_owned());
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
        let mut metadata = self.metadata();
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
        self.metadata().validation_error = Some(error.to_string());
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
        let mut metadata = self.metadata();
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
        self.metadata().status
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
        self.metadata().rejection = rejection;
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
        self.metadata().record_provider_error(
            format!("{error:?}"),
            error.provider_response_status(),
            error.provider_response_headers(),
            error.provider_response_body(),
        );
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
        let metadata = self.metadata();
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
    use std::thread;

    use rig_core::http_client::{Error as HttpError, HeaderValue};

    use super::super::invalid_output;
    use super::super::tests::{capture, event};
    use super::super::transport::TimeoutSource;
    use super::*;

    #[tokio::test]
    async fn metadata_recovery_preserves_reads_writes_and_finish_logs() {
        for poisoned in [false, true] {
            let diagnostics = Diagnostics::default();
            assert_eq!(diagnostics.status(), None);
            assert!(diagnostics.failure().is_none());
            let mut headers = HeaderMap::new();
            headers.insert("x-request-id", HeaderValue::from_static("req-recovery"));
            diagnostics.response(429, &headers);

            if poisoned {
                let shared = diagnostics.clone();
                assert!(
                    thread::spawn(move || {
                        let _guard = shared.0.lock().unwrap();
                        panic!("poison diagnostic metadata");
                    })
                    .join()
                    .is_err()
                );
            }

            assert_eq!(diagnostics.status(), Some(429));
            diagnostics.transport_failure(Failure::Timeout(TimeoutSource::ResponseBody));
            assert!(matches!(diagnostics.failure(), Some(Failure::Timeout(TimeoutSource::ResponseBody))));
            diagnostics.body(b"original response\xff");
            let body = r#"{"error":{"code":"vendor-limit","message":"try later"}}"#;
            diagnostics.rejection(Rejection::parse(body.into()));
            diagnostics.output_error(&DecodeError::Shape("original output failure"));
            let error = ProviderError::from(HttpError::instance(Failure::ResponseSize));
            diagnostics.error(&error);
            diagnostics
                .extraction_error(&StructuredOutputError::PromptError(PromptError::Report(error.report())));

            let (_, logs) = capture(async {
                let result: Result<()> = Err(invalid_output());
                diagnostics.finish(Instant::now(), &result, None);
            })
            .await;
            let fields = &event(&logs, "contract_ai_finished")["fields"];
            assert_eq!(fields["error_code"], "AI_INVALID_OUTPUT");
            assert_eq!(fields["http_status"], 429);
            assert_eq!(fields["provider_request_id"], "req-recovery");
            assert_eq!(fields["provider_error_body_state"], "received");
            assert_eq!(fields["provider_error_body"], body);
            assert_eq!(fields["provider_error_code"], "vendor-limit");
            assert_eq!(fields["provider_error_message"], "try later");
            assert_eq!(fields["provider_response_body"], "original response\u{fffd}");
            assert_eq!(fields["output_validation_error"], "response_shape: original output failure");

            let (_, logs) = capture(async {
                diagnostics.finish(Instant::now(), &Ok(()), None);
            })
            .await;
            let finished = event(&logs, "contract_ai_finished");
            assert_eq!(finished["level"], "INFO");
            assert_eq!(finished["fields"]["outcome"], "succeeded");
            assert_eq!(finished["fields"]["http_status"], 429);
        }
    }

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
