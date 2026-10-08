//! 只保留可公开的 HTTP 元数据；SDK 隔离区内不输出日志。
use std::sync::{Arc, Mutex};
use std::time::Instant;

use rig_core::ProviderError;
use rig_core::http_client::HeaderMap;

use super::Result;

#[derive(Clone, Default)]
pub(super) struct Diagnostics(Arc<Mutex<ResponseMetadata>>);

#[derive(Default)]
struct ResponseMetadata {
    status: Option<u16>,
    request_id: Option<String>,
}

impl Diagnostics {
    pub(super) fn response(&self, status: u16, headers: &HeaderMap) {
        let mut metadata = self.0.lock().unwrap_or_else(|error| error.into_inner());
        metadata.status = Some(status);
        metadata.request_id = request_id(headers);
    }

    pub(super) fn error(&self, error: &ProviderError) {
        if let Some(status) = error.provider_response_status() {
            let mut metadata = self.0.lock().unwrap_or_else(|error| error.into_inner());
            metadata.status = Some(status.as_u16());
            if metadata.request_id.is_none() {
                metadata.request_id = error.provider_response_headers().and_then(request_id);
            }
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
                "合同字段提取失败"
            ),
        }
    }
}

fn request_id(headers: &HeaderMap) -> Option<String> {
    ["x-request-id", "x-dashscope-request-id", "x-acs-request-id"].into_iter().find_map(|name| {
        let value = headers.get(name)?.to_str().ok()?;
        (!value.is_empty()
            && value.len() <= 128
            && value.bytes().all(|byte| byte.is_ascii_alphanumeric() || b"-_.".contains(&byte)))
        .then(|| value.to_owned())
    })
}

#[cfg(test)]
mod tests {
    use rig_core::http_client::HeaderValue;

    use super::*;

    #[test]
    fn accepts_only_bounded_request_ids_from_allowlisted_headers() {
        let mut headers = HeaderMap::new();
        headers.insert("authorization", HeaderValue::from_static("Bearer credential"));
        headers.insert("x-untrusted-id", HeaderValue::from_static("private-data"));
        assert_eq!(request_id(&headers), None);
        for invalid in ["".to_owned(), "x".repeat(129), "Bearer credential".into(), "合同正文".into()] {
            headers.insert("x-request-id", HeaderValue::from_str(&invalid).unwrap());
            assert_eq!(request_id(&headers), None);
        }
        headers.insert("x-dashscope-request-id", HeaderValue::from_static("req_123-abc.4"));
        assert_eq!(request_id(&headers).as_deref(), Some("req_123-abc.4"));
    }
}
