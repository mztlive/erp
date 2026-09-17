//! Consumer ports for audit persistence, attachment facts and sellable supply queries.

mod audit;
mod data_scope;
mod file_asset;
mod pending;
pub mod supply;

pub use audit::{CatalogAuditPort, FailClosedAuditPort, PreparedCatalogAudit};
pub use data_scope::{
    CatalogDataScopePort, CatalogResolvedClause, CatalogResolvedScope, CatalogScopeObject,
    FailClosedCatalogDataScopePort,
};
pub use file_asset::{EmptyFileAssetFacts, FileAssetFact, FileAssetFactsPort};
pub use pending::{EmptyPendingAttachments, PendingAttachmentBatch};
