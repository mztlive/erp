//! Consumer ports for audit persistence, attachment facts and sellable supply names.

mod audit;
mod file_asset;
mod pending;
pub mod supply;

pub use audit::{CatalogAuditPort, FailClosedAuditPort, PreparedCatalogAudit};
pub use file_asset::{EmptyFileAssetFacts, FileAssetFact, FileAssetFactsPort};
pub use pending::{EmptyPendingAttachments, PendingAttachmentBatch};
