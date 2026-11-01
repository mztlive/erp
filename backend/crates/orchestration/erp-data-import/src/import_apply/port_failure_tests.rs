//! 特征测试：写入端口失败时不得产生已完成的确认或工作项。

use async_trait::async_trait;
use erp_core::common::time::Instant;
use erp_core::ids::{LegacyImportBatchId, LegacyImportConfirmationId, WorkItemId};
use erp_import::{
    ConfirmationDecision, ConfirmationStatus, LegacyImportConfirmation, LegacyImportConfirmationData,
};
use persistence_core::{Executor, NoTransaction};

use crate::{Error, Result};

/// 用于证明确认与工作项在写入失败时不会前进的最小写入端口。
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

/// 先在本地应用领域决定，再经端口持久化。
///
/// 端口失败时返回错误，且不产生已完成结果。
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
    port.persist_confirmation_and_work_item(&confirmation, true, &mut executor).await?;
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
    assert!(result.is_err(), "write failure must not produce a completed result");
    assert!(
        result.as_ref().err().map(ToString::to_string).unwrap().contains("write failed"),
        "caller must observe the write-port error"
    );
}
