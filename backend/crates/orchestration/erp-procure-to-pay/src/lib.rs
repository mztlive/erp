//! 采购及费用来源的跨域业务编排。
mod authorization;
mod errors;
pub use errors::{Error, Result};
pub mod procure_to_pay;
pub mod purchase_correspondence;
