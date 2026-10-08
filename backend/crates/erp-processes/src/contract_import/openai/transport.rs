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

pub(super) const MAX_RESPONSE: usize = 1_048_576;

#[derive(Debug, thiserror::Error)]
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
                http_client::Error::instance(transport_error(error, TimeoutSource::ResponseHeaders))
            })?;
            let status = response.status();
            diagnostics.response(status.as_u16(), response.headers());
            if !status.is_success() {
                // 不读取/记录错误正文，也不向 Rig 传供应商响应头。
                return Err(http_client::Error::non_success_with_details(
                    status,
                    HeaderMap::new(),
                    String::new(),
                ));
            }
            let body: LazyBody<U> = Box::pin(async move {
                read_body(response).await.map(U::from).map_err(http_client::Error::instance)
            });
            Response::builder()
                .status(status)
                .header("content-type", "application/json")
                .body(body)
                .map_err(Into::into)
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
    use super::*;
    #[test]
    fn rejects_chunked_overflow_before_appending() {
        let mut bytes = vec![0; MAX_RESPONSE - 1];
        append(&mut bytes, &[1]).unwrap();
        assert!(append(&mut bytes, &[2]).is_err());
        assert_eq!(bytes.len(), MAX_RESPONSE);
        assert_eq!(bytes[MAX_RESPONSE - 1], 1);
    }
}
