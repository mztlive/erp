//! 选品拥有领域提供字段映射与对象事实，不决定授权组合。
use std::collections::BTreeSet;
use std::sync::Arc;

use async_trait::async_trait;
use erp_identity::entity::authorization::{
    AccessContext, AccessFact, FactKind, FactValue, MappingFacts, ObjectFacts, QueryModel, QueryModelKey,
    QueryOutput, RelationKey, ResourceTargets,
};
use erp_identity::ports::{AccessFactsPort, AccessMapping, QueryFragment, RelationMapping};
use erp_identity::{Error as IdentityError, Result as IdentityResult};
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
    /// # 参数
    /// resource 为选品册或选品方案资源。
    /// # 返回
    /// 提供选品拥有集合的映射。
    /// # 错误
    /// 未知资源拒绝。
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
    /// # 参数
    /// resource 为固定选品资源，customer 为客户集合映射。
    /// # 返回
    /// 同时提供选品责任与真实客户关系的映射。
    /// # 错误
    /// 未知选品资源或非客户映射时拒绝。
    pub fn with_customer(resource: &str, customer: Arc<dyn AccessMapping>) -> IdentityResult<Self> {
        if customer.model().resource != "customer" {
            return Err(IdentityError::ValidationError("选品客户来源映射不匹配".into()));
        }
        let mut mapping = Self::new(resource)?;
        mapping.customer = Some(customer);
        Ok(mapping)
    }

    /// 方案通过有效父册解释当前责任；选品册直接使用本行字段。
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
    fn key(&self, _: &AccessContext, _: &MappingFacts) -> IdentityResult<Bson> {
        Ok("$id".into())
    }
    async fn prepare(&self, _: &AccessContext, _: &mut dyn Executor) -> IdentityResult<MappingFacts> {
        Ok(MappingFacts::default())
    }
    fn records(&self, _: &AccessContext, _: &MappingFacts) -> IdentityResult<QueryFragment> {
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
        _: &MappingFacts,
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
        _: &MappingFacts,
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
    proposed: Option<ObjectFacts>,
}
impl SelectionFacts {
    /// 绑定本域集合。
    /// # 参数
    /// db 为选品数据库。
    /// # 返回
    /// 不执行查询的事实端口。
    /// # 错误
    /// 无。
    pub fn new(db: Database) -> Self {
        Self { db, proposed: None }
    }
    /// 绑定创建命令的拟写入责任。
    /// # 参数
    /// db 为数据库，owner/org 为已经校验的责任字段。
    /// # 返回
    /// 仅能读取固定创建目标的事实端口。
    /// # 错误
    /// 不执行 I/O。
    pub fn proposed(db: Database, owner: &str, org: &str) -> Self {
        let facts = object("sales_selection_booklet", "selection_new", "proposed".into(), owner, org);
        Self { db, proposed: Some(facts) }
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
        if let Some(facts) = &self.proposed {
            return Ok(vec![facts.clone()]);
        }
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
            result.push(book_object(&context.resource, id, version, &book));
        }
        Ok(result)
    }
}
fn book_object(resource: &str, id: &str, version: String, book: &SalesSelectionBooklet) -> ObjectFacts {
    object(resource, id, version, &book.sales_owner_user_id, &book.business_org_unit_id)
}
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
    use erp_identity::Permission;

    use super::*;

    fn context(resource: &str) -> AccessContext {
        AccessContext {
            actor: AuditActor::new("sales-a".into(), "销售甲".into(), AccountKind::Admin),
            permission: Permission::parse(format!("{resource}:list")).unwrap(),
            resource: resource.into(),
            model: Some(QueryModelKey::parse(resource).unwrap()),
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
        for resource in ["sales_selection_booklet", "sales_selection_proposal"] {
            let mapping = SelectionMapping::with_customer(resource, customer.clone()).unwrap();
            let relation = mapping
                .relation(
                    &RelationKey::parse("customer").unwrap(),
                    &context(resource),
                    &MappingFacts::default(),
                )
                .unwrap();
            assert_eq!(relation.expected_ids, Bson::Array(vec!["$customer_id".into()]));
            assert!(Arc::ptr_eq(&relation.mapping, &customer));
            assert!(
                mapping
                    .relation(
                        &RelationKey::parse("unregistered").unwrap(),
                        &context(resource),
                        &MappingFacts::default(),
                    )
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
                    &context("sales_selection_booklet"),
                    &MappingFacts::default(),
                )
                .is_err()
        );
        assert!(SelectionMapping::with_customer("sales_selection_booklet", Arc::new(mapping)).is_err());
    }

    #[test]
    fn booklet_maps_current_owner_and_organization_fields() {
        let mapping = SelectionMapping::new("sales_selection_booklet").unwrap();
        let facts = MappingFacts::default();
        let context = context("sales_selection_booklet");
        let QueryFragment::Match(owner) = mapping.predicate(&AccessFact::Owned, &context, &facts).unwrap()
        else {
            panic!("册责任必须使用本行字段");
        };
        assert_eq!(owner, doc! { "sales_owner_user_id": "sales-a" });
        let QueryFragment::Match(org) = mapping
            .predicate(&AccessFact::OrganizationIn(["org-a".into()].into()), &context, &facts)
            .unwrap()
        else {
            panic!("册组织必须使用本行字段");
        };
        assert_eq!(org, doc! { "business_org_unit_id": { "$in": ["org-a"] } });
    }

    #[test]
    fn proposal_responsibility_uses_live_parent_instead_of_snapshot_fields() {
        let mapping = SelectionMapping::new("sales_selection_proposal").unwrap();
        let QueryFragment::Exists { collection, bindings, pipeline } = mapping
            .predicate(&AccessFact::Owned, &context("sales_selection_proposal"), &MappingFacts::default())
            .unwrap()
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
                    &context("sales_selection_booklet"),
                    &MappingFacts::default()
                )
                .is_err()
        );
    }

    #[test]
    fn proposed_object_keeps_known_empty_supplements_and_explicit_responsibility() {
        let proposed =
            object("sales_selection_booklet", "selection_new", "proposed".into(), "sales-a", "org-a");
        assert_eq!(proposed.owner_id, FactValue::Known(Some("sales-a".into())));
        assert_eq!(proposed.org_unit_id, FactValue::Known(Some("org-a".into())));
        assert_eq!(proposed.collaborators, FactValue::Known(BTreeSet::new()));
        assert_eq!(proposed.history_participants, FactValue::Known(BTreeSet::new()));
        assert_eq!(proposed.resource, "sales_selection_booklet");
        assert_eq!(proposed.id, "selection_new");
    }
}
