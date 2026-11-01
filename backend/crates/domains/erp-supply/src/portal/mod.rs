//! 供应商门户申请、定向报价资格及受供应商归属约束的供给命令。
mod application;
mod confirm;
mod dto;
mod grants;
pub mod indexes;
mod quote_target;
mod repository;
mod service;
mod validation;

pub use application::{
    ApplicationDecision, ApplicationStatus, FrozenOfferingSubmission, OfferingApplication,
    OfferingApplicationResult, QuoteAccessDecision, QuoteAccessGrant,
};
pub use confirm::{ConfirmedOfferingInput, ConfirmedOfferingResult};
pub use dto::{
    ApplicationKind, OfferingApplicationSnapshot, PortalAvailabilityFact, PortalAvailabilityInput,
    PortalAvailabilityStatus, PortalAvailabilityUpdateResult, PortalQuoteInput,
};
pub use quote_target::QuoteTargetVersion;
pub use repository::{PortalCommandReceipt, PortalSupplyExt};
pub use service::{PortalOfferingService, PortalTermsValidationPort, validate_portal_terms};
pub use validation::{
    validate_portal_available_quantity, validate_portal_packaging_price, validate_portal_quantities,
    validate_portal_reported_at,
};
#[cfg(test)]
mod tests;
