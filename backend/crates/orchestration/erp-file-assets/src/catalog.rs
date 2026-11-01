//! 支持域附件批次到商品附件 Port 的类型化转换。

use std::collections::HashSet;
use std::sync::Arc;

use async_trait::async_trait;
use erp_catalog::PendingAttachmentBatch;
use erp_core::ids::FileAssetId;
use mongodb::Database;
use persistence_core::Executor;

/// 把已准备的支持域附件批次暴露为商品端口。
pub struct CatalogPendingAttachments {
    inner: Arc<dyn erp_support::PendingAttachmentBatch>,
}

impl CatalogPendingAttachments {
    /// 把支持域待提交批次包装为商品域事务内待提交端口。
    ///
    /// # 参数
    /// * `inner` - 支持域已准备的待提交附件批次。
    ///
    /// # 返回
    /// 返回商品域 `PendingAttachmentBatch`。
    ///
    /// # 错误
    /// 不返回错误。
    pub fn from_support(
        inner: Arc<dyn erp_support::PendingAttachmentBatch>,
    ) -> Arc<dyn PendingAttachmentBatch> {
        Arc::new(Self { inner })
    }
}

#[async_trait]
impl PendingAttachmentBatch for CatalogPendingAttachments {
    fn resolve_id(&self, id: &mut FileAssetId, used: &mut HashSet<String>) -> erp_core::Result<bool> {
        self.inner.resolve_id(id, used)
    }

    fn ensure_all_used(&self, used: &HashSet<String>) -> erp_core::Result<()> {
        self.inner.ensure_all_used(used)
    }

    fn contains_id(&self, id: &FileAssetId) -> bool {
        self.inner.contains_id(id)
    }

    async fn persist(&self, db: &Database, executor: &mut dyn Executor) -> erp_catalog::Result<()> {
        self.inner.persist(db, executor).await.map_err(map_support_to_catalog)
    }

    fn is_empty(&self) -> bool {
        self.inner.is_empty()
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
