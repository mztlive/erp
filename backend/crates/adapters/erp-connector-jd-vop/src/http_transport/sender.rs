//! 生产 HTTP 边界：禁止重定向，单次发送且对解压后的响应流限制大小。
use std::time::Duration;

use async_trait::async_trait;
use reqwest::Client;
use reqwest::redirect::Policy;
use reqwest::retry::never;

use super::{HttpFailure, HttpReply, HttpSender, RESPONSE_LIMIT};

pub(super) struct ReqwestSender {
    client: Client,
}
impl ReqwestSender {
    /// 创建单次 HTTP 发送边界。
    /// # 参数
    /// 已校验的超时秒数。
    /// # 返回
    /// 禁止重定向和隐藏重试的客户端。
    /// # 错误
    /// TLS 或客户端配置构造失败时返回无敏感正文的失败。
    pub(super) fn new(timeout_seconds: u64) -> Result<Self, HttpFailure> {
        let client = Client::builder()
            .redirect(Policy::none())
            .retry(never())
            .timeout(Duration::from_secs(timeout_seconds))
            .build()
            .map_err(|_| HttpFailure::Transport)?;
        Ok(Self { client })
    }
}
#[async_trait]
impl HttpSender for ReqwestSender {
    async fn send(&self, endpoint: &str, encoded: String) -> Result<HttpReply, HttpFailure> {
        let mut response = self
            .client
            .post(endpoint)
            .header("Content-Type", "application/x-www-form-urlencoded; charset=utf-8")
            .body(encoded)
            .send()
            .await
            .map_err(|_| HttpFailure::Transport)?;
        let status = response.status().as_u16();
        if (300..400).contains(&status) {
            return Ok(HttpReply { status, body: vec![] });
        }

        if response
            .content_length()
            .is_some_and(|size| size > u64::try_from(RESPONSE_LIMIT).expect("固定响应限制可表示"))
        {
            return Err(HttpFailure::Oversized);
        }
        let mut body = Vec::new();
        while let Some(chunk) = response.chunk().await.map_err(|_| HttpFailure::Transport)? {
            if body.len().checked_add(chunk.len()).is_none_or(|size| size > RESPONSE_LIMIT) {
                return Err(HttpFailure::Oversized);
            }
            body.extend_from_slice(&chunk);
        }
        Ok(HttpReply { status, body })
    }
}
