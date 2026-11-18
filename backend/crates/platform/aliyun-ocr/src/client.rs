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
    /// 在装配图片 OCR 时，用部署配置提供的 AK/SK 和可选 STS token 创建可复用的 HTTPS 客户端。
    /// 连接限时 5 秒，单次调用限时 30 秒；不跟随重定向，也不自动重试计费请求。
    /// 凭据格式不合法或 TLS 客户端无法初始化时返回 [`Error::Configuration`]。
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

    /// 识别一张图片的完整文字，保留签章，返回文字及供应商版本信息。
    /// PDF 由调用方逐页渲染；此处只接收非空且不超过 10 MB 的原始图片字节。
    /// 空文字不能作为空白页的依据。请求或响应失败时仅返回脱敏后的错误分类。
    pub async fn recognize_image(&self, image: &[u8]) -> Result<Page> {
        let request = self.request(image)?;
        let response = self.http.execute(request).await.map_err(transport_error)?;
        let status = response.status();
        // 错误正文可能包含输入或身份；不读取、不记录。
        if !status.is_success() {
            return Err(response::classify_http_error(status.as_u16()));
        }

        let bytes = read_response(response).await?;
        response::decode(&bytes)
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

// HTTP 成功后才聚合响应正文；声明长度或累计分块超过上限时立即返回 ResponseSize，
// 防止在解析供应商 JSON 前将过大的正文读入内存。
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
