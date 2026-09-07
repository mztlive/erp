//! Support MongoDB repositories and accessors.

pub mod bulk_job;
pub mod extensions;
pub mod file_asset;
pub mod owned;
pub mod source_registry;

pub use bulk_job::{
    BackgroundJobFilter, BackgroundJobItemRow, BackgroundJobRegistration, BackgroundJobRow,
    BulkJobRepository, BulkSelectionItemRow, BulkSelectionSnapshotFilter, BulkSelectionSnapshotRow,
};
pub use extensions::{BulkJobExt, FileAssetExt, SourceRegistryExt};
pub use file_asset::{FileAssetFilter, FileAssetRow};
pub use owned::{
    BackgroundJobItemRepository, BackgroundJobRepository, BulkSelectionItemRepository,
    BulkSelectionSnapshotRepository, DocumentAttachmentRepository, ExternalIdentityMapRepository,
    ExternalIdentityTargetRepository, FileAssetRepository, SourceSystemRepository,
};
pub use source_registry::{
    external_id_key_bson, ExpireTargetsOutcome, ExternalIdentityMapFilter, ExternalIdentityMapRow,
    SourceRegistryRepository, SourceSystemFilter, SourceSystemRow,
};

pub mod supplier_connection_job;
