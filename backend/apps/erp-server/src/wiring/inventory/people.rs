//! 库存调整申请人、当前开放审批人与当前姓名的批量事实 adapter。

use std::cmp::Reverse;
use std::collections::HashMap;
use std::sync::Arc;

use async_trait::async_trait;
use erp_identity::{AccessControlExt, AccountCoreRepositoryExt};
use erp_inventory::{
    AdjustmentPeopleFact, AdjustmentPeopleFactsPort, AdjustmentSnapshotReadFilter, applicant_object_ids,
    latest_snapshot_submitters, merge_adjustment_people,
};
use erp_workflow::repository::WorkItemRepositoryApprovalExt;
use erp_workflow::{
    ApprovalIntegrationExt, ApprovalSubjectSnapshot, DocumentType, WorkItem, WorkItemExt, WorkItemFilter,
    WorkItemStatus, WorkItemType,
};
use mongodb::Database;
use persistence_core::Executor;

const PEOPLE_LIMIT: usize = 20_000;

/// 库存调整申请人与当前开放审批人 Mongo 适配器。
#[derive(Clone)]
pub struct MongoInventoryPeopleFacts {
    db: Database,
}

impl MongoInventoryPeopleFacts {
    /// 绑定库存调整与审批集合所在数据库，构造时不读取。
    ///
    /// # 参数
    /// * `db` - 库存调整与工作项所在数据库。
    ///
    /// # 返回
    /// 返回未执行 I/O 的 adapter。
    ///
    /// # 错误
    /// 不返回错误。
    pub fn new(db: Database) -> Self {
        Self { db }
    }

    /// 包装为库存域可注入的调整人员事实 Port。
    ///
    /// # 参数
    /// * `db` - 库存调整与工作项所在数据库。
    ///
    /// # 返回
    /// 返回共享的调整人员事实 Port。
    ///
    /// # 错误
    /// 不返回错误。
    pub fn shared(db: Database) -> Arc<dyn AdjustmentPeopleFactsPort> {
        Arc::new(Self::new(db))
    }
}

#[async_trait]
impl AdjustmentPeopleFactsPort for MongoInventoryPeopleFacts {
    async fn names_by_ids(
        &self,
        ids: &[String],
        executor: &mut dyn Executor,
    ) -> erp_inventory::Result<HashMap<String, String>> {
        ensure_people_limit(ids.len())?;
        Ok(self.db.accounts().names_by_ids(ids, executor).await?)
    }

    async fn people_by_adjustment_ids(
        &self,
        ids: &[String],
        executor: &mut dyn Executor,
    ) -> erp_inventory::Result<HashMap<String, AdjustmentPeopleFact>> {
        if ids.is_empty() {
            return Ok(HashMap::new());
        }
        ensure_people_limit(ids.len())?;
        let snapshots = self
            .load_snapshots(
                AdjustmentSnapshotReadFilter {
                    business_object_ids: Some(ids.to_vec()),
                    ..AdjustmentSnapshotReadFilter::default()
                },
                executor,
            )
            .await?;
        let objects = ids
            .iter()
            .map(|id| (DocumentType::StockAdjustment.as_str().to_string(), id.clone()))
            .collect::<Vec<_>>();
        let mut tasks = self.db.work_items().list_active_approval_by_objects(&objects, executor).await?;
        ensure_people_limit(tasks.len())?;
        tasks.sort_by_key(|item| Reverse(item.base.created_at));
        Ok(merge_adjustment_people(
            latest_snapshot_submitters(snapshot_rows(&snapshots)),
            open_assignees(&tasks),
        ))
    }

    async fn adjustment_ids_submitted_by(
        &self,
        applicant_ids: &[String],
        executor: &mut dyn Executor,
    ) -> erp_inventory::Result<Vec<String>> {
        if applicant_ids.is_empty() {
            return Ok(Vec::new());
        }
        let matched = self
            .load_snapshots(
                AdjustmentSnapshotReadFilter {
                    submitted_by_ids: Some(applicant_ids.to_vec()),
                    ..AdjustmentSnapshotReadFilter::default()
                },
                executor,
            )
            .await?;
        let object_ids = matched.iter().map(|row| row.business_object_id.clone()).collect::<Vec<_>>();
        if object_ids.is_empty() {
            return Ok(Vec::new());
        }
        let snapshots = self
            .load_snapshots(
                AdjustmentSnapshotReadFilter {
                    business_object_ids: Some(object_ids),
                    ..AdjustmentSnapshotReadFilter::default()
                },
                executor,
            )
            .await?;
        Ok(applicant_object_ids(&latest_snapshot_submitters(snapshot_rows(&snapshots)), applicant_ids))
    }

    async fn adjustment_ids_assigned_to(
        &self,
        handler_ids: &[String],
        executor: &mut dyn Executor,
    ) -> erp_inventory::Result<Vec<String>> {
        if handler_ids.is_empty() {
            return Ok(Vec::new());
        }
        let tasks = self.load_open_tasks(Some(handler_ids.to_vec()), executor).await?;
        let mut ids = open_assignees(&tasks).into_iter().map(|(id, _)| id).collect::<Vec<_>>();
        ids.sort();
        ids.dedup();
        ensure_people_limit(ids.len())?;
        Ok(ids)
    }
}

impl MongoInventoryPeopleFacts {
    async fn load_snapshots(
        &self,
        mut filter: AdjustmentSnapshotReadFilter,
        executor: &mut dyn Executor,
    ) -> erp_inventory::Result<Vec<ApprovalSubjectSnapshot>> {
        let mut items = Vec::new();
        filter.page = 1;
        filter.page_size = 100;
        loop {
            let page = self
                .db
                .approval_subject_snapshots()
                .search(&filter, executor)
                .await
                .map_err(erp_inventory::Error::from)?;
            items.extend(page.items);
            ensure_people_limit(items.len())?;
            if items.len() as i64 >= page.total || page.total == 0 {
                break;
            }
            filter.page += 1;
        }
        Ok(items)
    }

    async fn load_open_tasks(
        &self,
        handler_ids: Option<Vec<String>>,
        executor: &mut dyn Executor,
    ) -> erp_inventory::Result<Vec<WorkItem>> {
        let mut filter = open_adjustment_task_filter(handler_ids);
        let mut items = Vec::new();
        loop {
            let page =
                self.db.work_items().search(&filter, executor).await.map_err(erp_inventory::Error::from)?;
            items.extend(page.items);
            ensure_people_limit(items.len())?;
            if items.len() as i64 >= page.total || page.total == 0 {
                break;
            }
            filter.page += 1;
        }
        Ok(items)
    }
}

fn ensure_people_limit(len: usize) -> erp_inventory::Result<()> {
    if len > PEOPLE_LIMIT {
        return Err(erp_inventory::Error::ValidationError(
            "库存范围超过 20000 个对象，请缩小人员筛选".into(),
        ));
    }
    Ok(())
}

fn open_adjustment_task_filter(handler_ids: Option<Vec<String>>) -> WorkItemFilter {
    WorkItemFilter {
        work_item_types: vec![WorkItemType::DocumentApproval],
        statuses: vec![WorkItemStatus::Open],
        object_access_shapes: Some(vec![(
            WorkItemType::DocumentApproval,
            DocumentType::StockAdjustment.as_str().to_string(),
        )]),
        managed_owner_ids: handler_ids,
        page: 1,
        page_size: 100,
        ..WorkItemFilter::default()
    }
}

fn snapshot_rows(snapshots: &[ApprovalSubjectSnapshot]) -> Vec<(String, u32, String)> {
    snapshots
        .iter()
        .map(|row| (row.business_object_id.clone(), row.subject_version, row.payload.submitted_by.clone()))
        .collect()
}

fn open_assignees(tasks: &[WorkItem]) -> Vec<(String, String)> {
    tasks
        .iter()
        .filter(|item| item.status == WorkItemStatus::Open)
        .filter_map(|item| Some((item.business_object_id.clone(), item.owner_user_id.clone()?)))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::{applicant_object_ids, latest_snapshot_submitters, merge_adjustment_people};

    #[test]
    fn applicant_filter_uses_latest_snapshot_not_created_by() {
        let latest = latest_snapshot_submitters([
            ("adj-1".into(), 1, "creator-1".into()),
            ("adj-1".into(), 3, "applicant-1".into()),
            ("adj-1".into(), 2, "creator-1".into()),
        ]);
        assert_eq!(latest.get("adj-1").map(String::as_str), Some("applicant-1"));
        assert!(applicant_object_ids(&latest, &["creator-1".into()]).is_empty());
        assert_eq!(
            merge_adjustment_people(latest, Vec::<(String, String)>::new())
                .get("adj-1")
                .and_then(|fact| fact.submitted_by.clone())
                .as_deref(),
            Some("applicant-1")
        );
    }
}
