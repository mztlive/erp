//! 库存查询把真实仓库来源交给统一编译器；不装载全量授权仓库 ID。
use std::collections::BTreeMap;
use std::sync::Arc;

use erp_identity::entity::authorization::{
    AccessContext, AccessFact, FactKind, QueryModel, QueryModelKey, QueryOutput, RelationKey,
};
use erp_identity::ports::{QueryFragment, RelationMapping};
use erp_identity::{AccessMapping, Error, Result};
use mongodb::Database;
use mongodb::bson::{Bson, doc};

use crate::ports::InventoryAction;
use crate::repository::InventoryExt;

/// 固定库存行模型及其真实仓库来源映射。
pub struct InventoryMapping {
    model: QueryModel,
    warehouse: Arc<dyn AccessMapping>,
}
impl InventoryMapping {
    /// 绑定拥有领域提供的仓库映射。
    /// 与库存集合和权限绑定的查询映射。
    /// action 为实际列表动作；warehouse 为仓库拥有领域的固定映射。
    /// 非列表动作或非法模型标识拒绝。
    pub fn shared(
        action: InventoryAction,
        warehouse: Arc<dyn AccessMapping>,
    ) -> Result<Arc<dyn AccessMapping>> {
        if warehouse.model().resource != "warehouse" {
            return Err(Error::ValidationError("库存来源必须绑定真实仓库资源".into()));
        }

        let permission = action.permission();
        let collection = match action {
            InventoryAction::BalanceList => <Database as InventoryExt>::STOCK_BALANCES,
            InventoryAction::MovementList => <Database as InventoryExt>::STOCK_MOVEMENTS,
            InventoryAction::ReservationList => <Database as InventoryExt>::STOCK_RESERVATIONS,
            InventoryAction::AdjustmentList => <Database as InventoryExt>::STOCK_ADJUSTMENTS,
            _ => return Err(Error::ValidationError("库存查询必须使用列表动作".into())),
        };

        Ok(Arc::new(Self {
            model: QueryModel {
                key: QueryModelKey::parse(format!("{}_rows", permission.resource()))?,
                resource: permission.resource().into(),
                collection: collection.into(),
                id_field: "id".into(),
                output: QueryOutput::Object,
                facts: [FactKind::Warehouse, FactKind::Collaborators, FactKind::HistoryParticipants]
                    .into_iter()
                    .chain((permission.resource() == "stock_adjustment").then_some(FactKind::Assignee))
                    .collect(),
            },
            warehouse,
        }))
    }
}
impl AccessMapping for InventoryMapping {
    fn model(&self) -> &QueryModel {
        &self.model
    }
    fn relations(&self) -> BTreeMap<RelationKey, Arc<dyn AccessMapping>> {
        [(RelationKey::parse("warehouse").expect("固定仓库来源标识"), self.warehouse.clone())].into()
    }
    fn key(&self, _: &AccessContext) -> Result<Bson> {
        Ok("$id".into())
    }
    fn records(&self, _: &AccessContext) -> Result<QueryFragment> {
        Ok(QueryFragment::Match(doc! {"deleted_at": 0}))
    }
    fn predicate(&self, fact: &AccessFact, context: &AccessContext) -> Result<QueryFragment> {
        let filter = match fact {
            AccessFact::Collaborating | AccessFact::HistoryParticipant => doc! { "$expr": false },
            AccessFact::WarehouseIn(ids) => {
                doc! { "warehouse_id": { "$in": ids.iter().cloned().collect::<Vec<_>>() } }
            },
            AccessFact::AssignedTo if self.model.resource == "stock_adjustment" => {
                doc! { "prepared_by": context.actor.id() }
            },
            _ => return Err(Error::ValidationError("库存未提供该事实维度".into())),
        };
        Ok(QueryFragment::Match(filter))
    }
    fn relation(&self, relation: &RelationKey, _: &AccessContext) -> Result<RelationMapping> {
        if relation.as_str() != "warehouse" {
            return Err(Error::ValidationError("未登记库存来源关系".into()));
        }
        Ok(RelationMapping {
            expected_ids: Bson::Array(vec!["$warehouse_id".into()]),
            mapping: self.warehouse.clone(),
        })
    }
}

#[cfg(test)]
mod tests {
    use application_core::AuditActor;
    use erp_core::AccountKind;
    use erp_core::common::time::Instant;
    use erp_permissions::inventory;

    use super::*;

    struct WarehouseSource(QueryModel);
    impl AccessMapping for WarehouseSource {
        fn model(&self) -> &QueryModel {
            &self.0
        }
        fn key(&self, _: &AccessContext) -> Result<Bson> {
            Ok("$id".into())
        }
        fn records(&self, _: &AccessContext) -> Result<QueryFragment> {
            Ok(QueryFragment::Match(doc! {"deleted_at":0}))
        }
        fn predicate(&self, _: &AccessFact, _: &AccessContext) -> Result<QueryFragment> {
            Err(Error::ValidationError("not used".into()))
        }
        fn relation(&self, _: &RelationKey, _: &AccessContext) -> Result<RelationMapping> {
            Err(Error::ValidationError("not used".into()))
        }
    }
    fn source() -> Arc<dyn AccessMapping> {
        Arc::new(WarehouseSource(QueryModel {
            key: QueryModelKey::parse("warehouse_source").unwrap(),
            resource: "warehouse".into(),
            collection: "warehouse_source".into(),
            id_field: "id".into(),
            output: QueryOutput::Object,
            facts: Default::default(),
        }))
    }
    fn context() -> AccessContext {
        AccessContext {
            actor: AuditActor::new("actor".into(), "actor".into(), AccountKind::Admin),
            permission: inventory::STOCK_BALANCE_LIST,
            resource: "stock_balance".into(),
            model: None,
            as_of: Instant::from_unix_secs(1),
        }
    }
    #[test]
    fn warehouse_relation_references_the_full_actual_warehouse_and_fixed_source() {
        let warehouse = source();
        let mapping = InventoryMapping::shared(InventoryAction::BalanceList, warehouse.clone()).unwrap();
        let relation = mapping.relation(&RelationKey::parse("warehouse").unwrap(), &context()).unwrap();
        assert_eq!(relation.expected_ids, Bson::Array(vec!["$warehouse_id".into()]));
        assert!(Arc::ptr_eq(&relation.mapping, &warehouse));
        assert!(mapping.relation(&RelationKey::parse("other").unwrap(), &context()).is_err());
        assert!(mapping.predicate(&AccessFact::Owned, &context()).is_err());
    }
    #[test]
    fn inventory_maps_actual_warehouse_and_task_assignment() {
        let mapping = InventoryMapping::shared(InventoryAction::AdjustmentList, source()).unwrap();

        let QueryFragment::Match(warehouse) =
            mapping.predicate(&AccessFact::WarehouseIn(["w1".into()].into()), &context()).unwrap()
        else {
            panic!("仓库范围应使用本行字段")
        };
        assert_eq!(warehouse, doc! {"warehouse_id": {"$in": ["w1"]}});

        let QueryFragment::Match(assigned) = mapping.predicate(&AccessFact::AssignedTo, &context()).unwrap()
        else {
            panic!("提交经办人应使用真实prepared_by")
        };
        assert_eq!(assigned, doc! {"prepared_by": "actor"});
    }

    #[test]
    fn inventory_queries_cannot_grant_collaboration_or_history_access() {
        for action in [
            InventoryAction::BalanceList,
            InventoryAction::MovementList,
            InventoryAction::ReservationList,
            InventoryAction::AdjustmentList,
        ] {
            let mapping = InventoryMapping::shared(action, source()).unwrap();
            for (fact, kind) in [
                (AccessFact::Collaborating, FactKind::Collaborators),
                (AccessFact::HistoryParticipant, FactKind::HistoryParticipants),
            ] {
                assert!(mapping.model().facts.contains(&kind));
                let predicate = mapping.predicate(&fact, &context()).unwrap();
                assert!(
                    matches!(predicate, QueryFragment::Match(document) if document == doc! { "$expr": false })
                );
            }
        }
    }

    #[test]
    fn list_models_are_fixed_to_owned_collections_and_reject_write_actions() {
        for (action, collection) in [
            (InventoryAction::BalanceList, <Database as InventoryExt>::STOCK_BALANCES),
            (InventoryAction::MovementList, <Database as InventoryExt>::STOCK_MOVEMENTS),
            (InventoryAction::ReservationList, <Database as InventoryExt>::STOCK_RESERVATIONS),
            (InventoryAction::AdjustmentList, <Database as InventoryExt>::STOCK_ADJUSTMENTS),
        ] {
            let mapping = InventoryMapping::shared(action, source()).unwrap();
            assert_eq!(mapping.model().collection, collection);
            assert!(
                matches!(mapping.records(&context()).unwrap(), QueryFragment::Match(filter) if filter == doc! {"deleted_at":0})
            );
        }

        for action in [
            InventoryAction::BalanceDetail,
            InventoryAction::AdjustmentDetail,
            InventoryAction::Create,
            InventoryAction::Update,
        ] {
            assert!(InventoryMapping::shared(action, source()).is_err());
        }
    }
}
