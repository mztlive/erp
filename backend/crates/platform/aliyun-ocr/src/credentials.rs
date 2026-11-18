use std::fmt;

use serde::Deserialize;

use crate::{Error, Result};

/// 长期或临时凭据；仅由部署配置提供，调试输出始终脱敏。
#[derive(Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Credentials {
    pub access_key_id: String,
    pub access_key_secret: String,
    #[serde(default)]
    pub security_token: Option<String>,
}

impl Credentials {
    /// 在加载配置或创建客户端时校验 AK/SK 与可选 STS token 的格式，避免用非法头值构造签名请求。
    /// ID 必须是 1～128 个 ASCII 字母或数字；密钥和 token 必须是非空的可打印 ASCII，
    /// 不含空格且不超过 16384 字节。此检查不联网，不验证权限或 token 是否过期。
    pub fn validate(&self) -> Result<()> {
        if self.access_key_id.is_empty()
            || self.access_key_id.len() > 128
            || !self.access_key_id.bytes().all(|b| b.is_ascii_alphanumeric())
            || !valid_secret(&self.access_key_secret)
            || self.security_token.as_ref().is_some_and(|value| !valid_secret(value))
        {
            return Err(Error::Configuration);
        }
        Ok(())
    }
}

// 本地凭据检查共用此规则：SK 和 STS token 必须是 1～16384 字节的可打印 ASCII，
// 不接受空格或控制字符，以免把非法材料带入签名或 HTTP 请求头。
fn valid_secret(value: &str) -> bool {
    !value.is_empty() && value.len() <= 16384 && value.bytes().all(|b| (33..=126).contains(&b))
}

impl fmt::Debug for Credentials {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Credentials")
            .field("access_key_id", &"<redacted>")
            .field("access_key_secret", &"<redacted>")
            .field("security_token", &"<redacted>")
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn credentials_validate_and_debug_never_discloses_secrets() {
        let mut value = Credentials {
            access_key_id: "LTAItest123".into(),
            access_key_secret: "secret123".into(),
            security_token: Some("token123".into()),
        };
        value.validate().unwrap();

        let debug = format!("{value:?}");
        for secret in ["LTAItest123", "secret123", "token123"] {
            assert!(!debug.contains(secret));
        }

        for bad in ["", " a", "a\r\nb", "a\tb"] {
            value.access_key_secret = bad.into();
            assert_eq!(value.validate(), Err(Error::Configuration));
        }
    }
}
