//! 审计持久化、附件事实与可售供给查询的消费端口。

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
