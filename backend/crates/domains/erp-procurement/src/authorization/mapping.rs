use std::collections::BTreeMap;
use std::sync::Arc;

use entity_core::NOT_DELETED_TIMESTAMP_BSON;
use erp_identity::entity::authorization::{
    AccessContext, AccessFact, FactKind, MappingFacts, QueryModel, QueryModelKey, QueryOutput, RelationKey,
};
use erp_identity::ports::{QueryFragment, RelationMapping};
use erp_identity::{AccessMapping, Error, Result};
use mongodb::Database;
use mongodb::bson::{Bson, doc};

use crate::repository::PurchaseOrderExt;

/// 采购当前责任字段与完整本单来源关系。
pub struct PurchaseMapping {
    model: QueryModel,
}

impl PurchaseMapping {
    /// 为服务端固定入口建立采购集合映射。
    /// # 参数
    /// resource 为组合根登记的入口资源，不接受 HTTP 参数。
    /// # 返回
    /// 返回以采购单为每行粒度的对象模型。
    /// # 错误
    /// 资源或模型标识非法时拒绝。
    pub fn new(resource: &str) -> Result<Self> {
        let model = QueryModel {
            key: QueryModelKey::parse(format!("{resource}_purchase"))?,
            resource: resource.into(),
            collection: <Database as PurchaseOrderExt>::PURCHASE_ORDERS.into(),
            id_field: "id".into(),
            output: QueryOutput::Object,
            facts: [FactKind::Owner, FactKind::Organization, FactKind::Warehouse].into(),
        };
        model.validate()?;
        Ok(Self { model })
    }
}

impl AccessMapping for PurchaseMapping {
    fn model(&self) -> &QueryModel {
        &self.model
    }

    fn relations(&self) -> BTreeMap<RelationKey, Arc<dyn AccessMapping>> {
        if self.model.resource == "purchase_order" {
            return BTreeMap::new();
        }
        [(
            RelationKey::parse("self_purchase").expect("固定来源标识"),
            Arc::new(Self::new("purchase_order").expect("固定采购模型")) as Arc<dyn AccessMapping>,
        )]
        .into()
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
        let predicate = match fact {
            AccessFact::Owned => doc! { "owner_user_id": context.actor.id() },
            AccessFact::OrganizationIn(ids) => {
                doc! { "business_org_unit_id": { "$in": ids.iter().cloned().collect::<Vec<_>>() } }
            },
            AccessFact::WarehouseIn(ids) => {
                doc! { "target_warehouse_id": { "$in": ids.iter().cloned().collect::<Vec<_>>() } }
            },
            _ => return Err(Error::ValidationError("采购 Mapping 未提供该事实维度".into())),
        };
        Ok(QueryFragment::Match(predicate))
    }

    fn relation(
        &self,
        relation: &RelationKey,
        _: &AccessContext,
        _: Option<&MappingFacts>,
    ) -> Result<RelationMapping> {
        if relation.as_str() != "self_purchase" {
            return Err(Error::ValidationError("采购 Mapping 未登记该来源关系".into()));
        }
        Ok(RelationMapping {
            expected_ids: Bson::Array(vec!["$id".into()]),
            mapping: Arc::new(Self::new("purchase_order")?),
        })
    }
}

#[cfg(test)]
mod tests {
    use application_core::AuditActor;
    use erp_core::AccountKind;
    use erp_core::common::time::Instant;
    use erp_identity::Permission;

    use super::*;

    fn context() -> AccessContext {
        AccessContext {
            actor: AuditActor::new("buyer".into(), "采购".into(), AccountKind::Admin),
            permission: Permission::parse("purchase_order:list").unwrap(),
            resource: "purchase_order".into(),
            model: None,
            as_of: Instant::from_unix_secs(1),
        }
    }

    #[test]
    fn facts_use_current_purchase_owner_org_and_target_warehouse() {
        let mapping = PurchaseMapping::new("purchase_order").unwrap();
        for (fact, expected) in [
            (AccessFact::Owned, doc! { "owner_user_id": "buyer" }),
            (
                AccessFact::OrganizationIn(["org".into()].into()),
                doc! { "business_org_unit_id": { "$in": ["org"] } },
            ),
            (
                AccessFact::WarehouseIn(["warehouse".into()].into()),
                doc! { "target_warehouse_id": { "$in": ["warehouse"] } },
            ),
        ] {
            let QueryFragment::Match(actual) = mapping.predicate(&fact, &context(), None).unwrap() else {
                panic!("采购字段事实应编译为单字段条件");
            };
            assert_eq!(actual, expected);
        }
        assert!(mapping.predicate(&AccessFact::Participant, &context(), None).is_err());
    }

    #[test]
    fn self_relation_keeps_the_complete_id_and_unknown_relationship_is_rejected() {
        let mapping = PurchaseMapping::new("supplier_payment").unwrap();
        let relation =
            mapping.relation(&RelationKey::parse("self_purchase").unwrap(), &context(), None).unwrap();
        assert_eq!(relation.expected_ids, Bson::Array(vec!["$id".into()]));
        assert_eq!(relation.mapping.model().resource, "purchase_order");
        assert!(mapping.relation(&RelationKey::parse("other").unwrap(), &context(), None).is_err());
        assert!(PurchaseMapping::new("bad.resource").is_err());
    }
}
