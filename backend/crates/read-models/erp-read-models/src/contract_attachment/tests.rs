use std::sync::Mutex;

use erp_core::AccountKind;
use persistence_core::testing::SnapshotExecutor;
use persistence_core::{ExecutionBoundary, NoTransaction};

use super::*;

struct Reads {
    denied: bool,
    bound: bool,
    asset_failure: bool,
    advance_during_asset: bool,
    fail_delivery: bool,
    calls: Mutex<Vec<(&'static str, usize)>>,
}
impl Default for Reads {
    fn default() -> Self {
        Self {
            denied: false,
            bound: true,
            asset_failure: false,
            advance_during_asset: false,
            fail_delivery: false,
            calls: Mutex::new(vec![]),
        }
    }
}
impl Reads {
    fn record(&self, step: &'static str, executor: &mut dyn Executor) {
        self.calls.lock().unwrap().push((step, executor as *mut dyn Executor as *mut () as usize));
    }
    fn steps(&self) -> Vec<&'static str> {
        let calls = self.calls.lock().unwrap();
        assert!(calls.iter().all(|(_, pointer)| *pointer == calls[0].1));
        calls.iter().map(|(step, _)| *step).collect()
    }
}
#[async_trait]
impl AttachmentRead for Reads {
    type Proof = ExecutionBoundary;
    type Asset = &'static str;

    async fn query(&self, _: &AuditActor, executor: &mut dyn Executor) -> Result<ExecutionBoundary> {
        self.record("query", executor);
        if self.denied {
            return Err(Error::Forbidden("contract-denied".into()));
        }
        executor.next_read();
        Ok(executor.boundary().unwrap())
    }
    async fn bound(
        &self,
        proof: &ExecutionBoundary,
        contract: &str,
        file: &str,
        executor: &mut dyn Executor,
    ) -> Result<bool> {
        self.record("revision-binding", executor);
        assert!(proof.is_current(executor));
        assert_eq!((contract, file), ("contract", "file"));
        Ok(self.bound)
    }
    async fn asset(
        &self,
        file: &str,
        actor: &AuditActor,
        preview: bool,
        executor: &mut dyn Executor,
    ) -> Result<&'static str> {
        self.record(if preview { "preview-and-audit" } else { "metadata" }, executor);
        assert_eq!((file, actor.id()), ("file", "reader"));
        if self.asset_failure {
            return Err(Error::Internal("asset-unavailable".into()));
        }
        if self.advance_during_asset {
            executor.next_read();
        }
        Ok("authorized-file")
    }
    async fn validate(&self, proof: &ExecutionBoundary, executor: &mut dyn Executor) -> Result<()> {
        self.record("validate", executor);
        if self.fail_delivery || !proof.is_current(executor) {
            return Err(Error::ConflictError("DATA_SCOPE_CHANGED".into()));
        }
        Ok(())
    }
}

fn actor() -> AuditActor {
    AuditActor::new("reader".into(), "reader".into(), AccountKind::Admin)
}

#[tokio::test]
async fn metadata_and_preview_bind_the_exact_file_and_complete_delivery_in_the_original_snapshot() {
    for preview in [false, true] {
        let port = Reads::default();
        let value = read(&port, "contract", "file", &actor(), preview, &mut SnapshotExecutor::default())
            .await
            .unwrap();
        assert_eq!(value, "authorized-file");
        assert_eq!(
            port.steps(),
            ["query", "revision-binding", if preview { "preview-and-audit" } else { "metadata" }, "validate"]
        );
    }
}

#[tokio::test]
async fn denied_contract_or_foreign_revision_never_reads_or_audits_the_asset() {
    let denied = Reads { denied: true, ..Default::default() };
    assert!(
        matches!(read(&denied, "contract", "file", &actor(), true, &mut SnapshotExecutor::default()).await, Err(Error::Forbidden(value)) if value == "contract-denied")
    );
    assert_eq!(denied.steps(), ["query"]);
    let foreign = Reads { bound: false, ..Default::default() };
    assert!(
        matches!(read(&foreign, "contract", "file", &actor(), true, &mut SnapshotExecutor::default()).await, Err(Error::NotFound(value)) if value == "合同附件不存在或无权查看")
    );
    assert_eq!(foreign.steps(), ["query", "revision-binding"]);
}

#[tokio::test]
async fn asset_audit_failure_or_invalid_delivery_discards_the_unreleased_result() {
    let unavailable = Reads { asset_failure: true, ..Default::default() };
    assert!(
        matches!(read(&unavailable, "contract", "file", &actor(), true, &mut SnapshotExecutor::default()).await, Err(Error::Internal(value)) if value == "asset-unavailable")
    );
    assert_eq!(unavailable.steps(), ["query", "revision-binding", "preview-and-audit"]);
    for port in [
        Reads { advance_during_asset: true, ..Default::default() },
        Reads { fail_delivery: true, ..Default::default() },
    ] {
        assert!(
            matches!(read(&port, "contract", "file", &actor(), false, &mut SnapshotExecutor::default()).await, Err(Error::ConflictError(value)) if value == "DATA_SCOPE_CHANGED")
        );
        assert_eq!(port.steps(), ["query", "revision-binding", "metadata", "validate"]);
    }
    let port = Reads::default();
    assert!(matches!(
        read(&port, "contract", "file", &actor(), false, &mut NoTransaction).await,
        Err(Error::ValidationError(_))
    ));
    assert!(port.calls.lock().unwrap().is_empty());
}
