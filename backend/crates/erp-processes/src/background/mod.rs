//! 统一后台任务执行器：单轮询循环按适配器分发导入、导出等任务。
//!
//! 前台只做任务投递，所有执行都由本模块认领，避免前台直调与轮询器并发写入同一任务。

mod adapter;
mod product_import_adapter;
mod runner;
mod sales_selection_adapter;

pub use adapter::BackgroundTaskAdapter;
pub use product_import_adapter::ProductImportTaskAdapter;
pub use runner::BackgroundRunner;
pub use sales_selection_adapter::SalesSelectionTaskAdapter;
