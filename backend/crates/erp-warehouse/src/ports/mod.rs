//! 身份事实、审计持久化与内容指纹的消费端口。

mod audit;
mod fingerprint;
mod identity;

pub use audit::{FailClosedAuditPort, PreparedWarehouseAudit, WarehouseAuditFacts, WarehouseAuditPort};
pub use fingerprint::{AttachmentFingerprintPort, FailClosedFingerprintPort};
pub use identity::{FailClosedIdentityFactPort, HandlerDuty, HandlerIdentityFact, IdentityFactPort};

mod directory;
pub use directory::WarehouseDirectoryAccess;
