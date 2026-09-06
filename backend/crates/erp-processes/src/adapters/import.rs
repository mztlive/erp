//! Import query adapters that bind consumer ports to support bulk jobs.

use std::sync::Arc;

use async_trait::async_trait;
use erp_import::{BulkJobFactsPort, LegacyImportService};
use erp_support::BulkJobExt;
use mongodb::Database;
use persistence_core::Executor;

/// MongoDB adapter that reads background-job identity for import batch views.
#[derive(Clone)]
pub struct MongoImportBulkJobs {
    db: Database,
}

impl MongoImportBulkJobs {
    /// Bind the adapter to `db`.
    pub fn new(db: Database) -> Self {
        Self { db }
    }

    /// Wrap the adapter as a shared port.
    pub fn shared(db: Database) -> Arc<dyn BulkJobFactsPort> {
        Arc::new(Self::new(db))
    }
}

#[async_trait]
impl BulkJobFactsPort for MongoImportBulkJobs {
    async fn background_job_id_by_request_id(
        &self,
        request_id: &str,
        executor: &mut dyn Executor,
    ) -> erp_import::Result<Option<String>> {
        Ok(self
            .db
            .background_jobs()
            .find_by_request_id(request_id, executor)
            .await
            .map_err(erp_import::Error::from)?
            .map(|job| job.base.id))
    }
}

/// Construct an import query service with the bulk-job identity adapter.
pub fn legacy_import_service(db: Database) -> LegacyImportService {
    LegacyImportService::new(db.clone(), MongoImportBulkJobs::shared(db))
}

/// Construct the import-apply process that owns cross-domain transactions.
pub fn import_apply_service(db: Database) -> crate::import_apply::ImportApplyService {
    crate::import_apply::ImportApplyService::new(db)
}
