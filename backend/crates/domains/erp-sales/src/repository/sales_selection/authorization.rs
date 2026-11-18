//! 选品拥有领域提供字段映射与对象事实，不决定授权组合。
use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use async_trait::async_trait;
use erp_identity::entity::authorization::{
    AccessContext, AccessFact, FactKind, FactValue, MappingFacts, ObjectFacts, QueryModel, QueryModelKey,
    QueryOutput, RelatedObjects, RelationKey, ResourceTargets,
};
use erp_identity::ports::{AccessFactsPort, AccessMapping, QueryFragment, RelationMapping};
use erp_identity::{Error as IdentityError, Result as IdentityResult};
use erp_permissions::customer;
use mongodb::Database;
use mongodb::bson::{Bson, Document, doc};
use persistence_core::Executor;

use crate::entity::sales_selection::SalesSelectionBooklet;
use crate::repository::SalesSelectionExt;

/// 固定选品查询模型；方案责任取实际所属册。
pub struct SelectionMapping {
    model: QueryModel,
    customer: Option<Arc<dyn AccessMapping>>,
}
impl SelectionMapping {
    /// 创建固定业务模型。
    /// 结果：提供选品拥有集合的映射。
    /// 未知资源拒绝。
    ///
    /// 输入约定：resource 为选品册或选品方案资源。
    pub fn new(resource: &str) -> IdentityResult<Self> {
        let collection = match resource {
            "sales_selection_booklet" => Database::SALES_SELECTION_BOOKLETS,
            "sales_selection_proposal" => Database::SALES_SELECTION_PROPOSALS,
            _ => return Err(IdentityError::ValidationError("非选品资源".into())),
        };
        Ok(Self {
            customer: None,
            model: QueryModel {
                key: QueryModelKey::parse(resource)?,
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
        })
    }
    /// 绑定客户拥有领域的来源映射。
    /// 结果：同时提供选品责任与真实客户关系的映射。
    /// 未知选品资源或非客户映射时拒绝。
    ///
    /// 输入约定：resource 为固定选品资源，customer 为客户集合映射。
    pub fn with_customer(resource: &str, customer: Arc<dyn AccessMapping>) -> IdentityResult<Self> {
        if customer.model().resource != "customer" {
            return Err(IdentityError::ValidationError("选品客户来源映射不匹配".into()));
        }
        let mut mapping = Self::new(resource)?;
        mapping.customer = Some(customer);
        Ok(mapping)
    }

    // 解释选品授权中的当前责任：方案沿有效父册取得责任，册对象直接使用本身的责任字段。
    fn responsibility(&self, condition: Document) -> QueryFragment {
        if self.model.resource == "sales_selection_booklet" {
            QueryFragment::Match(condition)
        } else {
            QueryFragment::Exists {
                collection: Database::SALES_SELECTION_BOOKLETS.into(),
                bindings: doc! { "book": "$booklet_id", "deleted": "$deleted_at" },
                pipeline: vec![doc! { "$match": { "deleted_at": 0, "$and": [
                    { "$expr": { "$and": [{ "$eq": ["$id", "$$book"] }, { "$eq": ["$$deleted", 0] }] } }, condition
                ] } }],
            }
        }
    }
}
#[async_trait]
impl AccessMapping for SelectionMapping {
    fn model(&self) -> &QueryModel {
        &self.model
    }
    fn relations(&self) -> BTreeMap<RelationKey, Arc<dyn AccessMapping>> {
        self.customer
            .as_ref()
            .map(|customer| {
                [(RelationKey::parse("customer").expect("固定客户来源标识"), customer.clone())].into()
            })
            .unwrap_or_default()
    }
    fn key(&self, _: &AccessContext, _: Option<&MappingFacts>) -> IdentityResult<Bson> {
        Ok("$id".into())
    }
    fn records(&self, _: &AccessContext, _: Option<&MappingFacts>) -> IdentityResult<QueryFragment> {
        if self.model.resource == "sales_selection_proposal" {
            Ok(self.responsibility(doc! {}))
        } else {
            Ok(QueryFragment::Match(doc! { "deleted_at": 0 }))
        }
    }
    fn predicate(
        &self,
        fact: &AccessFact,
        context: &AccessContext,
        _: Option<&MappingFacts>,
    ) -> IdentityResult<QueryFragment> {
        let condition = match fact {
            AccessFact::Owned => doc! { "sales_owner_user_id": context.actor.id() },
            AccessFact::OrganizationIn(ids) => {
                doc! { "business_org_unit_id": { "$in": ids.iter().collect::<Vec<_>>() } }
            },
            AccessFact::Collaborating | AccessFact::HistoryParticipant => doc! { "$expr": false },
            _ => return Err(IdentityError::ValidationError("选品未提供该事实维度".into())),
        };
        Ok(self.responsibility(condition))
    }
    fn relation(
        &self,
        relation: &RelationKey,
        _: &AccessContext,
        _: Option<&MappingFacts>,
    ) -> IdentityResult<RelationMapping> {
        if relation.as_str() != "customer" {
            return Err(IdentityError::ValidationError("选品模型未登记该来源关系".into()));
        }
        let mapping = self
            .customer
            .clone()
            .ok_or_else(|| IdentityError::ValidationError("选品客户来源映射未装配".into()))?;
        Ok(RelationMapping { expected_ids: Bson::Array(vec!["$customer_id".into()]), mapping })
    }
}

/// 选品对象事实；创建命令可提供已校验的拟写入责任事实。
pub struct SelectionFacts {
    db: Database,
    customers: Arc<dyn AccessFactsPort>,
}
impl SelectionFacts {
    /// 绑定本域集合。
    /// 结果：不执行查询的事实端口。
    ///
    /// 输入约定：db 为选品数据库，customers 为客户拥有领域的事实读取器。
    pub fn new(db: Database, customers: Arc<dyn AccessFactsPort>) -> Self {
        Self { db, customers }
    }
}
#[async_trait]
impl AccessFactsPort for SelectionFacts {
    async fn load(
        &self,
        context: &AccessContext,
        ids: &ResourceTargets,
        executor: &mut dyn Executor,
    ) -> IdentityResult<Vec<ObjectFacts>> {
        let mut result = Vec::new();
        for id in ids.ids() {
            let (book, version) = if context.resource == "sales_selection_booklet" {
                let Some(book) = self.db.sales_selection_booklets().find_by_id(id, executor).await? else {
                    continue;
                };
                let version = book.base.version.to_string();
                (book, version)
            } else {
                let Some(proposal) = self.db.sales_selection_proposals().find_by_id(id, executor).await?
                else {
                    continue;
                };
                let Some(book) = self
                    .db
                    .sales_selection_booklets()
                    .find_by_id(proposal.booklet_id.as_ref(), executor)
                    .await?
                else {
                    continue;
                };
                let version = format!("{}:{}:{}", proposal.base.version, book.base.id, book.base.version);
                (book, version)
            };

            let mut root = book_object(&context.resource, id, version, &book);
            let customer_id = book.customer_id.to_string();
            let source_context = AccessContext {
                resource: "customer".into(),
                permission: if context.permission.action() == "list" {
                    customer::LIST
                } else {
                    customer::DETAIL
                },
                model: None,
                ..context.clone()
            };
            let targets = ResourceTargets::new(&[&customer_id])?;
            let objects = self.customers.load(&source_context, &targets, executor).await?;
            root.relations.insert(
                RelationKey::parse("customer")?,
                RelatedObjects { expected_ids: [customer_id].into(), objects },
            );
            result.push(root);
        }
        Ok(result)
    }
}
fn book_object(resource: &str, id: &str, version: String, book: &SalesSelectionBooklet) -> ObjectFacts {
    object(resource, id, version, &book.sales_owner_user_id, &book.business_org_unit_id)
}
/// 构造选品册拟创建责任，候选不携带任何持久化身份版本。
/// 结果：固定选品册资源、空记录身份及空版本的创建候选。
/// 客户身份为空、未规范化或超过上限时拒绝。
///
/// 输入约定：customer_id 为实际客户身份；owner、org 为创建用例已校验的销售负责人和业务组织。
pub fn candidate(customer_id: &str, owner: &str, org: &str) -> IdentityResult<ObjectFacts> {
    let targets = ResourceTargets::new(&[customer_id])?;
    let mut candidate = object("sales_selection_booklet", "", String::new(), owner, org);
    candidate.relations.insert(
        RelationKey::parse("customer")?,
        RelatedObjects { expected_ids: targets.ids().iter().cloned().collect(), objects: vec![] },
    );
    Ok(candidate)
}

// 将已经读取或校验的责任字段转换为选品事实。
// 结果：保留原始身份、版本和当前责任的对象事实。
// 失败条件：不执行 I/O，字段合法性由创建用例保证。
//
// 输入约定：resource、id、version 为业务身份，owner、org 为已校验的销售责任。
fn object(resource: &str, id: &str, version: String, owner: &str, org: &str) -> ObjectFacts {
    ObjectFacts {
        resource: resource.into(),
        id: id.into(),
        version,
        owner_id: FactValue::Known(Some(owner.into())),
        org_unit_id: FactValue::Known(Some(org.into())),
        collaborators: FactValue::Known(BTreeSet::new()),
        history_participants: FactValue::Known(BTreeSet::new()),
        ..Default::default()
    }
}

#[cfg(test)]
mod tests {
    use application_core::AuditActor;
    use erp_core::AccountKind;
    use erp_core::common::time::Instant;
    use erp_permissions::{Permission, sales_selection};

    use super::*;

    fn context(permission: &Permission) -> AccessContext {
        AccessContext {
            actor: AuditActor::new("sales-a".into(), "销售甲".into(), AccountKind::Admin),
            permission: permission.clone(),
            resource: permission.resource().into(),
            model: Some(QueryModelKey::parse(permission.resource()).unwrap()),
            as_of: Instant::from_unix_secs(10),
        }
    }

    fn customer_mapping() -> Arc<dyn AccessMapping> {
        let mut mapping = SelectionMapping::new("sales_selection_booklet").unwrap();
        mapping.model.resource = "customer".into();
        mapping.model.collection = "test_customer_sources".into();
        mapping.model.key = QueryModelKey::parse("test_customer_sources").unwrap();
        Arc::new(mapping)
    }

    #[test]
    fn customer_relation_preserves_the_actual_reference_and_registered_source() {
        let customer = customer_mapping();
        for permission in [sales_selection::BOOKLET_LIST, sales_selection::PROPOSAL_LIST] {
            let resource = permission.resource();
            let mapping = SelectionMapping::with_customer(resource, customer.clone()).unwrap();
            let relation = mapping
                .relation(&RelationKey::parse("customer").unwrap(), &context(&permission), None)
                .unwrap();
            assert_eq!(relation.expected_ids, Bson::Array(vec!["$customer_id".into()]));
            assert!(Arc::ptr_eq(&relation.mapping, &customer));
            assert!(
                mapping
                    .relation(&RelationKey::parse("unregistered").unwrap(), &context(&permission), None,)
                    .is_err()
            );
        }
    }

    #[test]
    fn missing_or_mismatched_customer_mapping_does_not_drop_the_customer_boundary() {
        let mapping = SelectionMapping::new("sales_selection_booklet").unwrap();
        assert!(
            mapping
                .relation(
                    &RelationKey::parse("customer").unwrap(),
                    &context(&sales_selection::BOOKLET_LIST),
                    None,
                )
                .is_err()
        );
        assert!(SelectionMapping::with_customer("sales_selection_booklet", Arc::new(mapping)).is_err());
    }

    #[test]
    fn booklet_maps_current_owner_and_organization_fields() {
        let mapping = SelectionMapping::new("sales_selection_booklet").unwrap();
        let facts = None;
        let context = context(&sales_selection::BOOKLET_LIST);
        let QueryFragment::Match(owner) = mapping.predicate(&AccessFact::Owned, &context, facts).unwrap()
        else {
            panic!("册责任必须使用本行字段");
        };
        assert_eq!(owner, doc! { "sales_owner_user_id": "sales-a" });

        let QueryFragment::Match(org) =
            mapping.predicate(&AccessFact::OrganizationIn(["org-a".into()].into()), &context, facts).unwrap()
        else {
            panic!("册组织必须使用本行字段");
        };
        assert_eq!(org, doc! { "business_org_unit_id": { "$in": ["org-a"] } });
    }

    #[test]
    fn proposal_responsibility_uses_live_parent_instead_of_snapshot_fields() {
        let mapping = SelectionMapping::new("sales_selection_proposal").unwrap();
        let QueryFragment::Exists { collection, bindings, pipeline } =
            mapping.predicate(&AccessFact::Owned, &context(&sales_selection::PROPOSAL_LIST), None).unwrap()
        else {
            panic!("方案责任必须关联实际父册");
        };
        assert_eq!(collection, Database::SALES_SELECTION_BOOKLETS);
        assert_eq!(bindings, doc! { "book": "$booklet_id", "deleted": "$deleted_at" });
        let condition = pipeline[0].get_document("$match").unwrap();
        assert_eq!(condition.get_i32("deleted_at").unwrap(), 0);
        assert_eq!(condition.get_array("$and").unwrap()[1], doc! { "sales_owner_user_id": "sales-a" }.into());
    }

    #[test]
    fn unsupported_dimensions_and_resources_fail_instead_of_broadening() {
        assert!(SelectionMapping::new("work_item").is_err());
        let mapping = SelectionMapping::new("sales_selection_booklet").unwrap();
        assert!(
            mapping
                .predicate(
                    &AccessFact::WarehouseIn(["warehouse".into()].into()),
                    &context(&sales_selection::BOOKLET_LIST),
                    None
                )
                .is_err()
        );
    }

    #[test]
    fn proposed_object_keeps_known_empty_supplements_and_explicit_responsibility() {
        let proposed = candidate("customer", "sales-a", "org-a").unwrap();
        assert_eq!(proposed.owner_id, FactValue::Known(Some("sales-a".into())));
        assert_eq!(proposed.org_unit_id, FactValue::Known(Some("org-a".into())));
        assert_eq!(proposed.collaborators, FactValue::Known(BTreeSet::new()));
        assert_eq!(proposed.history_participants, FactValue::Known(BTreeSet::new()));
        assert_eq!(proposed.resource, "sales_selection_booklet");
        assert!(proposed.id.is_empty());
        assert!(proposed.version.is_empty());

        let relation = proposed.relations.get(&RelationKey::parse("customer").unwrap()).unwrap();
        assert_eq!(relation.expected_ids, ["customer".into()].into());
        assert!(relation.objects.is_empty());
        assert!(candidate("", "sales-a", "org-a").is_err());
    }
}
