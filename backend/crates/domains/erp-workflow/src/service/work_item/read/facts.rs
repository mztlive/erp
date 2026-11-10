//! 精确任务目标的责任事实读取，与查询 Mapping 使用相同主属关系。
use std::collections::BTreeMap;

use async_trait::async_trait;
use erp_identity::entity::authorization::{AccessContext, FactValue, ObjectFacts, ProposedFacts};
use erp_identity::entity::organization::OrgMembership;
use erp_identity::ports::AccessFactsPort;
use erp_identity::repository::organization::OrganizationRepository;
use erp_identity::{Error, ResourceTargets, Result};
use mongodb::Database;
use persistence_core::Executor;

use crate::WorkItemExt;
use crate::repository::WorkItemRepositoryExt;

pub(super) struct WorkItemFacts(pub Database);

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
        for id in targets.ids() {
            let Some(item) = self.0.work_items().find_work_item(id, executor).await? else { continue };
            let org = match &item.owner_user_id {
                Some(owner) => {
                    if !organizations.contains_key(owner) {
                        let memberships = OrganizationRepository::new(&self.0)
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
            facts.push(ObjectFacts {
                resource: "work_item".into(),
                id: item.base.id,
                version: item.base.version.to_string(),
                owner_id: FactValue::Known(item.owner_user_id),
                org_unit_id: FactValue::Known(org),
                ..Default::default()
            });
        }
        Ok(facts)
    }
    async fn proposed(
        &self,
        _: &AccessContext,
        _: &ProposedFacts,
        _: &mut dyn Executor,
    ) -> Result<ObjectFacts> {
        Err(Error::ValidationError("任务创建由业务责任分配规则执行".into()))
    }
}
