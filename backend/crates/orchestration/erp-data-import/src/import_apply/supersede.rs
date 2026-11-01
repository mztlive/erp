//! 取代编排：作废导入确认，并经工作流关闭对应工作项。

use std::collections::HashMap;

use erp_core::common::time::Instant;
use erp_core::ids::LegacyImportConfirmationId;
use erp_import::repository::prelude::*;
use erp_import::{ConfirmationStatus, LegacyImportConfirmation, LegacyImportExt};
use erp_workflow::WorkItemExt;
use erp_workflow::entity::work_item::{WorkItem, WorkItemCloseData, WorkItemStatus};
use erp_workflow::repository::prelude::*;
use mongodb::Database;
use persistence_core::Executor;

use crate::{Error, Result};

/// 收集被更新试算取代的确认所引用的任务 ID。
///
/// # 参数
/// * `confirmations` - 当前确认矩阵。
/// * `replacement_trial_version` - 新试算版本。
///
/// # 返回
/// 按矩阵顺序返回被该试算取代的确认的 `work_item_id`，同一 ID 只保留首次出现。
///
/// # 错误
/// 不返回错误。
pub fn replaced_confirmation_work_item_ids(
    confirmations: &[LegacyImportConfirmation],
    replacement_trial_version: u32,
) -> Vec<erp_core::ids::WorkItemId> {
    use std::collections::HashSet;
    let mut ids = Vec::new();
    let mut seen = HashSet::new();
    for confirmation in confirmations.iter().filter(|item| item.is_replaced_by(replacement_trial_version)) {
        if seen.insert(confirmation.work_item_id.to_string()) {
            ids.push(confirmation.work_item_id.clone());
        }
    }
    ids
}

/// 把已加载的任务快照映射为本次取代必须关闭的开放任务。
///
/// # 参数
/// * `confirmations` - 内存中已作废之后的确认矩阵。
/// * `replacement_trial_version` - 新试算版本。
/// * `work_items_by_id` - 以任务 ID 为键的快照；命中项会被移出。
/// * `replacement_id` - 取代确认的 ID。
///
/// # 返回
/// 返回仍为 `Open`、且已被该取代确认作废的任务。已关闭任务不进入关闭集合。
///
/// # 错误
/// 已作废确认缺少对应任务快照时返回 `Internal`。
pub fn collect_superseded_closable_work_items(
    confirmations: &[LegacyImportConfirmation],
    replacement_trial_version: u32,
    work_items_by_id: &mut HashMap<String, WorkItem>,
    replacement_id: &LegacyImportConfirmationId,
) -> Result<Vec<WorkItem>> {
    let mut to_close = Vec::new();
    for confirmation in confirmations.iter().filter(|item| {
        item.status == ConfirmationStatus::Invalidated
            && item.replacement_confirmation_id.as_ref() == Some(replacement_id)
            && item.trial_version < replacement_trial_version
    }) {
        let work_item = work_items_by_id
            .remove(confirmation.work_item_id.as_ref())
            .ok_or_else(|| Error::Internal("被新试算取代的确认任务缺失".to_string()))?;
        if work_item.status == WorkItemStatus::Open {
            to_close.push(work_item);
        }
    }
    Ok(to_close)
}

/// 在同一执行器上作废被取代的确认事实，并关闭关联的开放任务。
///
/// 任务写入走工作流仓储，确认作废留在导入确认仓储。任一失败由调用方事务回滚。
///
/// # 参数
/// * `db` - 数据库。
/// * `confirmations` - 当前试算矩阵；被取代的行就地作废。
/// * `replacement` - 新试算确认。
/// * `actor_id` - 当前操作人。
/// * `executor` - 调用方事务执行器。
///
/// # 返回
/// 无被取代确认时直接返回；否则作废事实并关闭仍开放的关联任务。
///
/// # 错误
/// 确认作废、任务缺失、关闭或后续 CAS 写入失败时返回对应错误。
pub async fn invalidate_replaced_confirmation(
    db: &Database,
    confirmations: &mut [LegacyImportConfirmation],
    replacement: &LegacyImportConfirmation,
    actor_id: &str,
    executor: &mut dyn Executor,
) -> Result<()> {
    let replaced_ids = replaced_confirmation_work_item_ids(confirmations, replacement.trial_version);
    if replaced_ids.is_empty() {
        return Ok(());
    }
    let work_items = db.work_items().list_legacy_import_confirmations_by_ids(&replaced_ids, executor).await?;
    let mut work_items_by_id = HashMap::new();
    for item in work_items {
        work_items_by_id.insert(item.base.id.clone(), item);
    }
    let replacement_id = LegacyImportConfirmationId::new(replacement.base.id.clone());
    for confirmation in confirmations.iter_mut().filter(|item| item.is_replaced_by(replacement.trial_version))
    {
        confirmation.invalidate(replacement_id.clone(), Instant::now())?;
    }
    let mut to_close = collect_superseded_closable_work_items(
        confirmations,
        replacement.trial_version,
        &mut work_items_by_id,
        &replacement_id,
    )?;
    for work_item in to_close.iter_mut() {
        work_item.close(
            actor_id,
            WorkItemCloseData { close_reason: "SUPERSEDED_BY_NEW_IMPORT_TRIAL".to_string() },
            Instant::now(),
        )?;
    }
    db.legacy_import_confirmations()
        .persist_invalidated_confirmations(confirmations, &replacement_id, executor)
        .await?;
    db.work_items().persist_closed_confirmation_work_items(&mut to_close, executor).await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use erp_core::ids::{LegacyImportBatchId, LegacyImportConfirmationId, WorkItemId};
    use erp_import::{LegacyImportConfirmation, LegacyImportConfirmationData};
    use erp_workflow::entity::work_item::WorkItemStatus;

    use super::collect_superseded_closable_work_items;

    #[test]
    fn missing_work_item_fails_closed_without_partial_close_set() {
        let replacement_id = LegacyImportConfirmationId::new("c-new");
        let mut confirmation = LegacyImportConfirmation::new(
            LegacyImportConfirmationId::new("c-old"),
            LegacyImportConfirmationData {
                batch_id: LegacyImportBatchId::new("batch-1"),
                confirmation_scope: "SALES".to_string(),
                owner_role: "role-sales".to_string(),
                batch_version: 1,
                trial_version: 1,
                import_rule_version: "rule-1".to_string(),
                work_item_id: WorkItemId::new("work-item-old"),
            },
        )
        .unwrap();
        confirmation
            .invalidate(
                replacement_id.clone(),
                erp_core::common::time::Instant::from_unix_secs(1_700_000_000),
            )
            .unwrap();
        let mut work_items = HashMap::new();
        let error = collect_superseded_closable_work_items(
            std::slice::from_ref(&confirmation),
            2,
            &mut work_items,
            &replacement_id,
        )
        .expect_err("missing work item must fail closed");
        assert!(error.to_string().contains("缺失"));
        assert!(work_items.is_empty());
        assert_eq!(confirmation.status, erp_import::ConfirmationStatus::Invalidated);
        let _ = WorkItemStatus::Open;
    }
}
