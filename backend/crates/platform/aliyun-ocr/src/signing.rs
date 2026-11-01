//! ACS3-HMAC-SHA256；二进制请求体摘要和全部 x-acs 头参与签名。
use std::collections::BTreeMap;

use hmac::{Hmac, KeyInit, Mac};
use reqwest::header::{HeaderMap, HeaderName, HeaderValue};
use sha2::{Digest, Sha256};

use crate::{API_VERSION, Credentials, ENDPOINT, Error, Result};

/// 组装参与 ACS3 签名的请求头；`authorization` 始终标为敏感，STS token 存在时同样标为敏感。
///
/// # 参数
/// * `credentials` - 访问密钥；`security_token` 为 `Some` 时加入 `x-acs-security-token`。
/// * `body` - 原始请求体，其 SHA-256 十六进制摘要写入 `x-acs-content-sha256`。
/// * `query` - 查询串，原样进入规范请求，不再编码。
/// * `date` - 写入 `x-acs-date` 的时间字符串。
/// * `nonce` - 写入 `x-acs-signature-nonce` 的随机串。
///
/// # 返回
/// 包含 `content-type`、`host`、`x-acs-*` 和 `authorization` 的请求头。
///
/// # 错误
/// 头值或签名字符串含非法 HTTP 头字符，或 HMAC 密钥初始化失败时，返回 `Error::Configuration`。
pub(crate) fn headers(
    credentials: &Credentials,
    body: &[u8],
    query: &str,
    date: &str,
    nonce: &str,
) -> Result<HeaderMap> {
    let payload_hash = hex::encode(Sha256::digest(body));
    let mut values = BTreeMap::from([
        ("content-type", "application/octet-stream"),
        ("host", ENDPOINT),
        ("x-acs-action", "RecognizeDocumentStructure"),
        ("x-acs-content-sha256", payload_hash.as_str()),
        ("x-acs-date", date),
        ("x-acs-signature-nonce", nonce),
        ("x-acs-version", API_VERSION),
    ]);
    if let Some(token) = &credentials.security_token {
        values.insert("x-acs-security-token", token);
    }
    let authorization = authorize(credentials, &values, query, &payload_hash)?;
    let mut headers = HeaderMap::new();
    for (key, value) in values {
        let mut value = HeaderValue::from_str(value).map_err(|_| Error::Configuration)?;
        value.set_sensitive(key == "x-acs-security-token");
        headers.insert(HeaderName::from_static(key), value);
    }
    let mut authorization = HeaderValue::from_str(&authorization).map_err(|_| Error::Configuration)?;
    authorization.set_sensitive(true);
    headers.insert("authorization", authorization);
    Ok(headers)
}

/// 查询串和头值不再编码；HMAC 初始化失败时返回 `Error::Configuration`。
fn authorize(
    credentials: &Credentials,
    values: &BTreeMap<&str, &str>,
    query: &str,
    payload_hash: &str,
) -> Result<String> {
    let signed = values.keys().copied().collect::<Vec<_>>().join(";");
    let canonical_headers = values.iter().map(|(k, v)| format!("{k}:{}\n", v.trim())).collect::<String>();
    let canonical = format!("POST\n/\n{query}\n{canonical_headers}\n{signed}\n{payload_hash}");
    let to_sign = format!("ACS3-HMAC-SHA256\n{}", hex::encode(Sha256::digest(canonical.as_bytes())));
    let mut mac = Hmac::<Sha256>::new_from_slice(credentials.access_key_secret.as_bytes())
        .map_err(|_| Error::Configuration)?;
    mac.update(to_sign.as_bytes());
    let signature = hex::encode(mac.finalize().into_bytes());
    Ok(format!(
        "ACS3-HMAC-SHA256 Credential={},SignedHeaders={signed},Signature={signature}",
        credentials.access_key_id
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn matches_aliyun_published_v3_signature_vector() {
        let credentials = Credentials {
            access_key_id: "YourAccessKeyId".into(),
            access_key_secret: "YourAccessKeySecret".into(),
            security_token: None,
        };
        let hash = hex::encode(Sha256::digest(b""));
        let values = BTreeMap::from([
            ("host", "ecs.cn-shanghai.aliyuncs.com"),
            ("x-acs-action", "RunInstances"),
            ("x-acs-content-sha256", hash.as_str()),
            ("x-acs-date", "2023-10-26T10:22:32Z"),
            ("x-acs-signature-nonce", "3156853299f313e23d1673dc12e1703d"),
            ("x-acs-version", "2014-05-26"),
        ]);
        let auth = authorize(
            &credentials,
            &values,
            "ImageId=win2019_1809_x64_dtc_zh-cn_40G_alibase_20230811.vhd&RegionId=cn-shanghai",
            &hash,
        )
        .unwrap();
        assert!(auth.ends_with("Signature=06563a9e1b43f5dfe96b81484da74bceab24a1d853912eee15083a6f0f3283c0"));
    }

    #[test]
    fn binary_body_query_and_sts_token_are_authenticated() {
        let credentials = Credentials {
            access_key_id: "test".into(),
            access_key_secret: "secret".into(),
            security_token: Some("sts-token".into()),
        };
        let a = headers(&credentials, b"image", "Row=true", "2026-10-07T00:00:00Z", "nonce").unwrap();
        let b = headers(&credentials, b"other", "Row=true", "2026-10-07T00:00:00Z", "nonce").unwrap();
        let c = headers(&credentials, b"image", "Row=false", "2026-10-07T00:00:00Z", "nonce").unwrap();
        assert_ne!(a["authorization"], b["authorization"]);
        assert_ne!(a["authorization"], c["authorization"]);
        assert!(a["authorization"].to_str().unwrap().contains("x-acs-security-token"));
        assert!(a["authorization"].is_sensitive());
        assert!(a["x-acs-security-token"].is_sensitive());
    }
}
