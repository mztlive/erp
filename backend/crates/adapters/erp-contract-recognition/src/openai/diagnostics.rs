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

    // 发送请求或读取响应正文失败时，存下原传输分类，避免 Rig 包装错误后丢失超时阶段等信息；锁中毒仍写入。
    pub(super) fn transport_failure(&self, failure: Failure) {
        self.metadata().transport_failure = Some(failure);
    }

    // 提取失败分类时取回底层传输错误，以补足 Rig 报告中丢失的类型；未记录时返回 None，锁中毒仍读取。
    pub(super) fn failure(&self) -> Option<Failure> {
        self.metadata().transport_failure
    }

    // Rig 提取失败后补记 SDK 错误及供应商响应；模型调用阶段的供应商错误沿用专门的诊断路径。
    // 若响应 hook 已给出更具体的校验原因，不用后续取消错误覆盖。
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

    // 成功响应的正文读完后保留一份有损 UTF-8 文本，供诊断 JSON 解码及输出校验失败；锁中毒仍写入。
    pub(super) fn body(&self, body: &[u8]) {
        self.metadata().response_body = Some(String::from_utf8_lossy(body).into_owned());
    }

    // 响应 hook 在校验完成结果前保存原始响应对象和结束原因，便于区分模型截断与提交结构异常。
    pub(super) fn output(&self, response: &CompletionResponse) {
        let mut metadata = self.metadata();
        metadata.decoded_response = Some(response.raw.clone());
        metadata.finish_reason = response.finish_reason().map(|reason| format!("{reason:?}"));
    }

    // 响应 hook 或领域结果转换拒绝输出时，保留具体校验原因供任务结束日志使用。
    pub(super) fn output_error(&self, error: &DecodeError) {
        self.metadata().validation_error = Some(error.to_string());
    }

    // 响应头到达即保存 HTTP 状态、请求标识和响应头，使正文未读完时也能按状态分类。
    // 非成功响应先把拒绝正文标记为待读取。
    pub(super) fn response(&self, status: u16, headers: &HeaderMap) {
        let mut metadata = self.metadata();
        metadata.status = Some(status);
        metadata.request_id = request_id(headers);
        metadata.headers = response_headers(headers);
        if !(200..300).contains(&status) {
            metadata.rejection = Rejection::pending();
        }
    }

    // 任务期限先于 HTTP 正文读取结束时，用已收到的状态识别供应商拒绝，避免误报任务超时。
    // 尚未记录时返回 None；锁中毒仍读取已有状态。
    pub(super) fn status(&self) -> Option<u16> {
        self.metadata().status
    }

    // 非成功响应的正文读取结束时，用已解析结果或读取失败记录替换待读取标记。
    pub(super) fn rejection(&self, rejection: Rejection) {
        self.metadata().rejection = rejection;
    }

    // Rig 报告供应商错误时收集调试文本、状态、响应头与可用正文，便于和 HTTP 记录对照。
    // 已有拒绝正文状态时保留传输层记录，不用 SDK 转述覆盖。
    pub(super) fn error(&self, error: &ProviderError) {
        self.metadata().record_provider_error(
            format!("{error:?}"),
            error.provider_response_status(),
            error.provider_response_headers(),
            error.provider_response_body(),
        );
    }

    // 提取结束后按结果写一条任务日志，用开始时间计算耗时；失败时附上阶段、错误码与收集到的响应证据。
    // 只记录诊断信息，不改变返回给合同任务的结果。
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
