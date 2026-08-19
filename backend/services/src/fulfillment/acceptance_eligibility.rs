use std::collections::HashMap;
use std::str::FromStr;

use database::{FulfillmentExt, NoTransaction, SalesOrderExt};
use entities::fulfillment::{
    AcceptanceFulfillmentAllocation, AllocationAction, Delivery, DeliveryLine, DeliveryState,
    ElectronicDelivery, ElectronicDeliveryState, FulfillmentFactType, ServiceFulfillment,
    ServiceFulfillmentState,
};
use entities::ids::{DeliveryId, SalesOrderId, SalesOrderRevisionLineId};
use entities::money::Quantity;
use mongodb::bson::doc;

use crate::errors::{Error, Result};

use super::{
    AcceptanceEligibilityView, AcceptanceSalesLineGroupView, EligibleFulfillmentFactView, FulfillmentService,
};

impl FulfillmentService {
    /// 查询客户验收工作台（W06：销售行 + 可验收事实 + 验收历史）。
    ///
    /// 客户验收签署为 `NO_APPROVAL`：工作台只计算可验收事实，不得查询审批
    /// 定义、不得展示或创建审批任务。
    ///
    /// 可验收数量守恒：`eligible = 净成功履约数量 − 净已验收分配（APPLY −
    /// REVERSE）`，全部由服务端计算（§8.2 第 5 条）。
    ///
    /// # 参数
    /// * `sales_order_id` - 销售单
    ///
    /// # 返回
    /// 返回验收工作台视图。
    ///
    /// # 错误
    /// * `NotFound` - 销售单或其生效版本不存在
    /// * `RepositoryError` - 数据库查询失败
    pub async fn acceptance_eligibility(&self, sales_order_id: &str) -> Result<AcceptanceEligibilityView> {
        let so_id = SalesOrderId::new(sales_order_id.to_string());
        let so = self
            .db
            .sales_orders()
            .find_by_id(sales_order_id, &mut NoTransaction)
            .await?
            .ok_or_else(|| Error::NotFound("销售单不存在".to_string()))?;
        let revision_id = so
            .stable
            .current_revision_id
            .clone()
            .ok_or_else(|| Error::NotFound("销售单没有生效版本".to_string()))?;
        let revision = self
            .db
            .sales_order_revisions()
            .find_by_id(&revision_id, &mut NoTransaction)
            .await?
            .ok_or_else(|| Error::NotFound("销售生效版本不存在".to_string()))?;
        let revision_lines = self
            .db
            .sales_order_revision_lines()
            .list_lines_by_revision(&revision.base.id.clone().into(), &mut NoTransaction)
            .await?;
        let revision_line_ids: Vec<SalesOrderRevisionLineId> = revision_lines
            .iter()
            .map(|line| line.base.id.clone().into())
            .collect();
        let goods_service_lines = self
            .db
            .sales_order_goods_service_line_revisions()
            .list_by_revision_line_ids(&revision_line_ids, &mut NoTransaction)
            .await?;
        let deliveries = self
            .db
            .deliveries()
            .find_many(
                doc! {
                    "sales_order_id": so_id.to_string(),
                    "status": { "$in": vec![DeliveryState::Shipped.as_str(), DeliveryState::Signed.as_str()] },
                },
                &mut NoTransaction,
            )
            .await?;
        let delivery_ids: Vec<DeliveryId> = deliveries
            .iter()
            .map(|delivery| delivery.base.id.clone().into())
            .collect();
        let delivery_lines = self
            .db
            .fulfillment()
            .delivery_lines_by_delivery_ids(&delivery_ids, &mut NoTransaction)
            .await?;
        let electronic = self
            .db
            .electronic_deliveries()
            .find_many(
                doc! {
                    "sales_order_line_id": { "$in": so_line_ids(&revision_lines) },
                    "status": ElectronicDeliveryState::Confirmed.as_str(),
                },
                &mut NoTransaction,
            )
            .await?;
        let service = self
            .db
            .service_fulfillments()
            .find_many(
                doc! {
                    "sales_order_line_id": { "$in": so_line_ids(&revision_lines) },
                    "status": ServiceFulfillmentState::Confirmed.as_str(),
                },
                &mut NoTransaction,
            )
            .await?;
        let delivery_fact_ids: Vec<String> = delivery_lines.iter().map(|line| line.base.id.clone()).collect();
        let electronic_fact_ids: Vec<String> =
            electronic.iter().map(|record| record.base.id.clone()).collect();
        let service_fact_ids: Vec<String> = service.iter().map(|record| record.base.id.clone()).collect();
        let delivery_allocations = self
            .db
            .fulfillment()
            .allocations_by_fulfillment_fact(
                FulfillmentFactType::Delivery,
                &delivery_fact_ids,
                &mut NoTransaction,
            )
            .await?;
        let electronic_allocations = self
            .db
            .fulfillment()
            .allocations_by_fulfillment_fact(
                FulfillmentFactType::ElectronicDelivery,
                &electronic_fact_ids,
                &mut NoTransaction,
            )
            .await?;
        let service_allocations = self
            .db
            .fulfillment()
            .allocations_by_fulfillment_fact(
                FulfillmentFactType::ServiceFulfillment,
                &service_fact_ids,
                &mut NoTransaction,
            )
            .await?;
        let history = self
            .db
            .customer_acceptances()
            .find_many_sorted(
                doc! { "sales_order_id": so_id.to_string() },
                doc! { "accepted_at": -1 },
                &mut NoTransaction,
            )
            .await?;
        let groups = build_eligibility_groups(
            &revision_lines,
            &goods_service_lines,
            &deliveries,
            &delivery_lines,
            &electronic,
            &service,
            &delivery_allocations,
            &electronic_allocations,
            &service_allocations,
        );
        Ok(AcceptanceEligibilityView {
            sales_order_id: so_id.to_string(),
            sales_lines: groups,
            history: history.into_iter().map(Into::into).collect(),
        })
    }
}

/// 取当前生效版本行涉及的销售稳定明细 ID 集合。
///
/// # 参数
/// * `revision_lines` - 销售版本公共行
///
/// # 返回
/// 返回销售稳定明细 ID 字符串集合。
fn so_line_ids(revision_lines: &[entities::sales_order::SalesOrderRevisionLine]) -> Vec<String> {
    revision_lines
        .iter()
        .map(|line| line.sales_order_line_id.to_string())
        .collect()
}

/// 构建验收工作台分组（销售行 + 可验收事实 + 净数量守恒计算）。
///
/// # 参数
/// * `revision_lines` - 销售版本公共行
/// * `goods_service_lines` - 实物及服务行（数量/单位）
/// * `deliveries` - 有效发货单
/// * `delivery_lines` - 发货行
/// * `electronic` - 已确认电子交付
/// * `service` - 已确认服务履约
/// * `delivery_allocations` - 发货事实的验收分配
/// * `electronic_allocations` - 电子交付事实的验收分配
/// * `service_allocations` - 服务履约事实的验收分配
///
/// # 返回
/// 返回按销售稳定明细分组的工作台视图。
///
/// 事实/分配入参由数据模型 §6.7 固定为三类来源，字段不可压缩。
#[allow(clippy::too_many_arguments)]
fn build_eligibility_groups(
    revision_lines: &[entities::sales_order::SalesOrderRevisionLine],
    goods_service_lines: &[entities::sales_order::SalesOrderGoodsServiceLineRevision],
    deliveries: &[Delivery],
    delivery_lines: &[DeliveryLine],
    electronic: &[ElectronicDelivery],
    service: &[ServiceFulfillment],
    delivery_allocations: &[AcceptanceFulfillmentAllocation],
    electronic_allocations: &[AcceptanceFulfillmentAllocation],
    service_allocations: &[AcceptanceFulfillmentAllocation],
) -> Vec<AcceptanceSalesLineGroupView> {
    let mut groups: HashMap<String, AcceptanceSalesLineGroupView> = HashMap::new();
    for line in revision_lines {
        let goods = goods_service_lines
            .iter()
            .find(|goods| goods.revision_line_id.to_string() == line.base.id);
        groups.insert(
            line.sales_order_line_id.to_string(),
            AcceptanceSalesLineGroupView {
                sales_order_line_id: line.sales_order_line_id.to_string(),
                line_no: line.line_no,
                item_snapshot: line.item_name_snapshot.clone(),
                unit_code: goods.map(|goods| goods.base_unit_code.clone()),
                required_quantity: goods
                    .map(|goods| goods.quantity)
                    .unwrap_or_else(|| Quantity::from_str("0").unwrap()),
                net_accepted_quantity: Quantity::from_str("0").unwrap(),
                fulfillment_facts: Vec::new(),
            },
        );
    }
    let delivery_by_id: HashMap<String, &Delivery> = deliveries
        .iter()
        .map(|delivery| (delivery.base.id.clone(), delivery))
        .collect();
    for line in delivery_lines {
        let delivery = delivery_by_id.get(line.delivery_id.as_ref());
        let allocations = net_allocation_quantity(
            delivery_allocations,
            &line.base.id,
            Quantity::from_str("0").unwrap(),
        );
        let line_id = line.sales_order_line_id.to_string();
        let item_snapshot = group_item_snapshot(&groups, &line_id);
        let unit_code = group_unit_code(&groups, &line_id);
        push_fact(
            &mut groups,
            &line_id,
            EligibleFulfillmentFactView {
                fulfillment_line_id: line.base.id.clone(),
                fulfillment_fact_type: FulfillmentFactType::Delivery,
                fulfillment_no: delivery
                    .map(|delivery| delivery.delivery_no.clone())
                    .unwrap_or_default(),
                sales_order_line_id: line_id.clone(),
                line_no: line.line_no,
                item_snapshot,
                unit_code,
                occurred_at: delivery
                    .and_then(|delivery| delivery.shipped_at)
                    .map(|instant| instant.unix_secs())
                    .unwrap_or_default(),
                net_successful_quantity: line.quantity,
                net_accepted_allocated_quantity: allocations,
                eligible_quantity: Quantity::try_from(line.quantity.to_decimal() - allocations.to_decimal())
                    .unwrap_or_else(|_| Quantity::from_str("0").unwrap()),
                carrier: delivery.and_then(|delivery| delivery.carrier.clone()),
                tracking_no: delivery.and_then(|delivery| delivery.tracking_no.clone()),
            },
        );
    }
    for record in electronic {
        let allocations = net_allocation_quantity(
            electronic_allocations,
            &record.base.id,
            Quantity::from_str("0").unwrap(),
        );
        let line_id = record.sales_order_line_id.to_string();
        let line_no = group_line_no(&groups, &line_id);
        let item_snapshot = group_item_snapshot(&groups, &line_id);
        let unit_code = group_unit_code(&groups, &line_id);
        push_fact(
            &mut groups,
            &line_id,
            EligibleFulfillmentFactView {
                fulfillment_line_id: record.base.id.clone(),
                fulfillment_fact_type: FulfillmentFactType::ElectronicDelivery,
                fulfillment_no: record.fulfillment_no.clone(),
                sales_order_line_id: line_id.clone(),
                line_no,
                item_snapshot,
                unit_code,
                occurred_at: record.fact.occurred_at.unix_secs(),
                net_successful_quantity: record.quantity,
                net_accepted_allocated_quantity: allocations,
                eligible_quantity: Quantity::try_from(
                    record.quantity.to_decimal() - allocations.to_decimal(),
                )
                .unwrap_or_else(|_| Quantity::from_str("0").unwrap()),
                carrier: None,
                tracking_no: None,
            },
        );
    }
    for record in service {
        let allocations = net_allocation_quantity(
            service_allocations,
            &record.base.id,
            Quantity::from_str("0").unwrap(),
        );
        let line_id = record.sales_order_line_id.to_string();
        let line_no = group_line_no(&groups, &line_id);
        let item_snapshot = group_item_snapshot(&groups, &line_id);
        let unit_code = group_unit_code(&groups, &line_id);
        push_fact(
            &mut groups,
            &line_id,
            EligibleFulfillmentFactView {
                fulfillment_line_id: record.base.id.clone(),
                fulfillment_fact_type: FulfillmentFactType::ServiceFulfillment,
                fulfillment_no: record.fulfillment_no.clone(),
                sales_order_line_id: line_id.clone(),
                line_no,
                item_snapshot,
                unit_code,
                occurred_at: record.fact.occurred_at.unix_secs(),
                net_successful_quantity: record.quantity,
                net_accepted_allocated_quantity: allocations,
                eligible_quantity: Quantity::try_from(
                    record.quantity.to_decimal() - allocations.to_decimal(),
                )
                .unwrap_or_else(|_| Quantity::from_str("0").unwrap()),
                carrier: None,
                tracking_no: None,
            },
        );
    }
    let mut groups: Vec<AcceptanceSalesLineGroupView> = groups.into_values().collect();
    groups.sort_by_key(|group| group.line_no);
    groups
}

/// 计算履约事实的净验收分配数量（`APPLY − REVERSE`，正数方向）。
///
/// # 参数
/// * `allocations` - 该事实的全部验收分配
/// * `fulfillment_line_id` - 履约事实行主键
/// * `initial` - 初始值（零）
///
/// # 返回
/// 返回净验收分配数量。
fn net_allocation_quantity(
    allocations: &[AcceptanceFulfillmentAllocation],
    fulfillment_line_id: &str,
    initial: Quantity,
) -> Quantity {
    let mut net = initial;
    for allocation in allocations {
        if allocation.fulfillment_line_id != fulfillment_line_id {
            continue;
        }
        net = match allocation.allocation_action {
            AllocationAction::Apply => {
                Quantity::try_from(net.to_decimal() + allocation.allocated_quantity.to_decimal())
                    .unwrap_or_else(|_| Quantity::from_str("0").unwrap())
            }
            AllocationAction::Reverse => {
                Quantity::try_from(net.to_decimal() - allocation.allocated_quantity.to_decimal())
                    .unwrap_or_else(|_| Quantity::from_str("0").unwrap())
            }
        };
    }
    net
}

/// 把可验收事实并入对应销售行分组（按销售稳定明细）。
///
/// # 参数
/// * `groups` - 分组映射（就地修改）
/// * `sales_order_line_id` - 销售稳定明细
/// * `fact` - 可验收事实
fn push_fact(
    groups: &mut HashMap<String, AcceptanceSalesLineGroupView>,
    sales_order_line_id: &str,
    fact: EligibleFulfillmentFactView,
) {
    if let Some(group) = groups.get_mut(sales_order_line_id) {
        group.fulfillment_facts.push(fact);
    }
}

/// 取分组行号。
fn group_line_no(groups: &HashMap<String, AcceptanceSalesLineGroupView>, line_id: &str) -> u32 {
    groups.get(line_id).map(|group| group.line_no).unwrap_or_default()
}

/// 取分组品名快照。
fn group_item_snapshot(groups: &HashMap<String, AcceptanceSalesLineGroupView>, line_id: &str) -> String {
    groups
        .get(line_id)
        .map(|group| group.item_snapshot.clone())
        .unwrap_or_default()
}

/// 取分组单位快照。
fn group_unit_code(groups: &HashMap<String, AcceptanceSalesLineGroupView>, line_id: &str) -> Option<String> {
    groups.get(line_id).and_then(|group| group.unit_code.clone())
}

#[cfg(test)]
mod customer_acceptance_eligibility_no_approval_tests {
    /// 验收工作台不得查询定义、启动审批或创建任务。
    #[test]
    fn eligibility_does_not_start_approval_or_create_tasks() {
        let production = include_str!("acceptance_eligibility.rs")
            .split("#[cfg(test)]")
            .next()
            .expect("生产代码");
        assert!(production.contains("pub async fn acceptance_eligibility"));
        assert!(!production.contains("start_approval"));
        assert!(!production.contains("prepare_start"));
        assert!(!production.contains("WorkItem"));
        assert!(!production.contains("definition_id"));
        assert!(!production.contains("CustomerAcceptanceAdapter"));
        assert!(!production.contains("bind_published_definition_on_document_create"));
        assert!(!production.contains("load_published_graph"));
        let eligibility = production
            .split("pub async fn acceptance_eligibility")
            .nth(1)
            .and_then(|rest| rest.split("fn so_line_ids").next())
            .expect("acceptance_eligibility 生产片段");
        assert!(eligibility.contains("build_eligibility_groups"));
        assert!(!eligibility.contains("submit_"));
        assert!(!eligibility.contains("start_approval"));
    }
}
