mod app_state;
mod core;

use app_state::AppState;
use config::{Config, SafeConfig};
use core::{
    routes,
    tracing::{init_tracing, TracingConfig},
};
use std::{
    io::{Error as IoError, ErrorKind},
    net::SocketAddr,
    path::{Path, PathBuf},
};
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
        .is_some_and(|value| matches!(value.to_ascii_lowercase().as_str(), "1" | "true" | "yes"))
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
    let upload_path = prepare_upload_directory(&config.app.upload_path).await?;

    let (_, db) = database::connect(&config.database.uri, &config.database.db_name).await?;

    let app_port = config.app.port;

    let state = AppState::new(db, cfg.clone(), upload_path);
    database::ensure_transaction_support(&state.db()).await?;
    database::ensure_indexes(&state.db()).await?;
    services::iam::ensure_root_role(&state.rbac()).await?;

    spawn_config_watcher(
        state.clone(),
        DbConfigKey::from_config(&config),
        config.app.secret.clone(),
        config.app.upload_path.clone(),
    );

    run_app(app_port, state).await
}

/// 创建并规范化启动期间固定使用的专用上传目录。
///
/// # 参数
/// * `configured_path` - 已通过配置层词法校验的上传目录
///
/// # 返回
/// 返回创建后的 canonical 目录路径。
///
/// # 错误
/// 目录无法创建或解析，最终不是目录，或解析到文件系统根/当前工作目录及其祖先时返回错误。
async fn prepare_upload_directory(configured_path: &str) -> Result<PathBuf> {
    tokio::fs::create_dir_all(configured_path).await?;
    let upload_path = tokio::fs::canonicalize(configured_path).await?;
    let current_directory = tokio::fs::canonicalize(std::env::current_dir()?).await?;
    if !is_dedicated_upload_directory(&upload_path, &current_directory).await? {
        return Err(IoError::new(
            ErrorKind::InvalidInput,
            "upload_path must resolve to a dedicated non-root directory",
        )
        .into());
    }
    Ok(upload_path)
}

/// 判断 canonical 上传路径是否为根目录以外、且不是当前工作目录或其祖先的真实目录。
async fn is_dedicated_upload_directory(path: &Path, current_directory: &Path) -> std::io::Result<bool> {
    let metadata = tokio::fs::metadata(path).await?;
    Ok(metadata.is_dir() && path.parent().is_some() && !current_directory.starts_with(path))
}

/// 启动配置监听并刷新可安全热更新的运行时状态。
///
/// 数据库与 RBAC 必须始终指向同一存储，因此数据库连接配置只在进程启动时生效；
/// 上传写入与只读静态服务也必须共用同一目录，因此 `upload_path` 同样固定为启动值。
/// 监听到这些变化时保留当前运行时，并记录需要重启的结构化日志。
///
/// # 参数
/// * `state` - 应用状态
/// * `initial_db` - 初始数据库配置摘要
/// * `initial_secret` - 初始 JWT 密钥
/// * `initial_upload_path` - 启动时使用的上传目录
fn spawn_config_watcher(
    state: AppState,
    initial_db: DbConfigKey,
    initial_secret: String,
    initial_upload_path: String,
) {
    let mut receiver = state.subscribe_config();

    tokio::spawn(async move {
        let active_db = initial_db.clone();
        let mut observed_db = initial_db;
        let mut current_secret = initial_secret;
        let active_upload_path = initial_upload_path.clone();
        let mut observed_upload_path = initial_upload_path;

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

            if next_config.app.upload_path != observed_upload_path {
                if upload_path_change_requires_restart(&active_upload_path, &next_config.app.upload_path) {
                    warn!(
                        restart_required = true,
                        "Upload path change ignored at runtime; restart the application to apply it"
                    );
                } else {
                    info!(
                        restart_required = false,
                        "Upload path reverted to the active startup value"
                    );
                }
                observed_upload_path = next_config.app.upload_path.clone();
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

/// 判断上传目录变化是否需要通过重启应用生效。
///
/// # 参数
/// * `active` - 启动时固定的上传目录
/// * `requested` - 配置中心请求的上传目录
///
/// # 返回
/// 两者不一致时返回 `true`。
fn upload_path_change_requires_restart(active: &str, requested: &str) -> bool {
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

    axum::serve(listener, app.into_make_service_with_connect_info::<SocketAddr>()).await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{
        database_change_requires_restart, env_flag, jwt_secret_changed, prepare_upload_directory,
        upload_path_change_requires_restart, DbConfigKey,
    };

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
    fn missing_environment_flag_is_disabled() {
        let name = format!("RS_PROJECT_TEMPLATE_MISSING_FLAG_{}", std::process::id());
        assert!(!env_flag(&name));
    }

    #[test]
    fn upload_path_change_should_require_restart() {
        assert!(upload_path_change_requires_restart("./uploads", "/mnt/uploads"));
        assert!(!upload_path_change_requires_restart("./uploads", "./uploads"));
    }

    #[tokio::test]
    async fn upload_directory_is_created_and_canonicalized() {
        let base = std::env::temp_dir().join(format!(
            "rs-project-template-upload-startup-{}",
            uuid::Uuid::new_v4()
        ));
        let configured = base.join("uploads");

        let prepared = prepare_upload_directory(configured.to_str().unwrap())
            .await
            .unwrap();

        assert_eq!(prepared, tokio::fs::canonicalize(&configured).await.unwrap());
        assert!(tokio::fs::metadata(&prepared).await.unwrap().is_dir());
        tokio::fs::remove_dir_all(base).await.unwrap();
    }

    #[tokio::test]
    async fn upload_directory_rejects_filesystem_root_and_current_directory() {
        let current_directory = std::env::current_dir().unwrap();
        let current_parent = current_directory.parent().unwrap();

        assert!(prepare_upload_directory("/").await.is_err());
        assert!(prepare_upload_directory(current_directory.to_str().unwrap())
            .await
            .is_err());
        assert!(prepare_upload_directory(current_parent.to_str().unwrap())
            .await
            .is_err());
    }
}
