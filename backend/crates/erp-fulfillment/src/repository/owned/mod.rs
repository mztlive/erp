//! 履约拥有仓储；通用访问委托 persistence-core。

mod customer_acceptance;
mod delivery;
mod electronic_delivery;
mod purchase_receipt;
mod service_fulfillment;

pub use customer_acceptance::CustomerAcceptanceRepository;
pub use delivery::DeliveryRepository;
pub use electronic_delivery::ElectronicDeliveryRepository;
pub use purchase_receipt::PurchaseReceiptRepository;
pub use service_fulfillment::ServiceFulfillmentRepository;
