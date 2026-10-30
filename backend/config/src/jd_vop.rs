//! 京东 VOP 技术参数；认证协议核验不能由配置开关代替。
use std::fmt;

use serde::Deserialize;
use url::Url;

use crate::{Error, Result};
use crate::jd_vop_execution::{JdVopExecutionConfig, JdVopMessageConfig};

/// 部署负责账号、Mongo 凭据身份及配额，业务连接不保存京东密钥或令牌。
#[derive(Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct JdVopConfig {
    /// 已核实账号消息范围；缺省不拉取和确认消息。
    #[serde(default)]
    pub messages: Option<JdVopMessageConfig>,
    /// 已核实订单执行资料；缺省保留只读装配。
    #[serde(default)]
    pub execution: Option<JdVopExecutionConfig>,
    #[serde(default)]
    pub enabled: bool,
    #[serde(default = "gateway")]
    pub gateway_url: String,
    #[serde(default)]
    pub app_key: String,
    #[serde(default)]
    pub app_secret: String,
    /// 集成域 Mongo 凭据记录的稳定身份，不接受 URI、Redis key 或令牌正文。
    #[serde(default, alias = "access_token_reference")]
    pub credential_id: String,
    /// 仅用于京东凭据密文的独立部署密钥；不得复用应用签名密钥。
    #[serde(default)]
    pub credential_encryption_key: String,
    /// 合同采购账号的稳定命名空间；更换账号必须更换此值。
    #[serde(default)]
    pub account_scope: String,
    /// 业务调用渠道；默认采用官方接口默认值 3，确有接入差异时覆盖。
    #[serde(default = "source_type")]
    pub source_type: u32,
    /// 官方资料或账号联调已核实的相对图片路径基地址；缺失不猜测 CDN。
    #[serde(default)]
    pub image_base_url: Option<String>,
    #[serde(default = "timeout")]
    pub timeout_seconds: u64,
    #[serde(default = "concurrency")]
    pub max_concurrent_reads: u32,
    #[serde(default = "rate")]
    pub requests_per_second: u32,
    #[serde(default = "sync_interval")]
    pub catalog_sync_interval_seconds: u64,
    #[serde(default = "full_interval")]
    pub catalog_full_sync_interval_seconds: u64,
    #[serde(default = "poll_interval")]
    pub message_poll_interval_seconds: u64,
}

fn gateway() -> String {
    "https://api.jd.com/routerjson".into()
}

fn source_type() -> u32 {
    3
}

fn timeout() -> u64 {
    15
}
fn concurrency() -> u32 {
    2
}
fn rate() -> u32 {
    5
}
fn sync_interval() -> u64 {
    3600
}
fn full_interval() -> u64 {
    86_400
}
fn poll_interval() -> u64 {
    60
}

impl fmt::Debug for JdVopConfig {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("JdVopConfig { [REDACTED] }")
    }
}

impl JdVopConfig {
    /// 校验启用参数；关闭模板允许空值，未知认证或写入开关一律拒绝反序列化。
    ///
    /// # 参数
    /// 无。
    /// # 返回
    /// 参数满足部署边界时返回 `Ok(())`。
    /// # 错误
    /// 地址、凭据身份、调用渠道或配额无效时返回脱敏配置错误。
    pub fn validate(&self) -> Result<()> {
        if !self.enabled {
            return Ok(());
        }
        self.validate_credential_settings()?;
        self.validate_runtime()?;
        self.validate_execution_binding()
    }

    /// 校验 Mongo 凭据配置，允许停用业务连接时受控导入或查询凭据。
    /// # 参数
    /// 无；仅检查部署账号、稳定凭据身份和专用加密密钥。
    /// # 返回
    /// enabled 为 false 时也执行相同凭据校验，不执行外部调用。
    /// # 错误
    /// 身份非法、密钥不足三十二字节或复用应用凭据密钥时返回脱敏错误。
    pub fn validate_credential_settings(&self) -> Result<()> {
        if !identifier(&self.app_key)
            || !secret(&self.app_secret)
            || !identifier(&self.account_scope)
            || !credential_id(&self.credential_id)
            || !encryption_key(&self.credential_encryption_key)
            || self.credential_encryption_key == self.app_secret
            || !(1..=30).contains(&self.timeout_seconds)
        {
            return Err(Error::Invalid(
                "jd_vop requires app credentials, account_scope, Mongo credential_id and an independent credential_encryption_key of at least 32 bytes".into(),
            ));
        }
        Ok(())
    }

    /// 禁止以应用登录密钥保护京东长期凭据。
    /// # 参数
    /// `application_secret` 为同一 SafeConfig 快照中的应用密钥。
    /// # 返回
    /// 独立密钥或尚未填写的停用模板返回成功，是否缺失由凭据校验负责。
    /// # 错误
    /// 已填写凭据密钥与应用密钥相同时返回脱敏错误。
    pub(crate) fn ensure_independent_key(&self, application_secret: &str) -> Result<()> {
        if !self.credential_encryption_key.is_empty() && self.credential_encryption_key == application_secret
        {
            return Err(Error::Invalid(
                "jd_vop credential_encryption_key must be independent from app.secret".into(),
            ));
        }
        Ok(())
    }

    fn validate_execution_binding(&self) -> Result<()> {
        let valid = |evidence: &str, account: &str, app: &str| {
            !evidence.trim().is_empty() && evidence.len() <= 1024
                && evidence.trim() == evidence && !evidence.chars().any(char::is_control)
                && account == self.account_scope && app == self.app_key
        };
        if self.messages.as_ref().is_some_and(|m| !valid(&m.evidence_reference, &m.account_scope, &m.app_key)
            || m.types.is_empty() || m.types.len() > 100 || m.types.contains(&0))
            || self.execution.as_ref().is_some_and(|e| !valid(&e.evidence_reference, &e.account_scope, &e.app_key)
                || !identifier(&e.external_funds_account_ref) || e.ordinary_skus.is_empty()
                || e.ordinary_skus.iter().any(|sku| sku.parse::<u64>().is_err() || sku == "0")
                || !(1..=300).contains(&e.preflight_validity_seconds))
        {
            return Err(Error::Invalid("jd_vop execution or messages require verified evidence and matching account/app identity".into()));
        }
        Ok(())
    }

    fn validate_runtime(&self) -> Result<()> {
        let invalid = || Error::Invalid("jd_vop requires HTTPS gateway and valid runtime limits".into());
        let url = Url::parse(&self.gateway_url).map_err(|_| invalid())?;
        if url.scheme() != "https"
            || url.host_str().is_none()
            || !url.username().is_empty()
            || url.password().is_some()
            || url.query().is_some()
            || url.fragment().is_some()
            || self.gateway_url.trim() != self.gateway_url
            || self.gateway_url.len() > 2048
            || self.source_type == 0
            || self.source_type > u32::try_from(i32::MAX).expect("Java int正上限可表示为u32")
            || !(1..=30).contains(&self.timeout_seconds)
            || !(1..=20).contains(&self.max_concurrent_reads)
            || !(1..=100).contains(&self.requests_per_second)
            || !(60..=604_800).contains(&self.catalog_sync_interval_seconds)
            || self.catalog_full_sync_interval_seconds < self.catalog_sync_interval_seconds
            || self.catalog_full_sync_interval_seconds > 604_800
            || !(10..=120).contains(&self.message_poll_interval_seconds)
            || self.image_base_url.as_deref().is_some_and(|value| !image_base(value))
        {
            return Err(invalid());
        }
        Ok(())
    }
}

fn image_base(value: &str) -> bool {
    value.len() <= 2048
        && value.ends_with('/')
        && Url::parse(value).is_ok_and(|url| {
            url.scheme() == "https"
                && url.host_str().is_some()
                && url.username().is_empty()
                && url.password().is_none()
                && url.query().is_none()
                && url.fragment().is_none()
        })
}

fn identifier(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 256
        && value.bytes().all(|byte| byte.is_ascii_alphanumeric() || b"_-.:".contains(&byte))
        && !value.starts_with("replace-with-")
}

fn secret(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 8192
        && value.bytes().all(|byte| byte.is_ascii_graphic())
        && !value.starts_with('<')
        && !value.starts_with("replace-with-")
}

fn encryption_key(value: &str) -> bool {
    (32..=8192).contains(&value.len())
        && value.trim() == value
        && !value.chars().any(char::is_control)
        && !value.starts_with('<')
        && !value.starts_with("replace-with-")
}

fn credential_id(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 128
        && value.bytes().all(|byte| byte.is_ascii_alphanumeric() || b"_-".contains(&byte))
        && !value.starts_with("replace-with-")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn configured() -> JdVopConfig {
        toml::from_str("enabled=true\napp_key='app-1'\napp_secret='never-print-this-secret'\ncredential_id='account-1-token'\ncredential_encryption_key='offline-test-only-independent-key-32bytes'\naccount_scope='account-1'").unwrap()
    }

    #[test]
    fn disabled_template_and_verified_gateway_shape_are_accepted() {
        let empty: JdVopConfig = toml::from_str("enabled=false").unwrap();
        assert!(empty.validate().is_ok());
        assert!(empty.validate_credential_settings().is_err());
        assert!(configured().validate().is_ok());
        assert_eq!(configured().gateway_url, "https://api.jd.com/routerjson");
        let debug = format!("{:?}", configured());
        assert!(!debug.contains("never-print"));
        assert!(!debug.contains("account-1"));
        assert!(!debug.contains("independent-key"));
    }

    #[test]
    fn invalid_gateway_and_limits_do_not_expose_credentials() {
        for gateway in [
            "http://gateway.example",
            "https://user:secret@gateway.example",
            "https://gateway.example/?token=secret",
            "https://gateway.example/#secret",
        ] {
            let mut settings = configured();
            settings.gateway_url = gateway.into();
            let failure = settings.validate().unwrap_err().to_string();
            assert!(!failure.contains("never-print"));
            assert!(!failure.contains("token=secret"));
        }
        let mut settings = configured();
        settings.max_concurrent_reads = 0;
        assert!(settings.validate().is_err());
        assert!(toml::from_str::<JdVopConfig>("enabled=false\nallow_unverified_signing=true").is_err());
    }

    #[test]
    fn disabled_credential_import_does_not_need_enabled_business() {
        let mut settings = configured();
        settings.enabled = false;
        assert!(settings.validate_credential_settings().is_ok());
        settings.credential_encryption_key = settings.app_secret.clone();
        assert!(settings.validate_credential_settings().is_err());
    }

    #[test]
    fn credential_cipher_key_cannot_reuse_application_signing_secret() {
        let settings = configured();
        assert!(settings.ensure_independent_key("different-application-signing-secret").is_ok());
        assert!(settings.ensure_independent_key(&settings.credential_encryption_key).is_err());
    }

    #[test]
    fn legacy_field_alias_accepts_only_mongo_identity_and_rejects_duplicate_names() {
        let alias: JdVopConfig = toml::from_str("access_token_reference='account-1-token'").unwrap();
        assert_eq!(alias.credential_id, "account-1-token");
        assert!(toml::from_str::<JdVopConfig>("credential_id='one'\naccess_token_reference='two'").is_err());
        for value in ["vault://account/token", "redis:key", "../token", "token with space"] {
            let mut settings = configured();
            settings.credential_id = value.into();
            assert!(settings.validate_credential_settings().is_err());
        }
    }

    #[test]
    fn enabled_business_accepts_default_channel_without_system_injected_context() {
        let settings = configured();
        assert_eq!(settings.source_type, 3);
        assert!(settings.validate().is_ok());
    }

    #[test]
    fn custom_channel_must_fit_positive_java_integer() {
        for source in [1, 4, u32::try_from(i32::MAX).unwrap()] {
            let mut settings = configured();
            settings.source_type = source;
            assert!(settings.validate().is_ok());
        }
        for source in [0, u32::MAX] {
            let mut settings = configured();
            settings.source_type = source;
            assert!(settings.validate().is_err());
        }
    }

    #[test]
    fn obsolete_system_context_is_rejected_instead_of_silently_ignored() {
        assert!(toml::from_str::<JdVopConfig>("enabled=false\n[request_context]\npin='obsolete'").is_err());
    }
}
