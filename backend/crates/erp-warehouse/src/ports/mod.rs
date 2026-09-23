//! Consumer ports for identity facts, audit persistence and attachment fingerprints.

mod audit;
mod fingerprint;
mod identity;

pub use audit::{FailClosedAuditPort, PreparedWarehouseAudit, WarehouseAuditFacts, WarehouseAuditPort};
pub use fingerprint::{AttachmentFingerprintPort, FailClosedFingerprintPort};
pub use identity::{FailClosedIdentityFactPort, HandlerDuty, HandlerIdentityFact, IdentityFactPort};

mod directory;
pub use directory::WarehouseDirectoryAccess;
