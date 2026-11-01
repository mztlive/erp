//! 不依赖业务领域的顺序轮询、停止排空和执行切片预算。

mod adapter;
mod budget;
mod periodic;
mod runner;

pub use adapter::{BackgroundTaskAdapter, TaskError, TaskResult};
pub use budget::ExecutionBudget;
pub use periodic::periodic;
pub use runner::BackgroundRunner;
