//! Supersede orchestration: invalidate import confirmations and close WorkItems via workflow.

use std::collections::HashMap;

use erp_core::common::time::Instant;
use erp_core::ids::LegacyImportConfirmationId;
use erp_import::{ConfirmationStatus, LegacyImportConfirmation, LegacyImportExt};
use erp_workflow::entity::work_item::{WorkItem, WorkItemCloseData, WorkItemStatus};
use erp_workflow::WorkItemExt;
use mongodb::Database;
use persistence_core::Executor;
use services::{Error, Result};

/// Collect work-item ids referenced by confirmations replaced by a newer trial.
///
/// # Parameters
/// * `confirmations` - current confirmation matrix
/// * `replacement_trial_version` - new trial version
///
/// # Returns
/// Work-item ids in matrix order, excluding empty replacements.
pub fn replaced_confirmation_work_item_ids(
    confirmations: &[LegacyImportConfirmation],
    replacement_trial_version: u32,
) -> Vec<erp_core::ids::WorkItemId> {
    use std::collections::HashSet;
    let mut ids = Vec::new();
    let mut seen = HashSet::new();
    for confirmation in confirmations
        .iter()
        .filter(|item| item.is_replaced_by(replacement_trial_version))
    {
        if seen.insert(confirmation.work_item_id.to_string()) {
            ids.push(confirmation.work_item_id.clone());
        }
    }
    ids
}

/// Map loaded work-item snapshots to the open items that must close for this replacement.
///
/// # Parameters
/// * `confirmations` - matrix after in-memory `invalidate`
/// * `replacement_trial_version` - new trial version
/// * `work_items_by_id` - snapshots keyed by work-item id (hits are removed)
/// * `replacement_id` - replacement confirmation id
///
/// # Errors
/// Missing work-item association for an invalidated confirmation.
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

/// Invalidate replaced confirmation facts and close associated work items on one executor.
///
/// WorkItem writes go through workflow; confirmation invalidation stays on the import
/// confirmation repository. The caller transaction rolls back on any failure.
///
/// # Parameters
/// * `db` - database handle
/// * `confirmations` - current trial matrix (replaced rows are invalidated in place)
/// * `replacement` - new trial confirmation
/// * `actor_id` - current actor
/// * `executor` - caller transaction executor
///
/// # Errors
/// Confirmation invalidate, missing work item, close, or CAS write failures.
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
    let work_items = db
        .work_items()
        .list_legacy_import_confirmations_by_ids(&replaced_ids, executor)
        .await?;
    let mut work_items_by_id = HashMap::new();
    for item in work_items {
        work_items_by_id.insert(item.base.id.clone(), item);
    }
    let replacement_id = LegacyImportConfirmationId::new(replacement.base.id.clone());
    for confirmation in confirmations
        .iter_mut()
        .filter(|item| item.is_replaced_by(replacement.trial_version))
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
            WorkItemCloseData {
                close_reason: "SUPERSEDED_BY_NEW_IMPORT_TRIAL".to_string(),
            },
            Instant::now(),
        )?;
    }
    db.legacy_import_confirmations()
        .persist_invalidated_confirmations(confirmations, &replacement_id, executor)
        .await?;
    db.work_items()
        .persist_closed_confirmation_work_items(&mut to_close, executor)
        .await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::collect_superseded_closable_work_items;
    use erp_core::ids::{LegacyImportBatchId, LegacyImportConfirmationId, WorkItemId};
    use erp_import::{LegacyImportConfirmation, LegacyImportConfirmationData};
    use erp_workflow::entity::work_item::WorkItemStatus;
    use std::collections::HashMap;

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
