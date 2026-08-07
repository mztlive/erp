//! # 配置管理模块
//!
//! 本模块为应用程序提供全面的配置管理功能,
//! 支持基于文件和基于 Nacos 的配置源。
//!
//! ## 功能特性
//!
//! * 从 TOML 文件加载配置
//! * 集成 Nacos 配置中心
//! * 通过 Tokio watch channel 提供线程安全的配置快照与订阅
//! * 使用 Nacos 时支持配置热重载
//! * 通过命令行参数解析选择配置源
//!
//! ## 使用方法
//!
//! ### 基于文件的配置
//!
//! ```no_run
//! use config::SafeConfig;
//!
//! #[tokio::main]
//! async fn main() {
//!     let config = SafeConfig::from_args().await.unwrap();
//!     let current_config = config.snapshot();
//!     println!("服务器端口: {}", current_config.app.port);
//! }
//! ```
//!
//! ### 基于 Nacos 的配置
//!
//! ```no_run
//! // 启用 nacos 运行:
//! // ./program --enable-nacos --nacos-addr="http://localhost:8848"
//! //           --nacos-namespace="public" --nacos-group="DEFAULT_GROUP"
//! //           --nacos-data-id="config.toml"
//! ```
//!
//! ## 配置结构
//!
//! 配置被组织成几个嵌套的组件:
//! * `Config`: 顶层配置容器
//! * `AppConfig`: 应用程序特定设置
//! * `DatabaseConfig`: 数据库连接设置
//! * `S3Config`: 必需的 S3 对象存储启动参数

use clap::Parser;
use command::Args;
use nacos::NacosConfig;
use nacos_watch::NacosConfigWatcher;
use serde::Deserialize;
use std::{fmt, path::Path};
use tokio::sync::watch;
use tracing::info;
use url::Url;

mod command;
mod errors;
mod nacos;
mod nacos_watch;
pub use errors::{Error, Result};
use nacos::NacosConfigClient;

const MIN_JWT_SECRET_BYTES: usize = 32;
const JWT_SECRET_PLACEHOLDERS: [&str; 2] = [
    "your-secret-key-change-me",
    "replace-with-at-least-32-random-bytes",
];

/// 包含所有应用程序设置的主配置结构。
///
/// 这个结构是所有配置设置的根容器,
/// 可以从 TOML 格式反序列化。
#[derive(Debug, Deserialize, Clone)]
pub struct Config {
    /// 应用程序特定配置
    pub app: AppConfig,
    /// 数据库特定配置
    pub database: DatabaseConfig,
    /// S3 对象存储参数，供存储运行时在启动期构建客户端。
    pub s3: S3Config,
}

/// 应用程序特定的配置设置。
///
/// 包含控制核心应用程序行为的设置。
#[derive(Debug, Deserialize, Clone)]
pub struct AppConfig {
    /// 应用程序将监听的 HTTP 服务器端口号
    pub port: u16,
    /// 用于 JWT 令牌生成和验证的密钥
    pub secret: String,
}

/// 数据库连接配置。
///
/// 包含建立数据库连接所需的所有设置。
#[derive(Debug, Deserialize, Clone)]
pub struct DatabaseConfig {
    /// 完整的数据库连接 URI,包括协议、主机和端口
    pub uri: String,
    /// 要连接的数据库名称
    pub db_name: String,
}

/// S3 或 S3-compatible 对象存储的启动配置。
#[derive(Deserialize, Clone, PartialEq, Eq)]
pub struct S3Config {
    /// 存放上传对象的 bucket。
    pub bucket: String,
    /// AWS region 或兼容服务使用的签名 region。
    pub region: String,
    /// 自定义 endpoint；直连 AWS S3 时可以省略。
    #[serde(default)]
    pub endpoint: Option<String>,
    /// S3 访问密钥 ID。
    pub access_key_id: String,
    /// S3 访问密钥。
    pub secret_access_key: String,
    /// 临时凭证的 session token。
    #[serde(default)]
    pub session_token: Option<String>,
    /// 统一加在所有对象键前的可选前缀。
    #[serde(default)]
    pub key_prefix: Option<String>,
    /// 对外返回的 CDN 或 bucket 公开访问基础 URL。
    pub public_base_url: String,
    /// 是否强制 path-style URL，MinIO 等兼容服务通常需要开启。
    #[serde(default)]
    pub force_path_style: bool,
}

impl fmt::Debug for S3Config {
    /// 输出不包含密钥正文的调试信息。
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("S3Config")
            .field("bucket", &self.bucket)
            .field("region", &self.region)
            .field("endpoint", &self.endpoint)
            .field("access_key_id", &"<redacted>")
            .field("secret_access_key", &"<redacted>")
            .field(
                "session_token",
                &self.session_token.as_ref().map(|_| "<redacted>"),
            )
            .field("key_prefix", &self.key_prefix)
            .field("public_base_url", &self.public_base_url)
            .field("force_path_style", &self.force_path_style)
            .finish()
    }
}

impl Config {
    /// 从指定路径加载 TOML 文件配置。
    ///
    /// # 参数
    ///
    /// * `path` - TOML 配置文件的路径
    ///
    /// # 返回
    ///
    /// * `Result<Self>` - 解析后的配置或错误
    pub async fn from_file<P: AsRef<Path>>(path: P) -> Result<Self> {
        let content = tokio::fs::read_to_string(path).await?;
        Self::from_toml_str(&content)
    }

    /// 从 TOML 字符串解析配置。
    ///
    /// # 参数
    ///
    /// * `content` - TOML 格式的配置字符串
    ///
    /// # 返回
    ///
    /// * `Result<Self>` - 解析后的配置或错误
    pub fn from_toml_str(content: &str) -> Result<Self> {
        let config: Config = toml::from_str(content)?;
        config.validate()?;
        Ok(config)
    }

    /// 校验会影响运行时安全和可用性的配置不变式。
    ///
    /// # 错误
    /// JWT 密钥或 S3 启动参数无效时返回错误。
    fn validate(&self) -> Result<()> {
        let secret = self.app.secret.as_bytes();
        if secret.len() < MIN_JWT_SECRET_BYTES || JWT_SECRET_PLACEHOLDERS.contains(&self.app.secret.as_str())
        {
            return Err(Error::Invalid(
                "app.secret must be at least 32 bytes and must not use the example placeholder".to_string(),
            ));
        }
        validate_s3_config(&self.s3)?;
        Ok(())
    }
}

/// 校验 S3 签名参数、endpoint 与对象键前缀。
fn validate_s3_config(config: &S3Config) -> Result<()> {
    for (name, value) in [
        ("bucket", config.bucket.as_str()),
        ("region", config.region.as_str()),
        ("access_key_id", config.access_key_id.as_str()),
        ("secret_access_key", config.secret_access_key.as_str()),
    ] {
        if value.trim().is_empty() || value.trim() != value {
            return Err(Error::Invalid(format!(
                "s3.{name} must not be empty or contain surrounding whitespace"
            )));
        }
    }

    if let Some(endpoint) = config.endpoint.as_deref() {
        validate_s3_url("endpoint", endpoint)?;
    }
    validate_s3_url("public_base_url", &config.public_base_url)?;
    if config
        .session_token
        .as_ref()
        .is_some_and(|token| token.trim().is_empty() || token.trim() != token)
    {
        return Err(Error::Invalid(
            "s3.session_token must not be empty or contain surrounding whitespace".to_string(),
        ));
    }
    if config
        .key_prefix
        .as_deref()
        .is_some_and(|prefix| !is_safe_s3_prefix(prefix))
    {
        return Err(Error::Invalid(
            "s3.key_prefix must be a relative object-key prefix without empty, '.' or '..' segments"
                .to_string(),
        ));
    }
    Ok(())
}

/// 校验 S3 endpoint 或公开基础 URL 为不含 query/fragment 的 HTTP(S) URL。
fn validate_s3_url(name: &str, value: &str) -> Result<()> {
    let url = Url::parse(value).map_err(|_| Error::Invalid(format!("s3.{name} must be a valid URL")))?;
    if !matches!(url.scheme(), "http" | "https")
        || url.host_str().is_none()
        || url.query().is_some()
        || url.fragment().is_some()
    {
        return Err(Error::Invalid(format!(
            "s3.{name} must be an HTTP(S) URL without query or fragment"
        )));
    }
    Ok(())
}

/// 判断 S3 对象键前缀是否可在存储根内安全拼接。
fn is_safe_s3_prefix(prefix: &str) -> bool {
    if prefix.is_empty()
        || prefix.trim() != prefix
        || prefix.starts_with('/')
        || prefix.ends_with('/')
        || prefix.contains('\\')
    {
        return false;
    }
    prefix
        .split('/')
        .all(|segment| !segment.is_empty() && segment != "." && segment != "..")
}

impl std::str::FromStr for Config {
    type Err = Error;

    /// 从 TOML 字符串解析配置。
    ///
    /// # 参数
    /// * `content` - TOML 格式的配置字符串
    ///
    /// # 返回
    /// 返回解析后的配置或错误
    fn from_str(content: &str) -> std::result::Result<Self, Self::Err> {
        Config::from_toml_str(content)
    }
}

/// 线程安全的配置包装器。
///
/// 使用 Tokio watch channel 统一保存快照并通知订阅方。
#[derive(Clone)]
pub struct SafeConfig {
    sender: watch::Sender<Config>,
}

impl SafeConfig {
    /// 使用给定配置创建新的 SafeConfig 实例。
    ///
    /// # 参数
    /// * `config` - 初始化配置
    ///
    /// # 返回
    /// 返回创建的实例。
    ///
    /// # 说明
    /// 原为私有构造（仅 `from_args` 可用）；P0-5 垂直样板的 web-api HTTP
    /// 集成测试需要在无 CLI 参数的环境构造最小 `AppState`，故放宽为 `pub`。
    /// 本改动为纯可见性放宽，不改变任何行为；建议随「地基修订 PR」正式合入。
    pub fn new(config: Config) -> Self {
        let (sender, _) = watch::channel(config);
        Self { sender }
    }

    /// 从命令行参数创建新的 SafeConfig 实例。
    ///
    /// 本方法依赖 command 模块中定义的 Args 结构体来解析命令行参数。
    /// Args 结构体通过 clap 实现命令行参数的解析，定义在 `command.rs` 文件中。
    ///
    /// Args 结构体包含以下字段:
    /// * `config_path`: 配置文件路径
    /// * `enable_nacos`: 是否启用 Nacos 配置中心
    /// * `nacos_addr`: Nacos 服务器地址
    /// * `nacos_namespace`: Nacos 命名空间
    /// * `nacos_group`: Nacos 配置组
    /// * `nacos_data_id`: Nacos 配置ID
    ///
    /// # 示例
    ///
    /// ```bash
    /// # 使用本地配置文件
    /// ./program --config-path=config.toml
    ///
    /// # 使用 Nacos 配置中心
    /// ./program --enable-nacos \
    ///           --nacos-addr="http://localhost:8848" \
    ///           --nacos-namespace="public" \
    ///           --nacos-group="DEFAULT_GROUP" \
    ///           --nacos-data-id="config.toml"
    /// ```
    ///
    /// # 返回
    ///
    /// * `Result<Self>` - 初始化的 SafeConfig 实例或错误
    pub async fn from_args() -> Result<Self> {
        let args = Args::parse();

        if args.is_enable_nacos() {
            return Self::from_nacos_with_watcher(args.to_nacos_config()).await;
        }

        let config = Config::from_file(&args.config_path).await?;
        Ok(Self::new(config))
    }

    /// 获取当前配置快照。
    ///
    /// # 返回
    ///
    /// * `Config` - 当前配置快照
    pub fn snapshot(&self) -> Config {
        self.sender.borrow().clone()
    }

    /// 订阅配置变更通知。
    ///
    /// # 返回
    /// 返回配置变更的接收器。
    pub fn subscribe(&self) -> watch::Receiver<Config> {
        self.sender.subscribe()
    }

    /// 从 Nacos 初始化配置并设置配置观察器。
    ///
    /// # 参数
    ///
    /// * `args` - Nacos 配置参数
    ///
    /// # 返回
    ///
    /// * `Result<Self>` - 初始化的 SafeConfig 实例或错误
    async fn from_nacos_with_watcher(args: NacosConfig) -> Result<Self> {
        let nacos_client = NacosConfigClient::from_config(args).await?;

        let content = nacos_client.fetch().await?;
        let config = Config::from_toml_str(&content)?;
        let safe_config = Self::new(config);

        let watcher = NacosConfigWatcher::new(safe_config.clone(), nacos_client);
        watcher.start();

        Ok(safe_config)
    }

    /// 从 Nacos 重新加载配置。
    ///
    /// # 参数
    ///
    /// * `nacos_client` - Nacos 客户端的引用
    ///
    /// # 返回
    ///
    /// * `Result<()>` - 成功或错误指示
    async fn reload_from_nacos(&self, nacos_client: &NacosConfigClient) -> Result<()> {
        let content = nacos_client.fetch().await?;
        let config = Config::from_toml_str(&content)?;
        self.sender.send_replace(config);

        info!("从 nacos 重新加载配置成功");
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::Config;

    const MINIMAL_CONFIG: &str = r#"
[app]
port = 10001
secret = "test-secret-that-is-at-least-32-bytes"

[database]
uri = "mongodb://localhost:27017"
db_name = "test"

[s3]
bucket = "erp-assets"
region = "cn-south-1"
endpoint = "https://s3.example.com"
access_key_id = "access-key"
secret_access_key = "secret-key"
session_token = "session-token"
key_prefix = "erp/uploads"
public_base_url = "https://cdn.example.com/assets"
force_path_style = true
"#;

    #[test]
    fn parses_required_s3_runtime_config() {
        let config = Config::from_toml_str(MINIMAL_CONFIG).unwrap();

        assert_eq!(config.app.port, 10001);
        assert_eq!(config.database.db_name, "test");
        assert_eq!(config.s3.bucket, "erp-assets");
        assert_eq!(config.s3.region, "cn-south-1");
        assert_eq!(config.s3.endpoint.as_deref(), Some("https://s3.example.com"));
        assert_eq!(config.s3.key_prefix.as_deref(), Some("erp/uploads"));
        assert_eq!(config.s3.public_base_url, "https://cdn.example.com/assets");
        assert!(config.s3.force_path_style);
    }

    #[test]
    fn removed_legacy_sections_do_not_break_existing_files() {
        let content = format!(
            "{MINIMAL_CONFIG}\n[server]\nhost = \"0.0.0.0\"\nport = 10001\n\
             [equipment]\nstart_authorization_ttl_seconds = 300\n"
        );

        let config = Config::from_toml_str(&content).unwrap();

        assert_eq!(config.app.port, 10001);
    }

    #[test]
    fn weak_or_example_jwt_secrets_are_rejected() {
        for secret in [
            "short",
            "your-secret-key-change-me",
            "replace-with-at-least-32-random-bytes",
        ] {
            let content = MINIMAL_CONFIG.replace("test-secret-that-is-at-least-32-bytes", secret);
            assert!(Config::from_toml_str(&content).is_err());
        }
    }

    #[test]
    fn missing_s3_config_is_rejected() {
        let content = MINIMAL_CONFIG.split("[s3]").next().unwrap();

        assert!(Config::from_toml_str(content).is_err());
    }

    #[test]
    fn rejects_unsafe_s3_config() {
        for invalid_field in [
            "bucket = \"\"",
            "endpoint = \"s3.example.com\"",
            "key_prefix = \"erp/../secret\"",
            "public_base_url = \"ftp://cdn.example.com\"",
            "public_base_url = \"https://cdn.example.com/assets?token=x\"",
        ] {
            let content = MINIMAL_CONFIG.replace(
                match invalid_field.split_once(" = ").unwrap().0 {
                    "bucket" => "bucket = \"erp-assets\"",
                    "endpoint" => "endpoint = \"https://s3.example.com\"",
                    "key_prefix" => "key_prefix = \"erp/uploads\"",
                    "public_base_url" => "public_base_url = \"https://cdn.example.com/assets\"",
                    _ => unreachable!(),
                },
                invalid_field,
            );

            assert!(
                Config::from_toml_str(&content).is_err(),
                "{invalid_field} must be rejected"
            );
        }
    }

    #[test]
    fn s3_debug_output_redacts_credentials() {
        let content = MINIMAL_CONFIG
            .replace("access-key", "visible-access-key")
            .replace("secret-key", "visible-secret-key")
            .replace("session-token", "visible-session-token");

        let debug = format!("{:?}", Config::from_toml_str(&content).unwrap());

        assert!(!debug.contains("visible-access-key"));
        assert!(!debug.contains("visible-secret-key"));
        assert!(!debug.contains("visible-session-token"));
    }
}
