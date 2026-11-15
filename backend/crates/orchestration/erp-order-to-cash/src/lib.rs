//! 销售、询价与选品的跨域业务编排。
mod errors;
pub(crate) mod repository;
pub use errors::{Error, Result};
pub mod order_to_cash;
pub mod sales_change;
pub mod sales_inquiry;
pub mod sales_selection;

pub mod services;

pub mod background;

/// server 装配使用的询价事实与静态映射。
pub mod authorization_bindings {
    pub use crate::repository::sales_inquiry_access::{
        inquiry_facts, inquiry_mapping, procurement_facts, procurement_mapping,
    };
}
