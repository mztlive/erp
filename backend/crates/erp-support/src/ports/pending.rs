//! Consumer port for a prepared attachment batch that business commands may persist.

use std::collections::HashSet;
use std::sync::Arc;

use async_trait::async_trait;
use erp_core::ids::FileAssetId;
use mongodb::Database;
use persistence_core::Executor;

use crate::entity::file_asset::{PendingFileReferenceSet, SensitivityClass};
use crate::error::Result;

/// Facts and persist hook for files constructed before a business transaction.
///
/// Business domains receive attachment IDs, sensitivity and consume-once checks.
/// They must not receive `FileAsset` entities, S3 clients or audit aggregates.
#[async_trait]
pub trait PendingAttachmentBatch: Send + Sync {
    /// Replace a temporary file reference with the formal asset id for this batch.
    fn resolve_id(&self, id: &mut FileAssetId, used: &mut HashSet<String>) -> erp_core::Result<bool>;

    /// Reject leftover uploaded files that the business command did not consume.
    fn ensure_all_used(&self, used: &HashSet<String>) -> erp_core::Result<()>;

    /// Return whether `id` belongs to this prepared batch.
    fn contains_id(&self, id: &FileAssetId) -> bool;

    /// Return the sensitivity captured when this batch prepared `id`.
    fn sensitivity(&self, id: &FileAssetId) -> Option<SensitivityClass>;

    /// Persist prepared file metadata on the caller-chosen executor.
    ///
    /// # Errors
    /// Duplicate-key or underlying write failures.
    async fn persist(&self, db: &Database, executor: &mut dyn Executor) -> Result<()>;

    /// Return whether this batch prepared any files to persist.
    fn is_empty(&self) -> bool;
}

/// Empty batch used by single-domain commands that do not register new files.
#[derive(Debug, Default, Clone, Copy)]
pub struct EmptyPendingAttachments;

#[async_trait]
impl PendingAttachmentBatch for EmptyPendingAttachments {
    fn resolve_id(&self, id: &mut FileAssetId, used: &mut HashSet<String>) -> erp_core::Result<bool> {
        PendingFileReferenceSet::default().resolve_id(id, used)
    }

    fn ensure_all_used(&self, used: &HashSet<String>) -> erp_core::Result<()> {
        PendingFileReferenceSet::default().ensure_all_used(used)
    }

    fn contains_id(&self, id: &FileAssetId) -> bool {
        PendingFileReferenceSet::default().contains_id(id)
    }

    fn sensitivity(&self, id: &FileAssetId) -> Option<SensitivityClass> {
        PendingFileReferenceSet::default().sensitivity(id)
    }

    async fn persist(&self, _db: &Database, _executor: &mut dyn Executor) -> Result<()> {
        Ok(())
    }

    fn is_empty(&self) -> bool {
        true
    }
}

#[async_trait]
impl PendingAttachmentBatch for Arc<dyn PendingAttachmentBatch> {
    fn resolve_id(&self, id: &mut FileAssetId, used: &mut HashSet<String>) -> erp_core::Result<bool> {
        (**self).resolve_id(id, used)
    }

    fn ensure_all_used(&self, used: &HashSet<String>) -> erp_core::Result<()> {
        (**self).ensure_all_used(used)
    }

    fn contains_id(&self, id: &FileAssetId) -> bool {
        (**self).contains_id(id)
    }

    fn sensitivity(&self, id: &FileAssetId) -> Option<SensitivityClass> {
        (**self).sensitivity(id)
    }

    async fn persist(&self, db: &Database, executor: &mut dyn Executor) -> Result<()> {
        (**self).persist(db, executor).await
    }

    fn is_empty(&self) -> bool {
        (**self).is_empty()
    }
}
