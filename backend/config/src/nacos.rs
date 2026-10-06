use std::env;
use std::time::Duration;

use nacos_sdk::api::config::{ConfigService, ConfigServiceBuilder};
use nacos_sdk::api::props::ClientProps;
use tokio::time::timeout;
use tracing::info;
use url::Url;

use crate::{Error, Result};

const REQUEST_TIMEOUT: Duration = Duration::from_secs(30);

#[derive(Clone)]
pub(crate) struct NacosConfig {
    addr: String,
    namespace: String,
    group: String,
    data_id: String,
}

impl NacosConfig {
    pub(crate) fn new(addr: &str, namespace: &str, group: &str, data_id: &str) -> Result<Self> {
        validate_address(addr)?;
        let is_uuid = namespace.len() == 36
            && namespace.bytes().enumerate().all(|(i, byte)| {
                if [8, 13, 18, 23].contains(&i) { byte == b'-' } else { byte.is_ascii_hexdigit() }
            });
        if !is_uuid {
            return Err(Error::Invalid("nacos_namespace 必须是 Namespace UUID".into()));
        }
        for (name, value) in [("nacos_group", group), ("nacos_data_id", data_id)] {
            if value.is_empty() || value.trim() != value || value.chars().any(char::is_control) {
                return Err(Error::Invalid(format!("{name} 必须非空且不含首尾空白或控制字符")));
            }
        }
        Ok(Self {
            addr: addr.into(),
            namespace: namespace.into(),
            group: group.into(),
            data_id: data_id.into(),
        })
    }
}

fn validate_address(addr: &str) -> Result<()> {
    let invalid =
        || Error::Invalid("nacos_addr 必须是 host:port 客户端地址（通常为 8848），不能使用 URL".into());
    if addr.is_empty() || addr.contains("://") || addr.chars().any(char::is_whitespace) {
        return Err(invalid());
    }
    let parsed = Url::parse(&format!("http://{addr}")).map_err(|_| invalid())?;
    if parsed.host_str().is_none()
        || parsed.port().is_none_or(|port| port == 0 || port > 64535)
        || !parsed.username().is_empty()
        || parsed.password().is_some()
        || parsed.path() != "/"
        || parsed.query().is_some()
        || parsed.fragment().is_some()
        || addr.ends_with('/')
    {
        return Err(invalid());
    }
    Ok(())
}

// 不实现 Debug，防止凭据经结构化日志输出。
struct Credentials {
    username: String,
    password: String,
}

impl Credentials {
    fn from_env() -> Result<Self> {
        Self::read(|key| env::var(key).ok())
    }

    fn read(mut value: impl FnMut(&str) -> Option<String>) -> Result<Self> {
        let mut required = |name| {
            value(name)
                .filter(|v| !v.trim().is_empty())
                .ok_or_else(|| Error::Invalid(format!("必须设置非空环境变量 {name}")))
        };
        Ok(Self {
            username: required("NACOS_CLIENT_USERNAME")?,
            password: required("NACOS_CLIENT_PASSWORD")?,
        })
    }
}

/// 只读 Nacos 配置客户端；启动失败不使用本地缓存或文件。
#[derive(Clone)]
pub(crate) struct NacosConfigClient {
    config: NacosConfig,
    service: ConfigService,
}

impl NacosConfigClient {
    pub(crate) async fn from_config(config: NacosConfig) -> Result<Self> {
        let credentials = Credentials::from_env()?;
        // 禁止 SDK 的环境优先级覆盖已校验的地址、Namespace 或缓存策略。
        let props = ClientProps::new()
            .env_first(false)
            .load_cache_at_start(false)
            .server_addr(&config.addr)
            .namespace(&config.namespace)
            .app_name("erp")
            .auth_username(credentials.username)
            .auth_password(credentials.password);
        let service =
            timeout(REQUEST_TIMEOUT, ConfigServiceBuilder::new(props).enable_auth_plugin_http().build())
                .await
                .map_err(|_| Error::NacosTimeout)??;
        info!(server = %config.addr, namespace = %config.namespace, group = %config.group,
            data_id = %config.data_id, "Nacos 配置客户端已连接");
        Ok(Self { config, service })
    }

    pub(crate) async fn fetch(&self) -> Result<String> {
        let response = timeout(
            REQUEST_TIMEOUT,
            self.service.get_config(self.config.data_id.clone(), self.config.group.clone()),
        )
        .await
        .map_err(|_| Error::NacosTimeout)??;
        if response.content().trim().is_empty() {
            return Err(Error::Invalid("Nacos 配置内容不能为空".into()));
        }
        Ok(response.content().clone())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const NAMESPACE: &str = "ccf7ec38-1d60-407e-bf2c-7c4654c481d0";

    #[test]
    fn validates_explicit_configuration_identity() {
        assert!(
            NacosConfig::new("nacos.infra.svc.cluster.local:8848", NAMESPACE, "DEFAULT_GROUP", "erp").is_ok()
        );
        for addr in [
            "",
            "localhost",
            "http://localhost:8848",
            "localhost:8848/",
            "user:secret@localhost:8848",
            "localhost:65535",
            "localhost:0",
        ] {
            assert!(NacosConfig::new(addr, NAMESPACE, "DEFAULT_GROUP", "erp").is_err());
        }
        for namespace in ["", "public", "test", "prod"] {
            assert!(NacosConfig::new("localhost:8848", namespace, "DEFAULT_GROUP", "erp").is_err());
        }
        for (group, data_id) in [("", "erp"), ("DEFAULT_GROUP", ""), (" g", "erp"), ("g", "e\nrp")] {
            assert!(NacosConfig::new("localhost:8848", NAMESPACE, group, data_id).is_err());
        }
    }

    #[test]
    fn credentials_require_both_values_without_trimming_password() {
        let credentials =
            Credentials::read(|key| Some(if key.ends_with("USERNAME") { "test" } else { " secret " }.into()))
                .unwrap();
        assert_eq!(credentials.username, "test");
        assert_eq!(credentials.password, " secret ");
        for missing in ["NACOS_CLIENT_USERNAME", "NACOS_CLIENT_PASSWORD"] {
            for value in [None, Some("".to_string()), Some("  ".to_string())] {
                let result =
                    Credentials::read(
                        |key| if key == missing { value.clone() } else { Some("secret".into()) },
                    );
                let err = result.err().unwrap();
                assert!(err.to_string().contains(missing));
                assert!(!format!("{err:?}").contains("secret"));
            }
        }
    }
}
