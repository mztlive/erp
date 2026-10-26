//! 来源图片网络读取：精确主机白名单、公共 DNS 地址固定、无重定向及有界响应。

use std::collections::BTreeSet;
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr, ToSocketAddrs};
use std::time::Duration;

use async_trait::async_trait;
use reqwest::redirect::Policy;
use reqwest::{Client, Url};
use tokio::task::spawn_blocking;
use tokio::time::timeout;

use super::{DownloadedSourceImage, SourceImageTransport};
use crate::{Error, Result};

pub(super) const MAX_SOURCE_IMAGE_BYTES: usize = 10 * 1024 * 1024;
const REQUEST_TIMEOUT: Duration = Duration::from_secs(20);

/// 只向部署明确列出的 HTTPS 图片主机读取；禁用代理和重定向。
pub struct HttpSourceImages {
    allowed_hosts: BTreeSet<String>,
}

impl HttpSourceImages {
    /// 校验并保存精确主机集合；不接受 IP、通配符、路径或端口。
    /// # 参数
    /// `allowed_hosts` 是部署登记的完整域名，空集合不允许下载任何图片。
    /// # 返回
    /// 无网络操作的来源读取器。
    /// # 错误
    /// 主机配置非法时拒绝。
    pub fn new(allowed_hosts: Vec<String>) -> Result<Self> {
        let allowed_hosts = allowed_hosts.iter().map(|host| normalize_host(host)).collect::<Result<_>>()?;
        Ok(Self { allowed_hosts })
    }

    async fn fetch(&self, value: &str) -> Result<DownloadedSourceImage> {
        let url = source_url(value, &self.allowed_hosts)?;
        let host = url.host_str().expect("来源 URL 已核验主机").to_owned();
        let addresses = public_addresses(host.clone()).await?;
        let client = Client::builder()
            .no_proxy()
            .redirect(Policy::none())
            .connect_timeout(Duration::from_secs(5))
            .timeout(REQUEST_TIMEOUT)
            .resolve_to_addrs(&host, &addresses)
            .build()
            .map_err(|_| Error::Internal("来源图片读取器初始化失败".into()))?;
        read_response(client, url).await
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

async fn read_response(client: Client, url: Url) -> Result<DownloadedSourceImage> {
    let mut response = client
        .get(url)
        .header(reqwest::header::CACHE_CONTROL, "no-cache")
        .send()
        .await
        .map_err(|_| Error::BusinessLogicError("供应商图片读取失败，请重试".into()))?;
    if !response.status().is_success() {
        return Err(Error::BusinessLogicError("供应商图片不可读取或要求跳转，请核对图片来源".into()));
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

async fn public_addresses(host: String) -> Result<Vec<SocketAddr>> {
    spawn_blocking(move || {
        let addresses = (host.as_str(), 443).to_socket_addrs().map_err(|_| read_error())?.collect::<Vec<_>>();
        if addresses.is_empty() || addresses.len() > 32 || addresses.iter().any(|ip| !public_ip(ip.ip())) {
            return Err(Error::ValidationError("供应商图片地址未解析到允许的公网地址".into()));
        }
        Ok(addresses)
    })
    .await
    .map_err(|_| read_error())?
}

fn normalize_host(value: &str) -> Result<String> {
    let url = Url::parse(&format!("https://{value}"));
    let valid = url.ok().filter(|url| {
        value.trim() == value
            && !value.is_empty()
            && value.len() <= 253
            && value.bytes().all(|byte| byte.is_ascii_alphanumeric() || b"-.".contains(&byte))
            && value.contains('.')
            && !value.ends_with('.')
            && value.parse::<IpAddr>().is_err()
            && url.host_str().is_some()
            && url.path() == "/"
            && url.port().is_none()
            && url.username().is_empty()
            && url.password().is_none()
            && url.query().is_none()
            && url.fragment().is_none()
    });
    valid.and_then(|url| url.host_str().map(ToOwned::to_owned)).ok_or_else(|| {
        Error::ValidationError("供应商图片允许主机必须为不含端口、通配符及路径的完整域名".into())
    })
}

fn source_url(value: &str, hosts: &BTreeSet<String>) -> Result<Url> {
    let url =
        Url::parse(value).map_err(|_| Error::ValidationError("供应商图片地址无效，请核对来源".into()))?;
    if value.len() > 4096
        || value.trim() != value
        || url.scheme() != "https"
        || url.port_or_known_default() != Some(443)
        || !url.username().is_empty()
        || url.password().is_some()
        || url.fragment().is_some()
        || !url.host_str().is_some_and(|host| hosts.contains(host))
    {
        return Err(Error::ValidationError("供应商图片来源未获授权，请登记图片域名后重试".into()));
    }
    Ok(url)
}

fn public_ip(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(ip) => public_v4(ip),
        IpAddr::V6(ip) => public_v6(ip),
    }
}

fn public_v4(ip: Ipv4Addr) -> bool {
    let [a, b, c, _] = ip.octets();
    !(ip.is_private()
        || ip.is_loopback()
        || ip.is_link_local()
        || ip.is_broadcast()
        || ip.is_documentation()
        || a == 0
        || a >= 224
        || (a == 100 && (64..=127).contains(&b))
        || (a == 169 && b == 254)
        || (a == 192 && b == 0 && c == 0)
        || (a == 198 && (b == 18 || b == 19)))
}

fn public_v6(ip: Ipv6Addr) -> bool {
    if let Some(v4) = ip.to_ipv4_mapped() {
        return public_v4(v4);
    }
    let segments = ip.segments();
    (segments[0] & 0xe000) == 0x2000
        && !(segments[0] == 0x2001 && (segments[1] < 0x0200 || segments[1] == 0x0db8))
        && segments[0] != 0x2002
        && segments[0] != 0x3fff
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
    fn exact_https_hosts_reject_credentials_redirect_targets_and_private_ips() {
        let hosts = BTreeSet::from([normalize_host("CDN.example.com").unwrap()]);
        assert!(source_url("https://cdn.example.com/p.png?signature=one", &hosts).is_ok());
        for url in [
            "http://cdn.example.com/p.png",
            "https://cdn.example.com:444/p.png",
            "https://user:pass@cdn.example.com/p.png",
            "https://cdn.example.com.evil.test/p.png",
            "https://other.example.com/p.png",
            "https://127.0.0.1/p.png",
        ] {
            assert!(source_url(url, &hosts).is_err());
        }
        for host in ["*.example.com", "127.0.0.1", "localhost", "cdn.example.com/path", "cdn.example.com:443"]
        {
            assert!(normalize_host(host).is_err());
        }
    }

    #[test]
    fn dns_addresses_reject_nonpublic_and_mapped_networks() {
        for ip in [
            "127.0.0.1",
            "10.2.1.1",
            "100.64.1.1",
            "169.254.169.254",
            "192.0.2.2",
            "198.19.1.1",
            "::1",
            "fd00::1",
            "fe80::1",
            "::ffff:127.0.0.1",
            "2001:db8::1",
            "2002:7f00:1::1",
        ] {
            assert!(!public_ip(ip.parse().unwrap()), "{ip}");
        }
        assert!(public_ip("8.8.8.8".parse().unwrap()));
        assert!(public_ip("2606:4700:4700::1111".parse().unwrap()));
    }
}
