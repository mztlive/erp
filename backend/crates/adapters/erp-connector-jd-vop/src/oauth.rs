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
    JdVopRefreshResult::new(result.access_token, result.refresh_token, expires_in)
}
// 刷新传输发送请求前，把应用凭据和原刷新令牌编码成京东查询参数；凭据编排不依赖这层格式。
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
// 刷新响应分类时，只以明确的会话或刷新令牌失效代码判定授权撤销；未知正文不足以判断旧令牌是否仍有效。
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
    fn refresh_validation_preserves_token_limits_and_duration_boundaries_in_both_envelopes() {
        for wrapped in [false, true] {
            for (access, refresh, seconds, accepted) in [
                ("a".repeat(8192), "r".repeat(8192), 1_u64, true),
                ("a".into(), "r".into(), u64::MAX, true),
                ("a".into(), "r".into(), 0, false),
                (String::new(), "r".into(), 1, false),
                ("a".into(), String::new(), 1, false),
                (" a".into(), "r".into(), 1, false),
                ("a".into(), "r ".into(), 1, false),
                ("a\n".into(), "r".into(), 1, false),
                ("a".into(), "r\u{007f}".into(), 1, false),
                ("a".repeat(8193), "r".into(), 1, false),
                ("a".into(), "r".repeat(8193), 1, false),
            ] {
                let tokens = serde_json::json!({
                    "access_token": access, "refresh_token": refresh, "expires_in": seconds
                });
                let body = if wrapped {
                    serde_json::json!({"success": true, "resultCode": "0000", "result": tokens})
                } else {
                    serde_json::json!({"code": 0, "access_token": access,
                        "refresh_token": refresh, "expires_in": seconds})
                };

                let result = parse_refresh(&serde_json::to_vec(&body).unwrap());
                if accepted {
                    let result = result.unwrap();
                    assert_eq!(result.refresh_token(), refresh);
                    let parts = result.into_credential_parts(Instant::from_unix_secs(0));
                    if seconds == u64::MAX {
                        assert_eq!(parts.unwrap_err(), token_error());
                    } else {
                        let (actual_access, actual_refresh, expires) = parts.unwrap();
                        assert_eq!(actual_access, access);
                        assert_eq!(actual_refresh, refresh);
                        assert_eq!(expires, Instant::from_unix_secs(1));
                    }
                } else {
                    assert_eq!(result.err().unwrap(), token_error());
                }
            }
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
