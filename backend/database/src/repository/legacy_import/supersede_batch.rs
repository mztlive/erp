//! 确认取代批量持久化：新试算失效与任务关闭的调用方事务写回（INT-R28）。
//!
//! 读取由 [`crate::repository::work_item::Repository::list_legacy_import_confirmations_by_ids`]
//! 单次 `$in` 完成；本文件只提供调用方事务内的批量 CAS 写回：已就地
//! `invalidate` 的确认与已就地 `close` 的任务。空集合零写，任一版本冲突
//! 由调用方事务整体回滚。全部使用调用方 executor，不开事务。

use entities::ids::LegacyImportConfirmationId;
use entities::legacy_import::{ConfirmationStatus, LegacyImportConfirmation};
use entities::work_item::WorkItem;

use super::super::Repository;
use crate::executor::Executor;
use crate::Result;

impl<'a> Repository<'a, LegacyImportConfirmation> {
    /// 批量写回本轮已失效的确认事实（INT-R28 批量写）。
    ///
    /// 调用方先经实体 `invalidate` 完成 `Pending → Invalidated` 迁移，
    /// 本方法只对 `replacement_confirmation_id` 等于本轮替代者的行执行
    /// 逐项 CAS 写回；空集合或无匹配行时零写，不访问数据库。
    /// 任一项版本冲突或写入失败时返回错误，由调用方事务整体回滚，
    /// 不产生部分提交。
    ///
    /// # 参数
    /// * `confirmations` - 调用方矩阵（就地更新版本号）
    /// * `replacement_id` - 本轮替代确认事实 ID
    /// * `executor` - 调用方事务执行器，必须位于同一事务中
    ///
    /// # 返回
    /// 返回执行结果，`Ok` 表示全部写回成功。
    ///
    /// # 错误
    /// 当任一行 CAS 未命中或 MongoDB 写入失败时返回错误。
    ///
    /// # 约束
    /// 不自行开启或提交事务；不做业务取代裁决，只持久化已失效行。
    pub async fn persist_invalidated_confirmations(
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

impl<'a> Repository<'a, WorkItem> {
    /// 批量写回已关闭的取代任务（INT-R28 批量写）。
    ///
    /// 调用方先经实体 `close` 完成开放任务关闭，本方法只执行逐项 CAS
    /// 写回；空集合零写，不访问数据库。已关闭任务由调用方过滤，
    /// 不传入本方法。任一项版本冲突或写入失败时返回错误，由调用方
    /// 事务整体回滚。
    ///
    /// # 参数
    /// * `work_items` - 已完成关闭 mutation 的任务（就地更新版本号）
    /// * `executor` - 调用方事务执行器，必须位于同一事务中
    ///
    /// # 返回
    /// 返回执行结果，`Ok` 表示全部写回成功。
    ///
    /// # 错误
    /// 当任一任务 CAS 未命中或 MongoDB 写入失败时返回错误。
    ///
    /// # 约束
    /// 不自行开启或提交事务；不做任务状态裁决，只持久化已关闭任务。
    pub async fn persist_closed_confirmation_work_items(
        &self,
        work_items: &mut [WorkItem],
        executor: &mut dyn Executor,
    ) -> Result<()> {
        if work_items.is_empty() {
            return Ok(());
        }
        for item in work_items.iter_mut() {
            self.update(item, executor).await?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use entities::ids::LegacyImportConfirmationId;
    use entities::legacy_import::{ConfirmationStatus, LegacyImportConfirmation};

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
        use entities::ids::{LegacyImportBatchId, WorkItemId};
        use entities::legacy_import::LegacyImportConfirmationData;
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
        use entities::common::time::Instant;
        let mut confirmation = pending_confirmation();
        confirmation
            .invalidate(
                LegacyImportConfirmationId::new(replacement),
                Instant::from_unix_secs(1_700_000_000),
            )
            .unwrap();
        confirmation
    }
}
