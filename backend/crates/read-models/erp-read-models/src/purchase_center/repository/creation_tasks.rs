//! 采购创建任务的真实来源映射与授权结果读取。

use std::sync::Arc;

use async_trait::async_trait;
use erp_identity::entity::authorization::{
    AccessContext, AccessFact, FactKind, MappingFacts, QueryModel, QueryModelKey, QueryOutput, RelationKey,
};
use erp_identity::ports::{AccessMapping, QueryFragment, RelationMapping};
use erp_identity::{CompiledAccess, Error as IdentityError, Result as IdentityResult};
use erp_workflow::WorkItemExt;
use erp_workflow::entity::work_item::{WorkItem, WorkItemStatus, WorkItemType};
use mongodb::Database;
use mongodb::bson::{Bson, Document, doc};
use persistence_core::{Executor, aggregate};

use crate::Result;
use crate::purchase_center::creation_authorization::relation;

/// 业务身份只收窄 identity 编译的任务结果。
pub(in crate::purchase_center) struct CreationTaskFilter {
    pub sales_order_id: Option<String>,
    pub work_item_id: Option<String>,
}

/// 执行 identity 编译的任务条件，再按业务身份收窄及排序。
/// # 参数
/// db、executor 为原事务；access 为本阶段授权，query 为业务身份条件。
/// # 返回
/// 允许读取的真实任务，按创建时间排序。
/// # 错误
/// 数据库查询或反序列化失败时返回原错误；凭证复验由调用编排执行。
pub(in crate::purchase_center) async fn read_creation_tasks(
    db: &Database,
    query: &CreationTaskFilter,
    access: &CompiledAccess,
    executor: &mut dyn Executor,
) -> Result<Vec<WorkItem>> {
    let mut pipeline = access.stages().to_vec();
    pipeline.extend([
        doc! { "$match": filter(query.sales_order_id.as_deref(), query.work_item_id.as_deref()) },
        doc! { "$sort": { "created_at": 1 } },
    ]);
    Ok(aggregate::rows(&db.work_items().collection(), pipeline, executor).await?)
}

fn filter(sales_order_id: Option<&str>, work_item_id: Option<&str>) -> Document {
    let mut filter = doc! {
        "work_item_type": WorkItemType::ProcurementOrderCreation.as_str(),
        "business_object_type": "sales_order",
        "status": WorkItemStatus::Open.as_str(),
    };
    if let Some(id) = sales_order_id {
        filter.insert("business_object_id", id);
    }
    if let Some(id) = work_item_id {
        filter.insert("id", id);
    }
    filter
}

/// 来源检查上下文与真实任务各自保留资源身份；不能把任务当作采购单对象。
pub(in crate::purchase_center) struct CreationTaskRoot(QueryModel);

impl CreationTaskRoot {
    /// 构造采购创建来源上下文的固定模型。
    /// # 参数
    /// 无。
    /// # 返回
    /// 保留真实任务来源关系的 Mapping。
    /// # 错误
    /// 固定模型身份非法时返回错误。
    pub(in crate::purchase_center) fn new() -> IdentityResult<Self> {
        let mut model = CreationTaskMapping::new()?.0;
        model.key = QueryModelKey::parse("purchase_creation_task_source")?;
        model.resource = "purchase_order".into();
        model.facts.clear();
        Ok(Self(model))
    }
}

#[async_trait]
impl AccessMapping for CreationTaskRoot {
    fn model(&self) -> &QueryModel {
        &self.0
    }
    fn key(&self, _: &AccessContext, _: &MappingFacts) -> IdentityResult<Bson> {
        Ok("$id".into())
    }
    async fn prepare(&self, _: &AccessContext, _: &mut dyn Executor) -> IdentityResult<MappingFacts> {
        Ok(MappingFacts::default())
    }
    fn records(&self, _: &AccessContext, _: &MappingFacts) -> IdentityResult<QueryFragment> {
        Ok(QueryFragment::Match(doc! { "deleted_at": 0 }))
    }
    fn predicate(
        &self,
        _: &AccessFact,
        _: &AccessContext,
        _: &MappingFacts,
    ) -> IdentityResult<QueryFragment> {
        Err(IdentityError::ValidationError("采购创建来源上下文不提供任务责任事实".into()))
    }
    fn relation(
        &self,
        key: &RelationKey,
        _: &AccessContext,
        _: &MappingFacts,
    ) -> IdentityResult<RelationMapping> {
        if key != &relation() {
            return Err(IdentityError::ValidationError("未知采购创建来源关系".into()));
        }
        Ok(RelationMapping {
            expected_ids: Bson::Array(vec!["$id".into()]),
            mapping: Arc::new(CreationTaskMapping::new()?),
        })
    }
}

struct CreationTaskMapping(QueryModel);

impl CreationTaskMapping {
    fn new() -> IdentityResult<Self> {
        Ok(Self(QueryModel {
            key: QueryModelKey::parse("purchase_creation_tasks")?,
            resource: "work_item".into(),
            collection: <Database as WorkItemExt>::WORK_ITEMS.into(),
            id_field: "id".into(),
            output: QueryOutput::Object,
            facts: [FactKind::Assignee].into(),
        }))
    }
}

#[async_trait]
impl AccessMapping for CreationTaskMapping {
    fn model(&self) -> &QueryModel {
        &self.0
    }
    fn key(&self, _: &AccessContext, _: &MappingFacts) -> IdentityResult<Bson> {
        Ok("$id".into())
    }
    async fn prepare(&self, _: &AccessContext, _: &mut dyn Executor) -> IdentityResult<MappingFacts> {
        Ok(MappingFacts::default())
    }
    fn records(&self, _: &AccessContext, _: &MappingFacts) -> IdentityResult<QueryFragment> {
        Ok(QueryFragment::Match(doc! { "deleted_at": 0 }))
    }
    fn predicate(
        &self,
        fact: &AccessFact,
        context: &AccessContext,
        _: &MappingFacts,
    ) -> IdentityResult<QueryFragment> {
        match fact {
            AccessFact::AssignedTo => Ok(QueryFragment::Match(doc! { "owner_user_id": context.actor.id() })),
            _ => Err(IdentityError::ValidationError("采购创建任务仅提供当前分配事实".into())),
        }
    }
    fn relation(
        &self,
        _: &RelationKey,
        _: &AccessContext,
        _: &MappingFacts,
    ) -> IdentityResult<RelationMapping> {
        Err(IdentityError::ValidationError("采购创建任务没有来源授权关系".into()))
    }
}

#[cfg(test)]
mod tests {
    use application_core::AuditActor;
    use erp_core::AccountKind;
    use erp_core::common::time::Instant;
    use erp_identity::{AccessMode, AccessRegistration, AuthorizationRegistry, Permission};
    use persistence_core::NoTransaction;

    use super::*;
    use crate::purchase_center::creation_authorization::data;

    #[tokio::test]
    async fn mapping_uses_identity_assignee_fact_and_requires_transaction_evidence() {
        let mapping = CreationTaskMapping::new().unwrap();
        let context = AccessContext {
            actor: AuditActor::new("buyer".into(), "buyer".into(), AccountKind::Admin),
            permission: Permission::parse("purchase_order:create").unwrap(),
            resource: "work_item".into(),
            model: Some(mapping.model().key.clone()),
            as_of: Instant::now(),
        };
        let facts = mapping.prepare(&context, &mut NoTransaction).await.unwrap();
        assert!(facts.validate(false, Instant::now()).is_err());
        facts.validate(true, Instant::now()).unwrap();
        let QueryFragment::Match(predicate) =
            mapping.predicate(&AccessFact::AssignedTo, &context, &facts).unwrap()
        else {
            panic!("分配事实映射为字段条件")
        };
        assert_eq!(predicate, doc! { "owner_user_id": "buyer" });
        assert!(mapping.predicate(&AccessFact::Owned, &context, &facts).is_err());
        let root = CreationTaskRoot::new().unwrap();
        let source = root.relation(&relation(), &context, &facts).unwrap();
        assert_eq!(root.model().resource, "purchase_order");
        assert_eq!(source.mapping.model().resource, "work_item");
        assert_eq!(source.expected_ids, Bson::Array(vec!["$id".into()]));
        assert_eq!(
            filter(Some("sales"), Some("task")),
            doc! {
                "work_item_type": WorkItemType::ProcurementOrderCreation.as_str(),
                "business_object_type": "sales_order",
                "status": WorkItemStatus::Open.as_str(),
                "business_object_id": "sales", "id": "task",
            }
        );
        assert!(
            AuthorizationRegistry::new(vec![AccessRegistration {
                permission: context.permission,
                required_permissions: vec![],
                access: AccessMode::Query {
                    data: data(),
                    mapping: Arc::new(CreationTaskRoot::new().unwrap())
                },
            }])
            .is_ok()
        );
    }
}
