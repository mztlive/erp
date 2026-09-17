//! Support MongoDB repositories and accessors.

mod background_job_cancel;
pub mod bulk_job;
pub mod extensions;
pub mod file_asset;
mod import_jobs;
pub mod owned;
mod page;
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
    ExpireTargetsOutcome, ExternalIdentityMapFilter, ExternalIdentityMapRow, SourceRegistryRepository,
    SourceSystemFilter, SourceSystemRow, external_id_key_bson,
};

pub mod supplier_connection_job;
