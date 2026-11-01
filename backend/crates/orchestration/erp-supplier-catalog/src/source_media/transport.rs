//! 按供应商提供的图片链接下载，限制请求时长和响应体大小。

use std::time::Duration;

use async_trait::async_trait;
use reqwest::{Client, Url};
use tokio::time::timeout;

use super::{DownloadedSourceImage, SourceImageTransport};
use crate::{Error, Result};

pub(super) const MAX_SOURCE_IMAGE_BYTES: usize = 10 * 1024 * 1024;
const REQUEST_TIMEOUT: Duration = Duration::from_secs(20);

/// 直接读取供应商的 HTTP/HTTPS 图片链接，使用客户端默认重定向策略。
pub struct HttpSourceImages {
    client: Client,
}

impl HttpSourceImages {
    /// 创建可复用的图片下载客户端。
    /// # 参数
    /// 无；无需登记图片域名。
    /// # 返回
    /// 不执行网络请求的来源读取器。
    /// # 错误
    /// HTTP 客户端初始化失败时返回错误。
    pub fn new() -> Result<Self> {
        let client = Client::builder()
            .no_proxy()
            .connect_timeout(Duration::from_secs(5))
            .timeout(REQUEST_TIMEOUT)
            .build()
            .map_err(|_| Error::Internal("来源图片读取器初始化失败".into()))?;
        Ok(Self { client })
    }

    async fn fetch(&self, value: &str) -> Result<DownloadedSourceImage> {
        read_response(&self.client, source_url(value)?).await
    }
}

#[async_trait]
impl SourceImageTransport for HttpSourceImages {
    async fn download(&self, url: &str) -> Result<DownloadedSourceImage> {
        timeout(REQUEST_TIMEOUT, self.fetch(url))
            .await
            .map_err(|_| Error::BusinessLogicError("供应商图片读取超时，请重试".into()))?
    }
}

async fn read_response(client: &Client, url: Url) -> Result<DownloadedSourceImage> {
    let mut response = client
        .get(url)
        .header(reqwest::header::CACHE_CONTROL, "no-cache")
        .send()
        .await
        .map_err(|_| Error::BusinessLogicError("供应商图片读取失败，请重试".into()))?;
    if !response.status().is_success() {
        return Err(Error::BusinessLogicError("供应商图片不可读取，请核对图片来源".into()));
    }
    if response.content_length().is_some_and(|length| length > u64::from(10_u32 * 1024 * 1024)) {
        return Err(image_size_error());
    }
    let content_type = response
        .headers()
        .get(reqwest::header::CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .map(ToOwned::to_owned);
    let mut bytes = Vec::new();
    while let Some(chunk) = response.chunk().await.map_err(|_| read_error())? {
        if bytes.len().saturating_add(chunk.len()) > MAX_SOURCE_IMAGE_BYTES {
            return Err(image_size_error());
        }
        bytes.extend_from_slice(&chunk);
    }
    Ok(DownloadedSourceImage { bytes, content_type })
}

fn source_url(value: &str) -> Result<Url> {
    Url::parse(value)
        .ok()
        .filter(|url| matches!(url.scheme(), "http" | "https") && url.host_str().is_some())
        .ok_or_else(|| Error::ValidationError("供应商图片地址无效，请核对来源".into()))
}

fn image_size_error() -> Error {
    Error::ValidationError("供应商商品图片超过 10 MiB，请核对来源".into())
}

fn read_error() -> Error {
    Error::BusinessLogicError("供应商图片读取失败，请重试".into())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn source_links_need_no_host_registration_and_preserve_signed_queries() {
        for value in [
            "https://cdn.example.com/p.png?signature=one%2Ftwo&expires=123",
            "http://other.example.com:8080/p.png",
            "https://cdn.example.com:444/p.png",
            "http://127.0.0.1:8080/p.png",
            "http://10.2.1.1/p.png",
            "http://[::1]:8080/p.png",
        ] {
            assert_eq!(source_url(value).unwrap().as_str(), value);
        }
    }

    #[test]
    fn invalid_or_non_http_links_return_explicit_errors() {
        for value in
            ["", "relative/image.png", "https://", "file:///image.png", "ftp://cdn.example.com/p.png"]
        {
            assert!(matches!(source_url(value), Err(Error::ValidationError(_))), "{value}");
        }
    }
}
