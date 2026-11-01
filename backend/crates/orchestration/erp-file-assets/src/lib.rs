//! 在业务事务中批量持久化文件资产与对应审计的跨域能力。
mod errors;
mod pending;
pub use errors::{Error, Result};
pub use pending::PendingFileAssets;

mod catalog;
pub use catalog::CatalogPendingAttachments;
