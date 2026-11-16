//! 精确任务目标的责任事实读取，与查询 Mapping 使用相同主属关系。
use std::collections::{BTreeMap, BTreeSet};

use async_trait::async_trait;
use erp_core::common::time::Instant;
use erp_identity::entity::authorization::{AccessContext, FactValue, ObjectFacts};
use erp_identity::ports::AccessFactsPort;
use erp_identity::{OrganizationDirectoryService, ResourceTargets, Result};
use mongodb::Database;
use persistence_core::Executor;

use crate::WorkItemExt;
use crate::entity::work_item::{WorkItem, WorkItemStatus};
use crate::repository::WorkItemRepositoryExt;

pub(super) struct WorkItemFacts {
    pub db: Database,
}

impl WorkItemFacts {
    /// 拟创建任务只读取输入事实，已有任务沿用原执行器读取持久化事实。
    async fn items(&self, targets: &ResourceTargets, executor: &mut dyn Executor) -> Result<Vec<WorkItem>> {
        let mut items = Vec::new();
        for id in targets.ids() {
            if let Some(item) = self.db.work_items().find_work_item(id, executor).await? {
                items.push(item);
            }
        }
        Ok(items)
    }
}

#[async_trait]
impl AccessFactsPort for WorkItemFacts {
    async fn load(
        &self,
        context: &AccessContext,
        targets: &ResourceTargets,
        executor: &mut dyn Executor,
    ) -> Result<Vec<ObjectFacts>> {
        let mut organizations = BTreeMap::new();
        let mut facts = Vec::new();
        for item in self.items(targets, executor).await? {
            let (org, valid_until) = match &item.owner_user_id {
                Some(owner) => {
                    if !organizations.contains_key(owner) {
                        organizations.insert(
                            owner.clone(),
                            OrganizationDirectoryService::new(self.db.clone())
                                .own_org_with_validity(owner, context.as_of, executor)
                                .await?,
                        );
                    }
                    organizations[owner].clone()
                },
                None => (None, None),
            };
            facts.push(item_facts(item, org, valid_until));
        }
        Ok(facts)
    }
}

/// 当前负责人提供范围及办理事实；历次负责人、完成人和关闭人提供真实历史参与。
pub(super) fn item_facts(item: WorkItem, org: Option<String>, valid_until: Option<Instant>) -> ObjectFacts {
    ObjectFacts {
        resource: "work_item".into(),
        id: item.base.id,
        version: item.base.version.to_string(),
        subject_type: FactValue::Known(item.work_item_type.as_str().into()),
        subject_source_resource: FactValue::Known(item.business_object_type),
        owner_id: FactValue::Known(item.owner_user_id.clone()),
        assignee_id: FactValue::Known(item.owner_user_id),
        completed: FactValue::Known(item.status != WorkItemStatus::Open),
        org_unit_id: FactValue::Known(org),
        valid_until,
        collaborators: FactValue::Known(BTreeSet::new()),
        history_participants: FactValue::Known(
            item.responsibility_actor_ids
                .into_iter()
                .chain(item.completed_by)
                .chain(item.closed_by)
                .collect(),
        ),
        ..Default::default()
    }
}

#[cfg(test)]
mod tests;
