//! 为已授权工作项批量解析往来方名称与稳定 ID，供队列与统计搜索使用。

use std::collections::{HashMap, HashSet};

use async_trait::async_trait;
use erp_workflow::ports::{
    ObjectKind, OrderTaskSource, WorkItemCounterpartyFact, WorkItemCounterpartyLookup,
    WorkItemCounterpartyPort,
};
use persistence_core::Executor;

use super::super::WorkbenchReadService;
use super::super::facts::{WorkbenchObjectFact, WorkbenchObjectFactMap, object_policy};

#[async_trait]
impl WorkItemCounterpartyPort for WorkbenchReadService {
    async fn load_counterparties(
        &self,
        items: &[WorkItemCounterpartyLookup],
        executor: &mut dyn Executor,
    ) -> erp_workflow::Result<HashMap<String, WorkItemCounterpartyFact>> {
        if items.is_empty() {
            return Ok(HashMap::new());
        }
        let keys = collect_lookup_keys(items);
        if keys.is_empty() {
            return Ok(HashMap::new());
        }

        let mut facts = self
            .load_object_facts(&keys, executor)
            .await
            .map_err(|error| erp_workflow::Error::Internal(error.to_string()))?;

        let parent_keys = missing_order_parent_keys(&facts);
        if !parent_keys.is_empty() {
            let parents = self
                .load_object_facts(&parent_keys, executor)
                .await
                .map_err(|error| erp_workflow::Error::Internal(error.to_string()))?;
            facts.extend(parents);
        }

        Ok(items
            .iter()
            .filter_map(|item| {
                let fact = resolve_counterparty_fact(item, &facts);
                (fact.counterparty_label.is_some() || !fact.counterparty_ids.is_empty())
                    .then(|| (item.work_item_id.clone(), fact))
            })
            .collect())
    }
}

fn collect_lookup_keys(items: &[WorkItemCounterpartyLookup]) -> HashSet<(ObjectKind, String)> {
    let mut keys = HashSet::new();
    for item in items {
        if let Some(policy) = object_policy(item.work_item_type, &item.business_object_type) {
            keys.insert((policy.object_kind, item.business_object_id.clone()));
        }
        for key in linked_order_keys(item) {
            keys.insert(key);
        }
    }
    keys
}

fn linked_order_keys(item: &WorkItemCounterpartyLookup) -> Vec<(ObjectKind, String)> {
    item.links
        .iter()
        .filter_map(|link| match link.object_type() {
            "sales_order" => Some((ObjectKind::SalesOrder, link.object_id().to_owned())),
            "purchase_order" => Some((ObjectKind::PurchaseOrder, link.object_id().to_owned())),
            _ => None,
        })
        .collect()
}

fn missing_order_parent_keys(facts: &WorkbenchObjectFactMap) -> HashSet<(ObjectKind, String)> {
    facts
        .values()
        .filter_map(|fact| match fact.authority.order_scope_source.as_ref()? {
            OrderTaskSource::Sales(id) => Some((ObjectKind::SalesOrder, id.clone())),
            OrderTaskSource::Purchase(id) => Some((ObjectKind::PurchaseOrder, id.clone())),
        })
        .filter(|key| !facts.contains_key(key))
        .collect()
}

fn resolve_counterparty_fact(
    item: &WorkItemCounterpartyLookup,
    facts: &WorkbenchObjectFactMap,
) -> WorkItemCounterpartyFact {
    let primary = object_policy(item.work_item_type, &item.business_object_type)
        .and_then(|policy| facts.get(&(policy.object_kind, item.business_object_id.clone())));

    let mut label = primary.and_then(|fact| primary_label(fact, &item.subject_version));
    let mut ids = Vec::new();
    if let Some(fact) = primary {
        extend_unique(&mut ids, &fact.authority.counterparty_ids);
        if let Some(parent_key) = order_parent_key(fact)
            && let Some(parent) = facts.get(&parent_key)
        {
            if label.is_none() {
                label = primary_label(parent, &item.subject_version);
            }
            extend_unique(&mut ids, &parent.authority.counterparty_ids);
        }
    }

    for key in linked_order_keys(item) {
        if let Some(linked) = facts.get(&key) {
            if label.is_none() {
                label = primary_label(linked, &item.subject_version);
            }
            extend_unique(&mut ids, &linked.authority.counterparty_ids);
        }
    }

    WorkItemCounterpartyFact { counterparty_ids: ids, counterparty_label: label }
}

fn primary_label(fact: &WorkbenchObjectFact, subject_version: &str) -> Option<String> {
    fact.display
        .subject_briefs
        .get(subject_version)
        .and_then(|brief| brief.counterparty_label.as_deref())
        .or(fact.display.counterparty_label.as_deref())
        .or(fact.authority.counterparty_label.as_deref())
        .map(str::trim)
        .filter(|label| !label.is_empty())
        .map(str::to_owned)
}

fn order_parent_key(fact: &WorkbenchObjectFact) -> Option<(ObjectKind, String)> {
    match fact.authority.order_scope_source.as_ref()? {
        OrderTaskSource::Sales(id) => Some((ObjectKind::SalesOrder, id.clone())),
        OrderTaskSource::Purchase(id) => Some((ObjectKind::PurchaseOrder, id.clone())),
    }
}

fn extend_unique(target: &mut Vec<String>, source: &[String]) {
    for id in source {
        let trimmed = id.trim();
        if !trimmed.is_empty() && !target.iter().any(|existing| existing == trimmed) {
            target.push(trimmed.to_owned());
        }
    }
}

#[cfg(test)]
mod tests {
    use erp_workflow::entity::work_item::{WorkItemLink, WorkItemType};
    use erp_workflow::ports::ObjectFact;

    use super::*;
    use crate::workbench::WorkbenchSubjectDisplay;

    #[test]
    fn resolves_subject_counterparty_and_falls_back_to_parent_or_linked_order() {
        let mut sales = ObjectFact::new("so-1", "销售单 SO-1", "sales");
        sales.counterparty_label = Some("星辰科技".into());
        sales.push_counterparty_id("cust-1");
        sales.push_counterparty_id("party-1");

        let delivery = ObjectFact::new("so-1", "仓库发货 · 销售单 SO-1", "__system__")
            .with_order_source(OrderTaskSource::Sales("so-1".into()));

        let mut purchase = ObjectFact::new("po-1", "采购单 PO-1", "buyer");
        purchase.counterparty_label = Some("默认供应商".into());
        purchase.push_counterparty_id("sup-1");
        let mut purchase_fact = WorkbenchObjectFact::from_authority(purchase);
        purchase_fact.display.subject_briefs.insert(
            "2".into(),
            WorkbenchSubjectDisplay {
                counterparty_label: Some("冻结版本供应商".into()),
                ..Default::default()
            },
        );

        let facts = WorkbenchObjectFactMap::from([
            ((ObjectKind::SalesOrder, "so-1".into()), WorkbenchObjectFact::from_authority(sales)),
            ((ObjectKind::Delivery, "del-1".into()), WorkbenchObjectFact::from_authority(delivery)),
            ((ObjectKind::PurchaseOrder, "po-1".into()), purchase_fact),
        ]);

        let delivery_lookup = WorkItemCounterpartyLookup {
            work_item_id: "wi-del".into(),
            work_item_type: WorkItemType::FulfillmentOperation,
            business_object_type: "delivery".into(),
            business_object_id: "del-1".into(),
            subject_version: "1".into(),
            links: vec![WorkItemLink::new("sales_order", "so-1").unwrap()],
        };
        let resolved_delivery = resolve_counterparty_fact(&delivery_lookup, &facts);
        assert_eq!(resolved_delivery.counterparty_label.as_deref(), Some("星辰科技"));
        assert_eq!(resolved_delivery.counterparty_ids, vec!["cust-1", "party-1"]);

        let approval_lookup = WorkItemCounterpartyLookup {
            work_item_id: "wi-po".into(),
            work_item_type: WorkItemType::DocumentApproval,
            business_object_type: "purchase_order".into(),
            business_object_id: "po-1".into(),
            subject_version: "2".into(),
            links: Vec::new(),
        };
        let resolved_po = resolve_counterparty_fact(&approval_lookup, &facts);
        assert_eq!(resolved_po.counterparty_label.as_deref(), Some("冻结版本供应商"));
        assert_eq!(resolved_po.counterparty_ids, vec!["sup-1"]);
    }
}
