//! 京东 OAuth 响应解释与请求编码。
use std::collections::BTreeMap;

use erp_supply::ports::connector::common::ConnectorResult;
use erp_supply::ports::jd_vop::oauth::{JdVopRefreshRequest, JdVopRefreshResult};
use erp_supply::ports::jd_vop::token::token_error;
use serde::Deserialize;
use serde_json::value::RawValue;
use serde_json::{from_slice, from_str};
use url::form_urlencoded::Serializer;

use super::wire::WireText;
#[derive(Deserialize)]
struct TokenData {
    #[serde(alias = "accessToken")]
    access_token: String,
    #[serde(alias = "refreshToken")]
    refresh_token: String,
    #[serde(alias = "expiresIn")]
    expires_in: WireText,
}
type RawObject = BTreeMap<String, Box<RawValue>>;

fn token_data(body: &[u8]) -> ConnectorResult<TokenData> {
    // RawValue 必须由 JSON 反序列化器逐层取得，不能经过 untagged/flatten 的中间 Content。
    let fields: RawObject = from_slice(body).map_err(|_| token_error())?;
    if fields.contains_key("resultCode") || fields.contains_key("success") || fields.contains_key("result") {
        if scalar(&fields, "success")?.0 != "true" || scalar(&fields, "resultCode")?.0 != "0000" {
            return Err(token_error());
        }
        let result = fields.get("result").ok_or_else(token_error)?;
        from_str(result.get()).map_err(|_| token_error())
    } else {
        if scalar(&fields, "code")?.0 != "0" {
            return Err(token_error());
        }
        from_slice(body).map_err(|_| token_error())
    }
}

fn scalar(fields: &RawObject, name: &str) -> ConnectorResult<WireText> {
    let value = fields.get(name).ok_or_else(token_error)?;
    from_str(value.get()).map_err(|_| token_error())
}

pub(super) fn parse_refresh(body: &[u8]) -> ConnectorResult<JdVopRefreshResult> {
    if body.len() > 1024 * 1024 {
        return Err(token_error());
    }
    let result = token_data(body)?;
    let expires_in: u64 = result.expires_in.0.parse().map_err(|_| token_error())?;
    if expires_in == 0
        || [&result.access_token, &result.refresh_token].iter().any(|value| {
            value.trim().is_empty()
                || value.trim() != value.as_str()
                || value.len() > 8192
                || value.chars().any(char::is_control)
        })
    {
        return Err(token_error());
    }
    JdVopRefreshResult::new(result.access_token, result.refresh_token, expires_in)
}
/// 编码刷新参数；只由协议 HTTP 发送，编排不接触请求格式。
pub(super) fn encode_refresh(request: &JdVopRefreshRequest) -> String {
    Serializer::new(String::new())
        .extend_pairs([
            ("app_key", request.app_key()),
            ("app_secret", request.app_secret()),
            ("grant_type", "refresh_token"),
            ("refresh_token", request.refresh_token()),
        ])
        .finish()
}
/// 只将官方明确的会话/刷新令牌失效代码识别为授权撤销；未知正文不推断旧令牌有效或无效。
pub(super) fn authorization_invalid(body: &[u8]) -> bool {
    let Ok(fields) = from_slice::<RawObject>(body) else { return false };
    ["code", "resultCode"]
        .iter()
        .any(|name| scalar(&fields, name).is_ok_and(|code| matches!(code.0.as_str(), "20010" | "20012")))
        || fields
            .get("error")
            .and_then(|value| from_str::<String>(value.get()).ok())
            .is_some_and(|value| value == "invalid_grant")
}

#[cfg(test)]
mod tests {
    use erp_core::common::time::Instant;
    use url::form_urlencoded::parse;

    use super::*;

    #[test]
    fn refresh_request_and_both_documented_response_envelopes_preserve_credentials() {
        let input = JdVopRefreshRequest::new("app", "secret&=", "old+token").unwrap();
        let request = encode_refresh(&input);
        let fields: BTreeMap<_, _> = parse(request.as_bytes()).into_owned().collect();
        assert_eq!(fields["grant_type"], "refresh_token");
        assert_eq!(fields["app_secret"], "secret&=");
        for body in [br#"{"code":0,"access_token":"next","refresh_token":"refresh","expires_in":86400}"#.as_slice(),br#"{"success":true,"resultCode":"0000","result":{"accessToken":"next","refreshToken":"refresh","expiresIn":86400}}"#.as_slice(),br#"{"code":"0","access_token":"next","refresh_token":"refresh","expires_in":"86400"}"#.as_slice(),br#"{"success":"true","resultCode":"0000","result":{"accessToken":"next","refreshToken":"refresh","expiresIn":"86400"}}"#.as_slice()] {
            let result=parse_refresh(body).unwrap();assert_eq!(result.refresh_token(),"refresh");
            let token=result.access_token("account".into(),"version-2".into(),Instant::from_unix_secs(1)).unwrap();
            assert_eq!(token.value_for("account",Instant::from_unix_secs(2)).unwrap(),"next");
        }
    }
    #[test]
    fn malformed_expired_and_failed_authorization_never_exposes_response_body() {
        for body in [
            br#"{"code":19,"error":"private-token"}"#.as_slice(),
            br#"{"code":0,"access_token":"private-token","refresh_token":"r","expires_in":0}"#.as_slice(),
            br#"{"success":false,"resultCode":"0000","result":{"accessToken":"private-token","refreshToken":"r","expiresIn":86400}}"#.as_slice(),
            br#"{"success":true,"resultCode":"20012","result":{"accessToken":"private-token","refreshToken":"r","expiresIn":86400}}"#.as_slice(),
            br#"{"code":19,"access_token":"private-token","refresh_token":"r","expires_in":86400}"#.as_slice(),
            br#"{"success":true,"resultCode":"0000","result":{"accessToken":"private-token","refreshToken":"r","expiresIn":"not-a-duration"}}"#.as_slice(),
            b"private-token".as_slice(),
        ] {
            let error = parse_refresh(body).err().unwrap();
            assert!(!error.summary.contains("private-token"));
        }
    }
}
