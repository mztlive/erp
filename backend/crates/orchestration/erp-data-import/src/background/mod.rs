//! 导入专用任务适配器、执行预算和不可变来源缓存。
pub(crate) mod import_budget;
pub(crate) mod import_source;
mod product_import_adapter;
mod supplier_import_adapter;
pub use product_import_adapter::ProductImportTaskAdapter;
pub use supplier_import_adapter::SupplierImportTaskAdapter;
