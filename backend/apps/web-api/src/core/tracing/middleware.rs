//! HTTP 请求 Trace ID 与耗时日志中间件。

use axum::{
    extract::Request,
    http::{HeaderMap, HeaderValue, StatusCode},
    middleware::Next,
    response::Response,
};
use std::time::Instant;
use tracing::info_span;
use uuid::Uuid;

pub(crate) const TRACE_ID_HEADER: &str = "X-Trace-Id";

/// 记录请求追踪信息并继续处理。
///
/// # 参数
/// * `request` - 请求对象
/// * `next` - 下一处理器
///
/// # 返回
/// 返回执行结果，`Ok` 表示成功，`Err` 表示失败。
///
/// # 错误
/// 当验证失败或底层操作失败时返回错误。
pub(crate) async fn trace_middleware(mut request: Request, next: Next) -> Result<Response, StatusCode> {
    let trace_id = extract_or_generate_trace_id(request.headers());

    let trace_value: HeaderValue = trace_id.parse().map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    request.headers_mut().insert(TRACE_ID_HEADER, trace_value.clone());

    let method = request.method().clone();
    let uri = request.uri().clone();
    let path = uri.path().to_string();

    let start_time = Instant::now();

    let span = info_span!(
        "http_request",
        trace_id = %trace_id,
        method = %method,
        path = %path,
        status_code = tracing::field::Empty,
        latency_ms = tracing::field::Empty,
    );

    let _enter = span.enter();

    let mut response = next.run(request).await;
    let status = response.status();
    let latency = start_time.elapsed();

    response.headers_mut().insert(TRACE_ID_HEADER, trace_value);

    span.record("status_code", status.as_u16());
    span.record("latency_ms", latency.as_millis());

    tracing::info!(
        target: "http",
        trace_id = %trace_id,
        method = %method,
        path = %path,
        status = %status.as_u16(),
        latency_ms = %latency.as_millis(),
        "Request completed"
    );

    Ok(response)
}

/// 提取或生成 Trace ID。
///
/// # 参数
/// * `headers` - 请求头
///
/// # 返回
/// 返回字符串结果。
fn extract_or_generate_trace_id(headers: &HeaderMap) -> String {
    headers
        .get(TRACE_ID_HEADER)
        .and_then(|v| v.to_str().ok())
        .map(|s| s.to_string())
        .unwrap_or_else(|| Uuid::new_v4().to_string())
}

#[cfg(test)]
mod tests {
    use axum::{
        body::Body,
        http::{HeaderMap, HeaderValue, Request},
        middleware,
        routing::get,
        Router,
    };
    use tower::ServiceExt;
    use uuid::Uuid;

    use super::{extract_or_generate_trace_id, trace_middleware, TRACE_ID_HEADER};

    #[test]
    fn trace_id_preserves_valid_request_header() {
        let mut headers = HeaderMap::new();
        headers.insert(TRACE_ID_HEADER, HeaderValue::from_static("client-trace-id"));

        assert_eq!(extract_or_generate_trace_id(&headers), "client-trace-id");
    }

    #[test]
    fn trace_id_generates_uuid_when_header_is_missing() {
        let trace_id = extract_or_generate_trace_id(&HeaderMap::new());

        assert!(Uuid::parse_str(&trace_id).is_ok());
    }

    #[test]
    fn trace_id_generates_uuid_when_header_is_not_text() {
        let mut headers = HeaderMap::new();
        let binary_value = HeaderValue::from_bytes(&[0xff]).expect("binary header should be valid");
        headers.insert(TRACE_ID_HEADER, binary_value);

        let trace_id = extract_or_generate_trace_id(&headers);

        assert!(Uuid::parse_str(&trace_id).is_ok());
    }

    #[tokio::test]
    async fn trace_id_is_returned_in_response_header() {
        let app = Router::new()
            .route("/", get(|| async { "ok" }))
            .layer(middleware::from_fn(trace_middleware));
        let request = Request::builder()
            .uri("/")
            .header(TRACE_ID_HEADER, "client-trace-id")
            .body(Body::empty())
            .expect("request should build");

        let response = app.oneshot(request).await.expect("request should complete");

        assert_eq!(
            response.headers().get(TRACE_ID_HEADER).unwrap(),
            "client-trace-id"
        );
    }
}
