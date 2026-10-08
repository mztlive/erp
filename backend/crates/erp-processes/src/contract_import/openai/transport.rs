//! 有界、无重试 HTTP 传输。Rig 负责 OpenAI 协议编解码。
use std::future::{Future, ready};
use std::time::Duration;

use bytes::Bytes;
use reqwest::redirect::Policy;
use reqwest::{Client, Response as HttpResponse, retry};
use rig_core::http_client::{
    self, HeaderMap, HttpClientExt, LazyBody, MultipartForm, Request, Response, StreamingResponse,
};

use super::diagnostics::Diagnostics;
use super::rejection::Rejection;

pub(super) const MAX_RESPONSE: usize = 1_048_576;

#[derive(Debug, Clone, Copy, thiserror::Error)]
pub(super) enum Failure {
    #[error("AI request timed out")]
    Timeout(TimeoutSource),
    #[error("AI transport unavailable")]
    Unavailable,
    #[error("AI response too large")]
    ResponseSize,
    #[error("AI transport operation unsupported")]
    Unsupported,
}

#[derive(Debug, Clone, Copy)]
pub(super) enum TimeoutSource {
    Connect,
    ResponseHeaders,
    ResponseBody,
}

impl TimeoutSource {
    pub(super) fn as_str(self) -> &'static str {
        match self {
            Self::Connect => "connect",
            Self::ResponseHeaders => "response_headers",
            Self::ResponseBody => "response_body",
        }
    }
}

pub(super) struct BoundedHttp {
    client: Client,
    diagnostics: Diagnostics,
}

impl BoundedHttp {
    pub(super) fn new(timeout: Duration, diagnostics: Diagnostics) -> Result<Self, Failure> {
        Client::builder()
            .connect_timeout(Duration::from_secs(5))
            .timeout(timeout)
            .redirect(Policy::none())
            .retry(retry::never())
            .build()
            .map(|client| Self { client, diagnostics })
            .map_err(|_| Failure::Unavailable)
    }
}

impl HttpClientExt for BoundedHttp {
    fn send<T, U>(
        &self,
        req: Request<T>,
    ) -> impl Future<Output = http_client::Result<Response<LazyBody<U>>>> + Send + 'static
    where
        T: Into<Bytes> + Send,
        U: From<Bytes> + Send + 'static,
    {
        let (mut parts, body) = req.into_parts();
        if let Some(header) = parts.headers.get_mut("authorization") {
            header.set_sensitive(true);
        }
        let request =
            self.client.request(parts.method, parts.uri.to_string()).headers(parts.headers).body(body.into());
        let diagnostics = self.diagnostics.clone();
        async move {
            let response = request.send().await.map_err(|error| {
                let failure = transport_error(error, TimeoutSource::ResponseHeaders);
                diagnostics.transport_failure(failure);
                http_client::Error::instance(failure)
            })?;
            receive(response, diagnostics).await.map_err(|error| *error)
        }
    }

    fn send_multipart<U>(
        &self,
        _: Request<MultipartForm>,
    ) -> impl Future<Output = http_client::Result<Response<LazyBody<U>>>> + Send + 'static
    where
        U: From<Bytes> + Send + 'static,
    {
        ready(Err(http_client::Error::instance(Failure::Unsupported)))
    }

    fn send_streaming<T>(
        &self,
        _: Request<T>,
    ) -> impl Future<Output = http_client::Result<StreamingResponse>> + Send
    where
        T: Into<Bytes> + Send,
    {
        ready(Err(http_client::Error::instance(Failure::Unsupported)))
    }
}

async fn receive<U: From<Bytes> + Send + 'static>(
    response: HttpResponse,
    diagnostics: Diagnostics,
) -> Result<Response<LazyBody<U>>, Box<http_client::Error>> {
    let status = response.status();
    diagnostics.response(status.as_u16(), response.headers());
    if !status.is_success() {
        // 完整读取错误文本供任务日志使用，失败分类仍由已收到的 HTTP 状态决定。
        let rejection = match response.text().await {
            Ok(body) => Rejection::parse(body),
            Err(error) => Rejection::read_failed(format!("{error:?}")),
        };
        diagnostics.rejection(rejection);
        return Err(Box::new(http_client::Error::non_success_with_details(
            status,
            HeaderMap::new(),
            String::new(),
        )));
    }
    let body: LazyBody<U> = Box::pin(async move {
        let body = read_body(response).await.map_err(|failure| {
            diagnostics.transport_failure(failure);
            http_client::Error::instance(failure)
        })?;
        diagnostics.body(&body);
        Ok(U::from(body))
    });
    Response::builder()
        .status(status)
        .header("content-type", "application/json")
        .body(body)
        .map_err(http_client::Error::from)
        .map_err(Box::new)
}

async fn read_body(mut response: HttpResponse) -> Result<Bytes, Failure> {
    if response.content_length().is_some_and(|length| length > 1_048_576) {
        return Err(Failure::ResponseSize);
    }
    let mut bytes = Vec::new();
    while let Some(chunk) =
        response.chunk().await.map_err(|error| transport_error(error, TimeoutSource::ResponseBody))?
    {
        append(&mut bytes, &chunk)?;
    }
    Ok(Bytes::from(bytes))
}

fn append(bytes: &mut Vec<u8>, chunk: &[u8]) -> Result<(), Failure> {
    if bytes.len().saturating_add(chunk.len()) > MAX_RESPONSE {
        return Err(Failure::ResponseSize);
    }
    bytes.extend_from_slice(chunk);
    Ok(())
}

fn transport_error(error: reqwest::Error, stage: TimeoutSource) -> Failure {
    if error.is_timeout() {
        Failure::Timeout(if error.is_connect() { TimeoutSource::Connect } else { stage })
    } else {
        Failure::Unavailable
    }
}

#[cfg(test)]
mod tests {
    use std::time::Instant;

    use rig_core::ProviderError;
    use serde_json::json;

    use super::super::tests::{capture, event};
    use super::super::{Result as ExtractResult, invalid_output, provider_error};
    use super::*;
    #[test]
    fn rejects_chunked_overflow_before_appending() {
        let mut bytes = vec![0; MAX_RESPONSE - 1];
        append(&mut bytes, &[1]).unwrap();
        assert!(append(&mut bytes, &[2]).is_err());
        assert_eq!(bytes.len(), MAX_RESPONSE);
        assert_eq!(bytes[MAX_RESPONSE - 1], 1);
    }

    #[tokio::test]
    async fn http_400_logs_complete_body_beyond_success_response_limit() {
        let message = format!("json_schema is not supported: {}", "错误详情".repeat(MAX_RESPONSE / 6));
        let body = json!({"error": {"type": "vendor.custom", "code": 400,
            "param": "response_format.type", "message": message}})
        .to_string();
        assert!(body.len() > MAX_RESPONSE);
        let response = Response::builder()
            .status(400)
            .header("x-vendor-detail", "original-detail")
            .body(body.clone())
            .unwrap();
        let diagnostics = Diagnostics::default();
        let (result, logs) = capture(async {
            let error = receive::<Bytes>(response.into(), diagnostics.clone()).await.err().unwrap();
            let error = ProviderError::from(*error);
            diagnostics.error(&error);
            let result: Result<(), _> = Err(provider_error(&error));
            diagnostics.finish(Instant::now(), &result, None);
            result
        })
        .await;
        assert_eq!(result.unwrap_err().code, "AI_REJECTED");
        let fields = &event(&logs, "contract_ai_finished")["fields"];
        assert_eq!(fields["http_status"], 400);
        assert_eq!(fields["provider_error_body_state"], "received");
        assert_eq!(fields["provider_error_body"].as_str(), Some(body.as_str()));
        assert_eq!(fields["provider_error_type"], "vendor.custom");
        assert_eq!(fields["provider_error_code"], "400");
        assert_eq!(fields["provider_error_param"], "response_format.type");
        assert_eq!(fields["provider_error_message"].as_str(), Some(message.as_str()));
        assert!(fields["provider_response_headers"].as_str().unwrap().contains("original-detail"));
    }

    #[tokio::test]
    async fn http_200_retains_original_body_when_output_is_rejected() {
        let body =
            json!({"status": "completed", "output": [], "vendor_extra": "完整正文".repeat(4096)}).to_string();
        let response = Response::builder()
            .status(200)
            .header("x-ds-trace-id", "deepseek-trace-123")
            .body(body.clone())
            .unwrap();
        let diagnostics = Diagnostics::default();
        let (_, logs) = capture(async {
            let response = receive::<Bytes>(response.into(), diagnostics.clone()).await.unwrap();
            assert_eq!(response.into_body().await.unwrap().as_ref(), body.as_bytes());
            let result: ExtractResult<()> = Err(invalid_output());
            diagnostics.finish(Instant::now(), &result, None);
        })
        .await;
        let fields = &event(&logs, "contract_ai_finished")["fields"];
        assert_eq!(fields["provider_response_body"], body);
        assert_eq!(fields["http_status"], 200);
        assert_eq!(fields["provider_request_id"], "deepseek-trace-123");
    }

    #[tokio::test]
    async fn success_response_still_uses_existing_body_limit() {
        let response = Response::builder().status(200).body("ok").unwrap();
        let response = receive::<Bytes>(response.into(), Diagnostics::default()).await.unwrap();
        assert_eq!(response.into_body().await.unwrap(), Bytes::from_static(b"ok"));
        let response = Response::builder().status(200).body(vec![b'x'; MAX_RESPONSE + 1]).unwrap();
        let response = receive::<Bytes>(response.into(), Diagnostics::default()).await.unwrap();
        assert!(response.into_body().await.is_err());
    }
}
