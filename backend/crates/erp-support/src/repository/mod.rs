//! Support MongoDB repositories and accessors.

mod background_job_cancel;
pub mod bulk_job;
pub mod extensions;
pub mod file_asset;
mod import_jobs;
pub mod owned;
mod page;
pub mod prelude;
pub mod source_registry;
pub mod supplier_connection_job;

pub use background_job_cancel::BackgroundJobRepositoryCancelExt;
pub use bulk_job::{
    BackgroundJobFilter, BackgroundJobItemRepositoryExt, BackgroundJobItemRow, BackgroundJobRegistration,
    BackgroundJobRepositoryExt, BackgroundJobRow, BulkJobRepository, BulkSelectionItemRepositoryExt,
    BulkSelectionItemRow, BulkSelectionSnapshotFilter, BulkSelectionSnapshotRepositoryExt,
    BulkSelectionSnapshotRow,
};
pub use extensions::{BulkJobExt, FileAssetExt, SourceRegistryExt};
pub use file_asset::{
    DocumentAttachmentRepositoryExt, FileAssetFilter, FileAssetRepositoryExt, FileAssetRow,
};
pub use import_jobs::{BackgroundJobItemRepositoryImportExt, BackgroundJobRepositoryImportExt};
pub use owned::{
    BackgroundJobItemRepository, BackgroundJobRepository, BulkSelectionItemRepository,
    BulkSelectionSnapshotRepository, DocumentAttachmentRepository, ExternalIdentityMapRepository,
    ExternalIdentityTargetRepository, FileAssetRepository, SourceSystemRepository,
};
pub use source_registry::{
    ExpireTargetsOutcome, ExternalIdentityMapFilter, ExternalIdentityMapRepositoryExt,
    ExternalIdentityMapRow, ExternalIdentityTargetRepositoryExt, SourceRegistryRepository,
    SourceSystemFilter, SourceSystemRepositoryExt, SourceSystemRow, external_id_key_bson,
};
pub use supplier_connection_job::BackgroundJobRepositorySupplierConnectionExt;
