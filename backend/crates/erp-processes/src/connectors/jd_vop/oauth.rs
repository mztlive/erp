//! 官方 OAuth 刷新参数及响应模型；网络及版本化持久化由凭据流程执行。
//! 依据 https://vop.jd.com/doc/guide?id=a3f98cc6-0eca-483d-b459-fd30c224ae4d 。
use std::collections::BTreeMap;

use erp_core::common::time::Instant;
use erp_supply::ports::connector::common::ConnectorResult;
use serde::Deserialize;
use serde_json::value::RawValue;
use serde_json::{from_slice, from_str};
use url::form_urlencoded::Serializer;

use super::token::{JdVopAccessToken, token_error};
use super::wire::WireText;

/// 含密钥的已编码刷新参数；不提供 Debug，不得进入日志或业务响应。
pub struct JdVopRefreshRequest {
    encoded: String,
}

impl JdVopRefreshRequest {
    /// 按官方 refresh_token 授权方式编码参数。
    /// # 参数
    /// 应用标识、应用密钥和受控存储中的原刷新令牌。
    /// # 返回
    /// 供凭据拥有服务受控发送的参数，不自行触发网络。
    /// # 错误
    /// 凭据缺失或含控制字符时返回脱敏错误。
    pub fn new(app_key: &str, app_secret: &str, refresh_token: &str) -> ConnectorResult<Self> {
        if [app_key, app_secret, refresh_token]
            .iter()
            .any(|v| v.trim().is_empty() || v.len() > 8192 || v.chars().any(char::is_control))
        {
            return Err(token_error());
        }
        let encoded = Serializer::new(String::new())
            .extend_pairs([
                ("app_key", app_key),
                ("app_secret", app_secret),
                ("grant_type", "refresh_token"),
                ("refresh_token", refresh_token),
            ])
            .finish();
        Ok(Self { encoded })
    }
    /// 返回官方刷新端点。
    /// # 参数
    /// 无。
    /// # 返回
    /// 不含密钥和令牌的固定 HTTPS 地址。
    /// # 错误
    /// 不返回错误。
    pub fn endpoint(&self) -> &'static str {
        "https://open-oauth.jd.com/oauth2/refresh_token"
    }
    /// 借用已编码的敏感刷新参数，调用方须按原串发送并脱敏。
    /// # 参数
    /// 无。
    /// # 返回
    /// 包含密钥和刷新令牌的参数串。
    /// # 错误
    /// 不返回错误。
    pub fn parameters(&self) -> &str {
        &self.encoded
    }
}

/// 尚未写回凭据存储的新令牌；持久化必须使用原版本进行 CAS，并确保账号不变。
pub struct JdVopRefreshResult {
    access_token: String,
    refresh_token: String,
    expires_in: u64,
}

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

impl JdVopRefreshResult {
    pub(super) fn into_credential_parts(
        self,
        observed_at: Instant,
    ) -> ConnectorResult<(String, String, Instant)> {
        let seconds = i64::try_from(self.expires_in).map_err(|_| token_error())?;
        let expires = observed_at.unix_secs().checked_add(seconds).ok_or_else(token_error)?;
        let expires = Instant::try_from_unix_secs(expires).map_err(|_| token_error())?;
        Ok((self.access_token, self.refresh_token, expires))
    }
    /// 解释官方授权响应，不按 HTTP 成功推导授权成功。
    /// # 参数
    /// `body` 是刷新端点或官方 VOP 包装的完整响应字节。
    /// # 返回
    /// 待凭据服务核对账号并按原版本原子保存的令牌结果。
    /// # 错误
    /// 未知错误码、缺失字段或有效期非法时返回脱敏鉴权错误。
    pub fn parse(body: &[u8]) -> ConnectorResult<Self> {
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
        Ok(Self { access_token: result.access_token, refresh_token: result.refresh_token, expires_in })
    }
    /// 取得待受控持久化的刷新令牌，不得写入日志。
    /// # 参数
    /// 无。
    /// # 返回
    /// 新刷新令牌原文。
    /// # 错误
    /// 不返回错误。
    pub fn refresh_token(&self) -> &str {
        &self.refresh_token
    }
    /// 将明确账号及存储版本绑定到新访问令牌，不自行写回存储。
    /// # 参数
    /// `account_scope`、`new_version` 来自凭据拥有服务；`observed_at` 为响应取得时刻。
    /// # 返回
    /// 有界计算有效期的令牌快照，供同一受控 CAS 保存。
    /// # 错误
    /// 时间溢出、账号或版本无效时返回脱敏错误。
    pub fn access_token(
        &self,
        account_scope: String,
        new_version: String,
        observed_at: Instant,
    ) -> ConnectorResult<JdVopAccessToken> {
        let seconds = i64::try_from(self.expires_in).map_err(|_| token_error())?;
        let expires = observed_at.unix_secs().checked_add(seconds).ok_or_else(token_error)?;
        let expires = Instant::try_from_unix_secs(expires).map_err(|_| token_error())?;
        JdVopAccessToken::new(self.access_token.clone(), new_version, account_scope, expires)
    }
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
    use url::form_urlencoded::parse;

    use super::*;

    #[test]
    fn refresh_request_and_both_documented_response_envelopes_preserve_credentials() {
        let request = JdVopRefreshRequest::new("app", "secret&=", "old+token").unwrap();
        assert_eq!(request.endpoint(), "https://open-oauth.jd.com/oauth2/refresh_token");
        let fields: BTreeMap<_, _> = parse(request.parameters().as_bytes()).into_owned().collect();
        assert_eq!(fields["grant_type"], "refresh_token");
        assert_eq!(fields["app_secret"], "secret&=");
        for body in [br#"{"code":0,"access_token":"next","refresh_token":"refresh","expires_in":86400}"#.as_slice(),br#"{"success":true,"resultCode":"0000","result":{"accessToken":"next","refreshToken":"refresh","expiresIn":86400}}"#.as_slice(),br#"{"code":"0","access_token":"next","refresh_token":"refresh","expires_in":"86400"}"#.as_slice(),br#"{"success":"true","resultCode":"0000","result":{"accessToken":"next","refreshToken":"refresh","expiresIn":"86400"}}"#.as_slice()] {
            let result=JdVopRefreshResult::parse(body).unwrap();assert_eq!(result.refresh_token(),"refresh");
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
            let error = JdVopRefreshResult::parse(body).err().unwrap();
            assert!(!error.summary.contains("private-token"));
        }
    }
}
