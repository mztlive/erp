//! HTTP 请求 Trace ID 与耗时日志中间件。

use axum::{
    extract::{MatchedPath, Request},
    http::{HeaderMap, HeaderValue, Method, StatusCode},
    middleware::Next,
    response::Response,
};
use opentelemetry::{
    propagation::{Extractor, TextMapPropagator},
    trace::{Status as OtelStatus, TraceContextExt},
    Context,
};
use opentelemetry_sdk::propagation::TraceContextPropagator;
use std::time::Instant;
use tracing::{info_span, Instrument};
use tracing_opentelemetry::OpenTelemetrySpanExt;
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
    let parent_context = extract_remote_context(request.headers());
    let trace_id = extract_or_generate_trace_id(request.headers());

    let trace_value: HeaderValue = trace_id.parse().map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    request.headers_mut().insert(TRACE_ID_HEADER, trace_value.clone());

    let method = request.method().clone();
    let uri = request.uri().clone();
    let path = uri.path().to_string();
    let matched_route = request
        .extensions()
        .get::<MatchedPath>()
        .map(|matched| matched.as_str().to_string());
    let span_name = http_span_name(&method, matched_route.as_deref());

    let start_time = Instant::now();

    let span = info_span!(
        target: "http",
        "http_request",
        otel.name = %span_name,
        otel.kind = "server",
        trace_id = %trace_id,
        "http.request.method" = %method,
        "http.route" = tracing::field::Empty,
        "url.path" = %path,
        "url.scheme" = "http",
        "http.response.status_code" = tracing::field::Empty,
        method = %method,
        path = %path,
        status_code = tracing::field::Empty,
        latency_ms = tracing::field::Empty,
    );
    if let Some(route) = matched_route.as_deref() {
        span.record("http.route", route);
    }
    if parent_context.span().span_context().is_valid() {
        let _ = span.set_parent(parent_context);
    }

    let mut response = next.run(request).instrument(span.clone()).await;
    let status = response.status();
    let latency = start_time.elapsed();

    response.headers_mut().insert(TRACE_ID_HEADER, trace_value);

    span.record("status_code", status.as_u16());
    span.record("http.response.status_code", status.as_u16());
    span.record("latency_ms", latency.as_millis());
    if status.is_server_error() {
        span.set_attribute("error.type", status.as_u16().to_string());
        span.set_status(OtelStatus::error(""));
    }

    span.in_scope(|| {
        tracing::info!(
            target: "http",
            trace_id = %trace_id,
            method = %method,
            path = %path,
            status = %status.as_u16(),
            latency_ms = %latency.as_millis(),
            "Request completed"
        );
    });

    Ok(response)
}

/// 从标准 W3C `traceparent`/`tracestate` 请求头提取远程父上下文。
///
/// 无有效传播头时返回空上下文，由当前进程创建新的根 trace。
fn extract_remote_context(headers: &HeaderMap) -> Context {
    TraceContextPropagator::new().extract(&HeaderExtractor(headers))
}

/// 使用低基数路由模板构造 HTTP server span 名称。
fn http_span_name(method: &Method, matched_route: Option<&str>) -> String {
    matched_route.map_or_else(|| method.to_string(), |route| format!("{method} {route}"))
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

/// 将 Axum 请求头适配为 OpenTelemetry 文本传播载体。
struct HeaderExtractor<'a>(&'a HeaderMap);

impl Extractor for HeaderExtractor<'_> {
    /// 按不区分大小写的 HTTP 头名读取 UTF-8 文本值。
    fn get(&self, key: &str) -> Option<&str> {
        self.0.get(key).and_then(|value| value.to_str().ok())
    }

    /// 返回传播器可读取的全部请求头名称。
    fn keys(&self) -> Vec<&str> {
        self.0.keys().map(|name| name.as_str()).collect()
    }
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

    use opentelemetry::trace::TraceContextExt;

    use super::{
        extract_or_generate_trace_id, extract_remote_context, http_span_name, trace_middleware,
        TRACE_ID_HEADER,
    };

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

    #[test]
    fn valid_traceparent_is_extracted_as_remote_parent() {
        let mut headers = HeaderMap::new();
        headers.insert(
            "traceparent",
            HeaderValue::from_static("00-4bf92f3577b34da6a3ce929d0e0e4736-00f067aa0ba902b7-01"),
        );

        let context = extract_remote_context(&headers);
        let span = context.span();
        let span_context = span.span_context();

        assert!(span_context.is_valid());
        assert!(span_context.is_remote());
        assert_eq!(
            span_context.trace_id().to_string(),
            "4bf92f3577b34da6a3ce929d0e0e4736"
        );
    }

    #[test]
    fn invalid_traceparent_does_not_create_remote_parent() {
        let mut headers = HeaderMap::new();
        headers.insert("traceparent", HeaderValue::from_static("invalid"));

        let context = extract_remote_context(&headers);

        assert!(!context.span().span_context().is_valid());
    }

    #[test]
    fn span_name_uses_matched_route_instead_of_concrete_path() {
        assert_eq!(
            http_span_name(&axum::http::Method::GET, Some("/orders/{id}")),
            "GET /orders/{id}"
        );
        assert_eq!(http_span_name(&axum::http::Method::POST, None), "POST");
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
