//! 采购、选品和库存的事实及静态查询实现绑定，不声明权限政策。
use std::collections::BTreeSet;
use std::sync::Arc;

use async_trait::async_trait;
use erp_identity::entity::authorization::{
    AccessContext, AccessFact, FactKind, FactValue, MappingFacts, ObjectFacts, QueryModel, RelatedObjects,
    RelationKey,
};
use erp_identity::ports::{AccessFactsPort, AccessMapping, QueryFragment, RelationMapping};
use erp_identity::{ResourceTargets, Result};
use erp_inventory::InventoryAction;
use erp_inventory::authorization::InventoryFacts;
use erp_inventory::repository::inventory::authorization::InventoryMapping;
use erp_permissions::{internal, inventory, purchase_order, sales_selection as selection};
use erp_procurement::authorization::{PurchaseFacts, PurchaseMapping};
use erp_sales::repository::sales_selection::authorization::{SelectionFacts, SelectionMapping};
use erp_workflow::DocumentRegistryExt;
use erp_workflow::repository::prelude::*;
use mongodb::Database;
use mongodb::bson::{Bson, doc};
use persistence_core::Executor;

use super::{FactsBindings, MappingBindings};

// 建立采购、选品与库存的全部实现绑定，供统一 Builder 校验。
// 按资源唯一的 Facts，以及按固定查询权限绑定的 Mapping。
// 固定模型标识或来源模型不匹配时拒绝；构造不执行 I/O。
pub(super) fn bindings(
    db: Database,
    customer_facts: Arc<dyn AccessFactsPort>,
    customer_mapping: Arc<dyn AccessMapping>,
    warehouse_facts: Arc<dyn AccessFactsPort>,
    warehouse_mapping: Arc<dyn AccessMapping>,
) -> Result<(FactsBindings, MappingBindings, Arc<dyn AccessMapping>)> {
    let purchase = purchase_mapping()?;
    let selections = Arc::new(SelectionFacts::new(db.clone(), customer_facts));
    let inventory: Arc<dyn AccessFactsPort> =
        Arc::new(InventoryObjects { inner: InventoryFacts::new(db.clone()), warehouses: warehouse_facts });
    let mut facts: FactsBindings = vec![
        ("purchase_order".into(), Arc::new(PurchaseObjects { db })),
        ("sales_selection_booklet".into(), selections.clone()),
        ("sales_selection_proposal".into(), selections),
    ];
    facts.extend(
        ["stock_balance", "stock_movement", "stock_reservation", "stock_adjustment"]
            .into_iter()
            .map(|resource| (resource.into(), inventory.clone())),
    );
    let mut mappings = vec![
        (purchase_order::LIST, purchase.clone()),
        (purchase_order::DETAIL, purchase.clone()),
        (internal::PURCHASE_DELIVERY_OPTIONS_READ, purchase.clone()),
        (internal::PAYMENT_REQUEST_CANDIDATES, purchase.clone()),
        (internal::PURCHASE_PORTAL_IMPACT_READ, purchase.clone()),
    ];
    selection_mappings(customer_mapping, &mut mappings)?;
    inventory_mappings(warehouse_mapping, &mut mappings)?;
    Ok((facts, mappings, purchase))
}

// 构造完整采购来源映射，供单次组合根装配共享。
// 可供采购入口与财务来源复用的共享静态 Mapping。
// 固定采购模型不满足接入合同时返回错误。
pub(super) fn purchase_mapping() -> Result<Arc<dyn AccessMapping>> {
    PurchaseHistoryMapping::shared()
}

fn selection_mappings(customer: Arc<dyn AccessMapping>, mappings: &mut MappingBindings) -> Result<()> {
    let booklet = [selection::BOOKLET_LIST, selection::ADMIN_IMAGE, selection::BOOKLET_COPY_LINK];
    let proposal =
        [selection::PROPOSAL_LIST, selection::PROPOSAL_DETAIL, internal::SALES_SELECTION_ORDER_BASIS];
    for (resource, permissions) in
        [("sales_selection_booklet", booklet.as_slice()), ("sales_selection_proposal", proposal.as_slice())]
    {
        let mapping: Arc<dyn AccessMapping> =
            Arc::new(SelectionMapping::with_customer(resource, customer.clone())?);
        mappings.extend(permissions.iter().cloned().map(|permission| (permission, mapping.clone())));
    }
    Ok(())
}

fn inventory_mappings(warehouse: Arc<dyn AccessMapping>, mappings: &mut MappingBindings) -> Result<()> {
    for (action, permissions) in [
        (InventoryAction::BalanceList, vec![inventory::STOCK_BALANCE_LIST, inventory::STOCK_BALANCE_DETAIL]),
        (InventoryAction::MovementList, vec![inventory::STOCK_MOVEMENT_LIST]),
        (InventoryAction::ReservationList, vec![inventory::STOCK_RESERVATION_LIST]),
        (
            InventoryAction::AdjustmentList,
            vec![inventory::STOCK_ADJUSTMENT_LIST, inventory::STOCK_ADJUSTMENT_DETAIL],
        ),
    ] {
        let mapping = InventoryMapping::shared(action, warehouse.clone())?;
        mappings.extend(permissions.into_iter().map(|permission| (permission, mapping.clone())));
    }
    Ok(())
}

struct PurchaseHistoryMapping {
    inner: PurchaseMapping,
    model: QueryModel,
}
impl PurchaseHistoryMapping {
    fn shared() -> Result<Arc<dyn AccessMapping>> {
        let inner = PurchaseMapping::new("purchase_order")?;
        let mut model = inner.model().clone();
        model.facts.extend([FactKind::HistoryParticipants, FactKind::Collaborators]);
        Ok(Arc::new(Self { inner, model }))
    }
}
impl AccessMapping for PurchaseHistoryMapping {
    fn model(&self) -> &QueryModel {
        &self.model
    }
    fn key(&self, context: &AccessContext, prepared: Option<&MappingFacts>) -> Result<Bson> {
        self.inner.key(context, prepared)
    }
    fn records(&self, context: &AccessContext, prepared: Option<&MappingFacts>) -> Result<QueryFragment> {
        self.inner.records(context, prepared)
    }
    fn predicate(
        &self,
        fact: &AccessFact,
        context: &AccessContext,
        prepared: Option<&MappingFacts>,
    ) -> Result<QueryFragment> {
        match fact {
            AccessFact::HistoryParticipant => Ok(QueryFragment::Exists {
                collection: Database::DOCUMENT_PARTICIPANTS.into(),
                bindings: doc! { "document": "$id" },
                pipeline: vec![doc! { "$match": {
                    "participant_user_id": context.actor.id(), "deleted_at": 0_i64,
                    "$expr": { "$eq": ["$document_id", "$$document"] }
                }}],
            }),
            AccessFact::Collaborating => Ok(QueryFragment::Match(doc! { "$expr": false })),
            _ => self.inner.predicate(fact, context, prepared),
        }
    }
    fn relation(
        &self,
        relation: &RelationKey,
        context: &AccessContext,
        prepared: Option<&MappingFacts>,
    ) -> Result<RelationMapping> {
        self.inner.relation(relation, context, prepared)
    }
}

struct PurchaseObjects {
    db: Database,
}
#[async_trait]
impl AccessFactsPort for PurchaseObjects {
    async fn load(
        &self,
        context: &AccessContext,
        ids: &ResourceTargets,
        executor: &mut dyn Executor,
    ) -> Result<Vec<ObjectFacts>> {
        let mut rows = PurchaseFacts::new(self.db.clone()).load(context, ids, executor).await?;
        if !context.allows_history()? {
            return Ok(rows);
        }
        let targets = ids.ids().iter().cloned().collect::<Vec<_>>();
        let participants = self
            .db
            .document_participants()
            .document_ids_in(context.actor.id(), &targets, executor)
            .await?
            .into_iter()
            .collect::<BTreeSet<_>>();
        for row in &mut rows {
            participants_for(row, context.actor.id(), &participants);
        }
        Ok(rows)
    }
}

fn participants_for(object: &mut ObjectFacts, actor: &str, participants: &BTreeSet<String>) {
    object.collaborators = FactValue::Known(BTreeSet::new());
    object.history_participants = FactValue::Known(if participants.contains(&object.id) {
        [actor.into()].into()
    } else {
        BTreeSet::new()
    });
}

struct InventoryObjects {
    inner: InventoryFacts,
    warehouses: Arc<dyn AccessFactsPort>,
}

#[async_trait]
impl AccessFactsPort for InventoryObjects {
    async fn load(
        &self,
        context: &AccessContext,
        ids: &ResourceTargets,
        executor: &mut dyn Executor,
    ) -> Result<Vec<ObjectFacts>> {
        let mut rows = self.inner.load(context, ids, executor).await?;
        for row in &mut rows {
            let FactValue::Known(Some(warehouse_id)) = &row.warehouse_id else { continue };
            let targets = ResourceTargets::new(&[warehouse_id])?;
            let source_context =
                AccessContext { resource: "warehouse".into(), model: None, ..context.clone() };
            let objects = self.warehouses.load(&source_context, &targets, executor).await?;
            row.relations.insert(
                RelationKey::parse("warehouse")?,
                RelatedObjects { expected_ids: [warehouse_id.clone()].into(), objects },
            );
        }
        Ok(rows)
    }
}

#[cfg(test)]
mod tests {
    use application_core::AuditActor;
    use erp_core::AccountKind;
    use erp_core::common::time::Instant;

    use super::*;

    fn context() -> AccessContext {
        AccessContext {
            actor: AuditActor::new("buyer".into(), "buyer".into(), AccountKind::Admin),
            permission: purchase_order::LIST,
            resource: "purchase_order".into(),
            model: None,
            as_of: Instant::from_unix_secs(1),
        }
    }

    #[test]
    fn purchase_history_is_static_and_correlates_the_actual_document_and_actor() {
        let mapping = PurchaseHistoryMapping::shared().unwrap();
        assert!(mapping.preparation().is_none());
        let QueryFragment::Exists { collection, bindings, pipeline } =
            mapping.predicate(&AccessFact::HistoryParticipant, &context(), None).unwrap()
        else {
            panic!("采购历史参与须为实际文档关联")
        };
        assert_eq!(collection, Database::DOCUMENT_PARTICIPANTS);
        assert_eq!(bindings, doc! { "document": "$id" });
        assert_eq!(
            pipeline[0].get_document("$match").unwrap().get_str("participant_user_id").unwrap(),
            "buyer"
        );
    }

    #[test]
    fn participant_facts_do_not_turn_other_documents_or_empty_relations_into_history() {
        let mut object = ObjectFacts { id: "purchase".into(), ..Default::default() };
        participants_for(&mut object, "buyer", &["other".into()].into());
        assert_eq!(object.history_participants, FactValue::Known(BTreeSet::new()));
        participants_for(&mut object, "buyer", &["purchase".into()].into());
        assert_eq!(object.history_participants, FactValue::Known(["buyer".into()].into()));
    }
}
