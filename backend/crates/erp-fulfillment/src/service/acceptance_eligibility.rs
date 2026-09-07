//! 履约事实按稳定销售行组织及数量资格派生；销售数据仅通过最小事实输入。
use crate::entity::facts::{AcceptanceSalesLineFact, AcceptanceSalesQuantityFact};
use crate::entity::fulfillment::{
    AcceptanceFactEligibility, AcceptanceFulfillmentAllocation, AcceptanceLineEligibility, DeliveryLine,
    ElectronicDelivery, ServiceFulfillment,
};
use crate::Result;
use erp_core::money::Quantity;
use std::{collections::HashMap, str::FromStr};
/// 验收行资格计算使用的本域履约事实与最小销售行数量快照。
pub struct EligibilitySources<'a> {
    /// 当前销售版本公共行，保留输入顺序。
    pub revision_lines: &'a [AcceptanceSalesLineFact],
    /// 当前版本行的履约数量。
    pub goods_service_lines: &'a [AcceptanceSalesQuantityFact],
    /// 已筛选为有效的发货行。
    pub delivery_lines: &'a [DeliveryLine],
    /// 已确认的电子交付。
    pub electronic: &'a [ElectronicDelivery],
    /// 已筛选为可验收的服务履约。
    pub service: &'a [ServiceFulfillment],
    /// 发货验收分配。
    pub delivery_allocations: &'a [AcceptanceFulfillmentAllocation],
    /// 电子交付验收分配。
    pub electronic_allocations: &'a [AcceptanceFulfillmentAllocation],
    /// 服务履约验收分配。
    pub service_allocations: &'a [AcceptanceFulfillmentAllocation],
}
/// 构建销售行验收资格投影（按销售稳定明细组织三类履约事实）。
///
/// # 用途
/// 按销售稳定明细汇总可验收事实；数量规则全部由领域投影 VO
/// （`AcceptanceFactEligibility`/`AcceptanceLineEligibility`）执行，本函数只做
/// 事实与分配的按行组织。
///
/// # 参数
/// * `sources` - 版本行、履约集合与分配
///
/// # 返回
/// 返回按销售稳定明细组织的行级资格投影（保持版本行顺序；同一稳定明细出现
/// 多行时后行覆盖应履约数量，与历史分组语义一致）。
///
/// # 错误
/// 既有净验收超过成功履约数量，或数量汇总溢出/超出统一精度时返回错误
/// （禁止静默回退为零）。
///
/// # 关键业务约束
/// 事实/分配入参由数据模型 §6.7 固定为三类来源，字段不可压缩。
pub fn build_line_eligibilities(sources: &EligibilitySources<'_>) -> Result<Vec<AcceptanceLineEligibility>> {
    let mut facts_by_line: HashMap<String, Vec<AcceptanceFactEligibility>> = HashMap::new();
    for revision_line in sources.revision_lines {
        facts_by_line.insert(revision_line.sales_order_line_id.to_string(), Vec::new());
    }
    for line in sources.delivery_lines {
        if let Some(facts) = facts_by_line.get_mut(&line.sales_order_line_id.to_string()) {
            facts.push(AcceptanceFactEligibility::from_fact(
                &line.base.id,
                line.quantity,
                sources.delivery_allocations,
            )?);
        }
    }
    for record in sources.electronic {
        if let Some(facts) = facts_by_line.get_mut(&record.sales_order_line_id.to_string()) {
            facts.push(AcceptanceFactEligibility::from_fact(
                &record.base.id,
                record.quantity,
                sources.electronic_allocations,
            )?);
        }
    }
    for record in sources.service {
        if let Some(facts) = facts_by_line.get_mut(&record.sales_order_line_id.to_string()) {
            facts.push(AcceptanceFactEligibility::from_fact(
                &record.base.id,
                record.quantity,
                sources.service_allocations,
            )?);
        }
    }
    let mut line_index: HashMap<String, usize> = HashMap::new();
    let mut line_inputs: Vec<(String, Quantity, Vec<AcceptanceFactEligibility>)> = Vec::new();
    for revision_line in sources.revision_lines {
        let key = revision_line.sales_order_line_id.to_string();
        let goods = sources
            .goods_service_lines
            .iter()
            .find(|goods| goods.revision_line_id.to_string() == revision_line.id);
        let required_quantity = goods
            .map(|goods| goods.quantity)
            .unwrap_or_else(|| Quantity::from_str("0").unwrap());
        if let Some(&index) = line_index.get(&key) {
            line_inputs[index].1 = required_quantity;
        } else {
            line_index.insert(key, line_inputs.len());
            line_inputs.push((
                revision_line.sales_order_line_id.to_string(),
                required_quantity,
                Vec::new(),
            ));
        }
    }
    for (key, facts) in facts_by_line {
        if let Some(&index) = line_index.get(&key) {
            line_inputs[index].2 = facts;
        }
    }
    let mut lines = Vec::with_capacity(line_inputs.len());
    for (sales_order_line_id, required_quantity, facts) in line_inputs {
        lines.push(AcceptanceLineEligibility::from_facts(
            sales_order_line_id,
            required_quantity,
            facts,
        )?);
    }
    Ok(lines)
}

#[cfg(test)]
mod tests {
    use super::{build_line_eligibilities, EligibilitySources};
    use crate::entity::facts::{AcceptanceSalesLineFact, AcceptanceSalesQuantityFact};
    use crate::entity::fulfillment::AcceptanceProgress;
    use erp_core::{
        ids::{SalesOrderLineId, SalesOrderRevisionLineId},
        money::Quantity,
    };
    use std::str::FromStr;

    fn sources<'a>(
        lines: &'a [AcceptanceSalesLineFact],
        quantities: &'a [AcceptanceSalesQuantityFact],
    ) -> EligibilitySources<'a> {
        EligibilitySources {
            revision_lines: lines,
            goods_service_lines: quantities,
            delivery_lines: &[],
            electronic: &[],
            service: &[],
            delivery_allocations: &[],
            electronic_allocations: &[],
            service_allocations: &[],
        }
    }
    /// 重复稳定行后版本覆盖数量，领域顺序仍按首次出现；无数量版本保持零。
    #[test]
    fn duplicate_stable_line_replaces_quantity_without_reordering_first_occurrence() {
        let lines = vec![
            AcceptanceSalesLineFact {
                id: "r2".into(),
                sales_order_line_id: SalesOrderLineId::new("s2"),
            },
            AcceptanceSalesLineFact {
                id: "r1".into(),
                sales_order_line_id: SalesOrderLineId::new("s1"),
            },
            AcceptanceSalesLineFact {
                id: "r2-later".into(),
                sales_order_line_id: SalesOrderLineId::new("s2"),
            },
            AcceptanceSalesLineFact {
                id: "missing".into(),
                sales_order_line_id: SalesOrderLineId::new("s3"),
            },
        ];
        let quantities = [("r2", "2"), ("r1", "3"), ("r2-later", "7")]
            .into_iter()
            .map(|(id, amount)| AcceptanceSalesQuantityFact {
                revision_line_id: SalesOrderRevisionLineId::new(id),
                quantity: Quantity::from_str(amount).unwrap(),
            })
            .collect::<Vec<_>>();
        let result = build_line_eligibilities(&sources(&lines, &quantities)).unwrap();
        assert_eq!(
            result
                .iter()
                .map(|line| line.sales_order_line_id.as_str())
                .collect::<Vec<_>>(),
            vec!["s2", "s1", "s3"]
        );
        assert_eq!(
            result
                .iter()
                .map(|line| line.required_quantity)
                .collect::<Vec<_>>(),
            vec![
                Quantity::from_str("7").unwrap(),
                Quantity::from_str("3").unwrap(),
                Quantity::from_str("0").unwrap()
            ]
        );
    }
    /// 无销售行事实必须维持 None，让生产完成步骤跳过销售资金刷新。
    #[test]
    fn absent_sales_lines_have_no_acceptance_projection() {
        let lines = build_line_eligibilities(&sources(&[], &[])).unwrap();
        assert!(lines.is_empty());
        assert!(AcceptanceProgress::derive(&lines).is_none());
    }
}
