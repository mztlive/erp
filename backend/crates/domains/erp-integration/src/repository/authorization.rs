//! 集成记录的登记、查询映射与对象事实；全部判定由 identity 执行。
use std::collections::BTreeSet;
use std::sync::Arc;

use async_trait::async_trait;
use erp_identity::entity::authorization::{
    AccessContext, AccessFact, FactKind, FactValue, MappingFacts, ObjectFacts, QueryModel, QueryModelKey,
    QueryOutput, RelationKey,
};
use erp_identity::ports::{QueryFragment, RelationMapping};
use erp_identity::{
    AccessFactsPort, AccessMapping, Error as IdentityError, ResourceTargets, Result as IdentityResult,
};
#[cfg(test)]
use erp_permissions::integration_ops;
use mongodb::Database;
use mongodb::bson::{Bson, doc};
use persistence_core::Executor;

use crate::repository::IntegrationOpsExt;
use crate::repository::integration_ops::reader::IntegrationReader;

/// 构造本域固定授权目录与查询映射。
/// # 参数
/// db 为事实仓储所在数据库。
/// # 返回
/// 完整目录和按资源登记的列表映射。
/// # 错误
/// 固定登记非法时启动失败；构造不执行 I/O。
pub fn facts(db: Database) -> Arc<dyn AccessFactsPort> {
    Arc::new(IntegrationFacts { reader: Arc::new(db) })
}
/// 返回固定集成资源查询映射。
/// # 参数
/// resource 为本域资源标识。
/// # 返回
/// 原始集合及真实归属字段的静态映射。
/// # 错误
/// 未知资源返回接入错误。
pub fn mapping(resource: &str) -> IdentityResult<Arc<dyn AccessMapping>> {
    let collection = match resource {
        "integration_error_task" => Database::INTEGRATION_ERROR_TASKS,
        "reconciliation_difference" => Database::RECONCILIATION_DIFFERENCES,
        _ => return Err(IdentityError::ValidationError("未知集成资源".into())),
    };
    Ok(Arc::new(IntegrationMapping::new(resource, collection)))
}

struct IntegrationFacts {
    reader: Arc<dyn IntegrationReader>,
}
#[async_trait]
impl AccessFactsPort for IntegrationFacts {
    async fn load(
        &self,
        context: &AccessContext,
        ids: &ResourceTargets,
        executor: &mut dyn Executor,
    ) -> IdentityResult<Vec<ObjectFacts>> {
        let mut objects = Vec::new();
        for id in ids.ids() {
            let object = match context.resource.as_str() {
                "integration_error_task" => self.reader.error_task(id, executor).await?.map(|row| {
                    object(
                        &context.resource,
                        row.base.id,
                        row.base.version,
                        row.owner_user_id,
                        row.owner_org_unit_id,
                    )
                }),
                "reconciliation_difference" => self.reader.difference(id, executor).await?.map(|row| {
                    let mut object = object(
                        &context.resource,
                        row.base.id,
                        row.base.version,
                        Some(row.owner_user_id.clone()),
                        row.owner_org_unit_id,
                    );
                    object.assignee_id = FactValue::Known(Some(row.owner_user_id));
                    object
                }),
                _ => return Err(IdentityError::ValidationError("集成事实资源未登记".into())),
            };
            if let Some(object) = object {
                objects.push(object);
            }
        }
        Ok(objects)
    }
}

#[cfg(test)]
/// 为单测连接真实 Facts 转换和可控的持久化读取边界。
/// # 参数
/// reader 为本域读取替身。
/// # 返回
/// 与生产工厂相同的 Facts 实现。
/// # 错误
/// 构造不执行 I/O，读取错误仍由真实 Facts 入口传播。
pub(crate) fn facts_using(reader: Arc<dyn IntegrationReader>) -> Arc<dyn AccessFactsPort> {
    Arc::new(IntegrationFacts { reader })
}

fn object(resource: &str, id: String, version: u64, owner: Option<String>, org: String) -> ObjectFacts {
    ObjectFacts {
        resource: resource.into(),
        id,
        version: version.to_string(),
        owner_id: FactValue::Known(owner),
        org_unit_id: FactValue::Known(Some(org)),
        collaborators: FactValue::Known(BTreeSet::new()),
        history_participants: FactValue::Known(BTreeSet::new()),
        ..Default::default()
    }
}

struct IntegrationMapping {
    model: QueryModel,
}
impl IntegrationMapping {
    fn new(resource: &str, collection: &str) -> Self {
        Self {
            model: QueryModel {
                key: QueryModelKey::parse(collection).expect("固定集成模型"),
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
            },
        }
    }
}
#[async_trait]
impl AccessMapping for IntegrationMapping {
    fn model(&self) -> &QueryModel {
        &self.model
    }
    fn key(&self, _: &AccessContext, _: Option<&MappingFacts>) -> IdentityResult<Bson> {
        Ok("$id".into())
    }
    fn records(&self, _: &AccessContext, _: Option<&MappingFacts>) -> IdentityResult<QueryFragment> {
        Ok(QueryFragment::Match(doc! {"deleted_at": 0_i64}))
    }
    fn predicate(
        &self,
        fact: &AccessFact,
        context: &AccessContext,
        _: Option<&MappingFacts>,
    ) -> IdentityResult<QueryFragment> {
        match fact {
            AccessFact::Collaborating | AccessFact::HistoryParticipant => {
                Ok(QueryFragment::Match(doc! { "$expr": false }))
            },
            AccessFact::Owned => Ok(QueryFragment::Match(doc! {"owner_user_id": context.actor.id()})),
            AccessFact::OrganizationIn(ids) => Ok(QueryFragment::Match(
                doc! {"owner_org_unit_id": {"$in": ids.iter().cloned().collect::<Vec<_>>()}},
            )),
            _ => Err(IdentityError::ValidationError("集成模型不支持该事实维度".into())),
        }
    }
    fn relation(
        &self,
        _: &RelationKey,
        _: &AccessContext,
        _: Option<&MappingFacts>,
    ) -> IdentityResult<RelationMapping> {
        Err(IdentityError::ValidationError("集成模型没有来源授权关系".into()))
    }
}

#[cfg(test)]
mod tests {
    use application_core::AuditActor;
    use erp_core::AccountKind;
    use erp_core::common::time::Instant;
    use erp_identity::AccessTarget;
    use erp_identity::entity::access_control::{ResolvedScope, ScopeClause};
    use erp_identity::testing::authorization;
    use persistence_core::testing::SnapshotExecutor;

    use super::*;
    use crate::repository::integration_ops::reader::testing::{
        MemoryReader, actor, own_scope, plan_contains,
    };

    #[tokio::test]
    async fn actual_owner_facts_and_query_mapping_agree_for_both_resources() {
        for permission in [integration_ops::ERROR_TASK_DETAIL, integration_ops::DIFFERENCE_DETAIL] {
            let reader = Arc::new(MemoryReader::default());
            let facts = facts_using(reader.clone());
            let service = authorization(
                actor(),
                vec![permission.clone()],
                own_scope(),
                [(permission.resource().into(), facts)].into(),
                [(permission.clone(), mapping(permission.resource()).unwrap())].into(),
            )
            .unwrap();
            let id = if permission == integration_ops::ERROR_TASK_DETAIL { "error" } else { "difference" };
            let mut executor = SnapshotExecutor::default();
            let query = service.query(&actor(), &permission, &mut executor).await.unwrap();
            assert!(plan_contains(query.stages(), doc! {"owner_user_id":"reader"}));
            service.validate(&query, &mut executor).await.unwrap();
            service
                .require(&actor(), &permission, AccessTarget::record(id).unwrap(), &mut executor)
                .await
                .unwrap();
            reader.error.lock().unwrap().as_mut().unwrap().owner_user_id = Some("other".into());
            reader.difference.lock().unwrap().as_mut().unwrap().owner_user_id = "other".into();
            assert!(matches!(
                service
                    .require(&actor(), &permission, AccessTarget::record(id).unwrap(), &mut executor)
                    .await,
                Err(IdentityError::Forbidden(_))
            ));
            assert!(matches!(
                service
                    .require(&actor(), &permission, AccessTarget::record("missing").unwrap(), &mut executor)
                    .await,
                Err(IdentityError::NotFound(_))
            ));
        }
    }

    #[tokio::test]
    async fn actual_organization_facts_and_query_mapping_agree_for_both_resources() {
        for permission in [integration_ops::ERROR_TASK_DETAIL, integration_ops::DIFFERENCE_DETAIL] {
            let reader = Arc::new(MemoryReader::default());
            reader.error.lock().unwrap().as_mut().unwrap().owner_user_id = Some("other".into());
            reader.difference.lock().unwrap().as_mut().unwrap().owner_user_id = "other".into();
            let facts = facts_using(reader.clone());
            let scope = ResolvedScope {
                role_clauses: vec![ScopeClause { org_unit_ids: ["org".into()].into(), ..Default::default() }],
                user_limit: None,
            };
            let service = authorization(
                actor(),
                vec![permission.clone()],
                scope,
                [(permission.resource().into(), facts)].into(),
                [(permission.clone(), mapping(permission.resource()).unwrap())].into(),
            )
            .unwrap();
            let id = if permission == integration_ops::ERROR_TASK_DETAIL { "error" } else { "difference" };
            let mut executor = SnapshotExecutor::default();
            let query = service.query(&actor(), &permission, &mut executor).await.unwrap();
            assert!(plan_contains(query.stages(), doc! {"owner_org_unit_id":{"$in":["org"]}}));
            service.validate(&query, &mut executor).await.unwrap();
            service
                .require(&actor(), &permission, AccessTarget::record(id).unwrap(), &mut executor)
                .await
                .unwrap();
            reader.error.lock().unwrap().as_mut().unwrap().owner_org_unit_id = "outside".into();
            reader.difference.lock().unwrap().as_mut().unwrap().owner_org_unit_id = "outside".into();
            assert!(matches!(
                service
                    .require(&actor(), &permission, AccessTarget::record(id).unwrap(), &mut executor)
                    .await,
                Err(IdentityError::Forbidden(_))
            ));
        }
    }
    fn context() -> AccessContext {
        AccessContext {
            actor: AuditActor::new("handler".into(), "handler".into(), AccountKind::Admin),
            permission: integration_ops::ERROR_TASK_LIST.clone(),
            resource: "integration_error_task".into(),
            model: None,
            as_of: Instant::from_unix_secs(1),
        }
    }
    #[test]
    fn actual_handler_and_handler_organization_are_independent_facts() {
        let mapping = IntegrationMapping::new("integration_error_task", Database::INTEGRATION_ERROR_TASKS);
        let own = mapping.predicate(&AccessFact::Owned, &context(), None).unwrap();
        assert!(
            matches!(own, QueryFragment::Match(document) if document == doc! {"owner_user_id":"handler"})
        );
        let org =
            mapping.predicate(&AccessFact::OrganizationIn(["org".into()].into()), &context(), None).unwrap();
        assert!(
            matches!(org, QueryFragment::Match(document) if document == doc! {"owner_org_unit_id":{"$in":["org"]}})
        );
    }
    #[test]
    fn real_integration_object_has_no_collaboration_or_history_membership() {
        for resource in ["integration_error_task", "reconciliation_difference"] {
            let object = object(resource, "record".into(), 7, Some("handler".into()), "org".into());
            assert_eq!(object.version, "7");
            assert_eq!(object.owner_id, FactValue::Known(Some("handler".into())));
            assert_eq!(object.org_unit_id, FactValue::Known(Some("org".into())));
            assert_eq!(object.collaborators, FactValue::Known(BTreeSet::new()));
            assert_eq!(object.history_participants, FactValue::Known(BTreeSet::new()));
        }
    }

    #[test]
    fn history_and_unknown_relations_cannot_grant_integration_read_access() {
        let mapping =
            IntegrationMapping::new("reconciliation_difference", Database::RECONCILIATION_DIFFERENCES);
        for (fact, kind) in [
            (AccessFact::Collaborating, FactKind::Collaborators),
            (AccessFact::HistoryParticipant, FactKind::HistoryParticipants),
        ] {
            let predicate = mapping.predicate(&fact, &context(), None).unwrap();
            assert!(
                matches!(predicate, QueryFragment::Match(document) if document == doc! { "$expr": false })
            );
            assert!(mapping.model().facts.contains(&kind));
        }
        assert!(mapping.relation(&RelationKey::parse("source").unwrap(), &context(), None).is_err());
    }
}
