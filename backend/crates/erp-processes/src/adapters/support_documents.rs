//! Composition adapter: support reads registered business-document ids via workflow.

use std::sync::Arc;

use async_trait::async_trait;
use erp_support::BusinessDocumentPort;
use erp_workflow::DocumentRegistryExt;
use mongodb::Database;
use persistence_core::Executor;

/// MongoDB adapter that exposes registered document ids without workflow types.
#[derive(Clone)]
pub struct MongoBusinessDocument {
    db: Database,
}

impl MongoBusinessDocument {
    /// Bind the adapter to `db`.
    ///
    /// # Parameters
    /// * `db` - MongoDB database used to read the document registry
    ///
    /// # Returns
    /// Adapter implementing [`BusinessDocumentPort`].
    pub fn new(db: Database) -> Self {
        Self { db }
    }

    /// Wrap the adapter as a shared port.
    pub fn shared(db: Database) -> Arc<dyn BusinessDocumentPort> {
        Arc::new(Self::new(db))
    }
}

#[async_trait]
impl BusinessDocumentPort for MongoBusinessDocument {
    async fn ensure_registered(
        &self,
        document_id: &str,
        executor: &mut dyn Executor,
    ) -> erp_support::Result<()> {
        self.db
            .business_documents()
            .find_by_id(document_id, executor)
            .await
            .map_err(erp_support::Error::from)?
            .ok_or_else(|| erp_support::Error::NotFound("业务单据未注册".to_string()))?;
        Ok(())
    }

    async fn find_registered_ids(
        &self,
        ids: &[String],
        executor: &mut dyn Executor,
    ) -> erp_support::Result<Vec<String>> {
        if ids.is_empty() {
            return Ok(Vec::new());
        }
        let documents = self
            .db
            .business_documents()
            .list_active_by_ids(ids, executor)
            .await
            .map_err(erp_support::Error::from)?;
        Ok(documents.into_iter().map(|document| document.base.id).collect())
    }
}
