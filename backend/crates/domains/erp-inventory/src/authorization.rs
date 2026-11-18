//! 库存拥有领域提供真实库存事实；授权政策由 identity 固定执行。
use std::collections::BTreeSet;

use application_core::AuditActor;
use async_trait::async_trait;
use erp_identity::entity::authorization::{
    AccessContext, FactValue, ObjectFacts, RelatedObjects, RelationKey,
};
use erp_identity::{AccessFactsPort, AccessTarget, AuthorizationService, Error, ResourceTargets, Result};
use erp_permissions::inventory;
use mongodb::Database;
use persistence_core::Executor;

use crate::StockAdjustment;
use crate::repository::InventoryExt;

/// 读取完整目标集合中的当前库存对象及其仓库归属。
pub struct InventoryFacts(Database);

impl InventoryFacts {
    /// 装配库存对象事实读取器，使其使用业务执行器对应的数据库；构造不读取数据，也不保存请求级事实或授权。
    pub fn new(db: Database) -> Self {
        Self(db)
    }

    async fn object(
        &self,
        resource: &str,
        id: &str,
        executor: &mut dyn Executor,
    ) -> Result<Option<ObjectFacts>> {
        let object = match resource {
            "stock_adjustment" => self.0.stock_adjustments().find_by_id(id, executor).await?.map(submit_fact),
            "stock_balance" => self.0.stock_balances().find_by_id(id, executor).await?.map(|row| {
                warehouse_fact(
                    resource,
                    row.base.id,
                    row.base.version.to_string(),
                    row.warehouse_id.to_string(),
                )
            }),
            "stock_movement" => self.0.stock_movements().find_by_id(id, executor).await?.map(|row| {
                warehouse_fact(
                    resource,
                    row.base.id,
                    row.base.version.to_string(),
                    row.warehouse_id.to_string(),
                )
            }),
            "stock_reservation" => self.0.stock_reservations().find_by_id(id, executor).await?.map(|row| {
                warehouse_fact(
                    resource,
                    row.base.id,
                    row.base.version.to_string(),
                    row.warehouse_id.to_string(),
                )
            }),
            _ => return Err(Error::ValidationError("未知库存事实资源".into())),
        };
        Ok(object)
    }
}

#[async_trait]
impl AccessFactsPort for InventoryFacts {
    async fn load(
        &self,
        context: &AccessContext,
        ids: &ResourceTargets,
        executor: &mut dyn Executor,
    ) -> Result<Vec<ObjectFacts>> {
        let mut facts = Vec::new();
        for id in ids.ids() {
            if let Some(object) = self.object(&context.resource, id, executor).await? {
                facts.push(object);
            }
        }
        Ok(facts)
    }
}

fn warehouse_fact(resource: &str, id: String, version: String, warehouse: String) -> ObjectFacts {
    ObjectFacts {
        resource: resource.into(),
        id,
        version,
        warehouse_id: FactValue::Known(Some(warehouse)),
        collaborators: FactValue::Known(BTreeSet::new()),
        history_participants: FactValue::Known(BTreeSet::new()),
        ..Default::default()
    }
}

fn submit_fact(adjustment: StockAdjustment) -> ObjectFacts {
    let mut object = warehouse_fact(
        "stock_adjustment",
        adjustment.base.id,
        adjustment.base.version.to_string(),
        adjustment.warehouse_id.to_string(),
    );
    object.assignee_id = FactValue::Known(Some(adjustment.prepared_by));
    object
}

/// 创建调整单前只用经过业务校验的目标仓库 ID 构造候选，不声明持久化版本或仓库归属结论。
/// 统一授权从仓库读取器取得实际来源事实；空或非法仓库 ID 返回校验错误。
pub fn adjustment_candidate(warehouse: &str) -> Result<ObjectFacts> {
    let targets = ResourceTargets::new(&[warehouse])?;
    Ok(ObjectFacts {
        resource: "stock_adjustment".into(),
        warehouse_id: FactValue::Known(Some(warehouse.into())),
        collaborators: FactValue::Known(BTreeSet::new()),
        history_participants: FactValue::Known(BTreeSet::new()),
        relations: [(
            RelationKey::parse("warehouse")?,
            RelatedObjects { expected_ids: targets.ids().iter().cloned().collect(), objects: vec![] },
        )]
        .into(),
        ..Default::default()
    })
}

/// 提交库存调整审批前，在原写事务通过共享授权服务检查真实提交人、调整单的固定提交权限、实际仓库范围及经办指派。
/// 功能、对象、指派或执行边界不满足时拒绝，不把来自不同对象或角色的结论拼成授权。
pub async fn require_submit(
    authorization: &AuthorizationService,
    actor: &AuditActor,
    id: &str,
    executor: &mut dyn Executor,
) -> Result<()> {
    authorization
        .require(actor, &inventory::STOCK_ADJUSTMENT_SUBMIT, AccessTarget::record(id)?, executor)
        .await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn adjustment_creation_preserves_reference_and_cannot_supply_existing_warehouse_facts() {
        let candidate = adjustment_candidate("warehouse").unwrap();
        assert!(candidate.id.is_empty());
        assert!(candidate.version.is_empty());
        let relation = candidate.relations.get(&RelationKey::parse("warehouse").unwrap()).unwrap();
        assert_eq!(relation.expected_ids, ["warehouse".into()].into());
        assert!(relation.objects.is_empty());
        assert!(adjustment_candidate("").is_err());
    }

    #[test]
    fn warehouse_facts_keep_actual_record_identity_and_known_warehouse() {
        let object = warehouse_fact("stock_balance", "balance".into(), "7".into(), "warehouse".into());
        assert_eq!(object.id, "balance");
        assert_eq!(object.version, "7");
        assert_eq!(object.warehouse_id, FactValue::Known(Some("warehouse".into())));
        assert_eq!(object.assignee_id, FactValue::Missing);
        assert_eq!(object.collaborators, FactValue::Known(BTreeSet::new()));
        assert_eq!(object.history_participants, FactValue::Known(BTreeSet::new()));
    }
}
