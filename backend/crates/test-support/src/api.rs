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
/// 本客户端专供测试使用，失败时直接 panic 而不返回 `Result`，与其他辅助
/// 返回 `Result` 的风格差异是刻意为之：`Bearer` token 非法、请求构造失败、
/// 路由调用失败、响应体读取失败或超限（见 [`MAX_RESPONSE_BYTES`]）均会
/// panic，调用方无需在测试中处理这些不可能恢复的夹具错误。
pub struct TestApi {
    router: Router,
}

impl TestApi {
    /// 绑定待测试路由。
    ///
    /// # 参数
    /// * `router` - 已组装完成的路由（状态已通过 `with_state` 注入）
    ///
    /// # 返回值
    /// 返回测试客户端实例。
    pub fn new(router: Router) -> Self {
        Self { router }
    }

    /// 发送 GET 请求。
    ///
    /// # 参数
    /// * `path` - 请求路径（如 `/admin/roles`）
    /// * `token` - 可选 JWT，携带时写入 `Authorization: Bearer <token>`
    ///
    /// # 返回值
    /// 返回 `(HTTP 状态码, JSON 响应体)`。
    pub async fn get(&self, path: &str, token: Option<&str>) -> (u16, Value) {
        self.send(Method::GET, path, token, None).await
    }

    /// 发送 POST 请求。
    ///
    /// # 参数
    /// * `path` - 请求路径
    /// * `token` - 可选 JWT，携带时写入 `Authorization: Bearer <token>`
    /// * `json` - 可选的 JSON 请求体，携带时写入 `Content-Type: application/json`
    ///
    /// # 返回值
    /// 返回 `(HTTP 状态码, JSON 响应体)`。
    pub async fn post(&self, path: &str, token: Option<&str>, json: Option<Value>) -> (u16, Value) {
        self.send(Method::POST, path, token, json).await
    }

    /// 发送任意方法的请求（`get`/`post` 的通用入口）。
    ///
    /// 新增 `PUT`/`PATCH`/`DELETE` 等方法时直接调用本方法，不再复制
    /// `send` 转调样板。
    ///
    /// # 参数
    /// * `method` - HTTP 方法
    /// * `path` - 请求路径（如 `/admin/roles`）
    /// * `token` - 可选 JWT，携带时写入 `Authorization: Bearer <token>`
    /// * `json` - 可选的 JSON 请求体，携带时写入 `Content-Type: application/json`
    ///
    /// # 返回值
    /// 返回 `(HTTP 状态码, JSON 响应体)`；响应体非 JSON 时返回 `Value::Null`。
    pub async fn request(
        &self,
        method: Method,
        path: &str,
        token: Option<&str>,
        json: Option<Value>,
    ) -> (u16, Value) {
        self.send(method, path, token, json).await
    }

    /// 组装并发送一次请求，解包响应为 `(状态码, JSON 响应体)`。
    ///
    /// # 参数
    /// * `method` - HTTP 方法
    /// * `path` - 请求路径
    /// * `token` - 可选 JWT
    /// * `json` - 可选 JSON 请求体
    ///
    /// # 返回值
    /// 返回 `(HTTP 状态码, JSON 响应体)`；响应体非 JSON 时返回 `Value::Null`。
    async fn send(
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
        let response = self.router.clone().oneshot(request).await.expect("路由调用失败");
        let status = response.status().as_u16();
        let bytes = to_bytes(response.into_body(), MAX_RESPONSE_BYTES).await.expect("响应体读取失败");
        let value = serde_json::from_slice(&bytes).unwrap_or(Value::Null);
        (status, value)
    }
}
