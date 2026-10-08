//! OpenAI 兼容合同提取配置；凭据只通过 SafeConfig 提供。
use std::fmt;

use serde::Deserialize;
use url::Url;

use crate::{Error, Result};

/// 整节缺省时维持未配置 AI，不读取环境变量或使用默认模型。
#[derive(Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ContractAiConfig {
    /// 非敏感供应商/网关连接标识；切换服务时使用新标识。
    pub provider_id: String,
    pub base_url: String,
    pub api_key: String,
    pub model: String,
    #[serde(default = "default_timeout")]
    pub timeout_seconds: u64,
    #[serde(default = "default_tokens")]
    pub max_output_tokens: u64,
}

fn default_timeout() -> u64 {
    90
}
fn default_tokens() -> u64 {
    8192
}

impl fmt::Debug for ContractAiConfig {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        // 连同可能误填凭据的 URL 和模型配置一起隐藏。
        formatter.write_str("ContractAiConfig { [REDACTED] }")
    }
}

impl ContractAiConfig {
    pub(crate) fn validate(&self) -> Result<()> {
        let invalid = || {
            Error::Invalid(
                "contract_ai requires a valid provider ID, base URL, API key, model and limits".into(),
            )
        };
        let url = Url::parse(&self.base_url).map_err(|_| invalid())?;
        let local = matches!(url.host_str(), Some("localhost" | "127.0.0.1" | "[::1]"));
        if (url.scheme() != "https" && !(url.scheme() == "http" && local))
            || url.host_str().is_none()
            || !url.username().is_empty()
            || url.password().is_some()
            || url.query().is_some()
            || url.fragment().is_some()
            || url.path().trim_end_matches('/').ends_with("/chat/completions")
            || url.path().trim_end_matches('/').ends_with("/responses")
            || self.base_url.trim() != self.base_url
            || self.base_url.len() > 2048
            || self.provider_id.is_empty()
            || self.provider_id.len() > 64
            || !self.provider_id.bytes().all(|byte| byte.is_ascii_alphanumeric() || b"-_.".contains(&byte))
            || self.api_key.is_empty()
            || self.api_key.len() > 8192
            || !self.api_key.bytes().all(|byte| byte.is_ascii_graphic())
            || self.model.is_empty()
            || self.model.len() > 96
            || !self.model.bytes().all(|byte| byte.is_ascii_graphic())
            || !(1..=120).contains(&self.timeout_seconds)
            || !(256..=32768).contains(&self.max_output_tokens)
        {
            return Err(invalid());
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn config() -> ContractAiConfig {
        toml::from_str("provider_id='gateway-a'\nbase_url='https://gateway.example/v1'\napi_key='secret-key'\nmodel='contract-model'")
            .unwrap()
    }
    #[test]
    fn defaults_limits_and_redacts_config() {
        let config = config();
        config.validate().unwrap();
        assert_eq!((config.timeout_seconds, config.max_output_tokens), (90, 8192));
        let debug = format!("{config:?}");
        for value in [&config.provider_id, &config.api_key, &config.base_url, &config.model] {
            assert!(!debug.contains(value));
        }
        assert!(toml::from_str::<ContractAiConfig>("model='model'").is_err());
    }

    #[test]
    fn requires_bounded_non_sensitive_provider_identifier() {
        let missing = "base_url='https://gateway.example/v1'\napi_key='key'\nmodel='model'";
        assert!(toml::from_str::<ContractAiConfig>(missing).is_err());
        for id in [
            "".into(),
            " ".into(),
            "gateway a".into(),
            "a\nb".into(),
            "https://host".into(),
            "网关".into(),
            "a".repeat(65),
        ] {
            let mut config = config();
            config.provider_id = id;
            assert!(config.validate().is_err());
        }
        for id in ["gateway-a_v2.prod".into(), "a".repeat(64)] {
            let mut config = config();
            config.provider_id = id;
            config.validate().unwrap();
        }
    }
    #[test]
    fn rejects_unsafe_urls_and_invalid_limits_without_echoing_values() {
        for base in [
            "http://gateway.example/v1",
            "https://user:secret@host/v1",
            "https://host/v1?key=secret",
            "https://host/#secret",
            "file:///tmp/model",
            " https://host/v1",
            "https://host/v1/chat/completions/",
            "https://api.deepseek.com/responses",
            "https://host/v1/responses/",
        ] {
            let mut config = config();
            config.base_url = base.into();
            let error = config.validate().unwrap_err().to_string();
            assert!(!error.contains(base));
        }
        for base in [
            "http://localhost:8080/v1",
            "http://127.0.0.1:8080/v1",
            "http://[::1]:8080/v1",
            "https://api.deepseek.com",
            "https://api.deepseek.com/",
            "https://gateway.example/v1",
        ] {
            let mut config = config();
            config.base_url = base.into();
            config.validate().unwrap();
        }
        let mut config = config();
        config.api_key = "secret\nheader".into();
        assert!(config.validate().is_err());
        config.api_key = "key".into();
        config.timeout_seconds = 0;
        assert!(config.validate().is_err());
        config.timeout_seconds = 121;
        assert!(config.validate().is_err());
        config.timeout_seconds = 90;
        config.max_output_tokens = 32769;
        assert!(config.validate().is_err());
        config.max_output_tokens = 8192;
        config.model.clear();
        assert!(config.validate().is_err());
    }
}
