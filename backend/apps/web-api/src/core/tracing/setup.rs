//! 进程级 tracing subscriber 配置。

use opentelemetry::{global, trace::TracerProvider as _, KeyValue};
use opentelemetry_otlp::SpanExporter;
use opentelemetry_sdk::{propagation::TraceContextPropagator, trace::SdkTracerProvider, Resource};
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
pub struct TracingConfig {
    /// `RUST_LOG` 未设置时使用的默认过滤表达式。
    pub env_filter: String,
    /// 是否同时写入滚动文件日志。
    pub log_to_file: bool,
    /// 滚动文件日志目录。
    pub log_directory: String,
    /// 滚动文件日志文件名前缀。
    pub log_file_prefix: String,
    /// 是否输出 JSON 格式日志。
    pub json_format: bool,
    /// 是否启用 OTLP trace 导出。
    pub otel_enabled: bool,
    /// OpenTelemetry `service.name`。
    pub service_name: String,
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
            otel_enabled: false,
            service_name: "erp-web-api".to_string(),
        }
    }
}

/// 持有日志写线程与 OpenTelemetry provider 的进程级生命周期。
///
/// guard 被释放时会先关闭 tracer provider，确保批量缓存中的 span 在运行时退出前完成导出。
pub struct TracingGuard {
    _file_guard: Option<WorkerGuard>,
    tracer_provider: Option<SdkTracerProvider>,
}

impl Drop for TracingGuard {
    /// 关闭 tracer provider 并刷新仍在批量队列中的 span。
    fn drop(&mut self) {
        let Some(provider) = self.tracer_provider.take() else {
            return;
        };

        if let Err(error) = provider.shutdown() {
            tracing::warn!(%error, "OpenTelemetry tracer provider shutdown failed");
        }
    }
}

/// 使用指定配置初始化全局 tracing subscriber。
///
/// `RUST_LOG` 存在时优先使用环境变量，否则回退到 `config.env_filter`。
///
/// # 返回值
/// 返回必须由进程入口持有至退出的 tracing 生命周期 guard。
///
/// # 错误
/// 创建滚动日志文件或 OTLP exporter 失败时返回错误。
pub fn init_tracing(config: TracingConfig) -> Result<TracingGuard, Box<dyn std::error::Error + Send + Sync>> {
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

    global::set_text_map_propagator(TraceContextPropagator::new());
    let tracer_provider = build_tracer_provider(&config)?;
    let telemetry_layer = tracer_provider.as_ref().map(|provider| {
        let tracer = provider.tracer("web-api");
        tracing_opentelemetry::layer().with_tracer(tracer)
    });

    tracing_subscriber::registry()
        .with(env_filter)
        .with(layers)
        .with(telemetry_layer)
        .init();

    info!(
        json_format = config.json_format,
        log_to_file = config.log_to_file,
        log_directory = %config.log_directory,
        otel_enabled = config.otel_enabled,
        service_name = %config.service_name,
        "Tracing initialized"
    );

    Ok(TracingGuard {
        _file_guard: file_guard,
        tracer_provider,
    })
}

/// 按配置创建 OTLP tracer provider；禁用时不构建网络 exporter。
///
/// # 参数
/// * `config` - 日志与 OpenTelemetry 进程配置
///
/// # 返回值
/// 启用时返回批量导出的 provider，禁用时返回 `None`。
///
/// # 错误
/// OTLP exporter 参数无效时返回构建错误。
fn build_tracer_provider(
    config: &TracingConfig,
) -> Result<Option<SdkTracerProvider>, Box<dyn std::error::Error + Send + Sync>> {
    if !config.otel_enabled {
        return Ok(None);
    }

    let exporter = SpanExporter::builder().with_tonic().build()?;
    let resource = Resource::builder()
        .with_service_name(config.service_name.clone())
        .with_attribute(KeyValue::new("service.version", env!("CARGO_PKG_VERSION")))
        .build();
    let provider = SdkTracerProvider::builder()
        .with_batch_exporter(exporter)
        .with_resource(resource)
        .build();
    global::set_tracer_provider(provider.clone());

    Ok(Some(provider))
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
        assert!(!config.otel_enabled);
        assert_eq!(config.service_name, "erp-web-api");
    }
}
