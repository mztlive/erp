//! 进程级 tracing subscriber 配置。

use tracing::info;
use tracing_appender::non_blocking::WorkerGuard;
use tracing_appender::rolling::{RollingFileAppender, Rotation};
use tracing_subscriber::{
    fmt::{self, format::FmtSpan},
    layer::SubscriberExt,
    util::SubscriberInitExt,
    EnvFilter, Layer,
};

/// Web API 的日志输出配置。
pub(crate) struct TracingConfig {
    pub(crate) env_filter: String,
    pub(crate) log_to_file: bool,
    pub(crate) log_directory: String,
    pub(crate) log_file_prefix: String,
    pub(crate) json_format: bool,
}

impl Default for TracingConfig {
    /// 返回默认值。
    ///
    /// # 返回
    /// 返回创建的实例。
    fn default() -> Self {
        Self {
            env_filter: "info".to_string(),
            log_to_file: false,
            log_directory: "logs".to_string(),
            log_file_prefix: "app".to_string(),
            json_format: false,
        }
    }
}

/// 使用指定配置初始化全局 tracing subscriber。
///
/// `RUST_LOG` 存在时优先使用环境变量，否则回退到 `config.env_filter`。
///
/// # 错误
/// # 返回值
/// 返回必须由进程入口持有的文件日志工作线程 guard；未启用文件日志时为 `None`。
///
/// # 错误
/// 创建滚动日志文件失败时返回错误。
pub(crate) fn init_tracing(
    config: TracingConfig,
) -> Result<Option<WorkerGuard>, Box<dyn std::error::Error + Send + Sync>> {
    let env_filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new(&config.env_filter));

    let stdout_layer = if config.json_format {
        fmt::layer()
            .json()
            .with_target(true)
            .with_thread_ids(true)
            .with_thread_names(true)
            .with_file(true)
            .with_line_number(true)
            .with_level(true)
            .with_span_events(FmtSpan::NEW | FmtSpan::CLOSE)
            .boxed()
    } else {
        fmt::layer()
            .with_target(true)
            .with_thread_ids(true)
            .with_thread_names(false)
            .with_file(true)
            .with_line_number(true)
            .with_level(true)
            .with_ansi(true)
            .with_span_events(FmtSpan::NEW | FmtSpan::CLOSE)
            .pretty()
            .boxed()
    };

    let mut layers = vec![stdout_layer];

    let file_guard = if config.log_to_file {
        let file_appender = RollingFileAppender::builder()
            .rotation(Rotation::DAILY)
            .filename_prefix(&config.log_file_prefix)
            .filename_suffix("log")
            .build(&config.log_directory)?;
        let (file_writer, guard) = tracing_appender::non_blocking(file_appender);

        let file_layer = if config.json_format {
            fmt::layer()
                .json()
                .with_target(true)
                .with_thread_ids(true)
                .with_thread_names(true)
                .with_file(true)
                .with_line_number(true)
                .with_level(true)
                .with_ansi(false)
                .with_span_events(FmtSpan::NEW | FmtSpan::CLOSE)
                .with_writer(file_writer)
                .boxed()
        } else {
            fmt::layer()
                .with_target(true)
                .with_thread_ids(true)
                .with_thread_names(false)
                .with_file(true)
                .with_line_number(true)
                .with_level(true)
                .with_ansi(false)
                .with_span_events(FmtSpan::NEW | FmtSpan::CLOSE)
                .with_writer(file_writer)
                .boxed()
        };

        layers.push(file_layer);
        Some(guard)
    } else {
        None
    };

    tracing_subscriber::registry()
        .with(env_filter)
        .with(layers)
        .init();

    info!(
        "Tracing initialized with configuration: json_format={}, log_to_file={}, log_directory={}",
        config.json_format, config.log_to_file, config.log_directory
    );

    Ok(file_guard)
}

#[cfg(test)]
mod tests {
    use super::TracingConfig;

    #[test]
    fn default_config_avoids_unbounded_file_logs() {
        let config = TracingConfig::default();

        assert_eq!(config.env_filter, "info");
        assert!(!config.log_to_file);
        assert_eq!(config.log_directory, "logs");
        assert_eq!(config.log_file_prefix, "app");
        assert!(!config.json_format);
    }
}
