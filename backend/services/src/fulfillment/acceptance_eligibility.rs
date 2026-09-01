use std::collections::HashMap;
use std::str::FromStr;

use database::{FulfillmentExt, NoTransaction, SalesOrderExt};
use entities::fulfillment::{
    AcceptanceFactEligibility, AcceptanceFulfillmentAllocation, AcceptanceLineEligibility, Delivery,
    DeliveryLine, ElectronicDelivery, FulfillmentFactType, ServiceFulfillment,
};
use entities::ids::{DeliveryId, SalesOrderId, SalesOrderLineId, SalesOrderRevisionLineId};
use entities::money::Quantity;

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
    #[tracing::instrument(
        name = "fulfillment.acceptance_eligibility",
        skip_all,
        fields(
            layer = "service",
            domain = "fulfillment",
            operation = "acceptance_eligibility"
        )
    )]
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
            .fulfillment()
            .list_acceptance_eligible_deliveries(&so_id, &mut NoTransaction)
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
        let sales_order_line_ids = so_line_ids(&revision_lines);
        let electronic = self
            .db
            .fulfillment()
            .list_confirmed_electronic_deliveries(&sales_order_line_ids, &mut NoTransaction)
            .await?;
        let service = self
            .db
            .fulfillment()
            .list_confirmed_service_fulfillments(&sales_order_line_ids, &mut NoTransaction)
            .await?
            .into_iter()
            .filter(ServiceFulfillment::is_acceptance_eligible)
            .collect::<Vec<_>>();
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
            .fulfillment()
            .list_customer_acceptance_history(&so_id, &mut NoTransaction)
            .await?;
        let sources = EligibilityGroupSources {
            revision_lines: &revision_lines,
            goods_service_lines: &goods_service_lines,
            deliveries: &deliveries,
            delivery_lines: &delivery_lines,
            electronic: &electronic,
            service: &service,
            delivery_allocations: &delivery_allocations,
            electronic_allocations: &electronic_allocations,
            service_allocations: &service_allocations,
        };
        let lines = build_line_eligibilities(&sources)?;
        let groups = build_eligibility_views(&sources, &lines);
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
/// 返回销售稳定明细 ID 集合。
pub(super) fn so_line_ids(
    revision_lines: &[entities::sales_order::SalesOrderRevisionLine],
) -> Vec<SalesOrderLineId> {
    revision_lines
        .iter()
        .map(|line| line.sales_order_line_id.clone())
        .collect()
}

/// 验收工作台分组的销售行、履约事实与分配集合。
///
/// # 用途
/// 将构建分组所需的版本行、履约集合与分配一次性传入。
///
/// # 参数
/// 无
///
/// # 返回
/// 无
///
/// # 错误
/// 无
///
/// # 关键业务约束
/// 事实/分配由数据模型 §6.7 固定为三类来源，字段不可压缩。
pub(super) struct EligibilityGroupSources<'a> {
    /// 销售版本公共行。
    pub(super) revision_lines: &'a [entities::sales_order::SalesOrderRevisionLine],
    /// 实物及服务行（数量/单位）。
    pub(super) goods_service_lines: &'a [entities::sales_order::SalesOrderGoodsServiceLineRevision],
    /// 有效发货单。
    pub(super) deliveries: &'a [Delivery],
    /// 发货行。
    pub(super) delivery_lines: &'a [DeliveryLine],
    /// 已确认电子交付。
    pub(super) electronic: &'a [ElectronicDelivery],
    /// 已确认服务履约。
    pub(super) service: &'a [ServiceFulfillment],
    /// 发货事实的验收分配。
    pub(super) delivery_allocations: &'a [AcceptanceFulfillmentAllocation],
    /// 电子交付事实的验收分配。
    pub(super) electronic_allocations: &'a [AcceptanceFulfillmentAllocation],
    /// 服务履约事实的验收分配。
    pub(super) service_allocations: &'a [AcceptanceFulfillmentAllocation],
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
pub(super) fn build_line_eligibilities(
    sources: &EligibilityGroupSources<'_>,
) -> Result<Vec<AcceptanceLineEligibility>> {
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
            .find(|goods| goods.revision_line_id.to_string() == revision_line.base.id);
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

/// 把销售行资格投影映射为验收工作台分组视图。
///
/// # 用途
/// 仅做展示字段映射：数量字段全部取自领域投影，展示字段（行号/品名/单位/
/// 单号/时间/物流）按来源事实与版本行组装，分组按行号稳定排序。
///
/// # 参数
/// * `sources` - 版本行、履约集合与分配（展示字段来源）
/// * `lines` - 领域行级资格投影
///
/// # 返回
/// 返回按行号稳定排序的工作台分组视图。
fn build_eligibility_views(
    sources: &EligibilityGroupSources<'_>,
    lines: &[AcceptanceLineEligibility],
) -> Vec<AcceptanceSalesLineGroupView> {
    let mut meta_by_line: HashMap<String, (u32, String, Option<String>)> = HashMap::new();
    for revision_line in sources.revision_lines {
        let goods = sources
            .goods_service_lines
            .iter()
            .find(|goods| goods.revision_line_id.to_string() == revision_line.base.id);
        meta_by_line.insert(
            revision_line.sales_order_line_id.to_string(),
            (
                revision_line.line_no,
                revision_line.item_name_snapshot.clone(),
                goods.map(|goods| goods.base_unit_code.clone()),
            ),
        );
    }
    let delivery_line_by_id: HashMap<&str, &DeliveryLine> = sources
        .delivery_lines
        .iter()
        .map(|line| (line.base.id.as_str(), line))
        .collect();
    let delivery_by_id: HashMap<&str, &Delivery> = sources
        .deliveries
        .iter()
        .map(|delivery| (delivery.base.id.as_str(), delivery))
        .collect();
    let electronic_by_id: HashMap<&str, &ElectronicDelivery> = sources
        .electronic
        .iter()
        .map(|record| (record.base.id.as_str(), record))
        .collect();
    let service_by_id: HashMap<&str, &ServiceFulfillment> = sources
        .service
        .iter()
        .map(|record| (record.base.id.as_str(), record))
        .collect();
    let mut groups: Vec<AcceptanceSalesLineGroupView> = Vec::with_capacity(lines.len());
    for line in lines {
        let (line_no, item_snapshot, unit_code) = meta_by_line
            .get(&line.sales_order_line_id)
            .cloned()
            .unwrap_or_default();
        let mut fact_views = Vec::with_capacity(line.facts.len());
        for fact in &line.facts {
            if let Some(delivery_line) = delivery_line_by_id.get(fact.fulfillment_line_id.as_str()) {
                let delivery = delivery_by_id.get(delivery_line.delivery_id.as_ref());
                fact_views.push(EligibleFulfillmentFactView {
                    fulfillment_line_id: fact.fulfillment_line_id.clone(),
                    fulfillment_fact_type: FulfillmentFactType::Delivery,
                    delivery_type: delivery.map(|delivery| delivery.delivery_type),
                    fulfillment_no: delivery
                        .map(|delivery| delivery.delivery_no.clone())
                        .unwrap_or_default(),
                    sales_order_line_id: line.sales_order_line_id.clone(),
                    line_no: delivery_line.line_no,
                    item_snapshot: item_snapshot.clone(),
                    unit_code: unit_code.clone(),
                    occurred_at: delivery
                        .and_then(|delivery| delivery.shipped_at)
                        .map(|instant| instant.unix_secs())
                        .unwrap_or_default(),
                    net_successful_quantity: delivery_line.quantity,
                    net_accepted_allocated_quantity: fact.net_accepted_quantity,
                    eligible_quantity: fact.eligible_quantity,
                    carrier: delivery.and_then(|delivery| delivery.carrier.clone()),
                    tracking_no: delivery.and_then(|delivery| delivery.tracking_no.clone()),
                });
            } else if let Some(record) = electronic_by_id.get(fact.fulfillment_line_id.as_str()) {
                fact_views.push(EligibleFulfillmentFactView {
                    fulfillment_line_id: fact.fulfillment_line_id.clone(),
                    fulfillment_fact_type: FulfillmentFactType::ElectronicDelivery,
                    delivery_type: None,
                    fulfillment_no: record.fulfillment_no.clone(),
                    sales_order_line_id: line.sales_order_line_id.clone(),
                    line_no,
                    item_snapshot: item_snapshot.clone(),
                    unit_code: unit_code.clone(),
                    occurred_at: record.fact.occurred_at.unix_secs(),
                    net_successful_quantity: record.quantity,
                    net_accepted_allocated_quantity: fact.net_accepted_quantity,
                    eligible_quantity: fact.eligible_quantity,
                    carrier: None,
                    tracking_no: None,
                });
            } else if let Some(record) = service_by_id.get(fact.fulfillment_line_id.as_str()) {
                fact_views.push(EligibleFulfillmentFactView {
                    fulfillment_line_id: fact.fulfillment_line_id.clone(),
                    fulfillment_fact_type: FulfillmentFactType::ServiceFulfillment,
                    delivery_type: None,
                    fulfillment_no: record.fulfillment_no.clone(),
                    sales_order_line_id: line.sales_order_line_id.clone(),
                    line_no,
                    item_snapshot: item_snapshot.clone(),
                    unit_code: unit_code.clone(),
                    occurred_at: record.fact.occurred_at.unix_secs(),
                    net_successful_quantity: record.quantity,
                    net_accepted_allocated_quantity: fact.net_accepted_quantity,
                    eligible_quantity: fact.eligible_quantity,
                    carrier: None,
                    tracking_no: None,
                });
            }
        }
        groups.push(AcceptanceSalesLineGroupView {
            sales_order_line_id: line.sales_order_line_id.clone(),
            line_no,
            item_snapshot,
            unit_code,
            required_quantity: line.required_quantity,
            net_accepted_quantity: line.net_accepted_quantity,
            fulfillment_facts: fact_views,
        });
    }
    groups.sort_by_key(|group| group.line_no);
    groups
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
        assert!(eligibility.contains("build_line_eligibilities"));
        assert!(eligibility.contains("build_eligibility_views"));
        assert!(!eligibility.contains("submit_"));
        assert!(!eligibility.contains("start_approval"));
    }
}

#[cfg(test)]
mod acceptance_eligibility_rule_source_tests {
    use std::str::FromStr;

    use entities::common::source::SourceType;
    use entities::common::time::Instant;
    use entities::fulfillment::{
        AcceptanceFulfillmentAllocation, AcceptanceFulfillmentAllocationData, AcceptanceProgress,
        AllocationAction, Delivery, DeliveryData, DeliveryLine, DeliveryLineData, DeliveryState,
        DeliveryType, ElectronicDelivery, ElectronicDeliveryData, ElectronicDeliveryState,
        FulfillmentFactType, FulfillmentResult, ServiceFulfillment, ServiceFulfillmentData,
        ServiceFulfillmentState,
    };
    use entities::ids::{
        AcceptanceFulfillmentAllocationId, CustomerAcceptanceLineId, DeliveryId, DeliveryLineId,
        ElectronicDeliveryId, FileAssetId, PurchaseLineSalesAllocationId, PurchaseOrderId,
        SalesOrderGoodsServiceLineRevisionId, SalesOrderId, SalesOrderLineId, SalesOrderRevisionId,
        SalesOrderRevisionLineId, ServiceFulfillmentId, SkuId, SkuRevisionId, StockReservationId,
        WarehouseId,
    };
    use entities::money::{Amount, Quantity, Rate, UnitPrice};
    use entities::sales_order::{
        FulfillmentProgress, LineType, SalesOrderGoodsServiceLineRevision,
        SalesOrderGoodsServiceLineRevisionData, SalesOrderRevisionLine, SalesOrderRevisionLineData,
    };

    use super::{build_eligibility_views, build_line_eligibilities, EligibilityGroupSources};

    /// 构造销售版本公共行（实物服务行）。
    fn revision_line(id: &str, line_no: u32, sales_order_line_id: &str) -> SalesOrderRevisionLine {
        SalesOrderRevisionLine::new(
            SalesOrderRevisionLineId::new(id),
            SalesOrderRevisionLineData {
                sales_order_revision_id: SalesOrderRevisionId::new("rev-1"),
                sales_order_line_id: SalesOrderLineId::new(sales_order_line_id),
                line_no,
                line_type: LineType::GoodsService,
                gross_amount: Amount::from_str("100.00").unwrap(),
                net_amount: Amount::from_str("88.50").unwrap(),
                tax_amount: Amount::from_str("11.50").unwrap(),
                sales_tax_rate: Rate::from_str("0.130000").unwrap(),
                item_name_snapshot: format!("商品-{sales_order_line_id}"),
                spec_snapshot: None,
                unit_snapshot: None,
            },
        )
        .unwrap()
    }

    /// 构造与公共行一对一的实物服务子行。
    fn goods_line(
        revision_line_id: &str,
        quantity: &str,
        unit_code: &str,
    ) -> SalesOrderGoodsServiceLineRevision {
        SalesOrderGoodsServiceLineRevision::new(
            SalesOrderGoodsServiceLineRevisionId::new(format!("gs-{revision_line_id}")),
            SalesOrderGoodsServiceLineRevisionData {
                revision_line_id: SalesOrderRevisionLineId::new(revision_line_id),
                sku_id: SkuId::new("sku-1"),
                sku_revision_id: SkuRevisionId::new("skurev-1"),
                welfare_scenario: None,
                service_region: None,
                fulfillment_due_at: Instant::from_unix_secs(1_800_000_000),
                quantity: Quantity::from_str(quantity).unwrap(),
                base_unit_code: unit_code.to_string(),
                unit_price_gross: UnitPrice::from_str("9.9900").unwrap(),
            },
        )
        .unwrap()
    }

    /// 构造已发货的仓发发货单。
    fn delivery(id: &str, delivery_no: &str) -> Delivery {
        let mut delivery = Delivery::new(
            DeliveryId::new(id),
            DeliveryData {
                delivery_no: delivery_no.to_string(),
                delivery_type: DeliveryType::WarehouseShip,
                sales_order_id: SalesOrderId::new("so-1"),
                purchase_order_id: None,
                warehouse_id: Some(WarehouseId::new("wh-1")),
                carrier: Some("顺丰".to_string()),
                tracking_no: Some("SF123456".to_string()),
                address_snapshot_encrypted: None,
                address_snapshot_fingerprint: None,
            },
        )
        .unwrap();
        delivery.status = DeliveryState::Shipped;
        delivery.shipped_at = Some(Instant::from_unix_secs(1_700_100_000));
        delivery
    }

    /// 构造仓发发货行。
    fn delivery_line(
        id: &str,
        delivery_id: &str,
        line_no: u32,
        sales_order_line_id: &str,
        quantity: &str,
    ) -> DeliveryLine {
        DeliveryLine::new(
            DeliveryLineId::new(id),
            DeliveryLineData {
                delivery_id: DeliveryId::new(delivery_id),
                line_no,
                sales_order_line_id: SalesOrderLineId::new(sales_order_line_id),
                quantity: Quantity::from_str(quantity).unwrap(),
                stock_reservation_id: Some(StockReservationId::new("res-1")),
                purchase_line_sales_allocation_id: None,
            },
            DeliveryType::WarehouseShip,
        )
        .unwrap()
    }

    /// 构造已确认的电子交付记录。
    fn electronic_delivery(
        id: &str,
        fulfillment_no: &str,
        sales_order_line_id: &str,
        quantity: &str,
    ) -> ElectronicDelivery {
        let mut record = ElectronicDelivery::new(
            ElectronicDeliveryId::new(id),
            ElectronicDeliveryData {
                fulfillment_no: fulfillment_no.to_string(),
                sales_order_line_id: SalesOrderLineId::new(sales_order_line_id),
                purchase_order_id: PurchaseOrderId::new("po-1"),
                purchase_line_sales_allocation_id: PurchaseLineSalesAllocationId::new("pla-1"),
                recipient_snapshot: "ciphertext-recipient".to_string(),
                recipient_snapshot_fingerprint: ElectronicDelivery::recipient_snapshot_fingerprint(
                    "plaintext-recipient",
                    b"test-key",
                ),
                quantity: Quantity::from_str(quantity).unwrap(),
                result: FulfillmentResult::Success,
                evidence_attachment_id: Some(FileAssetId::new("file-1")),
                fact_no: format!("F-{id}"),
                occurred_at: Instant::from_unix_secs(1_700_200_000),
                recorded_at: Instant::from_unix_secs(1_700_200_100),
                recorded_by: "operator-1".to_string(),
                source_type: SourceType::Erp,
                source_reference: None,
                reason_code: None,
                reason_text: None,
            },
        )
        .unwrap();
        record.status = ElectronicDeliveryState::Confirmed;
        record
    }

    /// 构造已确认的线下服务履约记录。
    fn service_fulfillment(
        id: &str,
        fulfillment_no: &str,
        sales_order_line_id: &str,
        quantity: &str,
        result: FulfillmentResult,
    ) -> ServiceFulfillment {
        let mut record = ServiceFulfillment::new(
            ServiceFulfillmentId::new(id),
            ServiceFulfillmentData {
                fulfillment_no: fulfillment_no.to_string(),
                sales_order_line_id: SalesOrderLineId::new(sales_order_line_id),
                purchase_order_id: PurchaseOrderId::new("po-1"),
                purchase_line_sales_allocation_id: PurchaseLineSalesAllocationId::new("pla-1"),
                recipient_snapshot: "ciphertext-recipient".to_string(),
                recipient_snapshot_fingerprint: ServiceFulfillment::recipient_snapshot_fingerprint(
                    "plaintext-recipient",
                    b"test-key",
                ),
                quantity: Quantity::from_str(quantity).unwrap(),
                result,
                evidence_attachment_id: Some(FileAssetId::new("file-1")),
                service_location_encrypted: "ciphertext-location".to_string(),
                service_location_fingerprint: ServiceFulfillment::service_location_fingerprint(
                    "plaintext-location",
                    b"test-key",
                ),
                service_started_at: Some(Instant::from_unix_secs(1_700_300_000)),
                service_ended_at: Some(Instant::from_unix_secs(1_700_303_600)),
                completion_note: Some("上门安装调试完成".to_string()),
                fact_no: format!("F-{id}"),
                occurred_at: Instant::from_unix_secs(1_700_300_000),
                recorded_at: Instant::from_unix_secs(1_700_300_100),
                recorded_by: "operator-1".to_string(),
                source_type: SourceType::Erp,
                source_reference: None,
                reason_code: None,
                reason_text: None,
            },
        )
        .unwrap();
        record.confirm().unwrap();
        record
    }

    /// 构造一条 APPLY 验收分配。
    fn apply_allocation(
        fact_type: FulfillmentFactType,
        line_id: &str,
        quantity: &str,
    ) -> AcceptanceFulfillmentAllocation {
        AcceptanceFulfillmentAllocation::new(
            AcceptanceFulfillmentAllocationId::new(format!("allocation-{line_id}-{quantity}")),
            AcceptanceFulfillmentAllocationData {
                customer_acceptance_line_id: CustomerAcceptanceLineId::new("acceptance-line-1"),
                fulfillment_fact_type: fact_type,
                fulfillment_line_id: line_id.to_string(),
                allocation_action: AllocationAction::Apply,
                allocated_quantity: Quantity::from_str(quantity).unwrap(),
                reverses_allocation_id: None,
            },
        )
        .unwrap()
    }

    /// 组装验收工作台与过账进度共用的分组输入（FUL-E07 同一规则源）。
    #[allow(clippy::too_many_arguments)]
    fn group_sources<'a>(
        revision_lines: &'a [SalesOrderRevisionLine],
        goods_service_lines: &'a [SalesOrderGoodsServiceLineRevision],
        deliveries: &'a [Delivery],
        delivery_lines: &'a [DeliveryLine],
        electronic: &'a [ElectronicDelivery],
        service: &'a [ServiceFulfillment],
        delivery_allocations: &'a [AcceptanceFulfillmentAllocation],
        electronic_allocations: &'a [AcceptanceFulfillmentAllocation],
        service_allocations: &'a [AcceptanceFulfillmentAllocation],
    ) -> EligibilityGroupSources<'a> {
        EligibilityGroupSources {
            revision_lines,
            goods_service_lines,
            deliveries,
            delivery_lines,
            electronic,
            service,
            delivery_allocations,
            electronic_allocations,
            service_allocations,
        }
    }

    /// 三类履约事实（发货/电子交付/服务履约）按稳定销售明细分组，净验收与
    /// 剩余可验收守恒；工作台视图保持行号稳定排序与展示字段。
    #[test]
    fn three_fact_types_group_per_stable_line_with_stable_sort_and_display_fields() {
        // 版本行故意乱序传入：行号 2 在前、行号 1 在后，视图必须按行号稳定排序。
        let revision_lines = vec![
            revision_line("rl-2", 2, "so-line-2"),
            revision_line("rl-1", 1, "so-line-1"),
        ];
        let goods_service_lines = vec![goods_line("rl-1", "5", "箱"), goods_line("rl-2", "3", "次")];
        let deliveries = vec![delivery("dlv-1", "DLV-2026-001")];
        let delivery_lines = vec![delivery_line("dl-1", "dlv-1", 1, "so-line-1", "5")];
        let electronic = vec![electronic_delivery("ed-1", "ED-2026-001", "so-line-2", "1")];
        let service = vec![service_fulfillment(
            "sf-1",
            "SF-2026-001",
            "so-line-2",
            "2",
            FulfillmentResult::Success,
        )];
        let delivery_allocations = vec![apply_allocation(FulfillmentFactType::Delivery, "dl-1", "4")];
        let electronic_allocations = vec![apply_allocation(
            FulfillmentFactType::ElectronicDelivery,
            "ed-1",
            "0.5",
        )];
        let service_allocations = vec![apply_allocation(
            FulfillmentFactType::ServiceFulfillment,
            "sf-1",
            "1.5",
        )];
        let sources = group_sources(
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

        let lines = build_line_eligibilities(&sources).unwrap();
        let line_1 = lines
            .iter()
            .find(|line| line.sales_order_line_id == "so-line-1")
            .unwrap();
        assert_eq!(line_1.required_quantity, Quantity::from_str("5").unwrap());
        assert_eq!(line_1.net_accepted_quantity, Quantity::from_str("4").unwrap());
        assert_eq!(
            line_1.remaining_eligible_quantity,
            Quantity::from_str("1").unwrap()
        );
        assert_eq!(line_1.facts.len(), 1);
        assert_eq!(line_1.facts[0].fulfillment_line_id, "dl-1");
        let line_2 = lines
            .iter()
            .find(|line| line.sales_order_line_id == "so-line-2")
            .unwrap();
        assert_eq!(line_2.required_quantity, Quantity::from_str("3").unwrap());
        assert_eq!(line_2.net_accepted_quantity, Quantity::from_str("2").unwrap());
        assert_eq!(
            line_2.remaining_eligible_quantity,
            Quantity::from_str("1").unwrap()
        );
        assert_eq!(line_2.facts.len(), 2);
        assert_eq!(line_2.facts[0].fulfillment_line_id, "ed-1");
        assert_eq!(line_2.facts[1].fulfillment_line_id, "sf-1");

        let groups = build_eligibility_views(&sources, &lines);
        assert_eq!(groups.len(), 2);
        assert_eq!(groups[0].line_no, 1);
        assert_eq!(groups[0].sales_order_line_id, "so-line-1");
        assert_eq!(groups[0].item_snapshot, "商品-so-line-1");
        assert_eq!(groups[0].unit_code.as_deref(), Some("箱"));
        assert_eq!(groups[0].fulfillment_facts.len(), 1);
        let delivery_fact = &groups[0].fulfillment_facts[0];
        assert_eq!(delivery_fact.fulfillment_fact_type, FulfillmentFactType::Delivery);
        assert_eq!(delivery_fact.fulfillment_line_id, "dl-1");
        assert_eq!(delivery_fact.delivery_type, Some(DeliveryType::WarehouseShip));
        assert_eq!(delivery_fact.fulfillment_no, "DLV-2026-001");
        assert_eq!(delivery_fact.line_no, 1);
        assert_eq!(delivery_fact.item_snapshot, "商品-so-line-1");
        assert_eq!(delivery_fact.unit_code.as_deref(), Some("箱"));
        assert_eq!(delivery_fact.occurred_at, 1_700_100_000);
        assert_eq!(
            delivery_fact.net_successful_quantity,
            Quantity::from_str("5").unwrap()
        );
        assert_eq!(
            delivery_fact.net_accepted_allocated_quantity,
            Quantity::from_str("4").unwrap()
        );
        assert_eq!(delivery_fact.eligible_quantity, Quantity::from_str("1").unwrap());
        assert_eq!(delivery_fact.carrier.as_deref(), Some("顺丰"));
        assert_eq!(delivery_fact.tracking_no.as_deref(), Some("SF123456"));

        assert_eq!(groups[1].line_no, 2);
        assert_eq!(groups[1].sales_order_line_id, "so-line-2");
        assert_eq!(groups[1].unit_code.as_deref(), Some("次"));
        assert_eq!(groups[1].fulfillment_facts.len(), 2);
        let electronic_fact = &groups[1].fulfillment_facts[0];
        assert_eq!(
            electronic_fact.fulfillment_fact_type,
            FulfillmentFactType::ElectronicDelivery
        );
        assert_eq!(electronic_fact.fulfillment_no, "ED-2026-001");
        assert_eq!(electronic_fact.line_no, 2);
        assert_eq!(
            electronic_fact.net_successful_quantity,
            Quantity::from_str("1").unwrap()
        );
        assert_eq!(
            electronic_fact.net_accepted_allocated_quantity,
            Quantity::from_str("0.5").unwrap()
        );
        assert_eq!(
            electronic_fact.eligible_quantity,
            Quantity::from_str("0.5").unwrap()
        );
        assert_eq!(electronic_fact.delivery_type, None);
        assert_eq!(electronic_fact.carrier, None);
        let service_fact = &groups[1].fulfillment_facts[1];
        assert_eq!(
            service_fact.fulfillment_fact_type,
            FulfillmentFactType::ServiceFulfillment
        );
        assert_eq!(service_fact.fulfillment_no, "SF-2026-001");
        assert_eq!(service_fact.line_no, 2);
        assert_eq!(
            service_fact.net_successful_quantity,
            Quantity::from_str("2").unwrap()
        );
        assert_eq!(
            service_fact.net_accepted_allocated_quantity,
            Quantity::from_str("1.5").unwrap()
        );
        assert_eq!(service_fact.eligible_quantity, Quantity::from_str("0.5").unwrap());

        let progress = AcceptanceProgress::derive(&lines).unwrap();
        assert_eq!(progress.progress, FulfillmentProgress::PartiallyFulfilled);
        assert!(progress.has_remaining_eligible);
    }

    /// 已确认但失败的线下服务履约不得影响派生进度与剩余可验收：工作台与过账
    /// 进入规则源前必须剔除该事实，否则全部验收后 `has_remaining_eligible`
    /// 仍为真，验收任务不会关闭。
    #[test]
    fn confirmed_failed_service_fact_has_no_effect_on_derived_progress() {
        let revision_lines = vec![revision_line("rl-1", 1, "so-line-1")];
        let goods_service_lines = vec![goods_line("rl-1", "2", "箱")];
        let deliveries = vec![delivery("dlv-1", "DLV-2026-001")];
        let delivery_lines = vec![delivery_line("dl-1", "dlv-1", 1, "so-line-1", "2")];
        let failed_service = service_fulfillment(
            "sf-failed",
            "SF-2026-FAIL",
            "so-line-1",
            "2",
            FulfillmentResult::Failure,
        );
        assert_eq!(failed_service.status, ServiceFulfillmentState::Confirmed);
        assert!(!failed_service.is_acceptance_eligible());
        let delivery_allocations = vec![apply_allocation(FulfillmentFactType::Delivery, "dl-1", "2")];

        // 工作台与过账共用同一筛选：进入分组前剔除不可验收的服务事实。
        let service: Vec<ServiceFulfillment> = vec![failed_service.clone()]
            .into_iter()
            .filter(ServiceFulfillment::is_acceptance_eligible)
            .collect();
        assert!(service.is_empty());

        let sources = group_sources(
            &revision_lines,
            &goods_service_lines,
            &deliveries,
            &delivery_lines,
            &[],
            &service,
            &delivery_allocations,
            &[],
            &[],
        );
        let lines = build_line_eligibilities(&sources).unwrap();
        assert_eq!(lines[0].facts.len(), 1);
        assert_eq!(lines[0].facts[0].fulfillment_line_id, "dl-1");
        let progress = AcceptanceProgress::derive(&lines).unwrap();
        assert_eq!(progress.progress, FulfillmentProgress::Completed);
        assert!(!progress.has_remaining_eligible);

        // 与根本不加载失败记录相比，派生结果完全一致（失败记录无任何影响）。
        let sources_without_failed = group_sources(
            &revision_lines,
            &goods_service_lines,
            &deliveries,
            &delivery_lines,
            &[],
            &[],
            &delivery_allocations,
            &[],
            &[],
        );
        let lines_without_failed = build_line_eligibilities(&sources_without_failed).unwrap();
        assert_eq!(lines, lines_without_failed);
        assert_eq!(AcceptanceProgress::derive(&lines_without_failed), Some(progress));

        // 未筛选（修复前行为）：失败记录被当作可验收事实，全部验收后
        // has_remaining_eligible 仍为真，验收任务保持开放。
        let raw_service = vec![failed_service];
        let raw_sources = group_sources(
            &revision_lines,
            &goods_service_lines,
            &deliveries,
            &delivery_lines,
            &[],
            &raw_service,
            &delivery_allocations,
            &[],
            &[],
        );
        let raw_lines = build_line_eligibilities(&raw_sources).unwrap();
        assert_eq!(raw_lines[0].facts.len(), 2);
        let raw_progress = AcceptanceProgress::derive(&raw_lines).unwrap();
        assert_eq!(raw_progress.progress, FulfillmentProgress::Completed);
        assert!(raw_progress.has_remaining_eligible);
    }

    /// 净验收超过成功履约数量时错误向上传递，禁止静默回退为零。
    #[test]
    fn net_over_acceptance_error_propagates_from_rule_source() {
        let revision_lines = vec![revision_line("rl-1", 1, "so-line-1")];
        let goods_service_lines = vec![goods_line("rl-1", "5", "箱")];
        let deliveries = vec![delivery("dlv-1", "DLV-2026-001")];
        let delivery_lines = vec![delivery_line("dl-1", "dlv-1", 1, "so-line-1", "5")];
        let over_accepted = vec![apply_allocation(FulfillmentFactType::Delivery, "dl-1", "6")];
        let sources = group_sources(
            &revision_lines,
            &goods_service_lines,
            &deliveries,
            &delivery_lines,
            &[],
            &[],
            &over_accepted,
            &[],
            &[],
        );
        let error =
            build_line_eligibilities(&sources).expect_err("净验收超过成功履约数量必须向上传递，禁止回退为零");
        assert!(error.to_string().contains("超过其净成功履约数量"));
    }

    /// 剩余可验收数量驱动过账后的任务关闭：无剩余 → 任务完成；有剩余或未
    /// 开始 → 任务保持开放（`persist_customer_acceptance_task_after_posting`
    /// 仅在 `!has_remaining_eligible` 时完成 WorkItem）。
    #[test]
    fn has_remaining_eligible_drives_acceptance_task_completion() {
        let revision_lines = vec![revision_line("rl-1", 1, "so-line-1")];
        let goods_service_lines = vec![goods_line("rl-1", "2", "箱")];
        let deliveries = vec![delivery("dlv-1", "DLV-2026-001")];
        let delivery_lines = vec![delivery_line("dl-1", "dlv-1", 1, "so-line-1", "2")];

        let fully_accepted = vec![apply_allocation(FulfillmentFactType::Delivery, "dl-1", "2")];
        let sources = group_sources(
            &revision_lines,
            &goods_service_lines,
            &deliveries,
            &delivery_lines,
            &[],
            &[],
            &fully_accepted,
            &[],
            &[],
        );
        let progress = AcceptanceProgress::derive(&build_line_eligibilities(&sources).unwrap()).unwrap();
        assert_eq!(progress.progress, FulfillmentProgress::Completed);
        assert!(!progress.has_remaining_eligible);

        let partial = vec![apply_allocation(FulfillmentFactType::Delivery, "dl-1", "1")];
        let sources = group_sources(
            &revision_lines,
            &goods_service_lines,
            &deliveries,
            &delivery_lines,
            &[],
            &[],
            &partial,
            &[],
            &[],
        );
        let progress = AcceptanceProgress::derive(&build_line_eligibilities(&sources).unwrap()).unwrap();
        assert_eq!(progress.progress, FulfillmentProgress::PartiallyFulfilled);
        assert!(progress.has_remaining_eligible);

        let sources = group_sources(
            &revision_lines,
            &goods_service_lines,
            &deliveries,
            &delivery_lines,
            &[],
            &[],
            &[],
            &[],
            &[],
        );
        let progress = AcceptanceProgress::derive(&build_line_eligibilities(&sources).unwrap()).unwrap();
        assert_eq!(progress.progress, FulfillmentProgress::NotStarted);
        assert!(progress.has_remaining_eligible);
    }

    /// 工作台与过账进度使用同一规则源：两条路径以同一仓储查询与同一筛选
    /// 构造服务履约事实集，送入同一个 `build_line_eligibilities`；过账路径再
    /// 用 `AcceptanceProgress::derive` 派生进度并以返回值驱动任务关闭。
    #[test]
    fn workbench_and_posting_feed_identical_service_fact_sets() {
        let workbench = include_str!("acceptance_eligibility.rs")
            .split("#[cfg(test)]")
            .next()
            .expect("生产代码");
        let eligibility = workbench
            .split("pub async fn acceptance_eligibility")
            .nth(1)
            .and_then(|rest| rest.split("fn so_line_ids").next())
            .expect("acceptance_eligibility 生产片段");
        assert!(eligibility.contains("list_confirmed_service_fulfillments"));
        assert!(eligibility.contains(".filter(ServiceFulfillment::is_acceptance_eligible)"));
        assert!(eligibility.contains("build_line_eligibilities"));
        assert!(
            eligibility.find(".filter(ServiceFulfillment::is_acceptance_eligible)")
                < eligibility.find("build_line_eligibilities")
        );

        let posting = include_str!("customer_acceptance_posting.rs")
            .split("#[cfg(test)]")
            .next()
            .expect("生产代码");
        let progress_update = posting
            .split("async fn update_sales_order_fulfillment_progress")
            .nth(1)
            .expect("update_sales_order_fulfillment_progress 生产片段");
        assert!(progress_update.contains("list_confirmed_service_fulfillments"));
        assert!(progress_update.contains(".filter(ServiceFulfillment::is_acceptance_eligible)"));
        assert!(progress_update.contains("build_line_eligibilities"));
        assert!(progress_update.contains("AcceptanceProgress::derive"));
        assert!(
            progress_update.find(".filter(ServiceFulfillment::is_acceptance_eligible)")
                < progress_update.find("build_line_eligibilities")
        );

        let task = include_str!("customer_acceptance_task.rs")
            .split("#[cfg(test)]")
            .next()
            .expect("生产代码");
        assert!(task.contains("if !has_remaining_eligible"));
        assert!(task.contains("complete_by_domain_command"));
    }
}
