//! 销售、询价与选品的跨域业务编排。
mod errors;
pub use errors::{Error, Result};
pub mod order_to_cash;
pub mod sales_change;
pub mod sales_inquiry;
pub mod sales_selection;

pub mod services;

pub mod background;
