//! Characterizing tests: a write-port failure must not produce a completed confirmation/WorkItem.

use crate::{Error, Result};
use async_trait::async_trait;
use erp_core::common::time::Instant;
use erp_core::ids::{LegacyImportBatchId, LegacyImportConfirmationId, WorkItemId};
use erp_import::{
    ConfirmationDecision, ConfirmationStatus, LegacyImportConfirmation, LegacyImportConfirmationData,
};
use persistence_core::{Executor, NoTransaction};

/// Minimal write port used to prove confirmation/WorkItem do not advance on failure.
#[async_trait]
trait ConfirmationWritePort: Send + Sync {
    async fn persist_confirmation_and_work_item(
        &self,
        confirmation: &LegacyImportConfirmation,
        work_item_completed: bool,
        executor: &mut dyn Executor,
    ) -> Result<()>;
}

struct FailingConfirmationWrite;

#[async_trait]
impl ConfirmationWritePort for FailingConfirmationWrite {
    async fn persist_confirmation_and_work_item(
        &self,
        _confirmation: &LegacyImportConfirmation,
        _work_item_completed: bool,
        _executor: &mut dyn Executor,
    ) -> Result<()> {
        Err(Error::Internal("confirmation write failed".to_string()))
    }
}

/// Apply a domain decision locally, then persist through the port.
///
/// On port failure the function returns Err and does not yield a completed result.
async fn complete_through_port(
    mut confirmation: LegacyImportConfirmation,
    port: &dyn ConfirmationWritePort,
) -> Result<(ConfirmationStatus, bool)> {
    confirmation.decide(
        ConfirmationDecision::ConfirmScope,
        "user-1",
        Instant::from_unix_secs(1_700_000_000),
        None,
        Some("确认".to_string()),
    )?;
    let mut executor = NoTransaction;
    port.persist_confirmation_and_work_item(&confirmation, true, &mut executor)
        .await?;
    Ok((confirmation.status, true))
}

#[tokio::test]
async fn write_port_failure_does_not_return_advanced_confirmation_or_work_item() {
    let confirmation = LegacyImportConfirmation::new(
        LegacyImportConfirmationId::new("confirmation-1"),
        LegacyImportConfirmationData {
            batch_id: LegacyImportBatchId::new("batch-1"),
            confirmation_scope: "SALES".to_string(),
            owner_role: "role-sales".to_string(),
            batch_version: 1,
            trial_version: 2,
            import_rule_version: "rule-1".to_string(),
            work_item_id: WorkItemId::new("work-item-1"),
        },
    )
    .unwrap();
    assert_eq!(confirmation.status, ConfirmationStatus::Pending);
    let result = complete_through_port(confirmation, &FailingConfirmationWrite).await;
    assert!(
        result.is_err(),
        "write failure must not produce a completed result"
    );
    assert!(
        result
            .as_ref()
            .err()
            .map(ToString::to_string)
            .unwrap()
            .contains("write failed"),
        "caller must observe the write-port error"
    );
}
