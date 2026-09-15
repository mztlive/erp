//! Consumer port for contract PDF attachment existence.

use async_trait::async_trait;
use erp_core::ids::FileAssetId;
use persistence_core::Executor;

use crate::error::{Error, Result};

/// Minimum file-asset fact required to confirm a contract PDF association.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileAssetFact {
    /// File asset stable id.
    pub id: String,
}

/// Port contract uses to confirm PDF attachments without depending on `erp-support`.
///
/// File-asset writes stay in `erp-processes`; this port is a read-only existence fact.
#[async_trait]
pub trait FileAssetFactsPort: Send + Sync {
    /// Load one undeleted file-asset fact.
    ///
    /// # Parameters
    /// * `attachment_id` - file asset id
    /// * `executor` - caller-chosen executor
    ///
    /// # Returns
    /// `None` when the asset does not exist or is deleted.
    ///
    /// # Errors
    /// Adapter query failures.
    async fn find_by_id(
        &self,
        attachment_id: &FileAssetId,
        executor: &mut dyn Executor,
    ) -> Result<Option<FileAssetFact>>;
}

/// Empty attachment lookup used by isolated unit tests.
#[derive(Debug, Default, Clone, Copy)]
pub struct EmptyFileAssetFacts;

#[async_trait]
impl FileAssetFactsPort for EmptyFileAssetFacts {
    async fn find_by_id(
        &self,
        _attachment_id: &FileAssetId,
        _executor: &mut dyn Executor,
    ) -> Result<Option<FileAssetFact>> {
        Ok(None)
    }
}

/// Fail-closed attachment lookup used when composition has not injected an adapter.
#[derive(Debug, Default, Clone, Copy)]
pub struct FailClosedFileAssetFacts;

#[async_trait]
impl FileAssetFactsPort for FailClosedFileAssetFacts {
    async fn find_by_id(
        &self,
        _attachment_id: &FileAssetId,
        _executor: &mut dyn Executor,
    ) -> Result<Option<FileAssetFact>> {
        Err(Error::Internal("附件端口未接线".to_string()))
    }
}

#[cfg(test)]
mod tests {
    use erp_core::ids::FileAssetId;
    use persistence_core::NoTransaction;
    use tokio::runtime::Builder;

    use super::{EmptyFileAssetFacts, FileAssetFactsPort};

    #[test]
    fn empty_file_asset_port_confirms_missing_attachment() {
        let runtime = Builder::new_current_thread().build().expect("runtime");
        runtime.block_on(async {
            let missing = EmptyFileAssetFacts
                .find_by_id(&FileAssetId::new("file-missing"), &mut NoTransaction)
                .await
                .expect("empty port");
            assert_eq!(missing, None);
        });
    }
}
