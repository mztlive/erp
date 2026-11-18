//! HTTP 请求 Trace ID 与耗时日志中间件。

use std::time::Instant;

use axum::extract::{MatchedPath, Request};
use axum::http::{HeaderMap, HeaderValue, Method, StatusCode};
use axum::middleware::Next;
use axum::response::Response;
use opentelemetry::Context;
use opentelemetry::propagation::{Extractor, TextMapPropagator};
use opentelemetry::trace::{Status as OtelStatus, TraceContextExt};
use opentelemetry_sdk::propagation::TraceContextPropagator;
use tracing::{Instrument, info_span};
use tracing_opentelemetry::OpenTelemetrySpanExt;
use uuid::Uuid;

pub(crate) const TRACE_ID_HEADER: &str = "X-Trace-Id";

/// 当前 HTTP 请求已确定的安全追踪号，与请求头和响应头使用同一值。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct RequestId(pub(crate) String);

// 在 HTTP 处理前注入安全请求关联 ID、接续远程 trace 并脱敏选品路径；响应完成后记录状态与耗时，回传同一 `X-Trace-Id`。
pub(crate) async fn trace_middleware(mut request: Request, next: Next) -> Result<Response, StatusCode> {
    let parent_context = extract_remote_context(request.headers());
    let (trace_id, trace_value) = prepare_trace(&mut request)?;

    let method = request.method().clone();
    let path = redacted_path(request.uri().path());
    let matched_route = request.extensions().get::<MatchedPath>().map(MatchedPath::as_str);
    let span_name = http_span_name(&method, matched_route);

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
    if let Some(route) = matched_route {
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

// 在提取远程父上下文后冻结本地请求关联 ID，同时写入请求头、审计扩展及响应头待用值；无法编码为安全 HTTP 头时返回 500。
fn prepare_trace(request: &mut Request) -> Result<(String, HeaderValue), StatusCode> {
    let trace_id = extract_or_generate_trace_id(request.headers());
    let trace_value: HeaderValue = trace_id.parse().map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    request.headers_mut().insert(TRACE_ID_HEADER, trace_value.clone());
    request.extensions_mut().insert(RequestId(trace_id.clone()));
    Ok((trace_id, trace_value))
}

// 请求进入追踪中间件时从 W3C `traceparent` 和 `tracestate` 恢复父上下文；没有可用传播头时让本进程创建根 trace。
fn extract_remote_context(headers: &HeaderMap) -> Context {
    TraceContextPropagator::new().extract(&HeaderExtractor(headers))
}

// 服务器 span 以低基数路由模板命名，避免业务 ID 等实际路径值扩展追踪维度。
fn http_span_name(method: &Method, matched_route: Option<&str>) -> String {
    matched_route.map_or_else(|| method.to_string(), |route| format!("{method} {route}"))
}

// 对外部关联 ID 仅接受不超过 128 字符、无控制字符及首尾空白的非空值；非法或缺失时生成 UUID，供日志与审计关联。
fn extract_or_generate_trace_id(headers: &HeaderMap) -> String {
    headers
        .get(TRACE_ID_HEADER)
        .and_then(|v| v.to_str().ok())
        .filter(|value| {
            !value.is_empty()
                && *value == value.trim()
                && value.chars().count() <= 128
                && !value.chars().any(char::is_control)
        })
        .map(|s| s.to_string())
        .unwrap_or_else(|| Uuid::new_v4().to_string())
}

/// 将 Axum 请求头适配为 OpenTelemetry 文本传播载体。
struct HeaderExtractor<'a>(&'a HeaderMap);

impl Extractor for HeaderExtractor<'_> {
    // 为 W3C 传播器提供不区分大小写的头查询，只交付可按 UTF-8 读取的值。
    fn get(&self, key: &str) -> Option<&str> {
        self.0.get(key).and_then(|value| value.to_str().ok())
    }

    // 枚举当前请求的头名，供传播器发现可读取的上下文字段。
    fn keys(&self) -> Vec<&str> {
        self.0.keys().map(|name| name.as_str()).collect()
    }
}

#[cfg(test)]
mod tests {
    use axum::body::Body;
    use axum::extract::Request as ExtractRequest;
    use axum::http::{HeaderMap, HeaderValue, Request};
    use axum::routing::get;
    use axum::{Router, middleware};
    use opentelemetry::trace::TraceContextExt;
    use tower::ServiceExt;
    use uuid::Uuid;

    use super::{
        RequestId, TRACE_ID_HEADER, extract_or_generate_trace_id, extract_remote_context, http_span_name,
        trace_middleware,
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

    // 用非法的客户端追踪值覆盖回退分支，确保空值、超长及非规范文本不会进入审计关联。
    #[test]
    fn trace_id_generates_uuid_when_header_is_empty_unbounded_or_not_normalized() {
        for invalid in
            [String::new(), "  ".into(), " client-id ".into(), "trace\tforged".into(), "r".repeat(129)]
        {
            let mut headers = HeaderMap::new();
            headers.insert(TRACE_ID_HEADER, HeaderValue::from_str(&invalid).unwrap());
            assert!(Uuid::parse_str(&extract_or_generate_trace_id(&headers)).is_ok());
        }
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
        assert_eq!(span_context.trace_id().to_string(), "4bf92f3577b34da6a3ce929d0e0e4736");
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
        assert_eq!(http_span_name(&axum::http::Method::GET, Some("/orders/{id}")), "GET /orders/{id}");
        assert_eq!(http_span_name(&axum::http::Method::POST, None), "POST");
    }

    #[tokio::test]
    async fn trace_id_is_returned_in_response_header() {
        let app = Router::new()
            .route(
                "/",
                get(|request: ExtractRequest| async move {
                    let request_id = request.extensions().get::<RequestId>().unwrap();
                    assert_eq!(request.headers().get(TRACE_ID_HEADER).unwrap(), request_id.0.as_str());
                    "ok"
                }),
            )
            .layer(middleware::from_fn(trace_middleware));
        let request = Request::builder()
            .uri("/")
            .header(TRACE_ID_HEADER, "client-trace-id")
            .body(Body::empty())
            .expect("request should build");

        let response = app.oneshot(request).await.expect("request should complete");

        assert_eq!(response.headers().get(TRACE_ID_HEADER).unwrap(), "client-trace-id");
    }
}

// 请求进入日志或追踪字段前去掉公开选品链接凭证，即便路由未匹配也不记录原始凭证。
fn redacted_path(path: &str) -> String {
    for prefix in ["/public/selection/", "/public/sales-selections/"] {
        if let Some(rest) = path.strip_prefix(prefix) {
            let suffix = rest.find('/').map(|index| &rest[index..]).unwrap_or("");
            return format!("{prefix}[REDACTED]{suffix}");
        }
    }
    path.to_string()
}

#[cfg(test)]
mod selection_path_tests {
    #[test]
    fn tokens_are_removed_for_both_prefixes_and_unknown_routes() {
        for prefix in ["/public/selection/", "/public/sales-selections/"] {
            for suffix in ["", "/session", "/submit", "/images/file", "/unknown"] {
                let path = super::redacted_path(&format!("{prefix}secret{suffix}"));
                assert_eq!(path, format!("{prefix}[REDACTED]{suffix}"));
            }
        }
        assert_eq!(super::redacted_path("/admin/customers"), "/admin/customers");
    }
}
