use std::time::Duration;

use axum::extract::DefaultBodyLimit;
use axum::routing::post;
use axum::{Extension, Router};

use crate::http::handler::auth;
use crate::http::rate_limit::RateLimiter;
use crate::http::state::HttpState;

const LOGIN_ATTEMPTS_PER_SOURCE: usize = 20;
const EMERGENCY_GLOBAL_LOGIN_ATTEMPTS: usize = 600;
const LOGIN_RATE_WINDOW: Duration = Duration::from_secs(60);
const MAX_CONCURRENT_LOGINS: usize = 4;
const MAX_LOGIN_REQUEST_BYTES: usize = 4 * 1024;

/// 挂载无需后台 JWT 的登录和公开选品入口；登录路由有独立请求体上限，并向处理器提供基于连接来源的限流器。
pub fn routes(app_state: HttpState) -> Router<HttpState> {
    let login = Router::new()
        .route("/login", post(auth::login::login))
        .layer(Extension(login_limiter()))
        .layer(login_body_limit());
    Router::new().merge(login).merge(super::sales_selection::public_routes()).with_state(app_state)
}

// 创建公开登录入口使用的进程内限流器。
// 返回每个“登录域 + TCP peer IP”20 次/60 秒、全局应急熔断 600 次/60 秒、
// 并发 4 个请求的限流器。同一来源上的账号不再单独计数。
pub(super) fn login_limiter() -> RateLimiter {
    RateLimiter::new(
        LOGIN_ATTEMPTS_PER_SOURCE,
        EMERGENCY_GLOBAL_LOGIN_ATTEMPTS,
        LOGIN_RATE_WINDOW,
        MAX_CONCURRENT_LOGINS,
    )
}

// 在后台与供应商两类登录入口共用 4 KiB 正文上限，避免解析超大登录输入。
pub(super) fn login_body_limit() -> DefaultBodyLimit {
    DefaultBodyLimit::max(MAX_LOGIN_REQUEST_BYTES)
}

#[cfg(test)]
mod tests {
    use axum::Router;
    use axum::body::Body;
    use axum::http::{Request, StatusCode};
    use axum::routing::post;
    use tower::Service;

    use super::{
        EMERGENCY_GLOBAL_LOGIN_ATTEMPTS, LOGIN_ATTEMPTS_PER_SOURCE, MAX_LOGIN_REQUEST_BYTES,
        login_body_limit, login_limiter,
    };
    use crate::http::rate_limit::Error as RateLimitError;

    #[test]
    fn login_policy_allows_repeated_account_until_source_cap() {
        let limiter = login_limiter();
        let source = "backoffice|192.0.2.1";
        for _ in 0..LOGIN_ATTEMPTS_PER_SOURCE {
            drop(limiter.admit(source).unwrap());
        }

        assert!(matches!(limiter.admit(source), Err(RateLimitError::KeyExceeded { .. })));
    }

    #[test]
    fn login_policy_keeps_high_emergency_global_fuse() {
        let limiter = login_limiter();
        for index in 0..EMERGENCY_GLOBAL_LOGIN_ATTEMPTS {
            let source = format!("backoffice|192.0.2.{index}");
            drop(limiter.admit(&source).unwrap());
        }

        assert!(matches!(
            limiter.admit("backoffice|198.51.100.1"),
            Err(RateLimitError::GlobalExceeded { .. })
        ));
    }

    #[tokio::test]
    async fn login_body_over_limit_returns_payload_too_large() {
        async fn accept_body(_: String) -> StatusCode {
            StatusCode::OK
        }

        let mut router = Router::new().route("/", post(accept_body)).layer(login_body_limit());
        let request = Request::post("/").body(Body::from("x".repeat(MAX_LOGIN_REQUEST_BYTES + 1))).unwrap();

        let response = router.call(request).await.unwrap();

        assert_eq!(response.status(), StatusCode::PAYLOAD_TOO_LARGE);
    }
}
