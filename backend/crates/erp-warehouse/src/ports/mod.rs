//! Consumer ports for identity facts, audit persistence and attachment fingerprints.

mod audit;
mod fingerprint;
mod identity;

pub use audit::{FailClosedAuditPort, PreparedWarehouseAudit, WarehouseAuditPort};
pub use fingerprint::{AttachmentFingerprintPort, FailClosedFingerprintPort};
pub use identity::{FailClosedIdentityFactPort, HandlerDuty, HandlerIdentityFact, IdentityFactPort};
