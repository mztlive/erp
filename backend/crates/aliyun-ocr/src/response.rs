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

/// 解码识别响应；非 2xx 不解析正文。空 `content` 仍是成功结果。
///
/// # 参数
/// * `status` - HTTP 状态码。
/// * `bytes` - 响应正文；仅在不超过上限且状态为 2xx 时按 JSON 解析。
///
/// # 返回
/// `Page`：`text` 为内层 `content`，`version` 为 `{API_VERSION};algo=…;prism=…`。
///
/// # 错误
/// `bytes` 超过 `MAX_RESPONSE_BYTES`，或 `content` 超过 100000 字节、`algo_version` 或 `prism_version` 超过 80 字节时，返回 `Error::ResponseSize`。
/// 信封或内层 `Data` 不是合法 JSON，或成功响应缺少 `Data` 时，返回 `Error::InvalidResponse`。
/// 状态非 2xx，或业务 `Code` 存在且不是 `200` 时，返回 `Error::Authorization`、`Error::Throttled`、`Error::Unavailable` 或 `Error::Rejected`。
pub(crate) fn decode(status: u16, bytes: &[u8]) -> Result<Page> {
    if bytes.len() > MAX_RESPONSE_BYTES {
        return Err(Error::ResponseSize);
    }
    if !(200..300).contains(&status) {
        return Err(classify(status, ""));
    }
    let envelope: Envelope = serde_json::from_slice(bytes).map_err(|_| Error::InvalidResponse)?;
    if let Some(code) = envelope.code.filter(|value| value != "200") {
        return Err(classify(status, &code));
    }
    let data: Data = serde_json::from_str(&envelope.data.ok_or(Error::InvalidResponse)?)
        .map_err(|_| Error::InvalidResponse)?;
    if data.content.len() > 100_000 || data.algo_version.len() > 80 || data.prism_version.len() > 80 {
        return Err(Error::ResponseSize);
    }
    let version = format!("{API_VERSION};algo={};prism={}", data.algo_version, data.prism_version);
    Ok(Page { text: data.content, version })
}

/// 只按状态码和错误码前缀归类，不把供应商正文写入 `Error`。
fn classify(status: u16, code: &str) -> Error {
    if matches!(status, 401 | 403)
        || code == "noPermission"
        || code.starts_with("InvalidAccessKeyId")
        || code.starts_with("SignatureDoesNotMatch")
        || code.starts_with("InvalidSecurityToken")
        || code.starts_with("Forbidden")
    {
        Error::Authorization
    } else if status == 429 || code.starts_with("Throttling") {
        Error::Throttled
    } else if status >= 500 || code.starts_with("InternalError") || code.starts_with("ServiceUnavailable") {
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
            let page = decode(200, &bytes).unwrap();
            assert_eq!(page.text, text);
            assert!(page.version.contains("prism=1.0.9"));
        }
    }
    #[test]
    fn fails_on_business_errors_malformed_or_oversize_results_without_leaks() {
        let error = decode(200, br#"{"Code":"noPermission","Message":"secret response"}"#).err().unwrap();
        assert_eq!(error, Error::Authorization);
        assert!(!error.to_string().contains("secret"));
        for data in [br#"{}"#.as_slice(), br#"{"Data":"{}"}"#, br#"{"Data":{}}"#, b"not-json"] {
            assert_eq!(decode(200, data).err(), Some(Error::InvalidResponse));
        }
        assert_eq!(decode(429, b"private").err(), Some(Error::Throttled));
        assert_eq!(decode(503, b"private").err(), Some(Error::Unavailable));
        assert_eq!(decode(200, &vec![0; MAX_RESPONSE_BYTES + 1]).err(), Some(Error::ResponseSize));
    }
}
