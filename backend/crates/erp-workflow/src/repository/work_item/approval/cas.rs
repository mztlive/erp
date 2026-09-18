//! 单据审批任务 CAS 写入：开放任务关闭与版本推进。

use bpm::ApprovalNodeExecutionId;
use entity_core::HasBaseModel;
use mongodb::bson::{doc, serialize_to_document};
use persistence_core::{Error, Executor, Repository, Result, mongo_ops};

use super::super::super::bpm::{CasWriteOutcome, approval_task_cas_filter, classify_cas_miss};
use crate::entity::work_item::{WorkItem, WorkItemStatus};

pub(super) async fn persist_open_approval_task(
    repo: &Repository<'_, WorkItem>,
    item: &WorkItem,
    expected_task_version: u64,
    approval_node_execution_id: &ApprovalNodeExecutionId,
    executor: &mut dyn Executor,
) -> Result<CasWriteOutcome<WorkItem>> {
    let next_version = next_task_version(expected_task_version)?;
    let mut set_doc = serialize_to_document(item)?;
    set_doc.insert("version", next_version);
    let matched = mongo_ops::update_one(
        &repo.collection(),
        approval_task_cas_filter(&item.base.id, expected_task_version, approval_node_execution_id)?,
        doc! { "$set": set_doc },
        false,
        executor,
    )
    .await?
    .matched_count;
    if matched > 0 {
        let mut applied = item.clone();
        applied.base_mut().version = expected_task_version.saturating_add(1);
        return Ok(CasWriteOutcome::Applied(applied));
    }
    let current = repo.find_by_id(&item.base.id, executor).await?;
    let expected_execution = approval_node_execution_id.clone();
    Ok(classify_cas_miss(current, expected_task_version, move |row| {
        approval_task_still_open(row, &expected_execution)
    }))
}
/// 计算审批任务 CAS 的下一持久化版本。
///
/// # 参数
/// * `expected_task_version` - 加载时任务版本
///
/// # 返回
/// 返回可写入 BSON 的下一版本。
///
/// # 错误
/// 版本溢出或无法表示为 BSON 整数时返回错误。
fn next_task_version(expected_task_version: u64) -> Result<i64> {
    let next = expected_task_version.checked_add(1).ok_or(Error::EntityMetadataOutOfRange("version"))?;
    i64::try_from(next).map_err(|_| Error::EntityMetadataOutOfRange("version"))
}

fn approval_task_still_open(item: &WorkItem, execution_id: &ApprovalNodeExecutionId) -> bool {
    item.status == WorkItemStatus::Open && item.approval_node_execution_id.as_ref() == Some(execution_id)
}

#[cfg(test)]
mod tests {
    use bpm::ApprovalNodeExecutionId;
    use entity_core::HasBaseModel;
    use erp_core::common::time::Instant;
    use erp_core::ids::WorkItemId;

    use super::approval_task_still_open;
    use crate::entity::work_item::{
        AssignmentSource, WorkItem, WorkItemData, WorkItemPriority, WorkItemStatus, WorkItemType,
    };
    use crate::repository::bpm::{CasWriteOutcome, approval_task_cas_filter, classify_cas_miss};

    fn assigned_item() -> WorkItem {
        WorkItem::new_at(
            WorkItemId::new("wi-1"),
            WorkItemData {
                work_item_type: WorkItemType::ImportBusinessConfirmation,
                business_object_type: "LEGACY_IMPORT_BATCH".to_string(),
                business_object_id: "batch-1".to_string(),
                subject_version: "v1".to_string(),
                owner_role: "sales".to_string(),
                owner_organization_id: "org-1".to_string(),
                owner_user_id: "alice".to_string(),
                assignment_source: AssignmentSource::SystemRule,
                priority: WorkItemPriority::Normal,
                due_at: None,
                reason_code: None,
                impact_summary: None,
            },
            Instant::from_unix_secs(100),
        )
        .unwrap()
    }

    #[test]
    fn approval_task_cas_miss_classifies_closed_and_version() {
        let execution = ApprovalNodeExecutionId::new("exec-1");
        let filter = approval_task_cas_filter("wi-1", 3, &execution).unwrap();
        assert_eq!(filter.get_str("status").unwrap(), "OPEN");
        assert_eq!(filter.get_str("approval_node_execution_id").unwrap(), "exec-1");

        let mut closed = assigned_item();
        closed.status = WorkItemStatus::Closed;
        closed.approval_node_execution_id = Some(execution.clone());
        assert!(!approval_task_still_open(&closed, &execution));
        let closed_version = closed.base().version;
        assert!(matches!(
            classify_cas_miss(Some(closed), closed_version, |item| {
                approval_task_still_open(item, &execution)
            }),
            CasWriteOutcome::StatusChanged(_)
        ));

        let mut stale = assigned_item();
        stale.approval_node_execution_id = Some(execution.clone());
        stale.base_mut().version = 4;
        assert!(matches!(
            classify_cas_miss(Some(stale), 3, |item| approval_task_still_open(item, &execution)),
            CasWriteOutcome::VersionConflict(_)
        ));

        let mut open_wrong = assigned_item();
        open_wrong.approval_node_execution_id = Some(ApprovalNodeExecutionId::new("exec-2"));
        let open_version = open_wrong.base().version;
        assert!(!approval_task_still_open(&open_wrong, &execution));
        assert!(matches!(
            classify_cas_miss(Some(open_wrong), open_version, |item| {
                approval_task_still_open(item, &execution)
            }),
            CasWriteOutcome::StatusChanged(_)
        ));
        assert!(matches!(
            classify_cas_miss::<WorkItem>(None, 1, |item| approval_task_still_open(item, &execution)),
            CasWriteOutcome::NotFound
        ));
    }
}
