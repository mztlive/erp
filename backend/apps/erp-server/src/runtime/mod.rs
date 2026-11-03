//! 进程信号、服务监督、停止排空与日志初始化。
mod role;
pub(crate) mod server;
pub(crate) mod telemetry;
pub use role::ProcessRole;
/// 应用启动和退出错误；业务错误由各领域定义。
pub type Result<T> = std::result::Result<T, Box<dyn std::error::Error + Send + Sync>>;
