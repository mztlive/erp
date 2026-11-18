//! 同一份准备批次直接实现商品附件 Port。

use std::collections::HashSet;

use async_trait::async_trait;
use erp_core::ids::FileAssetId;
use erp_support::PendingAttachmentBatch as SupportBatch;
use mongodb::Database;
use persistence_core::Executor;

use crate::PendingFileAssets;

#[async_trait]
impl erp_catalog::PendingAttachmentBatch for PendingFileAssets {
    fn resolve_id(&self, id: &mut FileAssetId, used: &mut HashSet<String>) -> erp_core::Result<bool> {
        SupportBatch::resolve_id(self, id, used)
    }

    fn ensure_all_used(&self, used: &HashSet<String>) -> erp_core::Result<()> {
        SupportBatch::ensure_all_used(self, used)
    }

    fn contains_id(&self, id: &FileAssetId) -> bool {
        SupportBatch::contains_id(self, id)
    }

    async fn persist(&self, db: &Database, executor: &mut dyn Executor) -> erp_catalog::Result<()> {
        SupportBatch::persist(self, db, executor).await.map_err(map_support_to_catalog)
    }

    fn is_empty(&self) -> bool {
        SupportBatch::is_empty(self)
    }
}

fn map_support_to_catalog(error: erp_support::Error) -> erp_catalog::Error {
    match error {
        erp_support::Error::Internal(message) => erp_catalog::Error::Internal(message),
        erp_support::Error::NotFound(message) => erp_catalog::Error::NotFound(message),
        erp_support::Error::ValidationError(message) => erp_catalog::Error::ValidationError(message),
        erp_support::Error::InvalidInput(message) => erp_catalog::Error::InvalidInput(message),
        erp_support::Error::BusinessLogicError(message) => erp_catalog::Error::BusinessLogicError(message),
        erp_support::Error::ConflictError(message) => erp_catalog::Error::ConflictError(message),
        erp_support::Error::ReceiptDuplicate(error) => erp_catalog::Error::ReceiptDuplicate(error),
        erp_support::Error::TransientTransaction(error) => erp_catalog::Error::TransientTransaction(error),
        erp_support::Error::Forbidden(message) => erp_catalog::Error::Forbidden(message),
        erp_support::Error::Unauthenticated(message) => erp_catalog::Error::Unauthenticated(message),
        erp_support::Error::Logic(error) => erp_catalog::Error::Logic(error),
        erp_support::Error::OutcomeUnknown(error) => erp_catalog::Error::OutcomeUnknown(error),
        erp_support::Error::RepositoryError(error) => erp_catalog::Error::RepositoryError(error),
    }
}
