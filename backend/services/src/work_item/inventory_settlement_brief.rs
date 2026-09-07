//! Stock-adjustment and supplier-settlement object facts.

use std::collections::{HashMap, HashSet};

use erp_core::ids::SupplierSettlementItemId;
use erp_inventory::InventoryExt;
use erp_supply::entity::supplier_settlement::{SupplierSettlementDifference, SupplierSettlementItem};
use erp_supply::repository::SupplierSettlementExt;
use persistence_core::Executor;

use super::{object_ids, ObjectFact, ObjectFactMap, ObjectKind};
use crate::errors::Result;

struct SettlementFactContext {
    supplier_names: HashMap<String, String>,
    items_by_statement: HashMap<String, Vec<SupplierSettlementItem>>,
    differences_by_item: HashMap<String, Vec<SupplierSettlementDifference>>,
}

impl crate::work_item::ProcessObjectFacts {
    /// Load stock-adjustment identity and impact.
    pub(super) async fn load_stock_adjustment_facts(
        &self,
        keys: &HashSet<(ObjectKind, String)>,
        facts: &mut ObjectFactMap,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        let ids = object_ids(keys, ObjectKind::StockAdjustment);
        if ids.is_empty() {
            return Ok(());
        }
        let adjustments = self
            .db
            .stock_adjustments()
            .list_active_by_ids(&ids, executor)
            .await?;
        for adjustment in adjustments {
            let mut fact = ObjectFact::new(
                adjustment.base.id.clone(),
                format!("库存调整单 {}", adjustment.adjustment_no),
                adjustment.prepared_by.clone(),
            );
            fact.impact_summary = Some("不审批则库存调整不能入账".to_string());
            facts.insert((ObjectKind::StockAdjustment, adjustment.base.id.clone()), fact);
        }
        Ok(())
    }

    /// Load supplier-settlement identity, counterparty and impact.
    pub(super) async fn load_supplier_settlement_facts(
        &self,
        keys: &HashSet<(ObjectKind, String)>,
        facts: &mut ObjectFactMap,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        let ids = object_ids(keys, ObjectKind::SupplierSettlement);
        if ids.is_empty() {
            return Ok(());
        }
        let statements = self
            .db
            .supplier_settlement_statements()
            .list_active_by_ids(&ids, executor)
            .await?;
        let context = self
            .supplier_settlement_fact_context(&statements, executor)
            .await?;
        for statement in statements {
            let supplier = context
                .supplier_names
                .get(&statement.supplier_id.to_string())
                .cloned();
            let items = context
                .items_by_statement
                .get(&statement.base.id)
                .map(Vec::as_slice)
                .unwrap_or_default();
            let differences = items
                .iter()
                .flat_map(|item| {
                    context
                        .differences_by_item
                        .get(&item.base.id)
                        .into_iter()
                        .flatten()
                })
                .collect::<Vec<_>>();
            let pending_count = differences
                .iter()
                .filter(|difference| difference.is_pending())
                .count();
            let mut fact = ObjectFact::new(
                statement.base.id.clone(),
                format!("供应商结算单 {}", statement.statement_no),
                statement.prepared_by.clone(),
            );
            fact.counterparty_label = supplier;
            fact.impact_summary = Some(settlement_review_instruction(differences.len(), pending_count));
            facts.insert((ObjectKind::SupplierSettlement, statement.base.id.clone()), fact);
        }
        Ok(())
    }

    async fn supplier_settlement_fact_context(
        &self,
        statements: &[erp_supply::entity::supplier_settlement::SupplierSettlementStatement],
        executor: &mut dyn Executor,
    ) -> Result<SettlementFactContext> {
        let statement_ids = statements
            .iter()
            .map(|statement| statement.base.id.clone())
            .collect::<Vec<_>>();
        let items = self
            .db
            .supplier_settlement_items()
            .list_by_statement_ids(&statement_ids, executor)
            .await?;
        let item_ids = items
            .iter()
            .map(|item| SupplierSettlementItemId::new(item.base.id.clone()))
            .collect::<Vec<_>>();
        let differences = self
            .db
            .supplier_settlement_differences()
            .list_by_statement_item_ids(&item_ids, executor)
            .await?;
        let supplier_names = self
            .supplier_display_names(
                &statements
                    .iter()
                    .map(|statement| statement.supplier_id.to_string())
                    .collect::<Vec<_>>(),
                executor,
            )
            .await?;
        Ok(SettlementFactContext {
            supplier_names,
            items_by_statement: group_settlement_items(items),
            differences_by_item: group_settlement_differences(differences),
        })
    }
}

fn group_settlement_items(
    items: Vec<SupplierSettlementItem>,
) -> HashMap<String, Vec<SupplierSettlementItem>> {
    let mut grouped: HashMap<String, Vec<SupplierSettlementItem>> = HashMap::new();
    for item in items {
        grouped
            .entry(item.statement_id.to_string())
            .or_default()
            .push(item);
    }
    grouped
}

fn group_settlement_differences(
    differences: Vec<SupplierSettlementDifference>,
) -> HashMap<String, Vec<SupplierSettlementDifference>> {
    let mut grouped: HashMap<String, Vec<SupplierSettlementDifference>> = HashMap::new();
    for difference in differences {
        grouped
            .entry(difference.statement_item_id.to_string())
            .or_default()
            .push(difference);
    }
    grouped
}

fn settlement_review_instruction(total: usize, pending: usize) -> String {
    if pending > 0 {
        format!("仍有 {pending} 项差异未形成正式结论，不得确认结算")
    } else if total > 0 {
        "全部差异已有正式结论；复核来源证据后方可确认结算".to_string()
    } else {
        "双方金额一致；复核冻结来源证据后方可确认结算".to_string()
    }
}
