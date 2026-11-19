//! 身份域目录的真实对象事实和静态数据库映射。
use std::collections::BTreeSet;
use std::sync::Arc;

use async_trait::async_trait;
use erp_core::AccountKind;
use erp_identity::entity::authorization::{
    AccessContext, AccessFact, FactKind, FactValue, ObjectFacts, QueryModel, QueryModelKey, QueryOutput,
    RelationKey,
};
use erp_identity::entity::organization::{OrgMembership, OrgUnit};
use erp_identity::entity::person_directory::{PersonDirectoryCategory, PersonQueryStatus};
use erp_identity::ports::{AccessFactsPort, AccessMapping, QueryFragment, RelationMapping};
use erp_identity::repository::organization::{ORG_UNITS, primary_membership_query};
use erp_identity::repository::{OrganizationRepository, PersonQueryQualificationRepositoryExt};
use erp_identity::{AccessControlExt, Error, ResourceTargets, Result};
use erp_permissions::{access_control as access, organization};
use mongodb::Database;
use mongodb::bson::{Bson, Document, doc};
use persistence_core::{Executor, Repository};

use super::{FactsBindings, MappingBindings};

// 构造组织、查询资格及三个人员目录的完整实现绑定。
// db 为身份数据库；所有读取继续使用调用方执行器。
// 五个资源的共享 Facts，以及组织和人员入口的静态 Mapping。
// 固定模型或资源标识非法时拒绝；构造不执行 I/O。
pub(super) fn bindings(db: Database) -> Result<(FactsBindings, MappingBindings)> {
    let reader: Arc<dyn AccessFactsPort> = Arc::new(DirectoryFacts { db });
    let resources =
        ["org_unit", "person_query_qualification", "business_person", "sales_person", "procurement_person"];
    let facts = resources.into_iter().map(|resource| (resource.into(), reader.clone())).collect();
    let mut mappings: MappingBindings = Vec::new();
    for permission in [
        organization::LIST,
        access::PERSON_DIRECTORY_LIST_BUSINESS_PEOPLE,
        access::PERSON_DIRECTORY_LIST_SALESPEOPLE,
        access::PERSON_DIRECTORY_LIST_PROCUREMENT_PEOPLE,
    ] {
        let mapping: Arc<dyn AccessMapping> = Arc::new(DirectoryMapping::new(permission.resource())?);
        mappings.push((permission, mapping));
    }
    Ok((facts, mappings))
}

struct DirectoryFacts {
    db: Database,
}
impl DirectoryFacts {
    async fn organization(&self, id: &str, executor: &mut dyn Executor) -> Result<Option<ObjectFacts>> {
        let Some(unit) = Repository::<OrgUnit>::new(&self.db, ORG_UNITS).find_by_id(id, executor).await?
        else {
            return Ok(None);
        };
        Ok(Some(ObjectFacts {
            resource: "org_unit".into(),
            id: unit.base.id.clone(),
            version: unit.base.version.to_string(),
            owner_id: FactValue::Known(None),
            org_unit_id: FactValue::Known(Some(unit.base.id)),
            collaborators: FactValue::Known(BTreeSet::new()),
            history_participants: FactValue::Known(BTreeSet::new()),
            ..Default::default()
        }))
    }

    async fn person(
        &self,
        context: &AccessContext,
        id: &str,
        executor: &mut dyn Executor,
    ) -> Result<Option<ObjectFacts>> {
        let Some(account) = self.db.accounts().find_by_id(id, executor).await? else { return Ok(None) };
        if account.kind != AccountKind::Admin {
            return Ok(None);
        }
        let mut qualification_version = None;
        if let Some(category) = category(&context.resource)? {
            let Some(qualification) =
                self.db.person_query_qualifications().find_for_account(id, category, executor).await?
            else {
                return Ok(None);
            };
            if qualification.status != PersonQueryStatus::Active {
                return Ok(None);
            }
            qualification_version = Some(qualification.base.version);
        }
        let memberships = OrganizationRepository::new(&self.db)
            .primary_membership_context(id, context.as_of, executor)
            .await?;
        person_object(context, id, account.base.version, qualification_version, &memberships).map(Some)
    }
}

#[async_trait]
impl AccessFactsPort for DirectoryFacts {
    async fn load(
        &self,
        context: &AccessContext,
        ids: &ResourceTargets,
        executor: &mut dyn Executor,
    ) -> Result<Vec<ObjectFacts>> {
        let mut objects = Vec::new();
        for id in ids.ids() {
            let object = if context.resource == "org_unit" {
                self.organization(id, executor).await?
            } else {
                self.person(context, id, executor).await?
            };
            if let Some(object) = object {
                objects.push(object);
            }
        }
        Ok(objects)
    }
}

fn person_object(
    context: &AccessContext,
    id: &str,
    version: u64,
    qualification: Option<u64>,
    memberships: &[OrgMembership],
) -> Result<ObjectFacts> {
    let organization = OrgMembership::primary_org(memberships, id, context.as_of)?.map(str::to_owned);
    let mut revisions =
        memberships.iter().map(|row| format!("{}:{}", row.base.id, row.base.version)).collect::<Vec<_>>();
    revisions.sort();
    Ok(ObjectFacts {
        resource: context.resource.clone(),
        id: id.into(),
        version: format!("{version}:{qualification:?}:{}", revisions.join(",")),
        valid_until: OrgMembership::next_boundary(memberships, id, context.as_of),
        owner_id: FactValue::Known(Some(id.into())),
        org_unit_id: FactValue::Known(organization),
        collaborators: FactValue::Known(BTreeSet::new()),
        history_participants: FactValue::Known(BTreeSet::new()),
        ..Default::default()
    })
}

fn category(resource: &str) -> Result<Option<PersonDirectoryCategory>> {
    match resource {
        "business_person" | "person_query_qualification" => Ok(None),
        "sales_person" => Ok(Some(PersonDirectoryCategory::Sales)),
        "procurement_person" => Ok(Some(PersonDirectoryCategory::Procurement)),
        _ => Err(Error::ValidationError("未知身份目录资源".into())),
    }
}

struct DirectoryMapping(QueryModel);
impl DirectoryMapping {
    fn new(resource: &str) -> Result<Self> {
        let collection = if resource == "org_unit" {
            ORG_UNITS
        } else {
            category(resource)?;
            "accounts"
        };
        Ok(Self(QueryModel {
            key: QueryModelKey::parse(format!("directory_{resource}"))?,
            resource: resource.into(),
            collection: collection.into(),
            id_field: "id".into(),
            output: QueryOutput::Object,
            facts: [
                FactKind::Owner,
                FactKind::Organization,
                FactKind::Collaborators,
                FactKind::HistoryParticipants,
            ]
            .into(),
        }))
    }
}
impl AccessMapping for DirectoryMapping {
    fn model(&self) -> &QueryModel {
        &self.0
    }
    fn key(&self, _: &AccessContext) -> Result<Bson> {
        Ok("$id".into())
    }
    fn records(&self, context: &AccessContext) -> Result<QueryFragment> {
        if self.0.resource == "org_unit" {
            return Ok(QueryFragment::Match(doc! { "deleted_at": 0 }));
        }
        let mut pipeline = vec![doc! { "$match": {
            "deleted_at": 0, "kind": AccountKind::Admin.as_str(), "$expr": { "$eq": ["$id", "$$account"] }
        }}];
        let QueryFragment::Exists { collection, bindings, pipeline: memberships } =
            primary_membership_query("$id".into(), None, context.as_of)
        else {
            return Err(Error::ValidationError("主属关系映射必须提供存在性校验".into()));
        };
        pipeline.extend([
            doc! { "$lookup": { "from": collection, "let": bindings, "pipeline": memberships, "as": "primary_membership" } },
            doc! { "$match": { "primary_membership.0": { "$exists": true } } },
        ]);
        if let Some(category) = category(&self.0.resource)? {
            pipeline.extend(qualification_stages(category));
        }
        Ok(QueryFragment::Exists {
            collection: "accounts".into(),
            bindings: doc! { "account": "$id" },
            pipeline,
        })
    }
    fn predicate(&self, fact: &AccessFact, context: &AccessContext) -> Result<QueryFragment> {
        match fact {
            AccessFact::Collaborating | AccessFact::HistoryParticipant => {
                Ok(QueryFragment::Match(doc! { "$expr": false }))
            },
            AccessFact::Owned => Ok(QueryFragment::Match(if self.0.resource == "org_unit" {
                doc! { "$expr": false }
            } else {
                doc! { "id": context.actor.id() }
            })),
            AccessFact::OrganizationIn(ids) if self.0.resource == "org_unit" => {
                Ok(QueryFragment::Match(doc! { "id": { "$in": ids.iter().cloned().collect::<Vec<_>>() } }))
            },
            AccessFact::OrganizationIn(ids) => {
                Ok(primary_membership_query("$id".into(), Some(ids), context.as_of))
            },
            _ => Err(Error::ValidationError("身份目录不支持此事实维度".into())),
        }
    }
    fn relation(&self, _: &RelationKey, _: &AccessContext) -> Result<RelationMapping> {
        Err(Error::ValidationError("身份目录没有来源关系".into()))
    }
}

fn qualification_stages(category: PersonDirectoryCategory) -> Vec<Document> {
    vec![
        doc! { "$lookup": {
            "from": Database::PERSON_QUERY_QUALIFICATIONS, "localField": "id", "foreignField": "account_id",
            "pipeline": [{ "$match": { "category": category.as_str(), "status": "active", "deleted_at": 0 } }, { "$limit": 1 }],
            "as": "query_qualification"
        }},
        doc! { "$match": { "query_qualification.0": { "$exists": true } } },
    ]
}

#[cfg(test)]
mod tests {
    use application_core::AuditActor;
    use entity_core::BaseModel;
    use erp_core::common::time::Instant;
    use erp_identity::entity::organization::OrgValidity;
    use erp_identity::repository::organization::ORG_MEMBERSHIPS;

    use super::*;

    fn context(resource: &str) -> AccessContext {
        AccessContext {
            actor: AuditActor::new("actor".into(), "actor".into(), AccountKind::Admin),
            permission: access::PERSON_DIRECTORY_LIST_BUSINESS_PEOPLE,
            resource: resource.into(),
            model: None,
            as_of: Instant::from_unix_secs(10),
        }
    }
    fn membership(id: &str, org: &str, from: i64, to: Option<i64>) -> OrgMembership {
        OrgMembership {
            base: BaseModel::new(id.into()),
            user_id: "person".into(),
            org_unit_id: org.into(),
            validity: OrgValidity {
                valid_from: Instant::from_unix_secs(from),
                valid_to: to.map(Instant::from_unix_secs),
            },
            changed_by: "admin".into(),
            reason: "组织调整".into(),
        }
    }
    #[test]
    fn person_fact_uses_unique_primary_membership_and_next_actual_boundary() {
        let context = context("sales_person");
        let memberships = [membership("current", "one", 1, Some(20)), membership("future", "two", 20, None)];
        let object = person_object(&context, "person", 7, Some(3), &memberships).unwrap();
        assert_eq!(object.owner_id, FactValue::Known(Some("person".into())));
        assert_eq!(object.collaborators, FactValue::Known(BTreeSet::new()));
        assert_eq!(object.history_participants, FactValue::Known(BTreeSet::new()));
        assert_eq!(object.org_unit_id, FactValue::Known(Some("one".into())));
        assert_eq!(object.valid_until, Some(Instant::from_unix_secs(20)));
        assert!(
            person_object(
                &context,
                "person",
                7,
                Some(3),
                &[memberships[0].clone(), membership("overlap", "two", 2, None)]
            )
            .is_err()
        );
    }
    #[test]
    fn person_models_are_static_and_organization_filters_use_real_memberships() {
        let model = DirectoryMapping::new("business_person").unwrap();

        let QueryFragment::Exists { collection, .. } = model
            .predicate(&AccessFact::OrganizationIn(["org".into()].into()), &context("business_person"))
            .unwrap()
        else {
            panic!("人员组织必须取真实主属关系")
        };
        assert_eq!(collection, ORG_MEMBERSHIPS);
        assert!(DirectoryMapping::new("unknown").is_err());
        assert!(model.predicate(&AccessFact::AssignedTo, &context("business_person")).is_err());
    }
    #[test]
    fn directories_do_not_grant_access_through_collaboration_or_history() {
        for resource in ["org_unit", "business_person", "sales_person", "procurement_person"] {
            let model = DirectoryMapping::new(resource).unwrap();
            for (fact, kind) in [
                (AccessFact::Collaborating, FactKind::Collaborators),
                (AccessFact::HistoryParticipant, FactKind::HistoryParticipants),
            ] {
                assert!(model.model().facts.contains(&kind));
                let predicate = model.predicate(&fact, &context(resource)).unwrap();
                assert!(
                    matches!(predicate, QueryFragment::Match(document) if document == doc! { "$expr": false })
                );
            }
        }
    }

    #[test]
    fn organization_self_scope_does_not_turn_a_node_into_the_actors_personal_record() {
        let model = DirectoryMapping::new("org_unit").unwrap();
        let QueryFragment::Match(filter) = model.predicate(&AccessFact::Owned, &context("org_unit")).unwrap()
        else {
            panic!("组织无个人负责语义")
        };
        assert_eq!(filter, doc! { "$expr": false });
    }
}
