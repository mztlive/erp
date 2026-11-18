//! 询价字段、任务分配及完整根对象事实；不解释或组合权限规则。
use std::sync::Arc;

use async_trait::async_trait;
use erp_identity::ResourceTargets;
use erp_identity::entity::authorization::{
    AccessContext, AccessFact, FactKind, FactValue, MappingFacts, ObjectFacts, QueryModel, QueryModelKey,
    QueryOutput, RelationKey,
};
use erp_identity::ports::{AccessFactsPort, AccessMapping, QueryFragment, RelationMapping};
use erp_sales::entity::sales_inquiry::SalesInquiry;
use erp_sales::repository::SalesInquiryExt;
use erp_workflow::entity::work_item::{WorkItem, WorkItemStatus, WorkItemType};
use erp_workflow::repository::{WorkItemExt, WorkItemRepositoryExt};
use mongodb::Database;
use mongodb::bson::{Bson, doc};
use persistence_core::Executor;
pub(crate) struct InquiryMapping(pub(crate) QueryModel);

#[async_trait]
impl AccessMapping for InquiryMapping {
    fn model(&self) -> &QueryModel {
        &self.0
    }
    fn key(&self, _: &AccessContext, _: Option<&MappingFacts>) -> erp_identity::Result<Bson> {
        Ok("$id".into())
    }
    fn records(&self, _: &AccessContext, _: Option<&MappingFacts>) -> erp_identity::Result<QueryFragment> {
        Ok(QueryFragment::Match(doc! { "deleted_at": 0_i64 }))
    }
    fn predicate(
        &self,
        fact: &AccessFact,
        context: &AccessContext,
        _: Option<&MappingFacts>,
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
        _: Option<&MappingFacts>,
    ) -> erp_identity::Result<RelationMapping> {
        Err(erp_identity::Error::ValidationError("询价未登记来源关系".into()))
    }
}

pub(crate) struct InquiryFacts {
    pub(crate) db: Database,
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
            let inquiry = self.db.sales_inquiries().find_by_id(id, executor).await?;
            if let Some(inquiry) = inquiry {
                rows.push(object(&inquiry));
            }
        }
        Ok(rows)
    }
}
pub(crate) fn object(inquiry: &SalesInquiry) -> ObjectFacts {
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

/// 新对象候选只提供可信归属，不携带已有记录版本。
pub(crate) fn inquiry_candidate(inquiry: &SalesInquiry) -> ObjectFacts {
    let mut candidate = object(inquiry);
    candidate.version.clear();
    candidate
}

pub(crate) struct TaskInquiryMapping {
    model: QueryModel,
}

impl AccessMapping for TaskInquiryMapping {
    fn model(&self) -> &QueryModel {
        &self.model
    }
    fn key(&self, _: &AccessContext, _: Option<&MappingFacts>) -> erp_identity::Result<Bson> {
        Ok("$id".into())
    }
    fn records(&self, _: &AccessContext, _: Option<&MappingFacts>) -> erp_identity::Result<QueryFragment> {
        Ok(QueryFragment::Match(doc! {"deleted_at": 0_i64}))
    }
    fn predicate(
        &self,
        fact: &AccessFact,
        context: &AccessContext,
        _: Option<&MappingFacts>,
    ) -> erp_identity::Result<QueryFragment> {
        let relation = match fact {
            AccessFact::AssignedTo => doc! {"owner_user_id": context.actor.id()},
            AccessFact::Participant if !matches!(context.permission.action(), "list" | "detail") => {
                doc! {"completed_by": context.actor.id(), "owner_user_id": context.actor.id()}
            },
            AccessFact::Participant => doc! {"completed_by": context.actor.id()},
            AccessFact::Completed(completed) => {
                doc! {"status": if *completed { "COMPLETED" } else { "OPEN" }, "owner_user_id": context.actor.id()}
            },
            _ => return Err(erp_identity::Error::ValidationError("采购询价不支持该事实".into())),
        };
        Ok(QueryFragment::Exists {
            collection: Database::WORK_ITEMS.into(),
            bindings: doc! {"inquiry_id": "$id"},
            pipeline: vec![doc! {"$match": {"$and": [
                {"deleted_at": 0_i64, "business_object_type": "sales_inquiry", "work_item_type": "SALES_INQUIRY_PROCESSING", "status": {"$in": ["OPEN", "COMPLETED"]}},
                {"$expr": {"$eq": ["$business_object_id", "$$inquiry_id"]}}, relation
            ]}}],
        })
    }
    fn relation(
        &self,
        _: &RelationKey,
        _: &AccessContext,
        _: Option<&MappingFacts>,
    ) -> erp_identity::Result<RelationMapping> {
        Err(erp_identity::Error::ValidationError("采购询价没有来源映射".into()))
    }
}

pub(crate) struct TaskFacts {
    db: Database,
}
#[async_trait]
impl AccessFactsPort for TaskFacts {
    async fn load(
        &self,
        _: &AccessContext,
        ids: &ResourceTargets,
        executor: &mut dyn Executor,
    ) -> erp_identity::Result<Vec<ObjectFacts>> {
        let mut objects = Vec::new();
        for id in ids.ids() {
            if let Some(task) = self.db.work_items().find_work_item(id, executor).await?
                && task.work_item_type == WorkItemType::SalesInquiryProcessing
                && task.business_object_type == "sales_inquiry"
                && task.status != WorkItemStatus::Closed
            {
                objects.push(task_object(&task));
            }
        }
        Ok(objects)
    }
}
fn task_object(task: &WorkItem) -> ObjectFacts {
    ObjectFacts {
        resource: "procurement_inquiry".into(),
        id: task.base.id.clone(),
        version: task.base.version.to_string(),
        assignee_id: FactValue::Known(task.owner_user_id.clone()),
        participants: FactValue::Known(task.completed_by.clone().into_iter().collect()),
        completed: FactValue::Known(task.status == WorkItemStatus::Completed),
        ..Default::default()
    }
}

/// 构造真实销售询价事实读取器。
/// # 参数
/// db 为共享业务库。
/// # 返回
/// 不缓存请求对象的事实实现。
/// # 错误
/// 无。
pub fn inquiry_facts(db: Database) -> Arc<dyn AccessFactsPort> {
    Arc::new(InquiryFacts { db })
}
/// 构造采购任务事实读取器。
/// # 参数
/// db 为共享业务库。
/// # 返回
/// 从原执行器读取精确任务的实现。
/// # 错误
/// 无。
pub fn procurement_facts(db: Database) -> Arc<dyn AccessFactsPort> {
    Arc::new(TaskFacts { db })
}
/// 构造静态销售询价查询映射。
/// # 参数
/// 无。
/// # 返回
/// 当前责任与组织的字段映射。
/// # 错误
/// 无。
pub fn inquiry_mapping() -> Arc<dyn AccessMapping> {
    Arc::new(InquiryMapping(model(
        "sales_inquiry",
        "sales_inquiries",
        [FactKind::Owner, FactKind::Organization, FactKind::Collaborators].into(),
    )))
}
/// 构造实际任务关系查询映射。
/// # 参数
/// 无。
/// # 返回
/// 用数据库存在性关联证明完整任务关系。
/// # 错误
/// 无。
pub fn procurement_mapping() -> Arc<dyn AccessMapping> {
    Arc::new(TaskInquiryMapping {
        model: model(
            "procurement_inquiry",
            "procurement_inquiries",
            [FactKind::Assignee, FactKind::Participants, FactKind::Completed].into(),
        ),
    })
}
fn model(resource: &str, key: &str, facts: std::collections::BTreeSet<FactKind>) -> QueryModel {
    QueryModel {
        key: QueryModelKey::parse(key).expect("固定询价模型"),
        resource: resource.into(),
        collection: "sales_inquiries".into(),
        id_field: "id".into(),
        output: QueryOutput::Object,
        facts,
    }
}

#[cfg(test)]
mod tests {
    use application_core::AuditActor;
    use erp_core::AccountKind;
    use erp_core::common::time::Instant;
    use erp_identity::entity::authorization::{FactKind, QueryModelKey, QueryOutput};
    use erp_permissions::sales_inquiry as sales_inquiry_permissions;

    use super::*;

    fn context() -> AccessContext {
        AccessContext {
            actor: AuditActor::new("sales".into(), "sales".into(), AccountKind::Admin),
            permission: sales_inquiry_permissions::QUERY_SALES_LIST,
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

        let QueryFragment::Match(owner) =
            mapping.predicate(&AccessFact::Owned, &context, Some(&facts)).unwrap()
        else {
            panic!("owner fact must use current sales owner");
        };
        assert_eq!(owner, doc! {"sales_owner_user_id":"sales"});

        let QueryFragment::Match(org) = mapping
            .predicate(&AccessFact::OrganizationIn(["org".into()].into()), &context, Some(&facts))
            .unwrap()
        else {
            panic!("organization must use the inquiry business organization");
        };
        assert_eq!(org, doc! {"business_org_unit_id":{"$in":["org"]}});

        assert!(
            mapping
                .predicate(&AccessFact::WarehouseIn(["warehouse".into()].into()), &context, Some(&facts))
                .is_err()
        );
        assert!(mapping.relation(&RelationKey::parse("other").unwrap(), &context, Some(&facts)).is_err());
    }
    #[test]
    fn static_inquiry_mappings_have_no_preparation_dependency() {
        assert!(mapping().preparation().is_none());
        assert!(procurement_mapping().preparation().is_none());
    }
    #[test]
    fn procurement_mapping_uses_real_task_existence_and_exact_current_owner_for_internal_data() {
        let mapping = procurement_mapping();
        let mut context = context();
        context.permission = sales_inquiry_permissions::INTERNAL_FEE_BASES;
        context.resource = "procurement_inquiry".into();
        for fact in [AccessFact::AssignedTo, AccessFact::Participant, AccessFact::Completed(false)] {
            let QueryFragment::Exists { collection, bindings, pipeline } =
                mapping.predicate(&fact, &context, None).unwrap()
            else {
                panic!("task relations must be read in the database query");
            };
            assert_eq!(collection, Database::WORK_ITEMS);
            assert_eq!(bindings, doc! {"inquiry_id": "$id"});
            let clauses = pipeline[0].get_document("$match").unwrap().get_array("$and").unwrap();
            assert_eq!(clauses[2].as_document().unwrap().get_str("owner_user_id").unwrap(), "sales");
        }
        assert!(mapping.predicate(&AccessFact::Owned, &context, None).is_err());
    }

    #[test]
    fn procurement_task_facts_follow_reassignment_completion_and_real_version() {
        use erp_core::ids::WorkItemId;
        use erp_workflow::entity::work_item::{
            AssignmentSource, WorkItemData, WorkItemPriority, inquiry_subject_version,
        };
        let mut task = WorkItem::new_with_responsibility_scope(
            WorkItemId::new("task"),
            WorkItemData {
                work_item_type: WorkItemType::SalesInquiryProcessing,
                business_object_type: "sales_inquiry".into(),
                business_object_id: "inquiry".into(),
                subject_version: inquiry_subject_version(2),
                owner_role: "inquiry_buyer".into(),
                owner_organization_id: "org".into(),
                owner_user_id: "buyer".into(),
                assignment_source: AssignmentSource::SystemRule,
                priority: WorkItemPriority::Normal,
                due_at: None,
                reason_code: None,
                impact_summary: None,
            },
            "inquiry:buyer",
            vec!["a".into()],
        )
        .unwrap();

        let before = task_object(&task);
        assert_eq!(before.assignee_id, FactValue::Known(Some("buyer".into())));
        assert_eq!(before.completed, FactValue::Known(false));

        task.reassign("next", Instant::now()).unwrap();
        task.complete_by_domain_command("next", Instant::now()).unwrap();
        let after = task_object(&task);
        assert_eq!(after.version, task.base.version.to_string());
        assert_eq!(after.assignee_id, FactValue::Known(Some("next".into())));
        assert_eq!(after.participants, FactValue::Known(["next".into()].into()));
        assert_eq!(after.completed, FactValue::Known(true));
    }
    #[test]
    fn new_inquiry_candidate_keeps_ownership_without_claiming_a_persisted_revision() {
        use erp_sales::entity::sales_inquiry::{InquiryCreateData, InquiryRequirementData};
        let inquiry = SalesInquiry::create(
            "new".into(),
            "INQ-1".into(),
            InquiryCreateData {
                customer_id: "customer".into(),
                customer_name: "客户".into(),
                sales_owner_user_id: "owner".into(),
                business_org_unit_id: "org".into(),
                deadline_at: Instant::from_unix_secs(1_900_000_000),
                copied_from_inquiry_id: None,
                draft: InquiryRequirementData::default(),
            },
        )
        .unwrap();
        assert!(!object(&inquiry).version.is_empty());

        let candidate = inquiry_candidate(&inquiry);
        assert!(candidate.version.is_empty());
        assert_eq!(candidate.owner_id, FactValue::Known(Some("owner".into())));
        assert_eq!(candidate.org_unit_id, FactValue::Known(Some("org".into())));
    }
}
