//! 库存调整与供应商结算审批任务的事项简报装载。

use std::collections::{HashMap, HashSet};

use database::{CatalogExt, Executor, InventoryExt, SupplierSettlementExt, WarehouseExt};
use entities::ids::SupplierSettlementItemId;
use entities::inventory::{StockAdjustment, StockAdjustmentLine};
use entities::supplier_settlement::{
    SupplierSettlementDifference, SupplierSettlementDifferenceEvidence, SupplierSettlementItem,
    SupplierSettlementSourceEvidence, SupplierSettlementStatement,
};

use super::brief::{
    format_instant_datetime, format_quantity, join_list_summary, non_empty, push_section, BriefLine,
    ObjectBriefSource, BRIEF_LINE_LIMIT,
};
use super::presentation::format_yuan;
use super::{object_ids, ObjectFact, ObjectFactMap, ObjectKind, WorkItemService};
use crate::errors::Result;

#[derive(Default)]
struct SettlementBriefContext {
    supplier_names: HashMap<String, String>,
    items_by_statement: HashMap<String, Vec<SupplierSettlementItem>>,
    differences_by_item: HashMap<String, Vec<SupplierSettlementDifference>>,
    evidence_by_difference: HashMap<String, Vec<SupplierSettlementDifferenceEvidence>>,
    source_evidence_by_hash: HashMap<String, SupplierSettlementSourceEvidence>,
}

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
        let warehouse_labels = self
            .stock_adjustment_warehouse_labels(&adjustments, executor)
            .await?;
        let lines_by_adjustment = self.stock_adjustment_brief_lines(&ids, executor).await?;
        for adjustment in adjustments {
            let warehouse = warehouse_labels
                .get(&adjustment.warehouse_id.to_string())
                .cloned();
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
            push_section(&mut sections, "仓库", warehouse.as_deref(), false);
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
                    warehouse,
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
        let statements = self
            .db
            .supplier_settlement_statements()
            .list_work_item_brief_entities_by_ids(&ids, executor)
            .await?;
        let context = self
            .supplier_settlement_brief_context(&statements, executor)
            .await?;
        for statement in statements {
            let period = format!("{} 至 {}", statement.period_start, statement.period_end);
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
            let source_evidence = context
                .source_evidence_by_hash
                .get(&statement.source_snapshot_hash);
            let all_lines = settlement_brief_lines(items, &differences, &context.evidence_by_difference);
            let more_count = all_lines.len().saturating_sub(BRIEF_LINE_LIMIT) as u32;
            let mut visible_lines = all_lines;
            visible_lines.truncate(BRIEF_LINE_LIMIT);
            let mut fact = ObjectFact::new(
                statement.base.id.clone(),
                format!("供应商结算单 {}", statement.statement_no),
                statement.prepared_by.clone(),
            );
            fact.counterparty_label = supplier.clone();
            fact.impact_summary = Some(settlement_review_instruction(differences.len(), pending_count));
            let mut sections = Vec::new();
            push_section(&mut sections, "供应商", supplier.as_deref(), false);
            push_section(&mut sections, "结算期间", Some(period.as_str()), false);
            push_section(&mut sections, "结算状态", Some(statement.status.label()), false);
            let external_bill = external_bill_label(&statement);
            push_section(&mut sections, "供应商账单", external_bill.as_deref(), false);
            let source_as_of = format_instant_datetime(statement.source_as_of);
            push_section(&mut sections, "来源事实水位", Some(&source_as_of), false);
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
            let difference_summary = settlement_difference_summary(differences.len(), pending_count);
            push_section(&mut sections, "差异处理", Some(&difference_summary), false);
            if let Some(source) = source_evidence {
                push_section(
                    &mut sections,
                    "账单证据",
                    non_empty(&source.external_bill_evidence_reference_id).as_deref(),
                    false,
                );
                let reference_count = source
                    .lines
                    .iter()
                    .flat_map(|line| line.evidence_reference_ids.iter())
                    .collect::<HashSet<_>>()
                    .len();
                let evidence_summary = format!("{} 行 · {} 项引用", source.lines.len(), reference_count);
                push_section(&mut sections, "逐行来源证据", Some(&evidence_summary), false);
            }
            let supplement_count = differences
                .iter()
                .flat_map(|difference| {
                    context
                        .evidence_by_difference
                        .get(&difference.base.id)
                        .into_iter()
                        .flatten()
                })
                .map(|evidence| evidence.evidence_reference_ids.len())
                .sum::<usize>();
            if supplement_count > 0 {
                let supplement_summary = format!("{supplement_count} 项正式引用");
                push_section(&mut sections, "差异补证", Some(&supplement_summary), false);
            }
            let review_instruction = settlement_review_instruction(differences.len(), pending_count);
            push_section(&mut sections, "复核条件", Some(&review_instruction), false);
            fact.brief_source = Some(ObjectBriefSource {
                customer: supplier.clone(),
                amount_label: Some(format_yuan(&statement.erp_amount)),
                extra_sections: sections,
                list_summary: join_list_summary([
                    supplier,
                    Some(period),
                    Some(format_yuan(&statement.erp_amount)),
                    (!statement.difference_amount.to_decimal().is_zero())
                        .then(|| format!("差异 {}", format_yuan(&statement.difference_amount))),
                    (pending_count > 0).then(|| format!("{pending_count} 项待处理")),
                ]),
                lines: visible_lines,
                more_count,
                submitter_name: non_empty(&statement.prepared_by),
            });
            facts.insert((ObjectKind::SupplierSettlement, statement.base.id.clone()), fact);
        }
        Ok(())
    }

    /// 批量读取供应商结算的明细、差异、补证和冻结来源证据。
    ///
    /// # 参数
    /// * `statements` - 本批已授权供应商结算单
    /// * `executor` - 数据访问执行器
    ///
    /// # 返回
    /// 返回按结算单和差异分组的工作台简报上下文。
    ///
    /// # 错误
    /// 任一仓储批量查询失败时返回错误。
    async fn supplier_settlement_brief_context(
        &self,
        statements: &[SupplierSettlementStatement],
        executor: &mut dyn Executor,
    ) -> Result<SettlementBriefContext> {
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
        let difference_ids = differences
            .iter()
            .map(|difference| difference.base.id.clone())
            .collect::<Vec<_>>();
        let difference_evidence = self
            .db
            .supplier_settlement_difference_evidence()
            .find_by_difference_ids(&difference_ids, executor)
            .await?;
        let source_hashes = statements
            .iter()
            .map(|statement| statement.source_snapshot_hash.clone())
            .collect::<Vec<_>>();
        let source_evidence = self
            .db
            .supplier_settlement_source_evidence()
            .list_by_source_hashes(&source_hashes, executor)
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
        Ok(SettlementBriefContext {
            supplier_names,
            items_by_statement: group_settlement_items(items),
            differences_by_item: group_settlement_differences(differences),
            evidence_by_difference: group_settlement_evidence(difference_evidence),
            source_evidence_by_hash: source_evidence
                .into_iter()
                .map(|evidence| (evidence.source_hash.clone(), evidence))
                .collect(),
        })
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
        let lines = self
            .db
            .stock_adjustment_lines()
            .list_work_item_brief_lines_by_adjustments(adjustment_ids, executor)
            .await?;
        let sku_labels = self.stock_adjustment_sku_labels(&lines, executor).await?;
        let mut grouped: HashMap<String, Vec<BriefLine>> = HashMap::new();
        for line in lines {
            let sku_label = sku_labels.get(&line.sku_id.to_string()).map(String::as_str);
            grouped
                .entry(line.stock_adjustment_id.to_string())
                .or_default()
                .push(stock_brief_line(&line, sku_label));
        }
        Ok(grouped)
    }

    /// 批量读取库存调整涉及的仓库名称与业务代码。
    ///
    /// # 参数
    /// * `adjustments` - 本批库存调整单
    /// * `executor` - 数据访问执行器
    ///
    /// # 返回
    /// 返回仓库 ID 到可读仓库标签；修订缺失时仅保留仓库业务代码。
    ///
    /// # 错误
    /// 仓储查询失败时返回错误。
    async fn stock_adjustment_warehouse_labels(
        &self,
        adjustments: &[StockAdjustment],
        executor: &mut dyn Executor,
    ) -> Result<HashMap<String, String>> {
        let warehouse_ids = adjustments
            .iter()
            .map(|adjustment| adjustment.warehouse_id.to_string())
            .collect::<Vec<_>>();
        let warehouses = self
            .db
            .warehouses()
            .list_work_item_brief_entities_by_ids(&warehouse_ids, executor)
            .await?;
        let revision_ids = warehouses
            .iter()
            .filter_map(|warehouse| warehouse.stable.current_revision_id.clone())
            .collect::<Vec<_>>();
        let revision_names = self
            .db
            .warehouse_revisions()
            .list_work_item_brief_entities_by_ids(&revision_ids, executor)
            .await?
            .into_iter()
            .map(|revision| (revision.base.id, revision.name))
            .collect::<HashMap<_, _>>();
        Ok(warehouses
            .into_iter()
            .map(|warehouse| {
                let name = warehouse
                    .stable
                    .current_revision_id
                    .as_ref()
                    .and_then(|id| revision_names.get(id));
                let label = match name
                    .map(String::as_str)
                    .map(str::trim)
                    .filter(|value| !value.is_empty())
                {
                    Some(name) => format!("{name}（{}）", warehouse.warehouse_code),
                    None => warehouse.warehouse_code,
                };
                (warehouse.base.id, label)
            })
            .collect())
    }

    /// 批量读取库存调整涉及的 SKU 名称、规格和业务编号。
    ///
    /// # 参数
    /// * `lines` - 本批库存调整明细
    /// * `executor` - 数据访问执行器
    ///
    /// # 返回
    /// 返回 SKU ID 到可读标签；修订缺失时仅保留 SKU 业务编号。
    ///
    /// # 错误
    /// 仓储查询失败时返回错误。
    async fn stock_adjustment_sku_labels(
        &self,
        lines: &[StockAdjustmentLine],
        executor: &mut dyn Executor,
    ) -> Result<HashMap<String, String>> {
        let sku_ids = lines
            .iter()
            .map(|line| line.sku_id.to_string())
            .collect::<Vec<_>>();
        let skus = self
            .db
            .skus()
            .list_work_item_brief_entities_by_ids(&sku_ids, executor)
            .await?;
        let revision_ids = skus
            .iter()
            .filter_map(|sku| sku.stable.current_revision_id.clone())
            .collect::<Vec<_>>();
        let revisions = self
            .db
            .sku_revisions()
            .list_work_item_brief_entities_by_ids(&revision_ids, executor)
            .await?
            .into_iter()
            .map(|revision| (revision.base.id.clone(), revision))
            .collect::<HashMap<_, _>>();
        Ok(skus
            .into_iter()
            .map(|sku| {
                let revision = sku
                    .stable
                    .current_revision_id
                    .as_ref()
                    .and_then(|id| revisions.get(id));
                (
                    sku.base.id,
                    sku_brief_title(
                        &sku.sku_no,
                        revision.map(|value| value.name.as_str()),
                        revision.and_then(|value| value.specification.as_deref()),
                    ),
                )
            })
            .collect())
    }
}

/// 按结算单分组冻结明细。
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

/// 按结算明细分组正式差异。
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

/// 按差异分组不可变补证。
fn group_settlement_evidence(
    evidence: Vec<SupplierSettlementDifferenceEvidence>,
) -> HashMap<String, Vec<SupplierSettlementDifferenceEvidence>> {
    let mut grouped: HashMap<String, Vec<SupplierSettlementDifferenceEvidence>> = HashMap::new();
    for item in evidence {
        grouped
            .entry(item.difference_id.to_string())
            .or_default()
            .push(item);
    }
    grouped
}

/// 返回供应商账单号和冻结版本。
fn external_bill_label(statement: &SupplierSettlementStatement) -> Option<String> {
    let number = statement.external_bill_no.as_deref()?.trim();
    if number.is_empty() {
        return None;
    }
    let version = statement
        .external_bill_version
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty());
    Some(match version {
        Some(version) => format!("{number} · 版本 {version}"),
        None => number.to_string(),
    })
}

/// 返回结算差异总量和待处理量。
fn settlement_difference_summary(total: usize, pending: usize) -> String {
    if total == 0 {
        "双方金额一致，无差异项".to_string()
    } else if pending == 0 {
        format!("{total} 项差异均已有正式结论")
    } else {
        format!("{total} 项差异 · {pending} 项待处理")
    }
}

/// 返回供应商结算复核的服务端判断条件。
fn settlement_review_instruction(total: usize, pending: usize) -> String {
    if pending > 0 {
        format!("仍有 {pending} 项差异未形成正式结论，不得确认结算")
    } else if total > 0 {
        "全部差异已有正式结论；复核来源证据后方可确认结算".to_string()
    } else {
        "双方金额一致；复核冻结来源证据后方可确认结算".to_string()
    }
}

/// 把差异或无差异明细转成工作台可判断的行级简报。
fn settlement_brief_lines(
    items: &[SupplierSettlementItem],
    differences: &[&SupplierSettlementDifference],
    evidence_by_difference: &HashMap<String, Vec<SupplierSettlementDifferenceEvidence>>,
) -> Vec<BriefLine> {
    if !differences.is_empty() {
        return differences
            .iter()
            .enumerate()
            .map(|(index, difference)| {
                let evidence_count = evidence_by_difference
                    .get(&difference.base.id)
                    .map(|values| {
                        values
                            .iter()
                            .map(|evidence| evidence.evidence_reference_ids.len())
                            .sum::<usize>()
                    })
                    .unwrap_or_default();
                let evidence_label = if difference.is_pending() {
                    if evidence_count > 0 {
                        format!("已有 {evidence_count} 项补证，待形成结论")
                    } else {
                        "缺少正式结论和补证".to_string()
                    }
                } else if evidence_count > 0 {
                    format!("正式结论已形成 · {evidence_count} 项补证")
                } else {
                    "正式结论已形成".to_string()
                };
                BriefLine {
                    title: format!("差异 {} · {}", index + 1, difference.difference_type.label()),
                    quantity: Some(format!(
                        "{} · {}",
                        format_yuan(&difference.difference_amount),
                        difference.status.label()
                    )),
                    due_label: Some(evidence_label),
                }
            })
            .collect();
    }
    items
        .iter()
        .enumerate()
        .map(|(index, item)| BriefLine {
            title: format!("结算明细 {}", index + 1),
            quantity: Some(format!(
                "{} · ERP {} · 供应商 {}",
                format_quantity(&item.quantity, None),
                format_yuan(&item.erp_calculated_amount),
                format_yuan(&item.supplier_billed_amount),
            )),
            due_label: Some(format!(
                "订单 {} + 运费 {} + 服务费 {} − 退款 {}",
                format_yuan(&item.order_amount),
                format_yuan(&item.freight_amount),
                format_yuan(&item.service_fee_amount),
                format_yuan(&item.refund_amount),
            )),
        })
        .collect()
}

/// 把库存调整明细转成简报行。
///
/// # 参数
/// * `line` - 调整明细
/// * `sku_label` - SKU 名称、规格和业务编号
///
/// # 返回
/// 返回 SKU、方向和数量；SKU 资料缺失时使用明确占位，不回退内部 ID。
///
/// # 错误
/// 无。
fn stock_brief_line(line: &StockAdjustmentLine, sku_label: Option<&str>) -> BriefLine {
    BriefLine {
        title: format!(
            "{} · {}",
            sku_label.unwrap_or("SKU 名称待补全"),
            line.direction.label()
        ),
        quantity: Some(format_quantity(&line.quantity, None)),
        due_label: None,
    }
}

/// 组装库存简报中的 SKU 可读标题。
///
/// # 参数
/// * `sku_no` - SKU 业务编号
/// * `name` - 当前修订名称
/// * `specification` - 当前修订规格
///
/// # 返回
/// 返回“名称 规格 · 编号”；名称缺失时仅返回业务编号。
///
/// # 错误
/// 无。
fn sku_brief_title(sku_no: &str, name: Option<&str>, specification: Option<&str>) -> String {
    let sku_no = sku_no.trim();
    let name = name.map(str::trim).filter(|value| !value.is_empty());
    let specification = specification
        .map(str::trim)
        .filter(|value| !value.is_empty() && value.chars().count() <= 16);
    let description = match (name, specification) {
        (Some(name), Some(specification)) => format!("{name} {specification}"),
        (Some(name), None) => name.to_string(),
        _ => return sku_no.to_string(),
    };
    if sku_no.is_empty() {
        description
    } else {
        format!("{description} · {sku_no}")
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
        assert_eq!(
            sku_brief_title("SKU-1", Some("福利卡"), Some("100 元")),
            "福利卡 100 元 · SKU-1"
        );
    }

    #[test]
    fn settlement_instruction_fails_closed_while_differences_are_pending() {
        assert_eq!(settlement_difference_summary(3, 1), "3 项差异 · 1 项待处理");
        assert_eq!(
            settlement_review_instruction(3, 1),
            "仍有 1 项差异未形成正式结论，不得确认结算"
        );
        assert_eq!(
            settlement_review_instruction(3, 0),
            "全部差异已有正式结论；复核来源证据后方可确认结算"
        );
    }
}
