//! Consumer port for background-job identity facts owned by support.

use async_trait::async_trait;
use persistence_core::Executor;

use crate::error::{Error, Result};

/// Minimal background-job identity fact used by import batch views.
#[async_trait]
pub trait BulkJobFactsPort: Send + Sync {
    /// Look up a background job id by the import batch request identity.
    ///
    /// # Parameters
    /// * `request_id` - import batch number used as job request id
    /// * `executor` - caller-chosen data-access executor
    ///
    /// # Errors
    /// Underlying job lookup failures.
    async fn background_job_id_by_request_id(
        &self,
        request_id: &str,
        executor: &mut dyn Executor,
    ) -> Result<Option<String>>;
}

/// Fail-closed bulk-job port used when composition has not injected an adapter.
#[derive(Debug, Default, Clone, Copy)]
pub struct FailClosedBulkJobFacts;

#[async_trait]
impl BulkJobFactsPort for FailClosedBulkJobFacts {
    async fn background_job_id_by_request_id(
        &self,
        _request_id: &str,
        _executor: &mut dyn Executor,
    ) -> Result<Option<String>> {
        Err(Error::Internal("导入后台任务端口未接线".to_string()))
    }
}

#[cfg(test)]
mod tests {
    use persistence_core::NoTransaction;

    use super::{BulkJobFactsPort, FailClosedBulkJobFacts};

    #[tokio::test]
    async fn fail_closed_bulk_job_port_does_not_invent_job_identity() {
        let mut executor = NoTransaction;
        let error = FailClosedBulkJobFacts
            .background_job_id_by_request_id("IMP-1", &mut executor)
            .await
            .expect_err("unwired port must fail closed");
        assert!(error.to_string().contains("未接线"));
    }
}
