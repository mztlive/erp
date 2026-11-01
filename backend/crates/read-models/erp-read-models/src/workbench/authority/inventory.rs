//! 库存调整与供应商结算的对象事实。

use std::collections::{HashMap, HashSet};

use erp_core::ids::SupplierSettlementItemId;
use erp_supply::entity::supplier_settlement::{SupplierSettlementDifference, SupplierSettlementItem};
use persistence_core::Executor;

use super::{ObjectFact, ObjectFactMap, ObjectKind, object_ids};
use crate::errors::Result;

struct SettlementFactContext {
    supplier_names: HashMap<String, String>,
    items_by_statement: HashMap<String, Vec<SupplierSettlementItem>>,
    differences_by_item: HashMap<String, Vec<SupplierSettlementDifference>>,
}

impl super::WorkItemFactsReader {
    /// 装载库存调整单的身份与影响。
    ///
    /// # 参数
    /// * `keys` - 本批对象键；没有库存调整键时不读取。
    /// * `facts` - 输出事实表。
    /// * `executor` - 数据访问执行器。
    ///
    /// # 返回
    /// 成功时写入读到的调整单。
    ///
    /// # 错误
    /// 仓储读取失败时返回错误。
    pub(in crate::workbench) async fn load_stock_adjustment_facts(
        &self,
        keys: &HashSet<(ObjectKind, String)>,
        facts: &mut ObjectFactMap,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        let ids = object_ids(keys, ObjectKind::StockAdjustment);
        if ids.is_empty() {
            return Ok(());
        }
        let adjustments = self.read_stock_adjustments(&ids, executor).await?;
        for adjustment in adjustments {
            let fact = stock_adjustment_fact(&adjustment);
            facts.insert((ObjectKind::StockAdjustment, adjustment.base.id.clone()), fact);
        }
        Ok(())
    }

    /// 装载供应商结算单的身份、往来方与影响。
    ///
    /// # 参数
    /// * `keys` - 本批对象键；没有结算单键时不读取。
    /// * `facts` - 输出事实表。
    /// * `executor` - 数据访问执行器。
    ///
    /// # 返回
    /// 成功时写入结算单，往来方取供应商显示名，影响按差异总数与未决数生成。
    ///
    /// # 错误
    /// 结算单、明细、差异或供应商名称读取失败时返回错误。
    pub(in crate::workbench) async fn load_supplier_settlement_facts(
        &self,
        keys: &HashSet<(ObjectKind, String)>,
        facts: &mut ObjectFactMap,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        let ids = object_ids(keys, ObjectKind::SupplierSettlement);
        if ids.is_empty() {
            return Ok(());
        }
        let statements = self.read_settlement_statements(&ids, executor).await?;
        let context = self.supplier_settlement_fact_context(&statements, executor).await?;
        for statement in statements {
            let supplier = context.supplier_names.get(&statement.supplier_id.to_string()).cloned();
            let items =
                context.items_by_statement.get(&statement.base.id).map(Vec::as_slice).unwrap_or_default();
            let differences = items
                .iter()
                .flat_map(|item| context.differences_by_item.get(&item.base.id).into_iter().flatten())
                .collect::<Vec<_>>();
            let pending_count = differences.iter().filter(|difference| difference.is_pending()).count();
            let fact = settlement_fact(&statement, supplier, differences.len(), pending_count);
            facts.insert((ObjectKind::SupplierSettlement, statement.base.id.clone()), fact);
        }
        Ok(())
    }

    async fn supplier_settlement_fact_context(
        &self,
        statements: &[erp_supply::entity::supplier_settlement::SupplierSettlementStatement],
        executor: &mut dyn Executor,
    ) -> Result<SettlementFactContext> {
        let statement_ids = statements.iter().map(|statement| statement.base.id.clone()).collect::<Vec<_>>();
        let items = self.read_settlement_items(&statement_ids, executor).await?;
        let item_ids =
            items.iter().map(|item| SupplierSettlementItemId::new(item.base.id.clone())).collect::<Vec<_>>();
        let differences = self.read_settlement_differences(&item_ids, executor).await?;
        let supplier_names = self
            .supplier_display_names(
                &statements.iter().map(|statement| statement.supplier_id.to_string()).collect::<Vec<_>>(),
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

/// 按结算单分组冻结明细。
///
/// # 参数
/// * `items` - 已读结算明细。
///
/// # 返回
/// 返回结算单 ID 到其明细的映射；不排序。
///
/// # 错误
/// 不返回错误。
pub(in crate::workbench) fn group_settlement_items(
    items: Vec<SupplierSettlementItem>,
) -> HashMap<String, Vec<SupplierSettlementItem>> {
    let mut grouped: HashMap<String, Vec<SupplierSettlementItem>> = HashMap::new();
    for item in items {
        grouped.entry(item.statement_id.to_string()).or_default().push(item);
    }
    grouped
}

/// 按结算明细分组正式差异。
///
/// # 参数
/// * `differences` - 已读结算差异。
///
/// # 返回
/// 返回结算明细 ID 到其差异的映射；不排序。
///
/// # 错误
/// 不返回错误。
pub(in crate::workbench) fn group_settlement_differences(
    differences: Vec<SupplierSettlementDifference>,
) -> HashMap<String, Vec<SupplierSettlementDifference>> {
    let mut grouped: HashMap<String, Vec<SupplierSettlementDifference>> = HashMap::new();
    for difference in differences {
        grouped.entry(difference.statement_item_id.to_string()).or_default().push(difference);
    }
    grouped
}

/// 返回供应商结算复核的服务端判断条件。
///
/// # 参数
/// * `total` - 差异总数。
/// * `pending` - 尚未形成正式结论的差异数。
///
/// # 返回
/// `pending` 大于 0 时说明不得确认；否则在有差异或金额一致两种文案中选择。
///
/// # 错误
/// 不返回错误。
pub(in crate::workbench) fn settlement_review_instruction(total: usize, pending: usize) -> String {
    if pending > 0 {
        format!("仍有 {pending} 项差异未形成正式结论，不得确认结算")
    } else if total > 0 {
        "全部差异已有正式结论；复核来源证据后方可确认结算".to_string()
    } else {
        "双方金额一致；复核冻结来源证据后方可确认结算".to_string()
    }
}

/// 从已读取的 stock_adjustment 来源构造唯一权威字段。
///
/// # 参数
/// * `adjustment` - 库存调整单。
///
/// # 返回
/// 返回调整单号标题与编制人，影响为不审批则不能入账。
///
/// # 错误
/// 不返回错误。
pub(in crate::workbench) fn stock_adjustment_fact(adjustment: &erp_inventory::StockAdjustment) -> ObjectFact {
    let mut fact = ObjectFact::new(
        adjustment.base.id.clone(),
        format!("库存调整单 {}", adjustment.adjustment_no),
        adjustment.prepared_by.clone(),
    );
    fact.impact_summary = Some("不审批则库存调整不能入账".to_string());
    fact
}

/// 从已读取的 settlement 来源构造唯一权威字段。
///
/// # 参数
/// * `statement` - 供应商结算单。
/// * `supplier` - 供应商显示名；缺失时往来方为空。
/// * `difference_count` - 差异总数。
/// * `pending_count` - 未决差异数。
///
/// # 返回
/// 返回结算单号标题与编制人，影响由 `settlement_review_instruction` 生成。
///
/// # 错误
/// 不返回错误。
pub(in crate::workbench) fn settlement_fact(
    statement: &erp_supply::entity::supplier_settlement::SupplierSettlementStatement,
    supplier: Option<String>,
    difference_count: usize,
    pending_count: usize,
) -> ObjectFact {
    let mut fact = ObjectFact::new(
        statement.base.id.clone(),
        format!("供应商结算单 {}", statement.statement_no),
        statement.prepared_by.clone(),
    );
    fact.counterparty_label = supplier;
    fact.impact_summary = Some(settlement_review_instruction(difference_count, pending_count));
    fact
}
