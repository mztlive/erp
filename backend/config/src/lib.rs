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

use clap::Parser;
use command::Args;
use nacos::NacosConfig;
use nacos_watch::NacosConfigWatcher;
use serde::Deserialize;
use std::path::{Component, Path};
use tokio::sync::watch;
use tracing::info;

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
    /// 上传文件存储的基础路径
    pub upload_path: String,
    /// 每次上传完成后必须保留的最小文件系统可用字节数
    #[serde(default = "default_upload_min_free_bytes")]
    pub upload_min_free_bytes: u64,
    /// 上传文件的公开访问基础 URL
    #[serde(alias = "statistic_host")]
    pub file_base_url: String,
}

const fn default_upload_min_free_bytes() -> u64 {
    512 * 1024 * 1024
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

    /// 为上传的文件生成完整 URL。
    ///
    /// # 参数
    ///
    /// * `filename` - 上传文件的名称
    ///
    /// # 返回
    ///
    /// * `String` - 访问文件的完整 URL
    pub fn file_url(&self, filename: &str) -> String {
        format!(
            "{}/{}",
            self.app.file_base_url.trim_end_matches('/'),
            filename.trim_start_matches('/')
        )
    }

    /// 获取配置的上传路径。
    ///
    /// # 返回
    ///
    /// * `&str` - 配置的上传路径
    pub fn upload_path(&self) -> &str {
        self.app.upload_path.as_str()
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
    /// JWT 密钥无效，或上传路径为空、仅为当前/根目录、含父目录分量时返回错误。
    fn validate(&self) -> Result<()> {
        let secret = self.app.secret.as_bytes();
        if secret.len() < MIN_JWT_SECRET_BYTES || JWT_SECRET_PLACEHOLDERS.contains(&self.app.secret.as_str())
        {
            return Err(Error::Invalid(
                "app.secret must be at least 32 bytes and must not use the example placeholder".to_string(),
            ));
        }
        if !is_safe_upload_path(&self.app.upload_path) {
            return Err(Error::Invalid(
                "app.upload_path must name a dedicated non-root directory without '..'".to_string(),
            ));
        }
        Ok(())
    }
}

/// 判断配置路径是否明确指向根目录以下的专用目录。
fn is_safe_upload_path(path: &str) -> bool {
    if path.is_empty() || path.trim() != path {
        return false;
    }

    let mut has_directory_name = false;
    for component in Path::new(path).components() {
        match component {
            Component::Normal(_) => has_directory_name = true,
            Component::ParentDir => return false,
            Component::Prefix(_) | Component::RootDir | Component::CurDir => {}
        }
    }
    has_directory_name
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
    fn new(config: Config) -> Self {
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
upload_path = "/tmp/uploads"
file_base_url = "http://localhost:10001"

[database]
uri = "mongodb://localhost:27017"
db_name = "test"
"#;

    #[test]
    fn minimal_config_only_requires_runtime_fields() {
        let config = Config::from_toml_str(MINIMAL_CONFIG).unwrap();

        assert_eq!(config.app.port, 10001);
        assert_eq!(config.upload_path(), "/tmp/uploads");
        assert_eq!(config.app.upload_min_free_bytes, 512 * 1024 * 1024);
        assert_eq!(config.database.db_name, "test");
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
    fn file_url_has_exactly_one_path_separator() {
        let mut config = Config::from_toml_str(MINIMAL_CONFIG).unwrap();
        config.app.file_base_url.push('/');

        assert_eq!(config.file_url("/image.png"), "http://localhost:10001/image.png");
    }

    #[test]
    fn legacy_statistic_host_remains_a_deserialization_alias() {
        let content = MINIMAL_CONFIG.replace("file_base_url", "statistic_host");

        let config = Config::from_toml_str(&content).unwrap();

        assert_eq!(config.app.file_base_url, "http://localhost:10001");
        assert_eq!(config.file_url("image.png"), "http://localhost:10001/image.png");
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
    fn unsafe_upload_paths_are_rejected() {
        for upload_path in ["", ".", "/", "..", "../uploads", "uploads/..", " uploads"] {
            let content = MINIMAL_CONFIG.replace("/tmp/uploads", upload_path);
            assert!(
                Config::from_toml_str(&content).is_err(),
                "{upload_path:?} must be rejected"
            );
        }
    }

    #[test]
    fn explicit_relative_upload_directory_is_accepted() {
        let content = MINIMAL_CONFIG.replace("/tmp/uploads", "./uploads");

        let config = Config::from_toml_str(&content).unwrap();

        assert_eq!(config.upload_path(), "./uploads");
    }
}
