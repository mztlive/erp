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
    /// 验证凭据可以安全构造签名请求头。
    /// # 参数
    /// 无；读取当前配置。
    /// # 返回
    /// 配置完整时成功，不联网验证权限。
    /// # 错误
    /// 空白、控制字符、无效 ID 或超长凭据返回脱敏错误。
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
