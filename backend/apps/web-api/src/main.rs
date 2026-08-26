//! web-api 进程入口。
//!
//! 本 crate 只保留一个二进制目标。原先库目标上的 `dead_code` 豁免一并移到这里：
//! 若干 Handler / AppState 字段尚未被启动路径读取，但属于已接线的 HTTP 面。

#![allow(dead_code)]

mod app_state;
mod core;

use app_state::AppState;
use config::{Config, S3Config, SafeConfig};
use core::{
    routes,
    tracing::{init_tracing, TracingConfig},
};
use std::net::SocketAddr;
use storage::{S3Storage, S3StorageConfig};
use tracing::{info, warn};

type Result<T> = std::result::Result<T, Box<dyn std::error::Error + Send + Sync>>;

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
        Self {
            uri: config.database.uri.clone(),
            db_name: config.database.db_name.clone(),
        }
    }
}

/// 程序入口。
///
/// # 返回
/// 返回执行结果，`Ok` 表示成功，`Err` 表示失败。
///
/// # 错误
/// 当验证失败或底层操作失败时返回错误。
#[tokio::main]
async fn main() -> Result<()> {
    let tracing_config = TracingConfig {
        env_filter: std::env::var("RUST_LOG").unwrap_or_else(|_| "info".to_string()),
        log_to_file: env_flag("LOG_TO_FILE"),
        log_directory: "logs".to_string(),
        log_file_prefix: "web-api".to_string(),
        json_format: std::env::var("LOG_FORMAT").unwrap_or_default() == "json",
        otel_enabled: otel_exporter_enabled(),
        service_name: non_empty_env("OTEL_SERVICE_NAME").unwrap_or_else(|| "erp-web-api".to_string()),
    };

    let _tracing_guard = init_tracing(tracing_config)?;

    let config = SafeConfig::from_args().await?;
    let app_port = config.snapshot().app.port;

    info!("Starting application with config: {}", app_port);

    start(config).await
}

/// 判断环境变量是否显式启用布尔开关。
fn env_flag(name: &str) -> bool {
    std::env::var(name)
        .ok()
        .is_some_and(|value| matches!(value.trim().to_ascii_lowercase().as_str(), "1" | "true" | "yes"))
}

/// 读取非空环境变量；空白值按未配置处理。
fn non_empty_env(name: &str) -> Option<String> {
    std::env::var(name)
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

/// 按标准 OTLP endpoint 与 SDK 禁用开关决定是否创建 exporter。
///
/// 未配置 endpoint 时保持原有纯日志模式，避免本地开发默认连接固定 Collector。
fn otel_exporter_enabled() -> bool {
    let traces_endpoint = non_empty_env("OTEL_EXPORTER_OTLP_TRACES_ENDPOINT");
    let endpoint = non_empty_env("OTEL_EXPORTER_OTLP_ENDPOINT");
    should_enable_otel(
        env_flag("OTEL_SDK_DISABLED"),
        traces_endpoint.as_deref(),
        endpoint.as_deref(),
    )
}

/// 对已解析的 OpenTelemetry 开关与 endpoint 执行确定性判定。
fn should_enable_otel(sdk_disabled: bool, traces_endpoint: Option<&str>, endpoint: Option<&str>) -> bool {
    !sdk_disabled
        && traces_endpoint
            .or(endpoint)
            .is_some_and(|value| !value.trim().is_empty())
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
async fn start(cfg: SafeConfig) -> Result<()> {
    let config = cfg.snapshot();
    let storage = build_storage(&config.s3)?;

    let (_, db) = database::connect(&config.database.uri, &config.database.db_name).await?;

    let app_port = config.app.port;

    let state = AppState::new(db, cfg.clone(), storage);
    database::ensure_transaction_support(&state.db()).await?;
    database::ensure_indexes(&state.db()).await?;
    ensure_registered_approval_policies()?;
    services::iam::ensure_root_role(&state.rbac()).await?;
    services::iam::ensure_predefined_roles(&state.rbac()).await?;

    spawn_config_watcher(
        state.clone(),
        DbConfigKey::from_config(&config),
        config.app.secret.clone(),
        config.s3.clone(),
    );

    let outbox_worker = state.start_approval_outbox_worker();
    let result = run_app(app_port, state).await;
    outbox_worker.stop().await;
    result
}

/// 启动前穷尽校验全部固定单据类型政策。
///
/// 任一类型缺少政策时按部署不变量失败关闭，不得启动旧定义 bootstrap。
///
/// # 错误
/// 政策缺失或权限字符串无法解析时返回服务错误。
fn ensure_registered_approval_policies() -> std::result::Result<(), services::Error> {
    for document_type in services::approval::policy::ALL_DOCUMENT_TYPES {
        services::approval::policy::policy_of(document_type)?;
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
    state: AppState,
    initial_db: DbConfigKey,
    initial_secret: String,
    initial_storage: S3Config,
) {
    let mut receiver = state.subscribe_config();

    tokio::spawn(async move {
        let active_db = initial_db.clone();
        let mut observed_db = initial_db;
        let mut current_secret = initial_secret;
        let active_storage = initial_storage.clone();
        let mut observed_storage = initial_storage;

        loop {
            let next_config = receiver.borrow().clone();
            let next_db = DbConfigKey::from_config(&next_config);

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
                if storage_change_requires_restart(&active_storage, &next_config.s3) {
                    warn!(
                        restart_required = true,
                        bucket_changed = active_storage.bucket != next_config.s3.bucket,
                        region_changed = active_storage.region != next_config.s3.region,
                        endpoint_changed = active_storage.endpoint != next_config.s3.endpoint,
                        credentials_changed = active_storage.access_key_id != next_config.s3.access_key_id
                            || active_storage.secret_access_key != next_config.s3.secret_access_key
                            || active_storage.session_token != next_config.s3.session_token,
                        key_prefix_changed = active_storage.key_prefix != next_config.s3.key_prefix,
                        public_base_url_changed =
                            active_storage.public_base_url != next_config.s3.public_base_url,
                        force_path_style_changed =
                            active_storage.force_path_style != next_config.s3.force_path_style,
                        "S3 configuration change ignored at runtime; restart the application to apply it"
                    );
                } else {
                    info!(
                        restart_required = false,
                        "S3 configuration reverted to the active startup value"
                    );
                }
                observed_storage = next_config.s3.clone();
            }

            if jwt_secret_changed(&current_secret, &next_config.app.secret) {
                state.invalidate_jwt_engine().await;
                current_secret = next_config.app.secret;
                info!("JWT engine cache reset");
            }

            let Ok(()) = receiver.changed().await else {
                info!("Config watcher stopped: sender dropped");
                return;
            };
        }
    });
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

/// 判断 JWT 密钥是否发生变化。
///
/// # 参数
/// * `current` - 当前 JWT 引擎使用的密钥
/// * `requested` - 配置中心当前请求的密钥
///
/// # 返回
/// 两者不一致时返回 `true`。
fn jwt_secret_changed(current: &str, requested: &str) -> bool {
    current != requested
}

/// 启动应用程序并监听指定端口
///
/// # 参数
/// * `app_port` - 应用程序监听的端口号
/// * `state` - 应用程序状态，包含数据库连接、配置等信息
///
/// # 示例
///
/// ```
/// let state = /* 通过数据库、配置与权限映射构建 AppState */;
/// run_app(3000, state).await;
/// ```
///
/// # 错误
/// 如果无法绑定到指定端口或无法启动服务，此函数将返回错误。
async fn run_app(app_port: u16, state: AppState) -> Result<()> {
    let app = routes::create(state);

    let listener = tokio::net::TcpListener::bind(format!("0.0.0.0:{}", app_port)).await?;

    axum::serve(listener, app.into_make_service_with_connect_info::<SocketAddr>())
        .with_graceful_shutdown(shutdown_signal())
        .await?;
    Ok(())
}

/// 等待进程终止信号，使 HTTP 服务、后台任务和 telemetry 按顺序关闭。
async fn shutdown_signal() {
    #[cfg(unix)]
    wait_for_unix_shutdown().await;

    #[cfg(not(unix))]
    wait_for_ctrl_c().await;

    info!("Shutdown signal received");
}

/// 等待跨平台 Ctrl-C 信号；注册失败时记录错误并触发安全关闭。
async fn wait_for_ctrl_c() {
    if let Err(error) = tokio::signal::ctrl_c().await {
        warn!(%error, "Failed to install Ctrl-C signal handler");
    }
}

/// 在 Unix 环境同时等待 SIGTERM 与 Ctrl-C，覆盖容器标准停止流程。
#[cfg(unix)]
async fn wait_for_unix_shutdown() {
    let terminate = async {
        let Ok(mut signal) = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate()) else {
            warn!("Failed to install SIGTERM signal handler");
            std::future::pending::<()>().await;
            return;
        };
        signal.recv().await;
    };

    tokio::select! {
        _ = wait_for_ctrl_c() => {}
        _ = terminate => {}
    }
}

#[cfg(test)]
mod tests {
    use super::{
        database_change_requires_restart, ensure_registered_approval_policies, env_flag, jwt_secret_changed,
        should_enable_otel, storage_change_requires_restart, DbConfigKey,
    };
    use config::S3Config;

    /// 构造测试使用的数据库配置摘要。
    fn database_key(uri: &str, db_name: &str) -> DbConfigKey {
        DbConfigKey {
            uri: uri.to_string(),
            db_name: db_name.to_string(),
        }
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
    fn jwt_secret_change_should_remain_hot_reloadable() {
        assert!(jwt_secret_changed("old-secret", "new-secret"));
        assert!(!jwt_secret_changed("same-secret", "same-secret"));
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
