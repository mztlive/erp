//! Owned support repositories composed from persistence-core.

mod background_job;
mod background_job_item;
mod bulk_selection_item;
mod bulk_selection_snapshot;
mod document_attachment;
mod external_identity_map;
mod external_identity_target;
mod file_asset;
mod source_system;

pub use background_job::BackgroundJobRepository;
pub use background_job_item::BackgroundJobItemRepository;
pub use bulk_selection_item::BulkSelectionItemRepository;
pub use bulk_selection_snapshot::BulkSelectionSnapshotRepository;
pub use document_attachment::DocumentAttachmentRepository;
pub use external_identity_map::ExternalIdentityMapRepository;
pub use external_identity_target::ExternalIdentityTargetRepository;
pub use file_asset::FileAssetRepository;
pub use source_system::SourceSystemRepository;
