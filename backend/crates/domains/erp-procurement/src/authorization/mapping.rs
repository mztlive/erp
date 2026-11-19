use entity_core::NOT_DELETED_TIMESTAMP_BSON;
use erp_identity::entity::authorization::{
    AccessContext, AccessFact, FactKind, QueryModel, QueryModelKey, QueryOutput, RelationKey,
};
use erp_identity::ports::{QueryFragment, RelationMapping};
use erp_identity::{AccessMapping, Error, Result};
use mongodb::Database;
use mongodb::bson::{Bson, doc};

use crate::repository::PurchaseOrderExt;

/// 采购当前责任字段映射。
pub struct PurchaseMapping {
    model: QueryModel,
}

impl PurchaseMapping {
    /// 为服务端固定入口建立采购集合映射。
    ///
    /// 返回以采购单为每行粒度的对象模型。资源或模型标识非法时拒绝。
    ///
    /// resource 为组合根登记的入口资源，不接受 HTTP 参数。
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

    fn key(&self, _: &AccessContext) -> Result<Bson> {
        Ok("$id".into())
    }

    fn records(&self, _: &AccessContext) -> Result<QueryFragment> {
        Ok(QueryFragment::Match(doc! { "deleted_at": NOT_DELETED_TIMESTAMP_BSON }))
    }

    fn predicate(&self, fact: &AccessFact, context: &AccessContext) -> Result<QueryFragment> {
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

    fn relation(&self, _: &RelationKey, _: &AccessContext) -> Result<RelationMapping> {
        Err(Error::ValidationError("采购 Mapping 未登记该来源关系".into()))
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use application_core::AuditActor;
    use erp_core::AccountKind;
    use erp_core::common::time::Instant;
    use erp_identity::entity::access_control::{ResolvedScope, ScopeClause};
    use erp_identity::testing::fixed_authorization;
    use erp_permissions::{internal, purchase_order, purchase_payment_request};
    use persistence_core::ReadExecutor;

    use super::*;

    fn context() -> AccessContext {
        AccessContext {
            actor: AuditActor::new("buyer".into(), "采购".into(), AccountKind::Admin),
            permission: purchase_order::LIST,
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
            let QueryFragment::Match(actual) = mapping.predicate(&fact, &context()).unwrap() else {
                panic!("采购字段事实应编译为单字段条件");
            };
            assert_eq!(actual, expected);
        }

        assert!(mapping.predicate(&AccessFact::Participant, &context()).is_err());
    }

    #[test]
    fn purchase_models_have_no_source_relationships() {
        for resource in ["purchase_order", "supplier_payment"] {
            let mapping = PurchaseMapping::new(resource).unwrap();
            assert!(mapping.relations().is_empty());
            for relation in ["self_purchase", "other"] {
                assert!(matches!(
                    mapping.relation(&RelationKey::parse(relation).unwrap(), &context()),
                    Err(Error::ValidationError(message)) if message == "采购 Mapping 未登记该来源关系"
                ));
            }
        }

        assert!(PurchaseMapping::new("bad.resource").is_err());
    }

    #[tokio::test]
    async fn company_purchase_queries_compile_without_history_or_cross_collection_lookups() {
        let entries = [
            purchase_order::LIST,
            purchase_order::DETAIL,
            internal::PAYMENT_REQUEST_CANDIDATES,
            internal::PURCHASE_PORTAL_IMPACT_READ,
        ];
        let authorization = fixed_authorization(
            &[purchase_order::LIST, purchase_order::DETAIL, purchase_payment_request::CREATE],
            ResolvedScope {
                role_clauses: vec![ScopeClause { company: true, ..Default::default() }],
                user_limit: None,
            },
            vec![],
            entries
                .iter()
                .cloned()
                .map(|entry| {
                    (
                        entry,
                        Arc::new(PurchaseMapping::new("purchase_order").unwrap()) as Arc<dyn AccessMapping>,
                    )
                })
                .collect(),
        )
        .unwrap();

        for entry in entries {
            let compiled =
                authorization.query(&context().actor, &entry, &mut ReadExecutor::default()).await.unwrap();
            assert_eq!(
                compiled.stages(),
                &[doc! { "$match": { "$and": [
                    { "deleted_at": NOT_DELETED_TIMESTAMP_BSON }, { "$expr": true }
                ] } }],
                "{entry}"
            );
        }
    }

    #[tokio::test]
    async fn company_purchase_query_still_requires_its_function_permission() {
        let authorization = fixed_authorization(
            &[purchase_order::DETAIL],
            ResolvedScope {
                role_clauses: vec![ScopeClause { company: true, ..Default::default() }],
                user_limit: None,
            },
            vec![],
            vec![(purchase_order::LIST, Arc::new(PurchaseMapping::new("purchase_order").unwrap()))],
        )
        .unwrap();

        assert!(matches!(
            authorization.query(&context().actor, &purchase_order::LIST, &mut ReadExecutor::default()).await,
            Err(Error::Forbidden(_))
        ));
    }
}
