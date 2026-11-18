use serde::Deserialize;

use crate::{API_VERSION, Error, MAX_RESPONSE_BYTES, Result};

/// 单张图片的完整文字与供应商版本；空文字不等于空白图片。
#[derive(Clone, PartialEq, Eq)]
pub struct Page {
    pub text: String,
    pub version: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "PascalCase")]
struct Envelope {
    data: Option<String>,
    code: Option<String>,
}

#[derive(Deserialize)]
struct Data {
    content: String,
    #[serde(default)]
    algo_version: String,
    #[serde(default)]
    prism_version: String,
}

// HTTP 成功后先检查信封的业务码，再将字符串 Data 解码为内层 JSON；业务错误不带供应商原文。
// 返回原样的 content 和 `API版本;algo=…;prism=…`；缺失的版本段留空，空文字不等于空白页。
// 正文上限 8 MiB，文本上限 100000 字节，两个版本字段分别限 80 字节。JSON 无效或缺少 Data
// 返回 InvalidResponse，长度超限返回 ResponseSize。
pub(super) fn decode(bytes: &[u8]) -> Result<Page> {
    if bytes.len() > MAX_RESPONSE_BYTES {
        return Err(Error::ResponseSize);
    }

    let envelope: Envelope = serde_json::from_slice(bytes).map_err(|_| Error::InvalidResponse)?;
    if let Some(code) = envelope.code.filter(|value| value != "200") {
        return Err(classify_business_error(&code));
    }

    let data: Data = serde_json::from_str(&envelope.data.ok_or(Error::InvalidResponse)?)
        .map_err(|_| Error::InvalidResponse)?;
    if data.content.len() > 100_000 || data.algo_version.len() > 80 || data.prism_version.len() > 80 {
        return Err(Error::ResponseSize);
    }

    let version = format!("{API_VERSION};algo={};prism={}", data.algo_version, data.prism_version);
    Ok(Page { text: data.content, version })
}

pub(super) fn classify_http_error(status: u16) -> Error {
    match status {
        401 | 403 => Error::Authorization,
        429 => Error::Throttled,
        500.. => Error::Unavailable,
        _ => Error::Rejected,
    }
}

// HTTP 成功但业务码非 200 时，仅将已知代码归为授权、限流或服务不可用；
// 其他错误统一视为请求被拒绝，避免供应商正文或原始错误码进入日志。
fn classify_business_error(code: &str) -> Error {
    if code == "noPermission"
        || code.starts_with("InvalidAccessKeyId")
        || code.starts_with("SignatureDoesNotMatch")
        || code.starts_with("InvalidSecurityToken")
        || code.starts_with("Forbidden")
    {
        Error::Authorization
    } else if code.starts_with("Throttling") {
        Error::Throttled
    } else if code.starts_with("InternalError") || code.starts_with("ServiceUnavailable") {
        Error::Unavailable
    } else {
        Error::Rejected
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;
    #[test]
    fn decodes_double_encoded_data_without_treating_empty_text_as_blank() {
        for text in ["甲方：某公司\n合计：100元", ""] {
            let bytes = serde_json::to_vec(
                &json!({"Data":json!({"content":text,"prism_version":"1.0.9"}).to_string(),"RequestId":"id"}),
            )
            .unwrap();
            let page = decode(&bytes).unwrap();
            assert_eq!(page.text, text);
            assert!(page.version.contains("prism=1.0.9"));
        }
    }
    #[test]
    fn fails_on_business_errors_malformed_or_oversize_results_without_leaks() {
        let error = decode(br#"{"Code":"noPermission","Message":"secret response"}"#).err().unwrap();
        assert_eq!(error, Error::Authorization);
        assert!(!error.to_string().contains("secret"));

        for (code, expected) in [
            ("InvalidSecurityToken.Expired", Error::Authorization),
            ("Throttling.User", Error::Throttled),
            ("ServiceUnavailable", Error::Unavailable),
            ("Unknown", Error::Rejected),
        ] {
            let data = serde_json::to_vec(&json!({"Code": code})).unwrap();
            assert_eq!(decode(&data).err(), Some(expected));
        }

        for data in [br#"{}"#.as_slice(), br#"{"Data":"{}"}"#, br#"{"Data":{}}"#, b"not-json"] {
            assert_eq!(decode(data).err(), Some(Error::InvalidResponse));
        }
        assert_eq!(decode(&vec![0; MAX_RESPONSE_BYTES + 1]).err(), Some(Error::ResponseSize));
    }

    #[test]
    fn non_success_http_status_needs_no_body_to_classify() {
        for (status, expected) in [
            (301, Error::Rejected),
            (403, Error::Authorization),
            (429, Error::Throttled),
            (503, Error::Unavailable),
        ] {
            assert_eq!(classify_http_error(status), expected);
        }
    }
}
