//! HTTP 与独立 Worker 的共用配置和启动装配。
//!
//! 按固定进程职责装配业务 HTTP 或独立后台任务，复用配置与领域能力。

use config::{Config, S3Config, SafeConfig, XxlJobConfig};
use erp_identity::service::iam::ensure_identity_foundation;
use erp_identity::{AdminService, InitializeSuperAdminParams};
use storage::{S3Storage, S3StorageConfig};
use tokio::task::JoinHandle;
use tracing::{info, warn};

use crate::runtime::telemetry::{TracingConfig, init_tracing};
use crate::runtime::{ProcessRole, Result, server};
use crate::wiring::{Application, ExternalConnectorPorts};

const INITIAL_ADMIN_ACCOUNT: &str = "admin";
const INITIAL_ADMIN_NAME: &str = "系统管理员";

#[derive(Clone, Debug, PartialEq, Eq)]
struct DbConfigKey {
    uri: String,
    db_name: String,
}

impl DbConfigKey {
    /// 从完整配置提取数据库配置摘要。
    ///
    /// # 参数
    /// * `config` - 完整配置
    ///
    /// # 返回
    /// 返回数据库配置摘要。
    fn from_config(config: &Config) -> Self {
        Self { uri: config.database.uri.clone(), db_name: config.database.db_name.clone() }
    }
}

/// 按固定职责启动应用。
///
/// # 参数
/// `role` 指定 业务 HTTP 或独立 Worker。
///
/// # 返回
/// 返回执行结果，`Ok` 表示成功，`Err` 表示失败。
///
/// # 错误
/// 当验证失败或底层操作失败时返回错误。
pub async fn run(role: ProcessRole) -> Result<()> {
    let tracing_config = TracingConfig {
        env_filter: std::env::var("RUST_LOG").unwrap_or_else(|_| "info".to_string()),
        log_to_file: env_flag("LOG_TO_FILE"),
        log_directory: "logs".to_string(),
        log_file_prefix: role.service_name().to_string(),
        json_format: std::env::var("LOG_FORMAT").unwrap_or_default() == "json",
        otel_enabled: otel_exporter_enabled(),
        service_name: non_empty_env("OTEL_SERVICE_NAME").unwrap_or_else(|| role.service_name().to_string()),
    };

    let _tracing_guard = init_tracing(tracing_config)?;

    let config = SafeConfig::from_args().await?;
    let app_port = config.snapshot().app.port;

    info!("Starting application with config: {}", app_port);

    start(config, role).await
}

/// 判断环境变量是否显式启用布尔开关。
fn env_flag(name: &str) -> bool {
    std::env::var(name)
        .ok()
        .is_some_and(|value| matches!(value.trim().to_ascii_lowercase().as_str(), "1" | "true" | "yes"))
}

/// 读取非空环境变量；空白值按未配置处理。
fn non_empty_env(name: &str) -> Option<String> {
    std::env::var(name).ok().map(|value| value.trim().to_string()).filter(|value| !value.is_empty())
}

/// 按标准 OTLP endpoint 与 SDK 禁用开关决定是否创建 exporter。
///
/// 未配置 endpoint 时保持原有纯日志模式，避免本地开发默认连接固定 Collector。
fn otel_exporter_enabled() -> bool {
    let traces_endpoint = non_empty_env("OTEL_EXPORTER_OTLP_TRACES_ENDPOINT");
    let endpoint = non_empty_env("OTEL_EXPORTER_OTLP_ENDPOINT");
    should_enable_otel(env_flag("OTEL_SDK_DISABLED"), traces_endpoint.as_deref(), endpoint.as_deref())
}

/// 对已解析的 OpenTelemetry 开关与 endpoint 执行确定性判定。
fn should_enable_otel(sdk_disabled: bool, traces_endpoint: Option<&str>, endpoint: Option<&str>) -> bool {
    !sdk_disabled && traces_endpoint.or(endpoint).is_some_and(|value| !value.trim().is_empty())
}

/// 启动应用程序。
///
/// # 参数
/// * `cfg` - 配置对象
///
/// # 返回
/// 返回执行结果，`Ok` 表示成功，`Err` 表示失败。
///
/// # 错误
/// 当验证失败或底层操作失败时返回错误。
async fn start(cfg: SafeConfig, role: ProcessRole) -> Result<()> {
    let config = cfg.snapshot();
    require_scheduler(role, config.xxl_job.as_ref())?;
    let storage = build_storage(&config.s3)?;
    let (_, db) = persistence_core::connect(&config.database.uri, &config.database.db_name).await?;
    let connectors = ExternalConnectorPorts::from_database(&config, db.clone())?;
    let state = Application::new_with_connectors(db, cfg.clone(), storage, connectors);
    persistence_core::ensure_transaction_support(&state.db()).await?;
    crate::indexes::ensure_indexes(&state.db()).await?;
    ensure_registered_approval_policies()?;
    if role.serves_api() {
        initialize_identity(&state, &config).await?;
    }
    warm_storage_connection(state.storage()).await;
    let watcher = spawn_config_watcher(state.clone(), DbConfigKey::from_config(&config), config.s3.clone());
    let result = server::serve(state, role, config.app.port).await;
    watcher.abort();
    result
}

/// 在连接外部基础设施之前检查进程必需的调度配置。
fn require_scheduler(role: ProcessRole, scheduler: Option<&XxlJobConfig>) -> Result<()> {
    if role.runs_workers() && scheduler.is_none() {
        return Err("Worker 必须配置 [xxl_job]".into());
    }
    Ok(())
}

/// 仅 HTTP 应用启动时初始化身份基础数据，Worker 不重复执行管理账号引导。
async fn initialize_identity(state: &Application, config: &Config) -> Result<()> {
    erp_identity::ensure_root_role(&state.rbac()).await?;
    ensure_identity_foundation(&state.rbac()).await?;
    bootstrap_initial_admin(state, config.bootstrap.initial_admin_password()).await?;
    let changed = state.party_service().sync_builtin_companies(state.sensitive_data()).await?;
    info!(changed, "内建公司主体同步完成");
    Ok(())
}

/// 将存储连接的 DNS/TLS 首次等待移至有界启动准备，失败不改变服务启动合同。
async fn warm_storage_connection(storage: &S3Storage) {
    match tokio::time::timeout(std::time::Duration::from_secs(2), storage.warm_connection()).await {
        Ok(Ok(())) => {},
        Ok(Err(_)) => warn!("Object storage connection warmup did not complete successfully"),
        Err(_) => warn!("Object storage connection warmup exceeded startup time limit"),
    }
}

/// 配置了初始密码且库中还没有超级管理员时，创建 `admin` 并绑定 root 角色。
///
/// 未配置密码，或账号已经存在时，不修改已有账号。
///
/// # 参数
/// * `state` - 已连接数据库并完成角色初始化的应用状态
/// * `password` - 配置中的初始密码；缺失表示不创建
///
/// # 返回
/// 创建完成或无需创建时返回 `Ok(())`。
///
/// # 错误
/// 密码不合法、同名账号不是系统管理员，或数据库写入失败时返回错误。
async fn bootstrap_initial_admin(state: &Application, password: Option<&str>) -> Result<()> {
    let Some(password) = password else {
        return Ok(());
    };
    AdminService::new(state.db(), state.rbac())
        .bootstrap_super_admin_if_absent(InitializeSuperAdminParams::new(
            INITIAL_ADMIN_ACCOUNT,
            password,
            INITIAL_ADMIN_NAME,
        ))
        .await?;
    Ok(())
}

/// 启动前穷尽校验全部固定单据类型政策。
///
/// 任一类型缺少政策时按部署不变量失败关闭，不得启动旧定义 bootstrap。
///
/// # 错误
/// 政策缺失或权限字符串无法解析时返回服务错误。
fn ensure_registered_approval_policies() -> erp_workflow::Result<()> {
    for document_type in erp_workflow::service::approval::policy::ALL_DOCUMENT_TYPES {
        erp_workflow::service::approval::policy::policy_of(document_type)?;
    }
    Ok(())
}

/// 根据已校验的启动配置构建 S3 存储客户端。
///
/// # 参数
/// * `config` - S3 连接、凭证、键前缀与公开 URL 配置
///
/// # 返回
/// 返回可由所有上传 handler 共享的 S3 客户端。
///
/// # 错误
/// S3 参数无效时返回存储配置错误。
fn build_storage(config: &S3Config) -> storage::Result<S3Storage> {
    S3Storage::new(S3StorageConfig {
        bucket: config.bucket.clone(),
        region: config.region.clone(),
        endpoint: config.endpoint.clone(),
        access_key_id: config.access_key_id.clone(),
        secret_access_key: config.secret_access_key.clone(),
        session_token: config.session_token.clone(),
        key_prefix: config.key_prefix.clone(),
        public_base_url: config.public_base_url.clone(),
        force_path_style: config.force_path_style,
    })
}

/// 启动配置监听并刷新可安全热更新的运行时状态。
///
/// 数据库与 RBAC 必须始终指向同一存储，因此数据库连接配置只在进程启动时生效；
/// S3 客户端固定为启动值，保证单次进程中 bucket、凭证、键前缀与公开 URL 一致。
/// 监听到这些变化时保留当前运行时，并记录需要重启的结构化日志。
///
/// # 参数
/// * `state` - 应用状态
/// * `initial_db` - 初始数据库配置摘要
/// * `initial_secret` - 初始 JWT 密钥
/// * `initial_storage` - 启动时使用的 S3 配置
fn spawn_config_watcher(
    state: Application,
    initial_db: DbConfigKey,
    initial_storage: S3Config,
) -> JoinHandle<()> {
    let mut receiver = state.subscribe_config();
    let active_xxl = state.config_snapshot().xxl_job;

    tokio::spawn(async move {
        let active_db = initial_db.clone();
        let mut observed_db = initial_db;
        let active_storage = initial_storage.clone();
        let mut observed_storage = initial_storage;
        let mut observed_xxl = active_xxl.clone();

        loop {
            let next_config = receiver.borrow().clone();
            let next_db = DbConfigKey::from_config(&next_config);
            if next_config.xxl_job != observed_xxl {
                warn!(
                    restart_required = next_config.xxl_job != active_xxl,
                    "XXL 配置变更不热切换；执行器继续使用启动配置"
                );
                observed_xxl = next_config.xxl_job.clone();
            }

            if next_db != observed_db {
                if database_change_requires_restart(&active_db, &next_db) {
                    warn!(
                        restart_required = true,
                        database_uri_changed = active_db.uri != next_db.uri,
                        database_name_changed = active_db.db_name != next_db.db_name,
                        "Database configuration change ignored at runtime; restart the application to apply it"
                    );
                } else {
                    info!(
                        restart_required = false,
                        "Database configuration reverted to the active startup value"
                    );
                }

                observed_db = next_db;
            }

            if next_config.s3 != observed_storage {
                log_storage_change(&active_storage, &next_config.s3);
                observed_storage = next_config.s3.clone();
            }

            let Ok(()) = receiver.changed().await else {
                info!("Config watcher stopped: sender dropped");
                return;
            };
        }
    })
}

/// 只记录 S3 变化的字段类别，不泄露凭据或配置值。
fn log_storage_change(active: &S3Config, requested: &S3Config) {
    if storage_change_requires_restart(active, requested) {
        warn!(
            restart_required = true,
            bucket_changed = active.bucket != requested.bucket,
            region_changed = active.region != requested.region,
            endpoint_changed = active.endpoint != requested.endpoint,
            credentials_changed = active.access_key_id != requested.access_key_id
                || active.secret_access_key != requested.secret_access_key
                || active.session_token != requested.session_token,
            key_prefix_changed = active.key_prefix != requested.key_prefix,
            public_base_url_changed = active.public_base_url != requested.public_base_url,
            force_path_style_changed = active.force_path_style != requested.force_path_style,
            "S3 configuration change ignored at runtime; restart the application to apply it"
        );
    } else {
        info!(restart_required = false, "S3 configuration reverted to the active startup value");
    }
}

/// 判断请求的数据库配置是否需要通过重启应用生效。
///
/// # 参数
/// * `active` - 应用启动时使用的数据库配置
/// * `requested` - 配置中心当前请求的数据库配置
///
/// # 返回
/// 两者不一致时返回 `true`。
fn database_change_requires_restart(active: &DbConfigKey, requested: &DbConfigKey) -> bool {
    active != requested
}

/// 判断 S3 配置变化是否需要通过重启应用生效。
///
/// # 参数
/// * `active` - 启动时固定的 S3 配置
/// * `requested` - 配置中心请求的 S3 配置
///
/// # 返回
/// 两者不一致时返回 `true`。
fn storage_change_requires_restart(active: &S3Config, requested: &S3Config) -> bool {
    active != requested
}

#[cfg(test)]
mod tests {
    use config::S3Config;

    use super::{
        DbConfigKey, database_change_requires_restart, ensure_registered_approval_policies, env_flag,
        should_enable_otel, storage_change_requires_restart,
    };

    #[test]
    fn worker_requires_scheduler_before_infrastructure_startup() {
        assert!(super::require_scheduler(super::ProcessRole::Api, None).is_ok());
        assert_eq!(
            super::require_scheduler(super::ProcessRole::Worker, None).unwrap_err().to_string(),
            "Worker 必须配置 [xxl_job]"
        );
    }

    /// 构造测试使用的数据库配置摘要。
    fn database_key(uri: &str, db_name: &str) -> DbConfigKey {
        DbConfigKey { uri: uri.to_string(), db_name: db_name.to_string() }
    }

    #[test]
    fn database_change_should_require_restart() {
        let active = database_key("mongodb://127.0.0.1:27017", "app");
        let changed_uri = database_key("mongodb://127.0.0.1:27018", "app");
        let changed_name = database_key("mongodb://127.0.0.1:27017", "app_next");

        assert!(database_change_requires_restart(&active, &changed_uri));
        assert!(database_change_requires_restart(&active, &changed_name));
        assert!(!database_change_requires_restart(&active, &active));
    }

    #[test]
    fn registered_approval_policies_are_exhaustive_at_startup() {
        ensure_registered_approval_policies().expect("20 个固定类型必须已注册政策");
    }

    #[test]
    fn missing_environment_flag_is_disabled() {
        let name = format!("RS_PROJECT_TEMPLATE_MISSING_FLAG_{}", std::process::id());
        assert!(!env_flag(&name));
    }

    #[test]
    fn otel_exporter_requires_endpoint_and_enabled_sdk() {
        assert!(!should_enable_otel(false, None, None));
        assert!(!should_enable_otel(true, None, Some("http://collector:4317")));
        assert!(!should_enable_otel(false, Some("   "), None));
        assert!(should_enable_otel(false, Some("http://collector:4317"), None));
        assert!(should_enable_otel(false, None, Some("http://collector:4317")));
    }

    #[test]
    fn storage_change_should_require_restart() {
        let active = storage_config("erp-assets", "https://cdn.example.com");
        let changed_bucket = storage_config("erp-assets-next", "https://cdn.example.com");
        let changed_public_url = storage_config("erp-assets", "https://cdn-next.example.com");

        assert!(storage_change_requires_restart(&active, &changed_bucket));
        assert!(storage_change_requires_restart(&active, &changed_public_url));
        assert!(!storage_change_requires_restart(&active, &active));
    }

    /// 构造用于重启判定的 S3 配置。
    fn storage_config(bucket: &str, public_base_url: &str) -> S3Config {
        S3Config {
            bucket: bucket.to_string(),
            region: "cn-south-1".to_string(),
            endpoint: Some("https://s3.example.com".to_string()),
            access_key_id: "access-key".to_string(),
            secret_access_key: "secret-key".to_string(),
            session_token: None,
            key_prefix: Some("erp/uploads".to_string()),
            public_base_url: public_base_url.to_string(),
            force_path_style: false,
        }
    }
}
