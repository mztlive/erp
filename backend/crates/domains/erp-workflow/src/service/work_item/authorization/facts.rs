//! 精确任务目标的责任事实读取，与查询 Mapping 使用相同主属关系。
use std::collections::BTreeMap;

use async_trait::async_trait;
use erp_identity::entity::authorization::{AccessContext, FactValue, ObjectFacts};
use erp_identity::entity::organization::OrgMembership;
use erp_identity::ports::AccessFactsPort;
use erp_identity::repository::organization::OrganizationRepository;
use erp_identity::{ResourceTargets, Result};
use mongodb::Database;
use persistence_core::Executor;

use crate::WorkItemExt;
use crate::entity::work_item::WorkItem;
use crate::repository::WorkItemRepositoryExt;

pub(super) struct WorkItemFacts {
    pub db: Database,
    pub proposed: Option<WorkItem>,
}

impl WorkItemFacts {
    /// 拟创建任务只读取输入事实，已有任务沿用原执行器读取持久化事实。
    async fn items(&self, targets: &ResourceTargets, executor: &mut dyn Executor) -> Result<Vec<WorkItem>> {
        if let Some(item) = &self.proposed {
            return Ok(proposed_targets(item, targets));
        }
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
            let org = match &item.owner_user_id {
                Some(owner) => {
                    if !organizations.contains_key(owner) {
                        let memberships = OrganizationRepository::new(&self.db)
                            .primary_memberships(owner, context.as_of, executor)
                            .await?;
                        organizations.insert(
                            owner.clone(),
                            OrgMembership::primary_org(&memberships, owner, context.as_of)?
                                .map(ToOwned::to_owned),
                        );
                    }
                    organizations[owner].clone()
                },
                None => None,
            };
            facts.push(item_facts(item, org));
        }
        Ok(facts)
    }
}

/// 同一个真实任务负责人同时提供范围归属和办理分配事实。
fn item_facts(item: WorkItem, org: Option<String>) -> ObjectFacts {
    ObjectFacts {
        resource: "work_item".into(),
        id: item.base.id,
        version: item.base.version.to_string(),
        owner_id: FactValue::Known(item.owner_user_id.clone()),
        assignee_id: FactValue::Known(item.owner_user_id),
        org_unit_id: FactValue::Known(org),
        ..Default::default()
    }
}

/// 创建检查仅提供本次拟保存的任务，其他根目标保持缺失。
fn proposed_targets(item: &WorkItem, targets: &ResourceTargets) -> Vec<WorkItem> {
    if targets.ids().contains(&item.base.id) { vec![item.clone()] } else { Vec::new() }
}

#[cfg(test)]
mod tests {
    use erp_core::ids::WorkItemId;

    use super::*;
    use crate::entity::work_item::SupplierPortalReviewTaskData;

    /// 拟创建任务按完整根目标取证，无需预先存在数据库记录。
    #[test]
    fn proposed_review_preserves_identity_and_rejects_unrelated_targets() {
        let item = WorkItem::new_supplier_portal_review(
            WorkItemId::new("review-task"),
            SupplierPortalReviewTaskData {
                request_id: "supplier-request".into(),
                subject_version: "submission-1".into(),
                owner_user_id: "reviewer".into(),
                owner_organization_id: "procurement".into(),
                due_at: None,
                impact_summary: None,
            },
        )
        .unwrap();
        let targets = ResourceTargets::new(&["review-task"]).unwrap();
        let loaded = proposed_targets(&item, &targets);
        assert_eq!(loaded.len(), 1);
        assert_eq!(loaded[0].base.id, "review-task");
        assert_eq!(loaded[0].owner_user_id.as_deref(), Some("reviewer"));
        assert_eq!(loaded[0].owner_organization_id, "procurement");
        assert_eq!(loaded[0].subject_version, "submission-1");
        let facts = item_facts(loaded[0].clone(), Some("current-org".into()));
        assert_eq!(facts.resource, "work_item");
        assert_eq!(facts.id, "review-task");
        assert_eq!(facts.owner_id, FactValue::Known(Some("reviewer".into())));
        assert_eq!(facts.assignee_id, facts.owner_id);
        assert_eq!(facts.org_unit_id, FactValue::Known(Some("current-org".into())));
        assert!(proposed_targets(&item, &ResourceTargets::new(&["other-task"]).unwrap()).is_empty());
        let incomplete = ResourceTargets::new(&["review-task", "other-task"]).unwrap();
        let loaded = proposed_targets(&item, &incomplete);
        assert_eq!(loaded.len(), 1);
    }
}
