//! HTTP 测试客户端：对已组装的 `axum::Router` 发送真实请求。

use axum::Router;
use axum::body::{Body, to_bytes};
use axum::http::header::{AUTHORIZATION, CONTENT_TYPE};
use axum::http::{HeaderValue, Method, Request};
use serde_json::Value;
use tower::ServiceExt;

/// 测试响应体的有界读取上限（8 MiB），防止无界内存读取。
const MAX_RESPONSE_BYTES: usize = 8 * 1024 * 1024;

/// 基于 `tower::ServiceExt::oneshot` 的进程内 HTTP 测试客户端。
///
/// 由调用方提供已组装完成的 `Router`（`web-api` 侧传入
/// `core::routes::create(AppState)` 的返回值即可），不启动真实端口监听。
/// axum 0.8 只为 `Router<()>` 实现 `tower::Service`，因此自定义路由需通过
/// `with_state` 组装后以 `Router<()>` 形态传入（`web-api` 的 `create()` 已满足）。
///
/// # Panics
/// `Bearer` token 非法、请求构造失败、响应体读取失败或超限（见 [`MAX_RESPONSE_BYTES`]）时 panic。
pub struct TestApi {
    router: Router,
}

impl TestApi {
    /// 测试已装配的 Axum 路由时，用状态已通过 `with_state` 注入的 `Router<()>` 构造进程内客户端，
    /// 直接经过路由及中间件，不启动 HTTP 监听端口。
    pub fn new(router: Router) -> Self {
        Self { router }
    }

    /// 向进程内路由发起 GET，例如用 `/admin/roles` 断言查询或鉴权结果。
    /// 可选 JWT 放入 `Authorization: Bearer <token>`，返回 HTTP 状态码与 JSON；非 JSON 响应为 `Value::Null`。
    /// 请求构造及读取限制见 [`Self::request`]。
    pub async fn get(&self, path: &str, token: Option<&str>) -> (u16, Value) {
        self.request(Method::GET, path, token, None).await
    }

    /// 测试提交类路由时发起 POST；传入 JSON 即设置 `Content-Type: application/json`，可选 JWT 作为 Bearer 凭据。
    /// 返回 HTTP 状态码与 JSON，非 JSON 响应为 `Value::Null`。请求构造及读取限制见 [`Self::request`]。
    pub async fn post(&self, path: &str, token: Option<&str>, json: Option<Value>) -> (u16, Value) {
        self.request(Method::POST, path, token, json).await
    }

    /// 在进程内对已装配路由执行任意 HTTP 方法，是 GET、POST 辅助入口及 PUT、PATCH、DELETE 等测试的共同入口。
    /// 按需附加 Bearer JWT 和 JSON 请求体，返回 HTTP 状态码与解析后的 JSON；非 JSON 正文返回 `Value::Null`。
    ///
    /// # Panics
    /// Bearer 头或请求非法、响应体无法读取或超过 8 MiB（[`MAX_RESPONSE_BYTES`]）时中止测试。
    pub async fn request(
        &self,
        method: Method,
        path: &str,
        token: Option<&str>,
        json: Option<Value>,
    ) -> (u16, Value) {
        let mut builder = Request::builder().method(method).uri(path);
        if let Some(token) = token {
            let value = HeaderValue::from_str(&format!("Bearer {token}")).expect("Bearer token 应合法");
            builder = builder.header(AUTHORIZATION, value);
        }
        let body = match json {
            Some(value) => {
                builder = builder.header(CONTENT_TYPE, "application/json");
                Body::from(value.to_string())
            },
            None => Body::empty(),
        };
        let request = builder.body(body).expect("HTTP 请求构造失败");

        let Ok(response) = self.router.clone().oneshot(request).await;
        let status = response.status().as_u16();
        let bytes = to_bytes(response.into_body(), MAX_RESPONSE_BYTES).await.expect("响应体读取失败");
        let value = serde_json::from_slice(&bytes).unwrap_or(Value::Null);
        (status, value)
    }
}
