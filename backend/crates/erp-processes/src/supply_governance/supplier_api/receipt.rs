//! 回执先写、审计后写、最后读取任务编号；复用同一调用方 Executor。
use application_core::AuditActor;
use erp_audit::{AuditActorLogs, AuditExt};
use erp_supply::dto::supplier_api::SupplierConnectionCommandResult;
use erp_supply::entity::supplier_api::{
    SupplierApiConnection, SupplierCommandOutcome, SupplierConnectionAction, SupplierConnectionCommandReceipt,
};
use erp_supply::service::supplier_api::SupplierApiService;
use erp_supply::service::supplier_api::command::CommandIdentity;
use erp_support::BulkJobExt;
use persistence_core::Executor;

use crate::Result;

pub(super) struct CommandReceiptWrite<'a> {
    pub(super) connection: &'a SupplierApiConnection,
    pub(super) action: SupplierConnectionAction,
    pub(super) identity: &'a CommandIdentity,
    pub(super) outcome: SupplierCommandOutcome,
    pub(super) job_id: Option<String>,
    pub(super) actor: &'a AuditActor,
}
pub(super) async fn persist_command_receipt(
    db: &mongodb::Database,
    write: CommandReceiptWrite<'_>,
    executor: &mut dyn persistence_core::Executor,
) -> Result<SupplierConnectionCommandResult> {
    let CommandReceiptWrite { connection, action, identity, outcome, job_id, actor } = write;
    let receipt = SupplierApiService::prepare_command_receipt(
        connection,
        action,
        identity,
        outcome,
        job_id.clone(),
        actor.id(),
    )?;
    let audit = actor.clone().resource_log_with_id(
        identity.audit_id.clone(),
        &format!("supplier_api_connection.{}", action.as_str().to_ascii_lowercase()),
        "supplier_api_connection",
        connection.base.id.clone(),
        Some(format!("request_sha256={}", identity.fingerprint)),
    )?;
    let job_no =
        persist_receipt(&MongoReceiptWrite(db), &receipt, &audit, job_id.as_deref(), executor).await?;
    Ok(SupplierConnectionCommandResult {
        outcome,
        action,
        operation_id: receipt.base.id,
        connection_version: connection.base.version,
        job_id,
        job_no,
        audit_event_id: identity.audit_id.clone(),
    })
}
trait ReceiptWritePort: Sync {
    type Receipt: Sync;
    type Audit: Sync;
    fn receipt(
        &self,
        receipt: &Self::Receipt,
        executor: &mut dyn Executor,
    ) -> impl std::future::Future<Output = Result<()>> + Send;
    fn audit(
        &self,
        audit: &Self::Audit,
        executor: &mut dyn Executor,
    ) -> impl std::future::Future<Output = Result<()>> + Send;
    fn job_no(
        &self,
        job_id: &str,
        executor: &mut dyn Executor,
    ) -> impl std::future::Future<Output = Result<Option<String>>> + Send;
}
async fn persist_receipt<P: ReceiptWritePort>(
    port: &P,
    receipt: &P::Receipt,
    audit: &P::Audit,
    job_id: Option<&str>,
    executor: &mut dyn Executor,
) -> Result<Option<String>> {
    port.receipt(receipt, executor).await?;
    port.audit(audit, executor).await?;
    match job_id {
        Some(id) => port.job_no(id, executor).await,
        None => Ok(None),
    }
}
struct MongoReceiptWrite<'a>(&'a mongodb::Database);
impl ReceiptWritePort for MongoReceiptWrite<'_> {
    type Receipt = SupplierConnectionCommandReceipt;
    type Audit = erp_audit::AuditLog;
    async fn receipt(&self, receipt: &Self::Receipt, executor: &mut dyn Executor) -> Result<()> {
        Ok(SupplierApiService::new(self.0.clone()).persist_command_receipt(receipt, executor).await?)
    }
    async fn audit(&self, audit: &Self::Audit, executor: &mut dyn Executor) -> Result<()> {
        self.0.audit_logs().create(audit, executor).await?;
        Ok(())
    }
    async fn job_no(&self, job_id: &str, executor: &mut dyn Executor) -> Result<Option<String>> {
        Ok(self.0.background_jobs().find_by_id(job_id, executor).await?.map(|job| job.job_no))
    }
}
#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use super::*;
    use crate::Error;
    struct TestExecutor {
        visits: usize,
    }
    impl Executor for TestExecutor {
        fn session(&mut self) -> Option<&mut mongodb::ClientSession> {
            self.visits += 1;
            None
        }
    }
    struct RecordingWrite {
        identity: usize,
        fail_at: Option<usize>,
        calls: Mutex<Vec<&'static str>>,
    }
    impl RecordingWrite {
        fn step(&self, name: &'static str, executor: &mut dyn Executor) -> Result<()> {
            assert_eq!(executor as *mut dyn Executor as *mut () as usize, self.identity);
            assert!(executor.session().is_none());
            let mut calls = self.calls.lock().unwrap();
            let index = calls.len();
            calls.push(name);
            if self.fail_at == Some(index) {
                return Err(Error::ConflictError(format!("failed {name}")));
            }
            Ok(())
        }
    }
    impl ReceiptWritePort for RecordingWrite {
        type Receipt = u8;
        type Audit = u8;
        async fn receipt(&self, receipt: &u8, executor: &mut dyn Executor) -> Result<()> {
            assert_eq!(*receipt, 1);
            self.step("receipt", executor)
        }
        async fn audit(&self, audit: &u8, executor: &mut dyn Executor) -> Result<()> {
            assert_eq!(*audit, 2);
            self.step("audit", executor)
        }
        async fn job_no(&self, id: &str, executor: &mut dyn Executor) -> Result<Option<String>> {
            assert_eq!(id, "job-1");
            self.step("job_no", executor)?;
            Ok(Some("JOB-001".to_string()))
        }
    }
    #[tokio::test]
    async fn receipt_audit_and_job_read_share_executor_in_original_order() {
        let mut executor = TestExecutor { visits: 0 };
        let port = RecordingWrite {
            identity: &mut executor as *mut TestExecutor as usize,
            fail_at: None,
            calls: Mutex::new(Vec::new()),
        };
        assert_eq!(
            persist_receipt(&port, &1, &2, Some("job-1"), &mut executor).await.unwrap().as_deref(),
            Some("JOB-001")
        );
        assert_eq!(*port.calls.lock().unwrap(), ["receipt", "audit", "job_no"]);
        assert_eq!(executor.visits, 3);
    }
    #[tokio::test]
    async fn receipt_failure_stops_before_each_later_write_or_read() {
        let expected = ["receipt", "audit", "job_no"];
        for fail_at in 0..3 {
            let mut executor = TestExecutor { visits: 0 };
            let port = RecordingWrite {
                identity: &mut executor as *mut TestExecutor as usize,
                fail_at: Some(fail_at),
                calls: Mutex::new(Vec::new()),
            };
            let error = persist_receipt(&port, &1, &2, Some("job-1"), &mut executor).await.unwrap_err();
            assert!(
                matches!(error,Error::ConflictError(message) if message==format!("failed {}",expected[fail_at]))
            );
            assert_eq!(*port.calls.lock().unwrap(), expected[..=fail_at]);
            assert_eq!(executor.visits, fail_at + 1);
        }
    }
    #[tokio::test]
    async fn synchronous_receipt_never_reads_a_background_job() {
        let mut executor = TestExecutor { visits: 0 };
        let port = RecordingWrite {
            identity: &mut executor as *mut TestExecutor as usize,
            fail_at: None,
            calls: Mutex::new(Vec::new()),
        };
        assert_eq!(persist_receipt(&port, &1, &2, None, &mut executor).await.unwrap(), None);
        assert_eq!(*port.calls.lock().unwrap(), ["receipt", "audit"]);
        assert_eq!(executor.visits, 2);
    }
}
