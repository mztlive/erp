//! 供应目录、来源媒体、供给治理及门户跨域命令。

mod errors;
pub mod wiring;
pub use errors::{Error, Result};
pub mod delivery_coverage_refresh;
pub mod integration_resolution;
pub mod product_source_sync;
pub mod sellable_projection_refresh;
pub mod source_media;
pub mod source_registry;
pub mod supplier_catalog_adoption;
pub mod supplier_catalog_callbacks;
pub mod supplier_connection_execution;
pub mod supplier_delivery_template;
pub mod supplier_portal;
pub mod supply_governance;

pub mod background;
