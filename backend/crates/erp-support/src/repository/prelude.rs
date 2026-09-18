//! Extension traits for collection repositories.

pub use super::background_job_cancel::BackgroundJobRepositoryCancelExt;
pub use super::bulk_job::{
    BackgroundJobItemRepositoryExt, BackgroundJobRepositoryExt, BulkSelectionItemRepositoryExt,
    BulkSelectionSnapshotRepositoryExt,
};
pub use super::file_asset::{DocumentAttachmentRepositoryExt, FileAssetRepositoryExt};
pub use super::import_jobs::{BackgroundJobItemRepositoryImportExt, BackgroundJobRepositoryImportExt};
pub use super::source_registry::{
    ExternalIdentityMapRepositoryExt, ExternalIdentityTargetRepositoryExt, SourceSystemRepositoryExt,
};
pub use super::supplier_connection_job::BackgroundJobRepositorySupplierConnectionExt;
