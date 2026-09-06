//! Consumer port for a prepared attachment batch that catalog commands may persist.

use std::collections::HashSet;
use std::sync::Arc;

use async_trait::async_trait;
use erp_core::ids::FileAssetId;
use mongodb::Database;
use persistence_core::Executor;

use crate::error::Result;

/// Facts and persist hook for files constructed before a catalog business transaction.
///
/// Catalog receives attachment IDs and consume-once checks. It must not receive
/// `FileAsset` entities, S3 clients or audit aggregates. Outer `PendingFileAssets`
/// preparation stays in `erp-processes`.
#[async_trait]
pub trait PendingAttachmentBatch: Send + Sync {
    /// Replace a temporary file reference with the formal asset id for this batch.
    fn resolve_id(&self, id: &mut FileAssetId, used: &mut HashSet<String>) -> erp_core::Result<bool>;

    /// Reject leftover uploaded files that the business command did not consume.
    fn ensure_all_used(&self, used: &HashSet<String>) -> erp_core::Result<()>;

    /// Return whether `id` belongs to this prepared batch.
    fn contains_id(&self, id: &FileAssetId) -> bool;

    /// Persist prepared file metadata on the caller-chosen executor.
    async fn persist(&self, db: &Database, executor: &mut dyn Executor) -> Result<()>;

    /// Return whether this batch prepared any files to persist.
    fn is_empty(&self) -> bool;
}

/// Empty batch used by single-domain commands that do not register new files.
#[derive(Debug, Default, Clone, Copy)]
pub struct EmptyPendingAttachments;

#[async_trait]
impl PendingAttachmentBatch for EmptyPendingAttachments {
    fn resolve_id(&self, id: &mut FileAssetId, _used: &mut HashSet<String>) -> erp_core::Result<bool> {
        let value = id.as_ref().trim();
        if !value.starts_with("pending-file:") {
            return Ok(false);
        }
        let token = value.strip_prefix("pending-file:").unwrap_or("");
        if token.trim().is_empty() {
            return Err(erp_core::Error::from("临时文件引用 token 不能为空"));
        }
        Err(erp_core::Error::from("业务命令引用了未上传的文件"))
    }

    fn ensure_all_used(&self, used: &HashSet<String>) -> erp_core::Result<()> {
        if used.is_empty() {
            Ok(())
        } else {
            Err(erp_core::Error::from("存在未被业务命令引用的上传文件"))
        }
    }

    fn contains_id(&self, _id: &FileAssetId) -> bool {
        false
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

    async fn persist(&self, db: &Database, executor: &mut dyn Executor) -> Result<()> {
        (**self).persist(db, executor).await
    }

    fn is_empty(&self) -> bool {
        (**self).is_empty()
    }
}
