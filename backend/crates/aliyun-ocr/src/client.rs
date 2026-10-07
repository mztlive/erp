use std::time::Duration;

use chrono::{SecondsFormat, Utc};
use reqwest::redirect::Policy;
use reqwest::{Client as HttpClient, Request, Response, retry};
use uuid::Uuid;

use crate::{
    Credentials, ENDPOINT, Error, MAX_IMAGE_BYTES, MAX_RESPONSE_BYTES, Page, Result, response, signing,
};

// 参数固定且按 ASCII 字典序排列；参数仅包含 RFC3986 非保留字符。
const QUERY: &str = "NeedRotate=true&NeedSortPage=true&NoStamp=false&OutputCharInfo=false&OutputTable=true&Page=false&Paragraph=true&Row=true&UseNewStyleOutput=false";

/// 可复用的 HTTPS 客户端；不隐式重试计费请求，不跟随重定向。
#[derive(Clone)]
pub struct Client {
    http: HttpClient,
    credentials: Credentials,
}
impl Client {
    /// 构造图片识别客户端。
    /// # 参数
    /// * `credentials` - 配置中心提供的 AK/SK 和可选 STS token
    /// # 返回
    /// 连接与单次请求分别限时 5 秒和 30 秒的客户端。
    /// # 错误
    /// 无效凭据或 TLS 客户端初始化失败。
    pub fn new(credentials: Credentials) -> Result<Self> {
        credentials.validate()?;
        let http = HttpClient::builder()
            .connect_timeout(Duration::from_secs(5))
            .timeout(Duration::from_secs(30))
            .redirect(Policy::none())
            .retry(retry::never())
            .build()
            .map_err(|_| Error::Configuration)?;
        Ok(Self { http, credentials })
    }

    /// 上传单张图片字节并识别全部文字，保留签章。
    /// # 参数
    /// * `image` - 不超过 10 MB 的 API 支持图片；PDF 须由调用层逐页渲染
    /// # 返回
    /// 图片文字和供应商算法版本。
    /// # 错误
    /// 超时、授权、限流、网络、业务错误及非法或超限响应；不包含原文。
    pub async fn recognize_image(&self, image: &[u8]) -> Result<Page> {
        let request = self.request(image)?;
        let response = self.http.execute(request).await.map_err(transport_error)?;
        let status = response.status().as_u16();
        // 错误正文可能包含输入或身份；不读取、不记录。
        if !(200..300).contains(&status) {
            return response::decode(status, &[]);
        }
        let bytes = read_response(response).await?;
        response::decode(status, &bytes)
    }

    fn request(&self, image: &[u8]) -> Result<Request> {
        if image.is_empty() || image.len() > MAX_IMAGE_BYTES {
            return Err(Error::ImageSize);
        }
        let date = Utc::now().to_rfc3339_opts(SecondsFormat::Secs, true);
        let nonce = Uuid::new_v4().to_string();
        let headers = signing::headers(&self.credentials, image, QUERY, &date, &nonce)?;
        self.http
            .post(format!("https://{ENDPOINT}/?{QUERY}"))
            .headers(headers)
            .body(image.to_vec())
            .build()
            .map_err(|_| Error::Configuration)
    }
}

async fn read_response(mut response: Response) -> Result<Vec<u8>> {
    if response.content_length().is_some_and(|length| length > MAX_RESPONSE_BYTES as u64) {
        return Err(Error::ResponseSize);
    }
    let mut bytes = Vec::new();
    while let Some(chunk) = response.chunk().await.map_err(transport_error)? {
        if bytes.len().saturating_add(chunk.len()) > MAX_RESPONSE_BYTES {
            return Err(Error::ResponseSize);
        }
        bytes.extend_from_slice(&chunk);
    }
    Ok(bytes)
}
fn transport_error(error: reqwest::Error) -> Error {
    if error.is_timeout() { Error::Timeout } else { Error::Transport }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn builds_binary_post_with_signed_query_and_fresh_nonce() {
        let client = Client::new(Credentials {
            access_key_id: "id".into(),
            access_key_secret: "secret".into(),
            security_token: None,
        })
        .unwrap();
        let a = client.request(b"image-bytes").unwrap();
        let b = client.request(b"image-bytes").unwrap();
        assert_eq!(a.method(), "POST");
        assert_eq!(a.url().scheme(), "https");
        assert_eq!(a.url().host_str(), Some(ENDPOINT));
        assert_eq!(a.url().query(), Some(QUERY));
        assert_eq!(a.body().unwrap().as_bytes(), Some(b"image-bytes".as_slice()));
        assert_eq!(a.headers()["content-type"], "application/octet-stream");
        assert_ne!(a.headers()["x-acs-signature-nonce"], b.headers()["x-acs-signature-nonce"]);
        assert!(matches!(client.request(&[]), Err(Error::ImageSize)));
        assert!(matches!(client.request(&vec![0; MAX_IMAGE_BYTES + 1]), Err(Error::ImageSize)));
    }
}
