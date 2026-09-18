//! Collection-scoped aliases for [`persistence_core::Repository`].
//!
//! Domain-specific methods are extension traits on the generic repository.

pub type CustomerAcceptanceRepository<'a> =
    persistence_core::Repository<'a, crate::entity::fulfillment::CustomerAcceptance>;

pub type DeliveryRepository<'a> = persistence_core::Repository<'a, crate::entity::fulfillment::Delivery>;

pub type ElectronicDeliveryRepository<'a> =
    persistence_core::Repository<'a, crate::entity::fulfillment::ElectronicDelivery>;

pub type PurchaseReceiptRepository<'a> =
    persistence_core::Repository<'a, crate::entity::fulfillment::PurchaseReceipt>;

pub type ServiceFulfillmentRepository<'a> =
    persistence_core::Repository<'a, crate::entity::fulfillment::ServiceFulfillment>;
