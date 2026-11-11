//! 库存查询把真实仓库来源交给统一编译器；不装载全量授权仓库 ID。
use std::sync::Arc;

use async_trait::async_trait;
use erp_identity::entity::authorization::{
    AccessContext, AccessFact, MappingFacts, QueryModel, QueryModelKey, QueryOutput, RelationKey,
};
use erp_identity::ports::{QueryFragment, RelationMapping};
use erp_identity::{AccessMapping, Error, Permission, Result};
use mongodb::Database;
use mongodb::bson::{Bson, doc};
use persistence_core::Executor;

use crate::ports::InventoryAction;
use crate::repository::InventoryExt;

/// 固定库存行模型及其真实仓库来源映射。
pub struct InventoryMapping {
    model: QueryModel,
    warehouse: Arc<dyn AccessMapping>,
}
impl InventoryMapping {
    /// 绑定拥有领域提供的仓库映射。
    /// # 参数
    /// action 为实际列表动作；warehouse 为仓库拥有领域的固定映射。
    /// # 返回
    /// 与库存集合和权限绑定的查询映射。
    /// # 错误
    /// 非列表动作或非法模型标识拒绝。
    pub fn shared(
        action: InventoryAction,
        warehouse: Arc<dyn AccessMapping>,
    ) -> Result<Arc<dyn AccessMapping>> {
        let permission = Permission::parse(action.permission())?;
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
                facts: Default::default(),
            },
            warehouse,
        }))
    }
}
#[async_trait]
impl AccessMapping for InventoryMapping {
    fn model(&self) -> &QueryModel {
        &self.model
    }
    fn key(&self, _: &AccessContext, _: &MappingFacts) -> Result<Bson> {
        Ok("$id".into())
    }
    async fn prepare(&self, _: &AccessContext, _: &mut dyn Executor) -> Result<MappingFacts> {
        Ok(MappingFacts::default())
    }
    fn records(&self, _: &AccessContext, _: &MappingFacts) -> Result<QueryFragment> {
        Ok(QueryFragment::Match(doc! {"deleted_at": 0}))
    }
    fn predicate(&self, _: &AccessFact, _: &AccessContext, _: &MappingFacts) -> Result<QueryFragment> {
        Err(Error::ValidationError("库存根模型不解释仓库权限".into()))
    }
    fn relation(
        &self,
        relation: &RelationKey,
        _: &AccessContext,
        _: &MappingFacts,
    ) -> Result<RelationMapping> {
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

    use super::*;

    struct WarehouseSource(QueryModel);
    #[async_trait]
    impl AccessMapping for WarehouseSource {
        fn model(&self) -> &QueryModel {
            &self.0
        }
        fn key(&self, _: &AccessContext, _: &MappingFacts) -> Result<Bson> {
            Ok("$id".into())
        }
        async fn prepare(&self, _: &AccessContext, _: &mut dyn Executor) -> Result<MappingFacts> {
            Ok(MappingFacts::default())
        }
        fn records(&self, _: &AccessContext, _: &MappingFacts) -> Result<QueryFragment> {
            Ok(QueryFragment::Match(doc! {"deleted_at":0}))
        }
        fn predicate(&self, _: &AccessFact, _: &AccessContext, _: &MappingFacts) -> Result<QueryFragment> {
            Err(Error::ValidationError("not used".into()))
        }
        fn relation(&self, _: &RelationKey, _: &AccessContext, _: &MappingFacts) -> Result<RelationMapping> {
            Err(Error::ValidationError("not used".into()))
        }
    }
    fn source() -> Arc<dyn AccessMapping> {
        Arc::new(WarehouseSource(QueryModel {
            key: QueryModelKey::parse("warehouse_source").unwrap(),
            resource: "stock_balance".into(),
            collection: "warehouse_source".into(),
            id_field: "id".into(),
            output: QueryOutput::Object,
            facts: Default::default(),
        }))
    }
    fn context() -> AccessContext {
        AccessContext {
            actor: AuditActor::new("actor".into(), "actor".into(), AccountKind::Admin),
            permission: Permission::parse("stock_balance:list").unwrap(),
            resource: "stock_balance".into(),
            model: None,
            as_of: Instant::from_unix_secs(1),
        }
    }
    #[test]
    fn warehouse_relation_references_the_full_actual_warehouse_and_fixed_source() {
        let warehouse = source();
        let mapping = InventoryMapping::shared(InventoryAction::BalanceList, warehouse.clone()).unwrap();
        let relation = mapping
            .relation(&RelationKey::parse("warehouse").unwrap(), &context(), &MappingFacts::default())
            .unwrap();
        assert_eq!(relation.expected_ids, Bson::Array(vec!["$warehouse_id".into()]));
        assert!(Arc::ptr_eq(&relation.mapping, &warehouse));
        assert!(
            mapping
                .relation(&RelationKey::parse("other").unwrap(), &context(), &MappingFacts::default())
                .is_err()
        );
        assert!(mapping.predicate(&AccessFact::Owned, &context(), &MappingFacts::default()).is_err());
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
                matches!(mapping.records(&context(), &MappingFacts::default()).unwrap(), QueryFragment::Match(filter) if filter == doc! {"deleted_at":0})
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
