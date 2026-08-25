//! 库存调整与供应商结算审批任务的事项简报装载。

use std::collections::{HashMap, HashSet};

use database::{Executor, InventoryExt, SupplierSettlementExt};
use entities::inventory::StockAdjustmentLine;

use super::brief::{
    format_quantity, join_list_summary, non_empty, push_section, BriefLine, ObjectBriefSource,
    BRIEF_LINE_LIMIT,
};
use super::presentation::format_yuan;
use super::{object_ids, ObjectFact, ObjectFactMap, ObjectKind, WorkItemService};
use crate::errors::Result;

impl WorkItemService {
    /// 库存调整审批任务的对象事实。
    ///
    /// # 参数
    /// * `keys` - 本批任务引用的对象键
    /// * `facts` - 输出的对象事实表
    /// * `executor` - 数据访问执行器
    ///
    /// # 返回
    /// 成功时写入调整原因、说明和明细方向数量。
    ///
    /// # 错误
    /// 仓储查询失败时返回错误。
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
            .list_work_item_brief_entities_by_ids(&ids, executor)
            .await?;
        if adjustments.is_empty() {
            return Ok(());
        }
        let lines_by_adjustment = self.stock_adjustment_brief_lines(&ids, executor).await?;
        for adjustment in adjustments {
            let lines = lines_by_adjustment
                .get(&adjustment.base.id)
                .cloned()
                .unwrap_or_default();
            let more_count = lines.len().saturating_sub(BRIEF_LINE_LIMIT) as u32;
            let mut visible = lines;
            visible.truncate(BRIEF_LINE_LIMIT);
            let mut fact = ObjectFact::new(
                adjustment.base.id.clone(),
                format!("库存调整单 {}", adjustment.adjustment_no),
                adjustment.prepared_by.clone(),
            );
            fact.impact_summary = Some("不审批则库存调整不能入账".to_string());
            let mut sections = Vec::new();
            push_section(
                &mut sections,
                "调整原因",
                Some(adjustment.reason_type.label()),
                false,
            );
            push_section(&mut sections, "说明", adjustment.note.as_deref(), false);
            if !visible.is_empty() {
                push_section(
                    &mut sections,
                    "明细",
                    Some(format!("{} 行", visible.len() + more_count as usize)).as_deref(),
                    false,
                );
            }
            fact.brief_source = Some(ObjectBriefSource {
                customer: None,
                amount_label: None,
                extra_sections: sections,
                list_summary: join_list_summary([
                    Some(adjustment.reason_type.label().to_string()),
                    adjustment.note.clone().and_then(|text| non_empty(&text)),
                    visible.first().map(|line| line.title.clone()),
                ]),
                lines: visible,
                more_count,
                submitter_name: non_empty(&adjustment.prepared_by),
            });
            facts.insert((ObjectKind::StockAdjustment, adjustment.base.id.clone()), fact);
        }
        Ok(())
    }

    /// 供应商结算复核任务的对象事实。
    ///
    /// # 参数
    /// * `keys` - 本批任务引用的对象键
    /// * `facts` - 输出的对象事实表
    /// * `executor` - 数据访问执行器
    ///
    /// # 返回
    /// 成功时写入结算期间和双方金额。
    ///
    /// # 错误
    /// 仓储查询失败时返回错误。
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
        for statement in self
            .db
            .supplier_settlement_statements()
            .list_work_item_brief_entities_by_ids(&ids, executor)
            .await?
        {
            let period = format!("{} 至 {}", statement.period_start, statement.period_end);
            let mut fact = ObjectFact::new(
                statement.base.id.clone(),
                format!("供应商结算单 {}", statement.statement_no),
                statement.prepared_by.clone(),
            );
            fact.impact_summary = Some("不复核则供应商结算不能确认".to_string());
            let mut sections = Vec::new();
            push_section(&mut sections, "结算期间", Some(period.as_str()), false);
            push_section(
                &mut sections,
                "ERP 金额",
                Some(format_yuan(&statement.erp_amount)).as_deref(),
                true,
            );
            push_section(
                &mut sections,
                "供应商金额",
                Some(format_yuan(&statement.supplier_amount)).as_deref(),
                true,
            );
            if !statement.difference_amount.to_decimal().is_zero() {
                push_section(
                    &mut sections,
                    "差异",
                    Some(format_yuan(&statement.difference_amount)).as_deref(),
                    true,
                );
            }
            fact.brief_source = Some(ObjectBriefSource {
                customer: None,
                amount_label: Some(format_yuan(&statement.erp_amount)),
                extra_sections: sections,
                list_summary: join_list_summary([
                    Some(period),
                    Some(format_yuan(&statement.erp_amount)),
                    (!statement.difference_amount.to_decimal().is_zero())
                        .then(|| format!("差异 {}", format_yuan(&statement.difference_amount))),
                ]),
                lines: Vec::new(),
                more_count: 0,
                submitter_name: non_empty(&statement.prepared_by),
            });
            facts.insert((ObjectKind::SupplierSettlement, statement.base.id.clone()), fact);
        }
        Ok(())
    }

    /// 读取本批库存调整明细并转成按调整单分组的简报行。
    ///
    /// # 参数
    /// * `adjustment_ids` - 调整单 ID
    /// * `executor` - 数据访问执行器
    ///
    /// # 返回
    /// 返回调整单 ID 到简报行。
    ///
    /// # 错误
    /// 仓储查询失败时返回错误。
    async fn stock_adjustment_brief_lines(
        &self,
        adjustment_ids: &[String],
        executor: &mut dyn Executor,
    ) -> Result<HashMap<String, Vec<BriefLine>>> {
        if adjustment_ids.is_empty() {
            return Ok(HashMap::new());
        }
        let mut grouped: HashMap<String, Vec<BriefLine>> = HashMap::new();
        for line in self
            .db
            .stock_adjustment_lines()
            .list_work_item_brief_lines_by_adjustments(adjustment_ids, executor)
            .await?
        {
            grouped
                .entry(line.stock_adjustment_id.to_string())
                .or_default()
                .push(stock_brief_line(&line));
        }
        Ok(grouped)
    }
}

/// 把库存调整明细转成简报行。
///
/// # 参数
/// * `line` - 调整明细
///
/// # 返回
/// 返回方向和数量；品名缺失时用方向作标题。
///
/// # 错误
/// 无。
fn stock_brief_line(line: &StockAdjustmentLine) -> BriefLine {
    BriefLine {
        title: line.direction.label().to_string(),
        quantity: Some(format_quantity(&line.quantity, None)),
        due_label: None,
    }
}

#[cfg(test)]
mod tests {
    use entities::inventory::MovementDirection;
    use entities::money::Quantity;

    use super::*;

    fn qty(value: &str) -> Quantity {
        value.parse().expect("测试数量必须合法")
    }

    #[test]
    fn stock_line_shows_direction_and_quantity() {
        assert_eq!(MovementDirection::Increase.label(), "增加");
        assert_eq!(format_quantity(&qty("3"), None), "×3");
    }
}
