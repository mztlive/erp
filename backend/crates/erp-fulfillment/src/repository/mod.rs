//! 履约 MongoDB 拥有仓储与事务内持久化。

pub mod extensions;
pub mod fulfillment;
pub mod owned;

pub use extensions::FulfillmentExt;

#[cfg(test)]
mod serialization_contract;
