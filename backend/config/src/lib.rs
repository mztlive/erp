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
//! // ./program --enable-nacos --nacos-addr="127.0.0.1:8848"
//! //           --nacos-namespace="ccf7ec38-1d60-407e-bf2c-7c4654c481d0" --nacos-group="DEFAULT_GROUP"
//! //           --nacos-data-id="erp"
//! ```
//!
//! ## 配置结构
//!
//! 配置被组织成几个嵌套的组件:
//! * `Config`: 顶层配置容器
//! * `AppConfig`: 应用程序特定设置
//! * `DatabaseConfig`: 数据库连接设置
//! * `S3Config`: 必需的 S3 对象存储启动参数
//! * `BootstrapConfig`: 可选的首次超级管理员密码；已有账号时不会使用

use std::fmt;
use std::path::Path;

use clap::Parser;
pub use command::ConfigArgs;
use nacos::NacosConfig;
use nacos_watch::NacosConfigWatcher;
use serde::Deserialize;
use tokio::sync::watch;
use tracing::info;
use url::Url;

mod aliyun_ocr;
pub use aliyun_ocr::AliyunOcrConfig;
mod purchase_mail;
pub use purchase_mail::PurchaseMailConfig;
mod contract_ai;
pub use contract_ai::ContractAiConfig;
mod dangaoshushu;
pub use dangaoshushu::{DangaoshushuConfig, SupplierTimestampUnit};
mod xxl_job;
pub use xxl_job::XxlJobConfig;
mod jd_vop;
pub mod jd_vop_execution;
pub use jd_vop::JdVopConfig;
mod command;
mod errors;
mod nacos;
mod nacos_watch;
pub use errors::{Error, Result};
use nacos::NacosConfigClient;

const MIN_JWT_SECRET_BYTES: usize = 32;
const JWT_SECRET_PLACEHOLDERS: [&str; 2] =
    ["your-secret-key-change-me", "replace-with-at-least-32-random-bytes"];
const BOOTSTRAP_PASSWORD_MIN_CHARS: usize = 6;
const BOOTSTRAP_PASSWORD_MAX_CHARS: usize = 32;
const BOOTSTRAP_PASSWORD_PLACEHOLDER: &str = "replace-with-6-to-32-characters";

/// 包含所有应用程序设置的主配置结构。
///
/// 这个结构是所有配置设置的根容器,
/// 可以从 TOML 格式反序列化。
#[derive(Debug, Deserialize, Clone)]
pub struct Config {
    /// Worker 必填的启动期调度配置；API 可省略，提供时必须合法。
    pub xxl_job: Option<XxlJobConfig>,
    /// 应用程序特定配置
    pub app: AppConfig,
    /// 数据库特定配置
    pub database: DatabaseConfig,
    /// S3 对象存储参数，供存储运行时在启动期构建客户端。
    pub s3: S3Config,
    /// 可选的首次超级管理员密码。缺省时不在启动时创建账号。
    #[serde(default)]
    pub bootstrap: BootstrapConfig,
    /// 可选的演示数据开关。缺省时关闭，不生成也不删除演示主数据。
    #[serde(default)]
    pub demo: DemoConfig,
    /// 可选阿里云 OCR；缺省时不启用外部识别。
    #[serde(default)]
    pub aliyun_ocr: Option<AliyunOcrConfig>,
    /// 可选 OpenAI 兼容合同提取服务。
    #[serde(default)]
    pub contract_ai: Option<ContractAiConfig>,
    /// 可选 Resend 兼容采购邮件发送服务。
    #[serde(default)]
    pub purchase_mail: Option<PurchaseMailConfig>,
    /// 可选蛋糕叔叔连接；凭据由 SafeConfig 提供，不存入业务表。
    #[serde(default)]
    pub dangaoshushu: Option<DangaoshushuConfig>,
    /// 可选京东 VOP 技术配置；认证与账号授权须独立核验。
    #[serde(default)]
    pub jd_vop: Option<JdVopConfig>,
}

/// 演示主数据开关。
///
/// 只控制「系统管理」里的演示主数据按钮。关闭时接口拒绝写入。
#[derive(Debug, Deserialize, Clone, Default)]
pub struct DemoConfig {
    /// 为 true 时允许生成和删除演示主数据。
    #[serde(default)]
    master_data: bool,
}

impl DemoConfig {
    /// 装配演示主数据用例时读取当前环境的写入开关。只有显式启用，才允许生成或删除演示主数据。
    pub fn master_data_enabled(&self) -> bool {
        self.master_data
    }
}

/// 首次启动时创建超级管理员的可选配置。
///
/// 只在数据库里还没有 `admin` 账号时使用。已有账号时进程忽略该密码，
/// 不修改密码、状态或角色。
#[derive(Deserialize, Clone, Default)]
pub struct BootstrapConfig {
    /// 超级管理员明文初始密码。空字符串视为未配置。
    #[serde(default)]
    initial_admin_password: Option<String>,
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

impl S3Config {
    /// 为调用方直接构造对象存储启动配置，包含存储桶、签名参数、访问凭据和对外访问根地址。
    /// 默认不设置自定义端点、临时令牌或对象键前缀，也不启用 path-style；构造时不做校验。
    pub fn new(
        bucket: impl Into<String>,
        region: impl Into<String>,
        access_key_id: impl Into<String>,
        secret_access_key: impl Into<String>,
        public_base_url: impl Into<String>,
    ) -> Self {
        Self {
            bucket: bucket.into(),
            region: region.into(),
            endpoint: None,
            access_key_id: access_key_id.into(),
            secret_access_key: secret_access_key.into(),
            session_token: None,
            key_prefix: None,
            public_base_url: public_base_url.into(),
            force_path_style: false,
        }
    }

    /// 接入 S3 兼容服务或专用存储地址时，指定客户端实际访问的端点，不改变上传后返回的公开根地址。
    pub fn with_endpoint(mut self, endpoint: impl Into<String>) -> Self {
        self.endpoint = Some(endpoint.into());
        self
    }

    /// 多个应用共用存储桶时，用相对前缀为本应用划分对象键；上传后返回的公开地址也包含这段前缀。
    pub fn with_key_prefix(mut self, key_prefix: impl Into<String>) -> Self {
        self.key_prefix = Some(key_prefix.into());
        self
    }

    /// 连接 MinIO 等不使用桶名子域的服务时可启用 path-style，让客户端在请求路径中携带桶名。
    pub fn with_force_path_style(mut self, force_path_style: bool) -> Self {
        self.force_path_style = force_path_style;
        self
    }
}

impl BootstrapConfig {
    /// 供服务启动时首次创建 `admin` 账号时读取明文密码。缺省或空字符串返回 `None`；
    /// 已有账号时启动流程不使用此密码更新账号。
    pub fn initial_admin_password(&self) -> Option<&str> {
        self.initial_admin_password.as_deref().filter(|password| !password.is_empty())
    }
}

impl fmt::Debug for BootstrapConfig {
    // 调试初始化配置时只保留密码字段是否已填写，不输出明文。
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("BootstrapConfig")
            .field("initial_admin_password", &self.initial_admin_password.as_ref().map(|_| "<redacted>"))
            .finish()
    }
}

impl fmt::Debug for S3Config {
    // 诊断存储连接时保留存储桶、端点等路由信息，但隐藏访问密钥 ID、密钥和临时令牌。
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("S3Config")
            .field("bucket", &self.bucket)
            .field("region", &self.region)
            .field("endpoint", &self.endpoint)
            .field("access_key_id", &"<redacted>")
            .field("secret_access_key", &"<redacted>")
            .field("session_token", &self.session_token.as_ref().map(|_| "<redacted>"))
            .field("key_prefix", &self.key_prefix)
            .field("public_base_url", &self.public_base_url)
            .field("force_path_style", &self.force_path_style)
            .finish()
    }
}

impl Config {
    /// 本地配置模式下异步读取指定 TOML 文件，并使用与 Nacos 相同的应用校验。
    /// 文件不可读、内容无法解析或配置无效时返回错误，不切换配置来源。
    pub async fn from_file<P: AsRef<Path>>(path: P) -> Result<Self> {
        let content = tokio::fs::read_to_string(path).await?;
        Self::from_toml_str(&content)
    }

    /// 把文件或 Nacos 提供的完整 TOML 解析并校验为配置，供启动和刷新共用。
    /// 解析失败只报告字节范围，不回显可能含密钥的原文。
    pub fn from_toml_str(content: &str) -> Result<Self> {
        let config: Config = toml::from_str(content)?;
        config.validate()?;
        Ok(config)
    }

    // 反序列化后、应用使用或发布快照前，检查 JWT 密钥、S3、初始管理员密码及已配置的外部集成。
    // API 可省略调度配置，但只要提供就必须通过校验。
    fn validate(&self) -> Result<()> {
        let secret = self.app.secret.as_bytes();
        if secret.len() < MIN_JWT_SECRET_BYTES || JWT_SECRET_PLACEHOLDERS.contains(&self.app.secret.as_str())
        {
            return Err(Error::Invalid(
                "app.secret must be at least 32 bytes and must not use the example placeholder".to_string(),
            ));
        }
        if let Some(ocr) = &self.aliyun_ocr {
            ocr.validate()?;
        }
        if let Some(ai) = &self.contract_ai {
            ai.validate()?;
        }
        if let Some(supplier) = &self.dangaoshushu {
            supplier.validate()?;
        }
        if let Some(supplier) = &self.jd_vop {
            supplier.validate()?;
            supplier.ensure_independent_key(&self.app.secret)?;
        }
        if let Some(mail) = &self.purchase_mail {
            mail.validate()?;
        }
        if let Some(xxl_job) = &self.xxl_job {
            xxl_job.validate()?;
        }
        validate_s3_config(&self.s3)?;
        validate_bootstrap_password(self.bootstrap.initial_admin_password())?;
        Ok(())
    }
}

// 加载启动配置时拒绝示例密码或长度不符合管理员规则的密码（6 到 32 个字符）。
// 未配置初始密码不影响启动，也不会触发自动创建。
fn validate_bootstrap_password(password: Option<&str>) -> Result<()> {
    let Some(password) = password else {
        return Ok(());
    };
    let length = password.chars().count();
    if !(BOOTSTRAP_PASSWORD_MIN_CHARS..=BOOTSTRAP_PASSWORD_MAX_CHARS).contains(&length)
        || password == BOOTSTRAP_PASSWORD_PLACEHOLDER
    {
        return Err(Error::Invalid(
            "bootstrap.initial_admin_password must be 6 to 32 characters and must not use the example placeholder"
                .to_string(),
        ));
    }
    Ok(())
}

// 构建对象存储客户端前检查存储桶、签名区域和凭据，并校验端点、公开地址及键前缀，
// 避免不合规的参数被用于签名请求或拼接上传后的访问地址。
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
    if config.session_token.as_ref().is_some_and(|token| token.trim().is_empty() || token.trim() != token) {
        return Err(Error::Invalid(
            "s3.session_token must not be empty or contain surrounding whitespace".to_string(),
        ));
    }
    if config.key_prefix.as_deref().is_some_and(|prefix| !is_safe_s3_prefix(prefix)) {
        return Err(Error::Invalid(
            "s3.key_prefix must be a relative object-key prefix without empty, '.' or '..' segments"
                .to_string(),
        ));
    }
    Ok(())
}

// 读取存储端点或公开根地址时只接受有主机的 HTTP(S) 绝对地址，拒绝查询参数和片段，
// 确保它们能用作请求或拼接对象路径的基址。
fn validate_s3_url(name: &str, value: &str) -> Result<()> {
    let url = Url::parse(value).map_err(|_| Error::Invalid(format!("s3.{name} must be a valid URL")))?;
    if !matches!(url.scheme(), "http" | "https")
        || url.host_str().is_none()
        || url.query().is_some()
        || url.fragment().is_some()
    {
        return Err(Error::Invalid(format!("s3.{name} must be an HTTP(S) URL without query or fragment")));
    }
    Ok(())
}

// 在把前缀用于对象键和公开地址前，拒绝绝对路径、反斜杠和空分段，也不允许 `.` 或 `..`。
fn is_safe_s3_prefix(prefix: &str) -> bool {
    if prefix.is_empty()
        || prefix.trim() != prefix
        || prefix.starts_with('/')
        || prefix.ends_with('/')
        || prefix.contains('\\')
    {
        return false;
    }
    prefix.split('/').all(|segment| !segment.is_empty() && segment != "." && segment != "..")
}

impl std::str::FromStr for Config {
    type Err = Error;

    /// 支持通过 `text.parse::<Config>()` 从内存中的 TOML 加载配置；与 [`Config::from_toml_str`]
    /// 使用同一套应用校验和脱敏解析错误，不会绕过文件或 Nacos 加载时的约束。
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
    /// 将调用方提供的配置封装为可读取、可订阅的快照，适用于已提前加载配置的入口或注入配置的测试。
    /// 此方法不会再次校验配置，也不会启动刷新任务。
    pub fn new(config: Config) -> Self {
        let (sender, _) = watch::channel(config);
        Self { sender }
    }

    /// 为常驻进程按启动参数加载配置。文件模式只读一次；Nacos 模式必须先成功读取并校验，
    /// 再启动后台刷新，首次读取失败不会回退到文件。单次运维命令应使用 [`ConfigArgs::load`]。
    ///
    /// # 示例
    ///
    /// ```bash
    /// # 使用本地配置文件
    /// ./program --config-path=config.toml
    ///
    /// # 使用 Nacos 配置中心
    /// ./program --enable-nacos \
    ///           --nacos-addr="127.0.0.1:8848" \
    ///           --nacos-namespace="ccf7ec38-1d60-407e-bf2c-7c4654c481d0" \
    ///           --nacos-group="DEFAULT_GROUP" \
    ///           --nacos-data-id="erp"
    /// ```
    pub async fn from_args() -> Result<Self> {
        let args = ConfigArgs::parse();

        if args.enable_nacos {
            return Self::from_nacos_with_watcher(args.nacos_config()?).await;
        }

        let config = args.load().await?;
        Ok(Self::new(config))
    }

    /// 复制当前整份配置，供一次请求或装配过程使用同一版本，避免读到来自不同刷新批次的字段。
    pub fn snapshot(&self) -> Config {
        self.sender.borrow().clone()
    }

    /// 为需要响应热更新的常驻组件提供配置接收器。接收器可读取当前值，并在新快照发布后应用
    /// 运行时支持更新的字段；固定在启动期的资源不会因此自动重建。
    pub fn subscribe(&self) -> watch::Receiver<Config> {
        self.sender.subscribe()
    }

    // 常驻进程选择 Nacos 时，先取得并校验首份配置，再启动后台轮询；启动失败不发布快照。
    async fn from_nacos_with_watcher(args: NacosConfig) -> Result<Self> {
        let nacos_client = NacosConfigClient::from_config(args).await?;

        let content = nacos_client.fetch().await?;
        let config = Config::from_toml_str(&content)?;
        let safe_config = Self::new(config);

        let watcher = NacosConfigWatcher::new(safe_config.clone(), nacos_client);
        watcher.start();

        Ok(safe_config)
    }

    // 后台轮询时重新拉取完整 TOML，只有解析和应用校验通过才替换当前快照；失败时保留上一份。
    async fn reload_from_nacos(&self, nacos_client: &NacosConfigClient) -> Result<()> {
        let content = nacos_client.fetch().await?;
        self.replace_from_toml(&content)?;

        info!("从 nacos 重新加载配置成功");
        Ok(())
    }

    fn replace_from_toml(&self, content: &str) -> Result<()> {
        let config = Config::from_toml_str(content)?;
        self.sender.send_replace(config);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::Config;

    const MINIMAL_CONFIG: &str = r#"
[xxl_job]
admin_url = "http://admin.invalid/xxl-job-admin"
executor_url = "http://worker.invalid:10002"
app_name = "erp-test"
access_token = "test-token-123456"

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
    fn api_configuration_can_omit_scheduler_but_provided_scheduler_is_validated() {
        let api = &MINIMAL_CONFIG[MINIMAL_CONFIG.find("[app]").unwrap()..];
        assert!(Config::from_toml_str(api).unwrap().xxl_job.is_none());
        assert!(Config::from_toml_str(MINIMAL_CONFIG).unwrap().xxl_job.is_some());
        let invalid = MINIMAL_CONFIG.replace("test-token-123456", "short");
        assert!(Config::from_toml_str(&invalid).is_err());
    }

    #[test]
    fn reload_validates_before_replacing_snapshot() {
        let config = super::SafeConfig::new(Config::from_toml_str(MINIMAL_CONFIG).unwrap());
        let receiver = config.subscribe();
        for invalid in [
            "secret = [invalid",
            "",
            &MINIMAL_CONFIG.replace("test-secret-that-is-at-least-32-bytes", "short"),
        ] {
            assert!(config.replace_from_toml(invalid).is_err());
            assert!(!receiver.has_changed().unwrap());
            assert_eq!(config.snapshot().app.port, 10001);
        }
        config.replace_from_toml(&MINIMAL_CONFIG.replace("10001", "10002")).unwrap();
        assert!(receiver.has_changed().unwrap());
        assert_eq!(config.snapshot().app.port, 10002);
    }

    #[test]
    fn parse_error_does_not_include_config_content() {
        let error = Config::from_toml_str("secret = super-private-secret").unwrap_err();
        assert!(!format!("{error} {error:?}").contains("super-private-secret"));
    }

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
        for secret in ["short", "your-secret-key-change-me", "replace-with-at-least-32-random-bytes"] {
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

            assert!(Config::from_toml_str(&content).is_err(), "{invalid_field} must be rejected");
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

    #[test]
    fn missing_bootstrap_section_leaves_initial_admin_unset() {
        let config = Config::from_toml_str(MINIMAL_CONFIG).unwrap();

        assert_eq!(config.bootstrap.initial_admin_password(), None);
    }

    #[test]
    fn blank_initial_admin_password_is_unset() {
        let content = format!("{MINIMAL_CONFIG}\n[bootstrap]\ninitial_admin_password = \"\"\n");

        let config = Config::from_toml_str(&content).unwrap();

        assert_eq!(config.bootstrap.initial_admin_password(), None);
    }

    #[test]
    fn demo_master_data_stays_closed_unless_explicitly_enabled() {
        let closed = Config::from_toml_str(MINIMAL_CONFIG).unwrap();
        assert!(!closed.demo.master_data_enabled());

        let opened =
            Config::from_toml_str(&format!("{MINIMAL_CONFIG}\n[demo]\nmaster_data = true\n")).unwrap();
        assert!(opened.demo.master_data_enabled());
    }

    #[test]
    fn initial_admin_password_is_kept_and_redacted_from_debug() {
        let content = format!(
            "{MINIMAL_CONFIG}\n[bootstrap]\ninitial_admin_password = \"visible-bootstrap-password\"\n"
        );

        let config = Config::from_toml_str(&content).unwrap();
        let debug = format!("{config:?}");

        assert_eq!(config.bootstrap.initial_admin_password(), Some("visible-bootstrap-password"));
        assert!(!debug.contains("visible-bootstrap-password"));
        assert!(debug.contains("<redacted>"));
    }

    #[test]
    fn invalid_initial_admin_password_is_rejected() {
        for password in ["short", "replace-with-6-to-32-characters", "123456789012345678901234567890123"] {
            let content = format!("{MINIMAL_CONFIG}\n[bootstrap]\ninitial_admin_password = \"{password}\"\n");

            assert!(Config::from_toml_str(&content).is_err(), "{password} must be rejected");
        }
    }
}
