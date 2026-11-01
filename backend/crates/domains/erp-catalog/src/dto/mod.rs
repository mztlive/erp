//! 供 Handler 复用的商品 HTTP 与应用 DTO。

pub mod catalog;

pub(crate) use catalog::validate_sales_price_range;
pub use catalog::*;
