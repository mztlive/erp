//! 商品、供应商和合同导入的跨域任务与执行切片。
mod background;
pub mod brand_import;
pub use brand_import::BrandImportProcess;
pub mod contract_import;
pub mod customer_import;
pub mod data_transfer;
mod errors;
pub mod import_apply;
pub mod product_import;
mod repository;
pub mod sales_inquiry_import;
pub mod supplier_import;
mod xlsx;
pub use contract_import::{ContractImportProcess, ContractImportServices};
pub use errors::{Error, Result};
pub use import_apply::ImportApplyProcess;
pub use product_import::{ProductImportProcess, ProductImportServices};
pub use supplier_import::SupplierImportProcess;
