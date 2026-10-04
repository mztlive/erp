//! 履约 MongoDB 拥有仓储与事务内持久化。

mod command_receipt;
pub mod extensions;
pub mod fulfillment;
pub mod owned;
pub mod prelude;

pub use command_receipt::{FulfillmentCommandExt, FulfillmentCommandReceiptRepository};
pub use extensions::FulfillmentExt;
pub use fulfillment::{
    CustomerAcceptanceRepositoryExt, DeliveryRepositoryExt, ElectronicDeliveryRepositoryExt,
    PurchaseReceiptRepositoryExt, ServiceFulfillmentRepositoryExt,
};

#[cfg(test)]
mod serialization_contract;
