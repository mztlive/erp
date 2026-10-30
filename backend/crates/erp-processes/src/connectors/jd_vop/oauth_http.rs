//! OAuth GET 刷新边界；凭据只存在于受控请求内，禁止重定向、隐藏重试及错误正文透出。
use std::time::Duration;

use async_trait::async_trait;
use reqwest::Client;
use reqwest::redirect::Policy;
use reqwest::retry::never;

use super::oauth::{JdVopRefreshRequest, JdVopRefreshResult, authorization_invalid};

const RESPONSE_LIMIT: usize = 1024 * 1024;

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum RefreshFailure {
    Unknown,
    AuthorizationInvalid,
}

#[async_trait]
pub(super) trait OAuthRefresh: Send + Sync {
    async fn refresh(&self, request: JdVopRefreshRequest) -> Result<JdVopRefreshResult, RefreshFailure>;
}

pub(super) struct OAuthHttp {
    client: Client,
}

impl OAuthHttp {
    pub(super) fn new(timeout_seconds: u64) -> Result<Self, RefreshFailure> {
        let client = Client::builder()
            .redirect(Policy::none())
            .retry(never())
            .timeout(Duration::from_secs(timeout_seconds))
            .build()
            .map_err(|_| RefreshFailure::Unknown)?;
        Ok(Self { client })
    }
}

#[async_trait]
impl OAuthRefresh for OAuthHttp {
    async fn refresh(&self, request: JdVopRefreshRequest) -> Result<JdVopRefreshResult, RefreshFailure> {
        // 官方授权指南规定 GET。敏感 query 不进入错误字符串、日志或可 Debug 对象。
        let endpoint = format!("{}?{}", request.endpoint(), request.parameters());
        let mut response = self.client.get(endpoint).send().await.map_err(|_| RefreshFailure::Unknown)?;
        let status = response.status().as_u16();
        if (300..400).contains(&status) {
            return Err(RefreshFailure::Unknown);
        }
        if response.content_length().is_some_and(|size| size > 1_048_576) {
            return Err(RefreshFailure::Unknown);
        }
        let mut body = Vec::new();
        while let Some(chunk) = response.chunk().await.map_err(|_| RefreshFailure::Unknown)? {
            if body.len().checked_add(chunk.len()).is_none_or(|size| size > RESPONSE_LIMIT) {
                return Err(RefreshFailure::Unknown);
            }
            body.extend_from_slice(&chunk);
        }
        decode(status, &body)
    }
}

fn decode(status: u16, body: &[u8]) -> Result<JdVopRefreshResult, RefreshFailure> {
    if body.len() > RESPONSE_LIMIT {
        return Err(RefreshFailure::Unknown);
    }
    if authorization_invalid(body) {
        return Err(RefreshFailure::AuthorizationInvalid);
    }
    if !(200..300).contains(&status) {
        return Err(RefreshFailure::Unknown);
    }
    JdVopRefreshResult::parse(body).map_err(|_| RefreshFailure::Unknown)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn oauth_status_and_known_revocation_are_classified_without_body_errors() {
        assert!(decode(200, br#"{"code":0,"access_token":"a","refresh_token":"r","expires_in":60}"#).is_ok());
        for body in [br#"{"code":20012,"private":"secret"}"#.as_slice(), br#"{"error":"invalid_grant"}"#] {
            assert!(matches!(decode(400, body), Err(RefreshFailure::AuthorizationInvalid)));
        }
        for (status, body) in [(302, b"private".as_slice()), (500, b"private"), (200, b"private")] {
            assert!(matches!(decode(status, body), Err(RefreshFailure::Unknown)));
        }
        assert!(matches!(decode(200, &vec![b' '; RESPONSE_LIMIT + 1]), Err(RefreshFailure::Unknown)));
    }
}
