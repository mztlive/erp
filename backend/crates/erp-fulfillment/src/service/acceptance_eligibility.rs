//! 履约事实按稳定销售行组织及数量资格派生；销售数据仅通过最小事实输入。
use std::collections::HashMap;
use std::str::FromStr;

use erp_core::money::Quantity;

use crate::Result;
use crate::entity::facts::{AcceptanceSalesLineFact, AcceptanceSalesQuantityFact};
use crate::entity::fulfillment::acceptance_eligibility::AcceptanceAllocationIndex;
use crate::entity::fulfillment::{
    AcceptanceFactEligibility, AcceptanceFulfillmentAllocation, AcceptanceLineEligibility, DeliveryLine,
    ElectronicDelivery, ServiceFulfillment,
};
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
    let mut facts_by_line = group_facts_by_line(sources)?;
    let line_inputs = group_revision_quantities(sources);
    let mut lines = Vec::with_capacity(line_inputs.len());
    for (sales_order_line_id, required_quantity, _) in line_inputs {
        let facts = facts_by_line.remove(&sales_order_line_id).unwrap_or_default();
        lines.push(AcceptanceLineEligibility::from_facts(sales_order_line_id, required_quantity, facts)?);
    }
    Ok(lines)
}

/// 按销售稳定明细组织三类履约事实（发货/电子/服务）。
///
/// # 参数
/// * `sources` - 版本行、履约集合与分配
///
/// # 返回
/// 返回按销售稳定明细索引的事实集合。
///
/// # 错误
/// 分配汇总溢出/超出统一精度时返回错误（禁止静默回退为零）。
fn group_facts_by_line(
    sources: &EligibilitySources<'_>,
) -> Result<HashMap<String, Vec<AcceptanceFactEligibility>>> {
    let delivery_allocations = AcceptanceAllocationIndex::new(sources.delivery_allocations);
    let electronic_allocations = AcceptanceAllocationIndex::new(sources.electronic_allocations);
    let service_allocations = AcceptanceAllocationIndex::new(sources.service_allocations);
    let mut facts_by_line: HashMap<String, Vec<AcceptanceFactEligibility>> = HashMap::new();
    for revision_line in sources.revision_lines {
        facts_by_line.insert(revision_line.sales_order_line_id.to_string(), Vec::new());
    }
    for line in sources.delivery_lines {
        if let Some(facts) = facts_by_line.get_mut(&line.sales_order_line_id.to_string()) {
            facts.push(delivery_allocations.fact(&line.base.id, line.quantity)?);
        }
    }
    for record in sources.electronic {
        // 无效电子交付事实已被领域判定为不可验收：跳过该事实并保留其他有效事实。
        if record.acceptance_quantity(&record.sales_order_line_id).is_err() {
            continue;
        }
        if let Some(facts) = facts_by_line.get_mut(&record.sales_order_line_id.to_string()) {
            facts.push(electronic_allocations.fact(&record.base.id, record.quantity)?);
        }
    }
    for record in sources.service {
        if let Some(facts) = facts_by_line.get_mut(&record.sales_order_line_id.to_string()) {
            facts.push(service_allocations.fact(&record.base.id, record.quantity)?);
        }
    }
    Ok(facts_by_line)
}

/// 按版本行顺序组装稳定明细的应履约数量（同一稳定明细后行覆盖数量）。
///
/// 预建版本行主键索引，消除内层线性查找。
///
/// # 参数
/// * `sources` - 版本行与数量快照
///
/// # 返回
/// 返回按首次出现顺序的（稳定明细、应履约数量、空事实）三元组。
///
/// # Panics
/// `Quantity::from_str("0")` 对字面量 `0` 必然合法；解析失败时 panic，避免把非法零数量传入资格计算。
fn group_revision_quantities(
    sources: &EligibilitySources<'_>,
) -> Vec<(String, Quantity, Vec<AcceptanceFactEligibility>)> {
    let quantity_by_revision: HashMap<&str, Quantity> = sources
        .goods_service_lines
        .iter()
        .map(|goods| (goods.revision_line_id.as_ref(), goods.quantity))
        .collect();
    let zero = Quantity::from_str("0").expect("字面量 0 必然合法");
    let mut line_index: HashMap<&str, usize> = HashMap::new();
    let mut line_inputs: Vec<(String, Quantity, Vec<AcceptanceFactEligibility>)> = Vec::new();
    for revision_line in sources.revision_lines {
        let key = revision_line.sales_order_line_id.as_ref();
        let required_quantity = quantity_by_revision.get(revision_line.id.as_str()).copied().unwrap_or(zero);
        if let Some(&index) = line_index.get(key) {
            line_inputs[index].1 = required_quantity;
        } else {
            line_index.insert(key, line_inputs.len());
            line_inputs.push((key.to_string(), required_quantity, Vec::new()));
        }
    }
    line_inputs
}

#[cfg(test)]
mod tests {
    use std::str::FromStr;
    use std::time::Instant as Stopwatch;

    use erp_core::Error as CoreError;
    use erp_core::ids::{
        AcceptanceFulfillmentAllocationId, CustomerAcceptanceLineId, DeliveryId, DeliveryLineId,
        ElectronicDeliveryId, SalesOrderLineId, SalesOrderRevisionLineId, ServiceFulfillmentId,
        StockReservationId,
    };
    use erp_core::money::Quantity;

    use super::{EligibilitySources, build_line_eligibilities};
    use crate::Error;
    use crate::entity::facts::{AcceptanceSalesLineFact, AcceptanceSalesQuantityFact};
    use crate::entity::fulfillment::electronic_delivery::tests::data as electronic_data;
    use crate::entity::fulfillment::service_fulfillment::tests::data as service_data;
    use crate::entity::fulfillment::{
        AcceptanceFulfillmentAllocation, AcceptanceFulfillmentAllocationData, AcceptanceProgress,
        AllocationAction, DeliveryLine, DeliveryLineData, DeliveryType, ElectronicDelivery,
        ElectronicDeliveryState, FulfillmentFactType, ServiceFulfillment,
    };

    /// 构造只有销售数量快照的资格输入。
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
            AcceptanceSalesLineFact { id: "r2".into(), sales_order_line_id: SalesOrderLineId::new("s2") },
            AcceptanceSalesLineFact { id: "r1".into(), sales_order_line_id: SalesOrderLineId::new("s1") },
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
            result.iter().map(|line| line.sales_order_line_id.as_str()).collect::<Vec<_>>(),
            vec!["s2", "s1", "s3"]
        );
        assert_eq!(
            result.iter().map(|line| line.required_quantity).collect::<Vec<_>>(),
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

    /// 构造数量为二的已筛选仓发明细。
    fn delivery(id: &str, sales_line: &str) -> DeliveryLine {
        DeliveryLine::new(
            DeliveryLineId::new(id),
            DeliveryLineData {
                delivery_id: DeliveryId::new("delivery-1"),
                line_no: 1,
                sales_order_line_id: SalesOrderLineId::new(sales_line),
                quantity: Quantity::from_str("2").unwrap(),
                stock_reservation_id: Some(StockReservationId::new("reservation-1")),
                purchase_line_sales_allocation_id: None,
            },
            DeliveryType::WarehouseShip,
        )
        .unwrap()
    }

    /// 构造关联指定发货事实的正向验收分配。
    fn allocation(id: &str, quantity: &str) -> AcceptanceFulfillmentAllocation {
        AcceptanceFulfillmentAllocation::new(
            AcceptanceFulfillmentAllocationId::new(format!("allocation-{id}")),
            AcceptanceFulfillmentAllocationData {
                customer_acceptance_line_id: CustomerAcceptanceLineId::new("acceptance-1"),
                fulfillment_fact_type: FulfillmentFactType::Delivery,
                fulfillment_line_id: id.into(),
                allocation_action: AllocationAction::Apply,
                allocated_quantity: Quantity::from_str(quantity).unwrap(),
                reverses_allocation_id: None,
            },
        )
        .unwrap()
    }

    /// 大批资格计算保持销售行顺序，并逐事实计算净验收与剩余数量。
    #[test]
    fn large_eligibility_projection_sample() {
        let lines = (0..1000)
            .map(|index| AcceptanceSalesLineFact {
                id: format!("revision-{index}"),
                sales_order_line_id: SalesOrderLineId::new(format!("sales-{index}")),
            })
            .collect::<Vec<_>>();
        let deliveries = (0..1000)
            .map(|index| delivery(&format!("delivery-{index}"), &format!("sales-{index}")))
            .collect::<Vec<_>>();
        let allocations =
            (0..1000).map(|index| allocation(&format!("delivery-{index}"), "1")).collect::<Vec<_>>();
        let mut input = sources(&lines, &[]);
        input.delivery_lines = &deliveries;
        input.delivery_allocations = &allocations;
        let one = Quantity::from_str("1").unwrap();
        let started = Stopwatch::now();
        for _ in 0..10 {
            let result = build_line_eligibilities(&input).unwrap();
            assert_eq!(result.first().unwrap().sales_order_line_id, "sales-0");
            assert_eq!(result.last().unwrap().sales_order_line_id, "sales-999");
            assert!(
                result
                    .iter()
                    .all(|line| line.net_accepted_quantity == one && line.remaining_eligible_quantity == one)
            );
        }
        eprintln!("fulfillment_1000_line_projections_10_runs={:?}", started.elapsed());
    }

    /// 未消费事实的错误分配不提前报错；重复事实与原输入顺序均保留。
    #[test]
    fn grouping_only_validates_consumed_facts_and_preserves_duplicates() {
        let lines =
            [AcceptanceSalesLineFact { id: "r1".into(), sales_order_line_id: SalesOrderLineId::new("s1") }];
        let deliveries = [
            delivery("dl-2", "s1"),
            delivery("dl-1", "s1"),
            delivery("dl-2", "s1"),
            delivery("ignored", "other-sales-line"),
        ];
        let allocations = [
            allocation("dl-1", "1"),
            allocation("ignored", "3"),
            allocation("dl-2", "1"),
            allocation("no-fact", "3"),
        ];
        let mut input = sources(&lines, &[]);
        input.delivery_lines = &deliveries;
        input.delivery_allocations = &allocations;
        let result = build_line_eligibilities(&input).unwrap();
        assert_eq!(
            result[0].facts.iter().map(|fact| fact.fulfillment_line_id.as_str()).collect::<Vec<_>>(),
            ["dl-2", "dl-1", "dl-2"]
        );
        assert_eq!(result[0].net_accepted_quantity, Quantity::from_str("3").unwrap());
        assert_eq!(result[0].remaining_eligible_quantity, Quantity::from_str("3").unwrap());
    }

    /// 多条事实出错时按原事实处理顺序报告，索引不提前汇总其他组。
    #[test]
    fn grouping_preserves_first_fact_error() {
        let lines =
            [AcceptanceSalesLineFact { id: "r1".into(), sales_order_line_id: SalesOrderLineId::new("s1") }];
        let mut reversed = allocation("negative", "1");
        reversed.allocation_action = AllocationAction::Reverse;
        reversed.reverses_allocation_id = Some(AcceptanceFulfillmentAllocationId::new("original"));
        let allocations = [allocation("excess", "3"), reversed];
        for (deliveries, message) in [
            ([delivery("negative", "s1"), delivery("excess", "s1")], "履约事实的净验收数量不得为负"),
            (
                [delivery("excess", "s1"), delivery("negative", "s1")],
                "履约事实的净验收数量超过其净成功履约数量",
            ),
        ] {
            let mut input = sources(&lines, &[]);
            input.delivery_lines = &deliveries;
            input.delivery_allocations = &allocations;
            assert_eq!(
                build_line_eligibilities(&input).unwrap_err().to_string(),
                Error::Logic(CoreError::from(message)).to_string()
            );
        }
    }

    /// 三类事实同 ID 仍独立消费各自分配；无效电子交付不触发其分配错误。
    #[test]
    fn grouping_keeps_fact_types_separate_and_skips_invalid_electronic_records() {
        let lines = [AcceptanceSalesLineFact {
            id: "r1".into(),
            sales_order_line_id: SalesOrderLineId::new("so-line-1"),
        }];
        let deliveries = [delivery("shared-id", "so-line-1")];
        let mut confirmed =
            ElectronicDelivery::new(ElectronicDeliveryId::new("shared-id"), electronic_data()).unwrap();
        confirmed.status = ElectronicDeliveryState::Confirmed;
        let electronic = [
            confirmed,
            ElectronicDelivery::new(ElectronicDeliveryId::new("invalid"), electronic_data()).unwrap(),
        ];
        let service =
            [ServiceFulfillment::new(ServiceFulfillmentId::new("shared-id"), service_data()).unwrap()];
        let delivery_allocations = [allocation("shared-id", "0.5")];
        let mut electronic_allocation = allocation("shared-id", "1");
        electronic_allocation.fulfillment_fact_type = FulfillmentFactType::ElectronicDelivery;
        let electronic_allocations = [electronic_allocation, allocation("invalid", "3")];
        let mut service_allocation = allocation("shared-id", "0.25");
        service_allocation.fulfillment_fact_type = FulfillmentFactType::ServiceFulfillment;
        let service_allocations = [service_allocation];
        let mut input = sources(&lines, &[]);
        input.delivery_lines = &deliveries;
        input.electronic = &electronic;
        input.service = &service;
        input.delivery_allocations = &delivery_allocations;
        input.electronic_allocations = &electronic_allocations;
        input.service_allocations = &service_allocations;
        let result = build_line_eligibilities(&input).unwrap();
        assert_eq!(result[0].facts.len(), 3);
        assert_eq!(
            result[0].facts.iter().map(|fact| fact.net_accepted_quantity.to_string()).collect::<Vec<_>>(),
            ["0.5", "1", "0.25"]
        );
        assert_eq!(
            result[0].facts.iter().map(|fact| fact.eligible_quantity.to_string()).collect::<Vec<_>>(),
            ["1.5", "1", "0.75"]
        );
    }
}
