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
    // 从启动或热更新快照提取 URI 和库名，用来判断当前 MongoDB 连接是否需要靠重启切换。
    fn from_config(config: &Config) -> Self {
        Self { uri: config.database.uri.clone(), db_name: config.database.db_name.clone() }
    }
}

/// 为二进制入口初始化日志、可选 OTLP 和配置，再按固定职责启动 Web API 或独立 Worker。初始化或服务退出出错时将错误交回入口。
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

// 读取启动环境的布尔开关，仅把去空白并忽略大小写后的 `1`、`true`、`yes` 视为开启。
fn env_flag(name: &str) -> bool {
    std::env::var(name)
        .ok()
        .is_some_and(|value| matches!(value.trim().to_ascii_lowercase().as_str(), "1" | "true" | "yes"))
}

// 读取并裁剪可选的环境配置；将缺失和纯空白统一视为未设置，供 telemetry 配置回退。
fn non_empty_env(name: &str) -> Option<String> {
    std::env::var(name).ok().map(|value| value.trim().to_string()).filter(|value| !value.is_empty())
}

// 启动时读取 trace 专用或通用 OTLP endpoint 以及 SDK 禁用项；没有 endpoint 时不向默认 Collector 发请求。
fn otel_exporter_enabled() -> bool {
    let traces_endpoint = non_empty_env("OTEL_EXPORTER_OTLP_TRACES_ENDPOINT");
    let endpoint = non_empty_env("OTEL_EXPORTER_OTLP_ENDPOINT");
    should_enable_otel(env_flag("OTEL_SDK_DISABLED"), traces_endpoint.as_deref(), endpoint.as_deref())
}

// 用已解析配置判定是否初始化 OTLP：SDK 必须启用，并优先检查 trace 专用 endpoint；否则使用通用值。
fn should_enable_otel(sdk_disabled: bool, traces_endpoint: Option<&str>, endpoint: Option<&str>) -> bool {
    !sdk_disabled && traces_endpoint.or(endpoint).is_some_and(|value| !value.trim().is_empty())
}

// 监听端口前校验调度配置和 MongoDB 事务能力，装配连接器和共享资源、初始化索引与审批政策。仅 API 引导身份数据，服务退出后停止配置监听。
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

// 在连接外部资源前拒绝未配置 `[xxl_job]` 的 Worker；Web API 不依赖调度节即可启动。
fn require_scheduler(role: ProcessRole, scheduler: Option<&XxlJobConfig>) -> Result<()> {
    if role.runs_workers() && scheduler.is_none() {
        return Err("Worker 必须配置 [xxl_job]".into());
    }
    Ok(())
}

// 在 API 启动阶段初始化 root 与身份基础数据，按需引导管理员并同步内建公司；Worker 不重复这些操作。
async fn initialize_identity(state: &Application, config: &Config) -> Result<()> {
    erp_identity::ensure_root_role(&state.rbac()).await?;
    ensure_identity_foundation(&state.rbac()).await?;
    bootstrap_initial_admin(state, config.bootstrap.initial_admin_password()).await?;
    let changed = state.party_service().sync_builtin_companies(state.sensitive_data()).await?;
    info!(changed, "内建公司主体同步完成");
    Ok(())
}

// 启动时给对象存储最多两秒完成首次连接预热，减少首个请求的 DNS/TLS 等待；失败只记日志，不阻止启动。
async fn warm_storage_connection(storage: &S3Storage) {
    match tokio::time::timeout(std::time::Duration::from_secs(2), storage.warm_connection()).await {
        Ok(Ok(())) => {},
        Ok(Err(_)) => warn!("Object storage connection warmup did not complete successfully"),
        Err(_) => warn!("Object storage connection warmup exceeded startup time limit"),
    }
}

// API 完成角色初始化后，只有配置初始密码才尝试补建 `admin` 并绑定 root；不会覆盖现有密码。同名非系统账号、非法密码或写库失败时返回错误。
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

// 启动前逐一验证所有固定单据类型能解析到有效审批政策；政策或权限值无效时中止，不用旧定义补齐。
fn ensure_registered_approval_policies() -> erp_workflow::Result<()> {
    for document_type in erp_workflow::service::approval::policy::ALL_DOCUMENT_TYPES {
        erp_workflow::service::approval::policy::policy_of(document_type)?;
    }
    Ok(())
}

// 按启动快照装配供上传及业务资产共用的 S3 客户端，绑定连接、凭证、键前缀和公开 URL；无效配置阻止装配。
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

// 启动后监听配置变化。数据库连接与 RBAC 必须共用启动存储，S3 和 XXL 执行器也固定使用启动配置；相关配置变化只记录重启提示，不热切换。
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

// 提醒对象存储配置需重启时，只记录发生变化的配置类别；不打印凭据或配置值。
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

// 把配置中心请求的 URI 和库名与启动值比较；任一项变化时提醒重启，不在监听器内重新连接。
fn database_change_requires_restart(active: &DbConfigKey, requested: &DbConfigKey) -> bool {
    active != requested
}

// 把请求的对象存储配置与正在使用的启动值比较，以决定是否提示重启而非热切换。
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

    // 为数据库配置热更新单测构造可分别改变 URI 或库名的比较值。
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

    // 为对象存储重启判定单测提供可改字段的启动配置基线。
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
