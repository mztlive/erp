//! 关联 HTTP 响应、原始错误正文与合同识别任务日志。
use std::sync::{Arc, Mutex};
use std::time::Instant;

use rig_core::ProviderError;
use rig_core::http_client::HeaderMap;

use super::Result;
use super::rejection::Rejection;

#[derive(Clone, Default)]
pub(super) struct Diagnostics(Arc<Mutex<ResponseMetadata>>);

#[derive(Default)]
struct ResponseMetadata {
    status: Option<u16>,
    request_id: Option<String>,
    headers: Vec<(String, String)>,
    sdk_error: Option<String>,
    rejection: Rejection,
}

impl Diagnostics {
    pub(super) fn response(&self, status: u16, headers: &HeaderMap) {
        let mut metadata = self.0.lock().unwrap_or_else(|error| error.into_inner());
        metadata.status = Some(status);
        metadata.request_id = request_id(headers);
        metadata.headers = response_headers(headers);
        if !(200..300).contains(&status) {
            metadata.rejection = Rejection::pending();
        }
    }

    pub(super) fn status(&self) -> Option<u16> {
        self.0.lock().unwrap_or_else(|error| error.into_inner()).status
    }

    pub(super) fn rejection(&self, rejection: Rejection) {
        self.0.lock().unwrap_or_else(|error| error.into_inner()).rejection = rejection;
    }

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

    pub(super) fn finish<T>(&self, start: Instant, result: &Result<T>, timeout_source: Option<&str>) {
        let metadata = self.0.lock().unwrap_or_else(|error| error.into_inner());
        let elapsed_ms = u64::try_from(start.elapsed().as_millis()).unwrap_or(u64::MAX);
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
                "合同字段提取失败"
            ),
        }
    }
}

fn request_id(headers: &HeaderMap) -> Option<String> {
    ["x-request-id", "x-dashscope-request-id", "x-acs-request-id"].into_iter().find_map(|name| {
        headers.get(name).map(|value| String::from_utf8_lossy(value.as_bytes()).into_owned())
    })
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
        assert!(response_headers(&headers).contains(&("x-untrusted-id".into(), "private-data".into())));
    }
}
