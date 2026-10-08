//! [`persistence_core::Repository`] 的集合范围别名。
//!
//! 领域方法以泛型仓储上的扩展 trait 提供。

pub type CustomerAcceptanceRepository<'a> =
    persistence_core::Repository<'a, crate::entity::fulfillment::CustomerAcceptance>;

pub type DeliveryRepository<'a> = persistence_core::Repository<'a, crate::entity::fulfillment::Delivery>;

pub type ElectronicDeliveryRepository<'a> =
    persistence_core::Repository<'a, crate::entity::fulfillment::ElectronicDelivery>;

pub type PurchaseReceiptRepository<'a> =
    persistence_core::Repository<'a, crate::entity::fulfillment::PurchaseReceipt>;

pub type ServiceFulfillmentRepository<'a> =
    persistence_core::Repository<'a, crate::entity::fulfillment::ServiceFulfillment>;
