//! Collection-scoped aliases for [`persistence_core::Repository`].
//!
//! Domain-specific methods are extension traits on the generic repository.

pub type BackgroundJobRepository<'a> =
    persistence_core::Repository<'a, crate::entity::bulk_job::BackgroundJob>;
pub type BackgroundJobItemRepository<'a> =
    persistence_core::Repository<'a, crate::entity::bulk_job::BackgroundJobItem>;
pub type BulkSelectionItemRepository<'a> =
    persistence_core::Repository<'a, crate::entity::bulk_job::BulkSelectionItem>;
pub type BulkSelectionSnapshotRepository<'a> =
    persistence_core::Repository<'a, crate::entity::bulk_job::BulkSelectionSnapshot>;
pub type DocumentAttachmentRepository<'a> =
    persistence_core::Repository<'a, crate::entity::file_asset::DocumentAttachment>;
pub type ExternalIdentityMapRepository<'a> =
    persistence_core::Repository<'a, crate::entity::source_registry::ExternalIdentityMap>;
pub type ExternalIdentityTargetRepository<'a> =
    persistence_core::Repository<'a, crate::entity::source_registry::ExternalIdentityTarget>;
pub type FileAssetRepository<'a> = persistence_core::Repository<'a, crate::entity::file_asset::FileAsset>;
pub type SourceSystemRepository<'a> =
    persistence_core::Repository<'a, crate::entity::source_registry::SourceSystem>;
