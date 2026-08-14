//! Web API 的日志初始化与 HTTP 请求追踪。

mod middleware;
mod setup;

pub(crate) use middleware::trace_middleware;
pub use setup::{init_tracing, TracingConfig};
