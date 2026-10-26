//! 统一后台任务执行器：单轮询循环按适配器分发导入、导出等任务。
//!
//! 前台只做任务投递，所有执行都由本模块认领，避免前台直调与轮询器并发写入同一任务。

mod adapter;
mod delivery_coverage_adapter;
pub(crate) mod import_budget;
pub(crate) mod import_source;
mod product_import_adapter;
mod sellable_projection_adapter;
mod supplier_catalog_adapter;
mod supplier_import_adapter;
pub use supplier_catalog_adapter::SupplierCatalogTaskAdapter;
pub use supplier_import_adapter::SupplierImportTaskAdapter;
mod runner;
mod sales_selection_adapter;

pub use adapter::BackgroundTaskAdapter;
pub use delivery_coverage_adapter::DeliveryCoverageRefreshAdapter;
pub use product_import_adapter::ProductImportTaskAdapter;
pub use runner::BackgroundRunner;
pub use sales_selection_adapter::SalesSelectionTaskAdapter;
pub use sellable_projection_adapter::SellableProjectionRefreshAdapter;
