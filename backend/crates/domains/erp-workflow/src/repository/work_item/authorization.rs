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
use mongodb::Database;
use mongodb::bson::{Bson, Document, doc};
use persistence_core::{
    Error as RepositoryError, Executor, Repository, Result as RepositoryResult, aggregate,
};
use serde::Deserialize;

use super::WorkItemFilter;
use crate::WorkItemExt;
use crate::entity::work_item::WorkItem;

pub(crate) struct WorkItemMapping {
    model: QueryModel,
}

impl WorkItemMapping {
    /// 固定模型只提供当前负责人及其主属组织。
    pub(crate) fn shared() -> Arc<dyn AccessMapping> {
        Arc::new(Self {
            model: QueryModel {
                key: QueryModelKey::parse("work_items").expect("固定模型标识合法"),
                resource: "work_item".into(),
                collection: <Database as WorkItemExt>::WORK_ITEMS.into(),
                id_field: "id".into(),
                output: QueryOutput::Object,
                facts: [FactKind::Owner, FactKind::Organization].into(),
            },
        })
    }
}

#[async_trait]
impl AccessMapping for WorkItemMapping {
    fn model(&self) -> &QueryModel {
        &self.model
    }
    async fn prepare(&self, _: &AccessContext, _: &mut dyn Executor) -> Result<MappingFacts> {
        // 任务与组织关联沿用事务快照；组织版本和到期边界由 identity 的范围解析绑定。
        Ok(MappingFacts::default())
    }
    fn key(&self, _: &AccessContext, _: &MappingFacts) -> Result<Bson> {
        Ok("$id".into())
    }
    fn records(&self, context: &AccessContext, _: &MappingFacts) -> Result<QueryFragment> {
        let mut fragment = primary_membership_query("$owner_user_id".into(), None, context.as_of);
        if let QueryFragment::Exists { bindings, pipeline, .. } = &mut fragment {
            bindings.insert("active", doc! { "$eq": ["$deleted_at", NOT_DELETED_TIMESTAMP_BSON] });
            pipeline.push(doc! { "$match": { "$expr": "$$active" } });
        }
        Ok(fragment)
    }
    fn predicate(
        &self,
        fact: &AccessFact,
        context: &AccessContext,
        _: &MappingFacts,
    ) -> Result<QueryFragment> {
        match fact {
            AccessFact::Owned => Ok(QueryFragment::Match(doc! { "owner_user_id": context.actor.id() })),
            AccessFact::OrganizationIn(ids) => {
                Ok(primary_membership_query("$owner_user_id".into(), Some(ids), context.as_of))
            },
            _ => Err(Error::ValidationError("任务查询不支持该事实维度".into())),
        }
    }
    fn relation(&self, _: &RelationKey, _: &AccessContext, _: &MappingFacts) -> Result<RelationMapping> {
        Err(Error::ValidationError("任务列表不接入来源单据授权".into()))
    }
}

/// 按查询模型检查约束后，供任务仓储拼接数据库阶段。
pub(super) fn prefix(filter: &WorkItemFilter) -> RepositoryResult<Vec<Document>> {
    match &filter.authorization {
        Some(access) => {
            check_model(access)?;
            Ok(access.stages().to_vec())
        },
        None => Ok(vec![]),
    }
}

/// 禁止将其他模型的授权结果用于任务集合。
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
    /// 在完整授权条件内读取当前批次的任务 ID。
    /// # 参数
    /// ids 为当前展示批次，access 为管理授权约束，executor 为原事务。
    /// # 返回
    /// 本批允许管理的实际任务 ID。
    /// # 错误
    /// 模型错配、批次超限或数据库失败时返回错误。
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
        if ids.is_empty() {
            return Ok(vec![]);
        }
        if ids.len() > 1000 {
            return Err(RepositoryError::EntityMetadataOutOfRange("任务授权批次超过 1000 项"));
        }
        let mut pipeline = vec![doc! { "$match": { "id": { "$in": ids } } }];
        pipeline.extend_from_slice(access.stages());
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
    use erp_identity::Permission;
    use persistence_core::NoTransaction;

    use super::*;

    fn context() -> AccessContext {
        AccessContext {
            actor: AuditActor::new("reader".into(), "reader".into(), AccountKind::Admin),
            permission: Permission::parse("work_item:list").unwrap(),
            resource: "work_item".into(),
            model: Some(QueryModelKey::parse("work_items").unwrap()),
            as_of: Instant::from_unix_secs(10),
        }
    }

    #[tokio::test]
    async fn mapping_requires_transaction_and_rejects_source_and_read_supplements() {
        let mapping = WorkItemMapping::shared();
        let context = context();
        let facts = mapping.prepare(&context, &mut NoTransaction).await.unwrap();
        assert!(facts.validate(false, context.as_of).is_err());
        assert!(facts.validate(true, context.as_of).is_ok());
        for fact in [
            AccessFact::HistoryParticipant,
            AccessFact::Collaborating,
            AccessFact::WarehouseIn(["w".into()].into()),
        ] {
            assert!(mapping.predicate(&fact, &context, &facts).is_err());
        }
        assert!(mapping.relation(&RelationKey::parse("sources").unwrap(), &context, &facts).is_err());
    }

    #[test]
    fn owner_uses_current_responsibility_and_org_uses_current_identity_membership() {
        let mapping = WorkItemMapping::shared();
        let context = context();
        let facts = MappingFacts::default();
        let QueryFragment::Match(owner) = mapping.predicate(&AccessFact::Owned, &context, &facts).unwrap()
        else {
            panic!("owner must use task responsibility")
        };
        assert_eq!(owner, doc! { "owner_user_id": "reader" });
        let orgs = ["dept".into()].into();
        let QueryFragment::Exists { bindings, pipeline, .. } =
            mapping.predicate(&AccessFact::OrganizationIn(orgs), &context, &facts).unwrap()
        else {
            panic!("org must resolve current membership")
        };
        assert_eq!(bindings, doc! { "member": "$owner_user_id" });
        assert_eq!(
            pipeline[0].get_document("$match").unwrap().get_document("valid_from").unwrap(),
            &doc! { "$lte": 10_i64 }
        );
        assert_eq!(pipeline[3], doc! { "$match": { "count": 1, "org": { "$in": ["dept"] } } });
        assert_eq!(mapping.key(&context, &facts).unwrap(), Bson::String("$id".into()));
    }
}
