//! 固定官方OAuth地址的单次GET；禁跟随重定向、禁重试、禁输出URI及响应正文。
use std::time::Duration;

use async_trait::async_trait;
use reqwest::Client;
use reqwest::redirect::Policy;
use reqwest::retry::never;
use serde_json::{Value, from_slice};
use zeroize::Zeroizing;

use super::super::super::oauth::JdVopRefreshResult;
use super::protocol::{AUTHORIZE_ENDPOINT, AuthorizationParameters, TOKEN_ENDPOINT, authorization_code};
use super::{AuthorizationError, Result};

const RESPONSE_LIMIT: usize = 1024 * 1024;

pub(super) struct AuthorizationGrant {
    pub(super) tokens: JdVopRefreshResult,
    pub(super) identity: Option<Zeroizing<String>>,
}

#[async_trait]
pub(super) trait AuthorizationHttp: Send + Sync {
    async fn code(&self, parameters: AuthorizationParameters) -> Result<Zeroizing<String>>;
    async fn exchange(&self, parameters: Zeroizing<String>) -> Result<AuthorizationGrant>;
}

pub(super) struct ReqwestAuthorizationHttp {
    client: Client,
}

impl ReqwestAuthorizationHttp {
    pub(super) fn new(timeout_seconds: u64) -> Result<Self> {
        let client = Client::builder()
            .redirect(Policy::none())
            .retry(never())
            .timeout(Duration::from_secs(timeout_seconds))
            .build()
            .map_err(|_| AuthorizationError::Unavailable)?;
        Ok(Self { client })
    }

    async fn get(&self, endpoint: &'static str, parameters: &str) -> Result<Reply> {
        let url = Zeroizing::new(format!("{endpoint}?{parameters}"));
        let mut response =
            self.client.get(url.as_str()).send().await.map_err(|_| AuthorizationError::Unknown)?;
        let status = response.status().as_u16();
        let location = response
            .headers()
            .get("location")
            .and_then(|value| value.to_str().ok())
            .map(|value| Zeroizing::new(value.to_string()));
        if (300..400).contains(&status) {
            return Ok(Reply { status, location, body: Zeroizing::new(Vec::new()) });
        }
        if response.content_length().is_some_and(|size| size > 1_048_576) {
            return Err(AuthorizationError::Unknown);
        }
        let mut body = Zeroizing::new(Vec::new());
        while let Some(chunk) = response.chunk().await.map_err(|_| AuthorizationError::Unknown)? {
            if body.len().checked_add(chunk.len()).is_none_or(|size| size > RESPONSE_LIMIT) {
                return Err(AuthorizationError::Unknown);
            }
            body.extend_from_slice(&chunk);
        }
        Ok(Reply { status, location, body })
    }
}

struct Reply {
    status: u16,
    location: Option<Zeroizing<String>>,
    body: Zeroizing<Vec<u8>>,
}

#[async_trait]
impl AuthorizationHttp for ReqwestAuthorizationHttp {
    async fn code(&self, parameters: AuthorizationParameters) -> Result<Zeroizing<String>> {
        let reply = self.get(AUTHORIZE_ENDPOINT, &parameters.query).await?;
        if reply.status == 302 {
            let location = reply.location.as_ref().ok_or(AuthorizationError::Unknown)?;
            return authorization_code(&parameters.callback, location);
        }
        if explicitly_rejected(&reply.body) {
            return Err(AuthorizationError::Rejected);
        }
        Err(AuthorizationError::Unknown)
    }

    async fn exchange(&self, parameters: Zeroizing<String>) -> Result<AuthorizationGrant> {
        let reply = self.get(TOKEN_ENDPOINT, &parameters).await?;
        if !(200..300).contains(&reply.status) {
            return Err(AuthorizationError::Unknown);
        }
        grant(&reply.body)
    }
}

fn explicitly_rejected(body: &[u8]) -> bool {
    let Ok(value) = from_slice::<Value>(body) else { return false };
    let code = value.get("code").or_else(|| value.get("resultCode"));
    let code = code.and_then(|code| {
        code.as_str().map(str::to_string).or_else(|| code.as_i64().map(|value| value.to_string()))
    });
    code.is_some_and(|code| {
        matches!(
            code.as_str(),
            "10001"
                | "10002"
                | "10003"
                | "20001"
                | "20002"
                | "20003"
                | "20004"
                | "20005"
                | "20006"
                | "20007"
                | "20008"
                | "20009"
                | "20020"
                | "20025"
                | "1026"
                | "1027"
                | "302"
                | "304"
                | "305"
        )
    })
}

fn grant(body: &[u8]) -> Result<AuthorizationGrant> {
    if body.len() > RESPONSE_LIMIT {
        return Err(AuthorizationError::Unknown);
    }
    let tokens = JdVopRefreshResult::parse(body).map_err(|_| AuthorizationError::Unknown)?;
    let root: Value = from_slice(body).map_err(|_| AuthorizationError::Unknown)?;
    let object = root.get("result").unwrap_or(&root);
    let origin = object.get("originJosResp").unwrap_or(object);
    let identity = origin
        .get("uid")
        .and_then(Value::as_str)
        .map(|id| ("uid", id))
        .or_else(|| {
            object
                .get("openId")
                .or_else(|| origin.get("open_id"))
                .and_then(Value::as_str)
                .map(|id| ("open_id", id))
        })
        .map(|(kind, id)| {
            if id.is_empty() || id.len() > 8192 || id.chars().any(char::is_control) {
                return Err(AuthorizationError::Unknown);
            }
            Ok(Zeroizing::new(format!("{kind}:{id}")))
        })
        .transpose()?;
    Ok(AuthorizationGrant { tokens, identity })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_documented_login_rejections_can_release_candidate_identity() {
        assert!(explicitly_rejected(br#"{"code":1026,"message":"private-password"}"#));
        assert!(!explicitly_rejected(br#"{"code":0}"#));
        assert!(!explicitly_rejected(br#"{"code":20018}"#));
        assert!(!explicitly_rejected(b"private-password"));
    }

    #[test]
    fn exchange_reads_remote_identity_but_never_exposes_token_body_in_errors() {
        let result =
            grant(br#"{"code":0,"uid":"remote-123","access_token":"a","refresh_token":"r","expires_in":60}"#)
                .unwrap();
        assert_eq!(result.identity.unwrap().as_str(), "uid:remote-123");
        assert!(grant(br#"{"code":20010,"message":"private-token"}"#).is_err());
    }
}
