//! 询价字段、任务分配及完整根对象事实；不解释或组合权限规则。
use async_trait::async_trait;
use erp_identity::ResourceTargets;
use erp_identity::entity::authorization::{
    AccessContext, AccessFact, FactValue, MappingFacts, ObjectFacts, QueryModel, RelationKey,
};
use erp_identity::ports::{AccessFactsPort, AccessMapping, QueryFragment, RelationMapping};
use erp_sales::entity::sales_inquiry::SalesInquiry;
use erp_sales::repository::SalesInquiryExt;
use mongodb::Database;
use mongodb::bson::{Bson, doc};
use persistence_core::Executor;
pub(crate) struct InquiryMapping(pub(crate) QueryModel);

#[async_trait]
impl AccessMapping for InquiryMapping {
    fn model(&self) -> &QueryModel {
        &self.0
    }
    fn key(&self, _: &AccessContext, _: &MappingFacts) -> erp_identity::Result<Bson> {
        Ok("$id".into())
    }
    async fn prepare(&self, _: &AccessContext, _: &mut dyn Executor) -> erp_identity::Result<MappingFacts> {
        Ok(MappingFacts::default())
    }
    fn records(&self, _: &AccessContext, _: &MappingFacts) -> erp_identity::Result<QueryFragment> {
        Ok(QueryFragment::Match(doc! { "deleted_at": 0_i64 }))
    }
    fn predicate(
        &self,
        fact: &AccessFact,
        context: &AccessContext,
        _: &MappingFacts,
    ) -> erp_identity::Result<QueryFragment> {
        let condition = match fact {
            AccessFact::Owned => doc! { "sales_owner_user_id": context.actor.id() },
            AccessFact::OrganizationIn(ids) => {
                doc! { "business_org_unit_id": { "$in": ids.iter().collect::<Vec<_>>() } }
            },
            AccessFact::Collaborating => doc! { "id": { "$in": [] } },
            _ => return Err(erp_identity::Error::ValidationError("询价不支持该授权事实".into())),
        };
        Ok(QueryFragment::Match(condition))
    }
    fn relation(
        &self,
        _: &RelationKey,
        _: &AccessContext,
        _: &MappingFacts,
    ) -> erp_identity::Result<RelationMapping> {
        Err(erp_identity::Error::ValidationError("询价未登记来源关系".into()))
    }
}

pub(crate) struct InquiryFacts {
    pub(crate) db: Database,
    pub(crate) candidate: Option<SalesInquiry>,
}
#[async_trait]
impl AccessFactsPort for InquiryFacts {
    async fn load(
        &self,
        _: &AccessContext,
        ids: &ResourceTargets,
        executor: &mut dyn Executor,
    ) -> erp_identity::Result<Vec<ObjectFacts>> {
        let mut rows = Vec::new();
        for id in ids.ids() {
            let inquiry = match &self.candidate {
                Some(candidate) if candidate.base.id == *id => Some(candidate.clone()),
                _ => self.db.sales_inquiries().find_by_id(id, executor).await?,
            };
            if let Some(inquiry) = inquiry {
                rows.push(object(&inquiry));
            }
        }
        Ok(rows)
    }
}
fn object(inquiry: &SalesInquiry) -> ObjectFacts {
    ObjectFacts {
        resource: "sales_inquiry".into(),
        id: inquiry.base.id.clone(),
        version: inquiry.base.version.to_string(),
        owner_id: FactValue::Known(Some(inquiry.sales_owner_user_id.clone())),
        org_unit_id: FactValue::Known(Some(inquiry.business_org_unit_id.clone())),
        collaborators: FactValue::Known(Default::default()),
        ..Default::default()
    }
}

pub(crate) struct TaskInquiryMapping {
    pub(crate) model: QueryModel,
    pub(crate) ids: Vec<String>,
    pub(crate) version: String,
}
#[async_trait]
impl AccessMapping for TaskInquiryMapping {
    fn model(&self) -> &QueryModel {
        &self.model
    }
    fn key(&self, _: &AccessContext, _: &MappingFacts) -> erp_identity::Result<Bson> {
        Ok("$id".into())
    }
    async fn prepare(&self, _: &AccessContext, _: &mut dyn Executor) -> erp_identity::Result<MappingFacts> {
        Ok(MappingFacts { version: Some(self.version.clone()), valid_until: None })
    }
    fn records(&self, _: &AccessContext, _: &MappingFacts) -> erp_identity::Result<QueryFragment> {
        Ok(QueryFragment::Match(doc! { "deleted_at": 0_i64 }))
    }
    fn predicate(
        &self,
        fact: &AccessFact,
        _: &AccessContext,
        _: &MappingFacts,
    ) -> erp_identity::Result<QueryFragment> {
        match fact {
            AccessFact::AssignedTo => Ok(QueryFragment::Match(doc! { "id": { "$in": &self.ids } })),
            _ => Err(erp_identity::Error::ValidationError("采购询价只支持实际任务分配事实".into())),
        }
    }
    fn relation(
        &self,
        _: &RelationKey,
        _: &AccessContext,
        _: &MappingFacts,
    ) -> erp_identity::Result<RelationMapping> {
        Err(erp_identity::Error::ValidationError("采购询价未登记来源".into()))
    }
}

pub(crate) struct TaskFacts(pub(crate) erp_workflow::entity::work_item::WorkItem);
#[async_trait]
impl AccessFactsPort for TaskFacts {
    async fn load(
        &self,
        _: &AccessContext,
        ids: &ResourceTargets,
        _: &mut dyn Executor,
    ) -> erp_identity::Result<Vec<ObjectFacts>> {
        if !ids.ids().contains(&self.0.base.id) {
            return Ok(vec![]);
        }
        Ok(vec![ObjectFacts {
            resource: "procurement_inquiry".into(),
            id: self.0.base.id.clone(),
            version: self.0.base.version.to_string(),
            assignee_id: FactValue::Known(self.0.owner_user_id.clone()),
            ..Default::default()
        }])
    }
}

#[cfg(test)]
mod tests {
    use application_core::AuditActor;
    use erp_core::AccountKind;
    use erp_core::common::time::Instant;
    use erp_identity::Permission;
    use erp_identity::entity::authorization::{FactKind, QueryModelKey, QueryOutput};
    use persistence_core::NoTransaction;

    use super::*;

    fn context() -> AccessContext {
        AccessContext {
            actor: AuditActor::new("sales".into(), "sales".into(), AccountKind::Admin),
            permission: Permission::parse("sales_inquiry:list").unwrap(),
            resource: "sales_inquiry".into(),
            model: None,
            as_of: Instant::from_unix_secs(1),
        }
    }
    fn mapping() -> InquiryMapping {
        InquiryMapping(QueryModel {
            key: QueryModelKey::parse("sales_inquiries").unwrap(),
            resource: "sales_inquiry".into(),
            collection: "sales_inquiries".into(),
            id_field: "id".into(),
            output: QueryOutput::Object,
            facts: [FactKind::Owner, FactKind::Organization, FactKind::Collaborators].into(),
        })
    }
    #[test]
    fn inquiry_mapping_uses_current_responsibility_and_rejects_unavailable_dimensions() {
        let mapping = mapping();
        let context = context();
        let facts = MappingFacts::default();
        let QueryFragment::Match(owner) = mapping.predicate(&AccessFact::Owned, &context, &facts).unwrap()
        else {
            panic!("owner fact must use current sales owner");
        };
        assert_eq!(owner, doc! {"sales_owner_user_id":"sales"});
        let QueryFragment::Match(org) =
            mapping.predicate(&AccessFact::OrganizationIn(["org".into()].into()), &context, &facts).unwrap()
        else {
            panic!("organization must use the inquiry business organization");
        };
        assert_eq!(org, doc! {"business_org_unit_id":{"$in":["org"]}});
        assert!(
            mapping
                .predicate(&AccessFact::WarehouseIn(["warehouse".into()].into()), &context, &facts)
                .is_err()
        );
        assert!(mapping.relation(&RelationKey::parse("other").unwrap(), &context, &facts).is_err());
    }
    #[tokio::test]
    async fn inquiry_reads_require_original_transaction() {
        let context = context();
        let facts = mapping().prepare(&context, &mut NoTransaction).await.unwrap();
        assert!(facts.validate(false, context.as_of).is_err());
        assert!(facts.validate(true, context.as_of).is_ok());
    }
}
