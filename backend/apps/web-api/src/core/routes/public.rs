use std::time::Duration;

use crate::{app_state::AppState, core::handler::auth};
use axum::{extract::DefaultBodyLimit, routing::post, Extension, Router};

use crate::core::rate_limit::RateLimiter;

const LOGIN_ATTEMPTS_PER_SOURCE: usize = 20;
const LOGIN_ATTEMPTS_PER_SOURCE_ACCOUNT: usize = 5;
const EMERGENCY_GLOBAL_LOGIN_ATTEMPTS: usize = 600;
const LOGIN_RATE_WINDOW: Duration = Duration::from_secs(60);
const MAX_CONCURRENT_LOGINS: usize = 4;
const MAX_LOGIN_REQUEST_BYTES: usize = 4 * 1024;

/// 构建路由集合。
///
/// # 参数
/// * `app_state` - 应用状态
///
/// # 返回
/// 返回 `Router<AppState>` 结果。
pub fn routes(app_state: AppState) -> Router<AppState> {
    Router::new()
        .route("/login", post(auth::login::login))
        .route("/consumer/login", post(auth::consumer::login))
        .layer(Extension(login_limiter()))
        .layer(login_body_limit())
        .with_state(app_state)
}

/// 创建两个公开登录入口共享的进程内限流器。
///
/// # 返回值
/// 返回每个“登录域 + TCP peer IP”20 次/60 秒、每个来源与账号组合 5 次/60 秒、
/// 全局应急熔断 600 次/60 秒、并发 4 个请求的限流器。
fn login_limiter() -> RateLimiter {
    RateLimiter::with_key_limits(
        &[LOGIN_ATTEMPTS_PER_SOURCE, LOGIN_ATTEMPTS_PER_SOURCE_ACCOUNT],
        EMERGENCY_GLOBAL_LOGIN_ATTEMPTS,
        LOGIN_RATE_WINDOW,
        MAX_CONCURRENT_LOGINS,
    )
}

/// 创建两个登录入口共享的 4 KiB 请求体上限。
fn login_body_limit() -> DefaultBodyLimit {
    DefaultBodyLimit::max(MAX_LOGIN_REQUEST_BYTES)
}

#[cfg(test)]
mod tests {
    use axum::{
        body::Body,
        http::{Request, StatusCode},
        routing::post,
        Router,
    };
    use tower::Service;

    use crate::core::rate_limit::Error as RateLimitError;

    use super::{
        login_body_limit, login_limiter, EMERGENCY_GLOBAL_LOGIN_ATTEMPTS, LOGIN_ATTEMPTS_PER_SOURCE,
        LOGIN_ATTEMPTS_PER_SOURCE_ACCOUNT, MAX_LOGIN_REQUEST_BYTES,
    };

    #[test]
    fn login_policy_limits_one_source_account_combination() {
        let limiter = login_limiter();
        let source = "backoffice|192.0.2.1";
        let source_account = "backoffice|192.0.2.1|account01";
        for _ in 0..LOGIN_ATTEMPTS_PER_SOURCE_ACCOUNT {
            drop(limiter.admit_hierarchy(&[source, source_account]).unwrap());
        }

        assert!(matches!(
            limiter.admit_hierarchy(&[source, source_account]),
            Err(RateLimitError::KeyExceeded { .. })
        ));
    }

    #[test]
    fn login_policy_limits_account_rotation_from_one_source() {
        let limiter = login_limiter();
        let source = "backoffice|192.0.2.1";
        for index in 0..LOGIN_ATTEMPTS_PER_SOURCE {
            let source_account = format!("{source}|account-{index}");
            drop(limiter.admit_hierarchy(&[source, &source_account]).unwrap());
        }

        let next_source_account = format!("{source}|next-account");
        assert!(matches!(
            limiter.admit_hierarchy(&[source, &next_source_account]),
            Err(RateLimitError::KeyExceeded { .. })
        ));
    }

    #[test]
    fn login_policy_keeps_high_emergency_global_fuse() {
        let limiter = login_limiter();
        for index in 0..EMERGENCY_GLOBAL_LOGIN_ATTEMPTS {
            let source = format!("backoffice|192.0.2.{index}");
            let source_account = format!("{source}|account-{index}");
            drop(limiter.admit_hierarchy(&[&source, &source_account]).unwrap());
        }

        assert!(matches!(
            limiter.admit_hierarchy(&["backoffice|198.51.100.1", "backoffice|198.51.100.1|next-account"]),
            Err(RateLimitError::GlobalExceeded { .. })
        ));
    }

    #[tokio::test]
    async fn login_body_over_limit_returns_payload_too_large() {
        async fn accept_body(_: String) -> StatusCode {
            StatusCode::OK
        }

        let mut router = Router::new()
            .route("/", post(accept_body))
            .layer(login_body_limit());
        let request = Request::post("/")
            .body(Body::from("x".repeat(MAX_LOGIN_REQUEST_BYTES + 1)))
            .unwrap();

        let response = router.call(request).await.unwrap();

        assert_eq!(response.status(), StatusCode::PAYLOAD_TOO_LARGE);
    }
}
