//! 采购创建任务的真实来源映射与授权结果读取。

use erp_identity::entity::authorization::{
    AccessContext, AccessFact, FactKind, MappingFacts, QueryModel, QueryModelKey, QueryOutput, RelationKey,
};
use erp_identity::ports::{AccessMapping, QueryFragment, RelationMapping};
use erp_identity::repository::organization::primary_membership_query;
use erp_identity::{CompiledAccess, Error as IdentityError, Result as IdentityResult};
use erp_workflow::WorkItemExt;
use erp_workflow::entity::work_item::{WorkItem, WorkItemStatus, WorkItemType};
use mongodb::Database;
use mongodb::bson::{Bson, Document, doc};
use persistence_core::{Executor, aggregate};

use crate::Result;

/// 业务身份只收窄 identity 编译的任务结果。
pub(crate) struct CreationTaskFilter {
    pub sales_order_id: Option<String>,
    pub work_item_id: Option<String>,
}

// 读取采购创建任务时，执行 identity 编译的任务条件，再按业务身份收窄及排序。
//
// 调用参数：
// db、executor 为原事务；access 为本阶段授权，query 为业务身份条件。
//
// 允许读取的真实任务，按创建时间排序。
//
// 数据库查询或反序列化失败时返回原错误；凭证复验由调用编排执行。
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
        "deleted_at": 0,
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

/// 采购创建入口的真实开放任务查询模型。
pub struct CreationTaskMapping(QueryModel);
impl CreationTaskMapping {
    /// 装配采购创建任务时，绑定实际工作项集合、当前负责人及其当前主属组织事实。
    ///
    /// # 返回
    /// 固定任务查询映射。
    ///
    /// # 错误
    /// 固定模型标识非法时拒绝。
    pub fn new() -> IdentityResult<Self> {
        Ok(Self(QueryModel {
            key: QueryModelKey::parse("purchase_creation_tasks")?,
            resource: "purchase_order".into(),
            collection: <Database as WorkItemExt>::WORK_ITEMS.into(),
            id_field: "id".into(),
            output: QueryOutput::Object,
            facts: [FactKind::Owner, FactKind::Organization, FactKind::Assignee].into(),
        }))
    }
}
impl AccessMapping for CreationTaskMapping {
    fn model(&self) -> &QueryModel {
        &self.0
    }
    fn key(&self, _: &AccessContext, _: Option<&MappingFacts>) -> IdentityResult<Bson> {
        Ok("$id".into())
    }
    fn records(&self, _: &AccessContext, _: Option<&MappingFacts>) -> IdentityResult<QueryFragment> {
        Ok(QueryFragment::Match(filter(None, None)))
    }
    fn predicate(
        &self,
        fact: &AccessFact,
        context: &AccessContext,
        _: Option<&MappingFacts>,
    ) -> IdentityResult<QueryFragment> {
        match fact {
            AccessFact::Owned | AccessFact::AssignedTo => {
                Ok(QueryFragment::Match(doc! { "owner_user_id": context.actor.id() }))
            },
            AccessFact::OrganizationIn(ids) => {
                Ok(primary_membership_query("$owner_user_id".into(), Some(ids), context.as_of))
            },
            _ => Err(IdentityError::ValidationError("采购创建任务不提供该事实维度".into())),
        }
    }
    fn relation(
        &self,
        _: &RelationKey,
        _: &AccessContext,
        _: Option<&MappingFacts>,
    ) -> IdentityResult<RelationMapping> {
        Err(IdentityError::ValidationError("采购创建任务没有来源授权关系".into()))
    }
}

#[cfg(test)]
mod tests {
    use application_core::AuditActor;
    use erp_core::AccountKind;
    use erp_core::common::time::Instant;
    use erp_permissions::purchase_order;

    use super::*;

    #[test]
    fn creation_task_mapping_uses_real_owner_and_current_primary_organization() {
        let mapping = CreationTaskMapping::new().unwrap();
        mapping.model().validate().unwrap();
        assert_eq!(
            mapping.model().facts,
            [FactKind::Owner, FactKind::Organization, FactKind::Assignee].into()
        );

        let context = AccessContext {
            actor: AuditActor::new("buyer".into(), "buyer".into(), AccountKind::Admin),
            permission: purchase_order::CREATE,
            resource: "purchase_order".into(),
            model: Some(mapping.model().key.clone()),
            as_of: Instant::from_unix_secs(100),
        };
        for fact in [AccessFact::Owned, AccessFact::AssignedTo] {
            let QueryFragment::Match(predicate) = mapping.predicate(&fact, &context, None).unwrap() else {
                panic!("任务归属和分配必须使用真实负责人")
            };
            assert_eq!(predicate, doc! { "owner_user_id": "buyer" });
        }

        let organizations = ["current-org".into()].into();
        let QueryFragment::Exists { bindings, pipeline, .. } =
            mapping.predicate(&AccessFact::OrganizationIn(organizations), &context, None).unwrap()
        else {
            panic!("任务组织必须解析当前负责人主属组织")
        };
        assert_eq!(bindings, doc! { "member": "$owner_user_id" });
        assert_eq!(
            pipeline[0].get_document("$match").unwrap().get_document("valid_from").unwrap(),
            &doc! { "$lte": 100_i64 }
        );
        assert_eq!(pipeline[3], doc! { "$match": { "count": 1, "org": { "$in": ["current-org"] } } });

        let QueryFragment::Match(records) = mapping.records(&context, None).unwrap() else {
            panic!("任务查询仅按真实开放任务收窄")
        };
        assert_eq!(records, filter(None, None));
        assert_eq!(mapping.key(&context, None).unwrap(), Bson::String("$id".into()));
    }
    #[test]
    fn task_filter_keeps_exact_business_and_open_task_identity() {
        assert_eq!(
            filter(Some("sales"), Some("task")),
            doc! {
                "deleted_at": 0,
                "work_item_type": WorkItemType::ProcurementOrderCreation.as_str(),
                "business_object_type": "sales_order", "status": WorkItemStatus::Open.as_str(),
                "business_object_id": "sales", "id": "task",
            }
        );
    }
}
