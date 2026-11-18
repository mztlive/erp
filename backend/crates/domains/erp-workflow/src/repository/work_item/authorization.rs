//! 任务只映射自身责任事实；来源单据不参与任务列表授权。
use std::sync::Arc;

use async_trait::async_trait;
use entity_core::NOT_DELETED_TIMESTAMP_BSON;
use erp_identity::entity::authorization::{
    AccessContext, AccessFact, FactKind, MappingFacts, QueryModel, QueryModelKey, QueryOutput, RelationKey,
};
use erp_identity::ports::{AccessMapping, QueryFragment, RelationMapping};
use erp_identity::repository::organization::primary_membership_query;
use erp_identity::{CompiledAccess, Error, Result};
#[cfg(test)]
use erp_permissions::work_item;
use mongodb::Database;
use mongodb::bson::{Bson, Document, doc};
use persistence_core::{
    Error as RepositoryError, Executor, Repository, Result as RepositoryResult, aggregate,
};
use serde::Deserialize;

use super::WorkItemFilter;
use crate::WorkItemExt;
use crate::entity::work_item::{WorkItem, WorkItemStatus};

pub(crate) struct WorkItemMapping {
    model: QueryModel,
}

impl WorkItemMapping {
    // 装配固定工作项模型，映射当前负责人、主属组织和真实历次责任；协作关系明确不命中。
    pub(crate) fn shared() -> Arc<dyn AccessMapping> {
        Arc::new(Self {
            model: QueryModel {
                key: QueryModelKey::parse("work_items").expect("固定模型标识合法"),
                resource: "work_item".into(),
                collection: <Database as WorkItemExt>::WORK_ITEMS.into(),
                id_field: "id".into(),
                output: QueryOutput::Object,
                facts: [
                    FactKind::Owner,
                    FactKind::Collaborators,
                    FactKind::Organization,
                    FactKind::HistoryParticipants,
                    FactKind::Completed,
                ]
                .into(),
            },
        })
    }
}

#[async_trait]
impl AccessMapping for WorkItemMapping {
    fn model(&self) -> &QueryModel {
        &self.model
    }
    fn key(&self, _: &AccessContext, _: Option<&MappingFacts>) -> Result<Bson> {
        Ok("$id".into())
    }
    fn records(&self, _: &AccessContext, _: Option<&MappingFacts>) -> Result<QueryFragment> {
        Ok(QueryFragment::Match(doc! { "deleted_at": NOT_DELETED_TIMESTAMP_BSON }))
    }
    fn predicate(
        &self,
        fact: &AccessFact,
        context: &AccessContext,
        _: Option<&MappingFacts>,
    ) -> Result<QueryFragment> {
        match fact {
            AccessFact::Owned => Ok(QueryFragment::Match(doc! { "owner_user_id": context.actor.id() })),
            AccessFact::Collaborating => Ok(QueryFragment::Match(doc! { "$expr": false })),
            AccessFact::HistoryParticipant => Ok(QueryFragment::Match(doc! { "$or": [
                { "responsibility_actor_ids": context.actor.id() },
                { "completed_by": context.actor.id() },
                { "closed_by": context.actor.id() },
            ] })),
            AccessFact::Completed(completed) => Ok(QueryFragment::Match(if *completed {
                doc! { "status": { "$ne": WorkItemStatus::Open.as_str() } }
            } else {
                doc! { "status": WorkItemStatus::Open.as_str() }
            })),
            AccessFact::OrganizationIn(ids) => {
                Ok(primary_membership_query("$owner_user_id".into(), Some(ids), context.as_of))
            },
            _ => Err(Error::ValidationError("任务查询不支持该事实维度".into())),
        }
    }
    fn relation(
        &self,
        _: &RelationKey,
        _: &AccessContext,
        _: Option<&MappingFacts>,
    ) -> Result<RelationMapping> {
        Err(Error::ValidationError("任务列表不接入来源单据授权".into()))
    }
}

// 任务仓储有授权过滤时，先核对凭证的固定工作项模型、原读取阶段和有效期，再取聚合阶段；未提供授权过滤时不附加阶段。
pub(super) fn prefix(
    filter: &WorkItemFilter,
    executor: &mut dyn Executor,
) -> RepositoryResult<Vec<Document>> {
    match &filter.authorization {
        Some(access) => {
            check_model(access)?;
            access
                .ensure_current(executor)
                .map_err(|_| RepositoryError::UnsupportedDeployment("工作项授权阶段已失效"))?;
            Ok(access.stages().to_vec())
        },
        None => Ok(vec![]),
    }
}

// 仓储拼装任务查询前核对凭证的资源、集合、固定模型标识及完整对象粒度，拒绝将其他资源或份额凭证用于工作项集合。
fn check_model(access: &CompiledAccess) -> RepositoryResult<()> {
    let model = access.model();
    if model.resource != "work_item"
        || model.collection != <Database as WorkItemExt>::WORK_ITEMS
        || model.key.as_str() != "work_items"
        || model.output != QueryOutput::Object
    {
        return Err(RepositoryError::UnsupportedDeployment("任务授权模型不匹配"));
    }
    Ok(())
}

#[derive(Deserialize)]
struct TaskId {
    id: String,
}

#[allow(async_fn_in_trait)]
pub(crate) trait WorkItemAuthorizationExt {
    // 在完整授权条件内读取当前批次的任务 ID。
    // 本批允许管理的实际任务 ID。
    //
    // 模型错配、批次超限或数据库失败时返回错误。
    //
    // ids 为当前展示批次，access 为原读取凭证及其固定管理过滤，executor 为原事务。
    async fn authorized_ids(
        &self,
        ids: &[String],
        access: &CompiledAccess,
        executor: &mut dyn Executor,
    ) -> RepositoryResult<Vec<String>>;
}

impl WorkItemAuthorizationExt for Repository<'_, WorkItem> {
    async fn authorized_ids(
        &self,
        ids: &[String],
        access: &CompiledAccess,
        executor: &mut dyn Executor,
    ) -> RepositoryResult<Vec<String>> {
        check_model(access)?;
        access
            .ensure_current(executor)
            .map_err(|_| RepositoryError::UnsupportedDeployment("工作项授权阶段已失效"))?;

        if ids.is_empty() {
            return Ok(vec![]);
        }
        if ids.len() > 1000 {
            return Err(RepositoryError::EntityMetadataOutOfRange("任务授权批次超过 1000 项"));
        }

        let mut pipeline = vec![doc! { "$match": { "id": { "$in": ids } } }];
        pipeline.extend_from_slice(
            access
                .management_stages()
                .map_err(|_| RepositoryError::UnsupportedDeployment("工作项读取缺少固定管理动作过滤"))?,
        );
        pipeline.push(doc! { "$project": { "id": 1, "_id": 0 } });
        Ok(aggregate::rows(&self.collection().clone_with_type::<TaskId>(), pipeline, executor)
            .await?
            .into_iter()
            .map(|row| row.id)
            .collect())
    }
}

#[cfg(test)]
mod tests {
    use application_core::AuditActor;
    use erp_core::AccountKind;
    use erp_core::common::time::Instant;

    use super::*;

    fn context() -> AccessContext {
        AccessContext {
            actor: AuditActor::new("reader".into(), "reader".into(), AccountKind::Admin),
            permission: work_item::LIST.clone(),
            resource: "work_item".into(),
            model: Some(QueryModelKey::parse("work_items").unwrap()),
            as_of: Instant::from_unix_secs(10),
        }
    }

    #[tokio::test]
    async fn static_mapping_rejects_source_and_unsupported_warehouse_facts() {
        let mapping = WorkItemMapping::shared();
        let context = context();
        assert!(mapping.preparation().is_none());
        let facts = None;
        assert!(mapping.predicate(&AccessFact::WarehouseIn(["w".into()].into()), &context, facts).is_err());
        assert!(mapping.relation(&RelationKey::parse("sources").unwrap(), &context, facts).is_err());
    }

    #[test]
    fn owner_uses_current_responsibility_and_org_uses_current_identity_membership() {
        let mapping = WorkItemMapping::shared();
        let context = context();
        let facts = None;
        let QueryFragment::Match(owner) = mapping.predicate(&AccessFact::Owned, &context, facts).unwrap()
        else {
            panic!("owner must use task responsibility")
        };
        assert_eq!(owner, doc! { "owner_user_id": "reader" });

        let orgs = ["dept".into()].into();
        let QueryFragment::Exists { bindings, pipeline, .. } =
            mapping.predicate(&AccessFact::OrganizationIn(orgs), &context, facts).unwrap()
        else {
            panic!("org must resolve current membership")
        };
        assert_eq!(bindings, doc! { "member": "$owner_user_id" });
        assert_eq!(
            pipeline[0].get_document("$match").unwrap().get_document("valid_from").unwrap(),
            &doc! { "$lte": 10_i64 }
        );
        assert_eq!(pipeline[3], doc! { "$match": { "count": 1, "org": { "$in": ["dept"] } } });
        assert_eq!(mapping.key(&context, facts).unwrap(), Bson::String("$id".into()));
    }
}
