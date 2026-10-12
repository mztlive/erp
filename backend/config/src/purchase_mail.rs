//! 采购邮件配置；只通过 SafeConfig 加载，不记录凭据。
use std::fmt;

use serde::Deserialize;
use url::Url;

use crate::{Error, Result};

/// Resend 兼容 HTTP 发件服务。缺省整节时禁止发送。
#[derive(Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PurchaseMailConfig {
    pub endpoint: String,
    pub api_key: String,
    pub from: String,
    #[serde(default = "timeout")]
    pub timeout_seconds: u64,
}
fn timeout() -> u64 {
    20
}
impl fmt::Debug for PurchaseMailConfig {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("PurchaseMailConfig { [REDACTED] }")
    }
}
impl PurchaseMailConfig {
    pub(crate) fn validate(&self) -> Result<()> {
        let invalid = || {
            Error::Invalid("purchase_mail requires HTTPS endpoint, API key, sender and timeout 1..60".into())
        };
        let url = Url::parse(&self.endpoint).map_err(|_| invalid())?;
        if url.scheme() != "https"
            || url.host_str().is_none()
            || !url.username().is_empty()
            || url.password().is_some()
            || url.query().is_some()
            || url.fragment().is_some()
            || self.api_key.is_empty()
            || !self.api_key.bytes().all(|b| b.is_ascii_graphic())
            || self.from.trim().is_empty()
            || !self.from.contains('@')
            || self.from.contains(['\r', '\n'])
            || !(1..=60).contains(&self.timeout_seconds)
        {
            return Err(invalid());
        }
        Ok(())
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn validates_transport_and_redacts_credentials() {
        let mut config = PurchaseMailConfig {
            endpoint: "https://api.resend.com/emails".into(),
            api_key: "test-key".into(),
            from: "orders@example.com".into(),
            timeout_seconds: 20,
        };
        assert!(config.validate().is_ok());
        assert!(!format!("{config:?}").contains("test-key"));
        config.endpoint = "http://example.com".into();
        assert!(config.validate().is_err());
    }
}
