//! Catalog HTTP/application DTOs reused by handlers.

pub mod catalog;

pub(crate) use catalog::validate_sales_price_range;
pub use catalog::*;
