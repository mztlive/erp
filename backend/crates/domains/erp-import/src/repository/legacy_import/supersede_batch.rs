//! 确认取代批量持久化：新试算失效确认的调用方事务写回（INT-R28）。
//!
//! 本文件只提供调用方事务内的批量 CAS 写回：已就地 `invalidate` 的确认。
//! 空集合零写，任一版本冲突由调用方事务整体回滚。全部使用调用方 executor，不开事务。

use erp_core::ids::LegacyImportConfirmationId;
use persistence_core::{Executor, Result};

use crate::entity::legacy_import::{ConfirmationStatus, LegacyImportConfirmation};

/// 导入确认集合仓储扩展：确认取代批量写回。
#[allow(async_fn_in_trait)]
pub trait LegacyImportConfirmationSupersedeBatchExt {
    /// 用原事务逐项按版本写回已由 `invalidate` 置为失效且替代者为本轮确认 ID 的记录；空集合或无匹配时不访问数据库。不裁决哪些事实应取代，也不开事务；CAS 未命中或写入失败须由调用方整体回滚。
    async fn persist_invalidated_confirmations(
        &self,
        confirmations: &mut [LegacyImportConfirmation],
        replacement_id: &LegacyImportConfirmationId,
        executor: &mut dyn Executor,
    ) -> Result<()>;
}

impl LegacyImportConfirmationSupersedeBatchExt
    for persistence_core::Repository<'_, LegacyImportConfirmation>
{
    async fn persist_invalidated_confirmations(
        &self,
        confirmations: &mut [LegacyImportConfirmation],
        replacement_id: &LegacyImportConfirmationId,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        for confirmation in confirmations.iter_mut().filter(|item| {
            item.status == ConfirmationStatus::Invalidated
                && item.replacement_confirmation_id.as_ref() == Some(replacement_id)
        }) {
            self.update(confirmation, executor).await?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use erp_core::ids::LegacyImportConfirmationId;

    use crate::entity::legacy_import::{ConfirmationStatus, LegacyImportConfirmation};

    #[test]
    fn invalidated_filter_pins_current_replacement_round() {
        let replacement = LegacyImportConfirmationId::new("c-new");
        let matches = |confirmation: &LegacyImportConfirmation| {
            confirmation.status == ConfirmationStatus::Invalidated
                && confirmation.replacement_confirmation_id.as_ref() == Some(&replacement)
        };
        assert!(!matches(&pending_confirmation()));
        assert!(matches(&invalidated_confirmation("c-new")));
        assert!(!matches(&invalidated_confirmation("c-old")));
    }

    fn pending_confirmation() -> LegacyImportConfirmation {
        use erp_core::ids::{LegacyImportBatchId, WorkItemId};

        use crate::entity::legacy_import::LegacyImportConfirmationData;
        LegacyImportConfirmation::new(
            LegacyImportConfirmationId::new("c-1"),
            LegacyImportConfirmationData {
                batch_id: LegacyImportBatchId::new("batch-1"),
                confirmation_scope: "SALES".to_string(),
                owner_role: "role-sales".to_string(),
                batch_version: 1,
                trial_version: 1,
                import_rule_version: "rule-1".to_string(),
                work_item_id: WorkItemId::new("work-item-1"),
            },
        )
        .unwrap()
    }

    fn invalidated_confirmation(replacement: &str) -> LegacyImportConfirmation {
        use erp_core::common::time::Instant;
        let mut confirmation = pending_confirmation();
        confirmation
            .invalidate(LegacyImportConfirmationId::new(replacement), Instant::from_unix_secs(1_700_000_000))
            .unwrap();
        confirmation
    }
}
