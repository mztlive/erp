//! 蛋糕叔叔协议配置；连接身份、地区与计量映射由服务端固定。
use std::collections::{BTreeMap, BTreeSet};
use std::fmt;

use serde::Deserialize;
use url::Url;

use crate::{Error, Result};

#[derive(Debug, Clone, Copy, Default, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SupplierTimestampUnit {
    Seconds,
    #[default]
    Milliseconds,
}

/// 缺省或 enabled=false 时关闭；启用后必须补全身份、密钥及超时。
#[derive(Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DangaoshushuConfig {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default)]
    pub connection_id: String,
    #[serde(default)]
    pub supplier_id: String,
    #[serde(default = "testing")]
    pub environment: String,
    #[serde(default = "testing_url")]
    pub base_url: String,
    #[serde(default)]
    pub channel_no: String,
    #[serde(default)]
    pub private_key: String,
    #[serde(default)]
    pub user_id: String,
    #[serde(default)]
    pub timestamp_unit: SupplierTimestampUnit,
    #[serde(default = "timeout")]
    pub timeout_seconds: u64,
    #[serde(default = "skew")]
    pub callback_max_skew_seconds: u64,
    #[serde(default = "requests")]
    pub requests_per_second: u32,
    /// 采购核实 clearing_price 为人民币含税供货价后才能打开。
    #[serde(default)]
    pub clearing_price_is_tax_inclusive_cny: bool,
    /// 供应商 spec_id -> 与公司 SKU 一致的计量单位；缺失不猜测。
    #[serde(default)]
    pub spec_units: BTreeMap<String, String>,
    /// 供应商 city_id -> 公司标准地区编码；禁止直接混用 ID。
    #[serde(default)]
    pub city_regions: BTreeMap<String, String>,
}

fn testing() -> String {
    "testing".into()
}
fn testing_url() -> String {
    "https://dev.dangaoss.cn".into()
}
fn timeout() -> u64 {
    15
}
fn skew() -> u64 {
    300
}
fn requests() -> u32 {
    5
}

impl fmt::Debug for DangaoshushuConfig {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("DangaoshushuConfig { [REDACTED] }")
    }
}

impl DangaoshushuConfig {
    /// 校验启用配置；关闭的模板允许保留空凭据。
    ///
    /// # 参数
    /// 无。
    /// # 返回
    /// 配置满足启用边界时返回 Ok。
    /// # 错误
    /// 非法地址、身份、凭据、限额或映射返回脱敏配置错误。
    pub fn validate(&self) -> Result<()> {
        if !self.enabled {
            return Ok(());
        }
        let invalid = || {
            Error::Invalid("dangaoshushu requires valid connection, supplier, environment, HTTPS origin, credentials and limits".into())
        };
        let url = Url::parse(&self.base_url).map_err(|_| invalid())?;
        if url.scheme() != "https"
            || url.host_str().is_none()
            || !url.username().is_empty()
            || url.password().is_some()
            || url.query().is_some()
            || url.fragment().is_some()
            || !matches!(url.path(), "" | "/")
            || self.base_url.trim() != self.base_url
            || self.base_url.len() > 2048
            || !matches!(self.environment.as_str(), "testing" | "production")
            || (self.environment == "production" && url.host_str() == Some("dev.dangaoss.cn"))
            || [self.connection_id.as_str(), self.supplier_id.as_str(), self.channel_no.as_str()]
                .iter()
                .any(|value| !identifier(value))
            || self.private_key.is_empty()
            || self.private_key.len() > 8192
            || !self.private_key.bytes().all(|byte| byte.is_ascii_graphic())
            || self.private_key.starts_with('<')
            || self.private_key.starts_with("replace-with-")
            || (!self.user_id.is_empty() && !identifier(&self.user_id))
            || !(1..=30).contains(&self.timeout_seconds)
            || !(1..=900).contains(&self.callback_max_skew_seconds)
            || !(1..=100).contains(&self.requests_per_second)
        {
            return Err(invalid());
        }
        self.validate_maps()
    }

    fn validate_maps(&self) -> Result<()> {
        let valid_units = self.spec_units.iter().all(|(id, unit)| {
            identifier(id) && !unit.trim().is_empty() && unit.trim() == unit && unit.chars().count() <= 32
        });
        let mut regions = BTreeSet::new();
        let valid_regions = self
            .city_regions
            .iter()
            .all(|(id, region)| identifier(id) && identifier(region) && regions.insert(region));
        if !valid_units || !valid_regions {
            return Err(Error::Invalid(
                "dangaoshushu requires explicit, unambiguous unit and region mappings".into(),
            ));
        }
        Ok(())
    }
}

fn identifier(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 128
        && value.bytes().all(|byte| byte.is_ascii_alphanumeric() || b"-_.".contains(&byte))
        && !value.starts_with("replace-with-")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn configured() -> DangaoshushuConfig {
        toml::from_str("enabled=true\nconnection_id='connection-1'\nsupplier_id='supplier-1'\nchannel_no='channel-1'\nprivate_key='test-secret'").unwrap()
    }

    #[test]
    fn disabled_template_and_enabled_defaults_are_valid() {
        let disabled: DangaoshushuConfig = toml::from_str("enabled=false").unwrap();
        disabled.validate().unwrap();
        let config = configured();
        config.validate().unwrap();
        assert_eq!(config.timeout_seconds, 15);
        assert!(format!("{config:?}").contains("REDACTED"));
        assert!(!format!("{config:?}").contains("test-secret"));
        let mut config = disabled;
        config.enabled = true;
        assert!(config.validate().is_err());
    }

    #[test]
    fn rejects_unsafe_urls_limits_and_ambiguous_regions() {
        for url in [
            "http://dev.dangaoss.cn",
            "https://user:secret@host",
            "https://host/path",
            "https://host?key=secret",
        ] {
            let mut config = configured();
            config.base_url = url.into();
            assert!(config.validate().is_err());
        }
        let mut config = configured();
        config.timeout_seconds = 0;
        assert!(config.validate().is_err());
        let mut config = configured();
        config.environment = "production".into();
        assert!(config.validate().is_err());
        let mut config = configured();
        config.city_regions = BTreeMap::from([("2".into(), "110100".into()), ("3".into(), "110100".into())]);
        assert!(config.validate().is_err());
    }
}
