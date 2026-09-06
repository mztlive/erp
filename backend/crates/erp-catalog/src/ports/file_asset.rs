//! Consumer port for catalog media and brand-logo file existence.

use async_trait::async_trait;
use erp_core::ids::FileAssetId;
use persistence_core::Executor;

use crate::error::Result;

/// Minimal file-asset existence fact consumed by catalog.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileAssetFact {
    /// File asset stable id.
    pub id: String,
}

/// Catalog reads media/logo existence through this consumer port.
///
/// File-asset entities and collections remain owned by `erp-support`.
#[async_trait]
pub trait FileAssetFactsPort: Send + Sync {
    /// Load an undeleted file-asset fact by id.
    ///
    /// # Parameters
    /// * `asset_id` - file asset id
    /// * `executor` - caller-chosen executor
    ///
    /// # Returns
    /// `None` when the asset does not exist or is deleted.
    async fn find_by_id(
        &self,
        asset_id: &FileAssetId,
        executor: &mut dyn Executor,
    ) -> Result<Option<FileAssetFact>>;

    /// Return file-asset ids that are not yet registered, preserving input order.
    ///
    /// Empty input returns an empty vector. Duplicate missing ids appear once.
    async fn missing_ids(
        &self,
        asset_ids: &[FileAssetId],
        executor: &mut dyn Executor,
    ) -> Result<Vec<FileAssetId>>;
}

/// Empty attachment lookup used by isolated catalog unit tests.
#[derive(Debug, Default, Clone, Copy)]
pub struct EmptyFileAssetFacts;

#[async_trait]
impl FileAssetFactsPort for EmptyFileAssetFacts {
    async fn find_by_id(
        &self,
        _asset_id: &FileAssetId,
        _executor: &mut dyn Executor,
    ) -> Result<Option<FileAssetFact>> {
        Ok(None)
    }

    async fn missing_ids(
        &self,
        asset_ids: &[FileAssetId],
        _executor: &mut dyn Executor,
    ) -> Result<Vec<FileAssetId>> {
        let mut missing = Vec::new();
        let mut seen = std::collections::HashSet::new();
        for id in asset_ids {
            if seen.insert(id.to_string()) {
                missing.push(id.clone());
            }
        }
        Ok(missing)
    }
}
