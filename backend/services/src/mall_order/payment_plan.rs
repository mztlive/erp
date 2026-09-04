//! 消费入账写入计划与金额守恒校验。

use database::{CardInstanceExt, NoTransaction};
use entities::common::time::Instant;
use entities::cost::{CostAllocation, CostEntry};
use entities::ids::{
    MallConsumptionEntryId, MallItemFundingAllocationId, MallOrderFactId, MallOrderId, MallOrderItemId,
    MallPaymentSourceId,
};
use entities::mall_order::{
    AttributionRollup, AttributionStatus, FulfillmentChain, FundingOrderAmounts, FundingPlan,
    MallConsumptionCostAssessment, MallConsumptionEntry, MallItemFundingAllocation, MallOrder, MallOrderData,
    MallOrderFact, MallOrderItem, MallOrderItemLineInput, MallPaymentSource, MallPaymentSourceData,
    PaymentSourceType, PlannedAllocationRequest, ProcessingStatus,
};
use entities::money::{Amount, Quantity, Rate, UnitPrice};
use id_generator::next_id;
use std::collections::HashMap;
use std::str::FromStr;

use super::dto;
use super::dto::ReceiveMallOrderFactRequest;
use super::MallOrderService;
use crate::audit::AuditActor;
use crate::errors::{Error, Result};

/// 消费入账写入计划（事务闭包内全部实体的不可变快照）。
#[derive(Debug, Clone)]
pub(super) struct PaymentPlan {
    /// 商城订单追溯对象。
    pub(super) order: MallOrder,
    /// 商品明细。
    pub(super) items: Vec<MallOrderItem>,
    /// 支付来源。
    pub(super) sources: Vec<MallPaymentSource>,
    /// 商品 × 支付来源分摊。
    pub(super) allocations: Vec<MallItemFundingAllocation>,
    /// 消费事实（每行分摊一条）。
    pub(super) entries: Vec<MallConsumptionEntry>,
    /// 成本评估（每行消费一条链首评估）。
    pub(super) assessments: Vec<MallConsumptionCostAssessment>,
    /// 实际成本事实（D20）。
    pub(super) cost_entries: Vec<CostEntry>,
    /// 实际成本分配（D20）。
    pub(super) cost_allocations: Vec<CostAllocation>,
}

impl MallOrderService {
    /// 构建消费入账写入计划（金额守恒校验 + 归集 + 成本评估）。
    ///
    /// # 用途
    /// 将支付事实载荷编排为事务内写入所需的订单图、消费事实和成本事实快照。
    ///
    /// # 参数
    /// * `req` - 事实接收请求
    /// * `payment` - 付款载荷
    /// * `fact_id` - 事实 ID
    /// * `fact` - 待写入的支付事实（归集结果写入处理状态）
    /// * `order_id` - 订单 ID
    /// * `actor` - 成本评估记录使用的审计操作人
    ///
    /// # 返回
    /// 返回全部待写实体。
    ///
    /// # 错误
    /// 金额守恒不成立、明细行/来源引用缺失或实体校验失败时返回错误。
    ///
    /// # 关键约束
    /// 守恒规则由实体层统一评估；Service 保留外部错误文案、归集编排和事务写入边界。
    pub(super) async fn build_payment_plan(
        &self,
        req: &ReceiveMallOrderFactRequest,
        payment: dto::PaymentFactData,
        fact_id: MallOrderFactId,
        fact: &mut MallOrderFact,
        order_id: &MallOrderId,
        actor: &AuditActor,
    ) -> Result<PaymentPlan> {
        let occurred = Instant::from_unix_secs(req.occurred_at as i64);
        let cutover = self
            .db
            .mall_consumption_cutovers()
            .find_enabled_cutover_by_mall_id(&req.mall_id, &mut NoTransaction)
            .await?;
        let chain = FulfillmentChain::from_payment_occurred_at(
            occurred,
            cutover.as_ref().and_then(|cutover| cutover.enabled_at),
        );

        let mut items: Vec<MallOrderItem> = Vec::with_capacity(payment.items.len());
        for line in &payment.items {
            // 解析顺序与旧内联实现一致（数量→单价→优惠→运费→税率），首个字段错误保持不变。
            let quantity = Quantity::from_str(&line.quantity)?;
            let unit_price = UnitPrice::from_str(&line.unit_price_gross)?;
            let allocated_discount = Amount::from_str(&line.allocated_discount_amount)?;
            let allocated_freight = Amount::from_str(&line.allocated_freight_amount)?;
            items.push(MallOrderItem::from_line_primitives(
                MallOrderItemId::new(next_id()),
                order_id.clone(),
                MallOrderItemLineInput {
                    external_item_id: line.external_item_id.clone(),
                    sku_id: line.sku_id.clone().map(entities::ids::SkuId::new),
                    product_publication_revision_id: line
                        .product_publication_revision_id
                        .clone()
                        .map(entities::ids::ProductPublicationRevisionId::new),
                    supplier_offering_revision_id: line
                        .supplier_offering_revision_id
                        .clone()
                        .map(entities::ids::SupplierOfferingRevisionId::new),
                    name_snapshot: line.name_snapshot.clone(),
                    spec_snapshot: line.spec_snapshot.clone(),
                    quantity,
                    unit_price_gross: unit_price,
                    allocated_discount_amount: allocated_discount,
                    allocated_freight_amount: allocated_freight,
                    sales_tax_rate: Rate::from_str(&line.sales_tax_rate)?,
                    unit_cost_snapshot: line
                        .unit_cost_snapshot
                        .as_deref()
                        .map(UnitPrice::from_str)
                        .transpose()?,
                    cost_snapshot_total: line
                        .cost_snapshot_total
                        .as_deref()
                        .map(Amount::from_str)
                        .transpose()?,
                    cost_tax_inclusion: line.cost_tax_inclusion,
                    cost_input_tax_rate: line
                        .cost_input_tax_rate
                        .as_deref()
                        .map(Rate::from_str)
                        .transpose()?,
                },
            )?);
        }

        let mut card_refs: Vec<String> = Vec::new();
        for source in &payment.payment_sources {
            if source.source_type == PaymentSourceType::Card {
                if let Some(reference) = source.source_card_instance_ref.as_deref() {
                    if !card_refs.iter().any(|known| known == reference) {
                        card_refs.push(reference.to_string());
                    }
                }
            }
        }
        let card_map = self
            .db
            .mall_card_instances()
            .list_by_identity_refs(&req.mall_id, &card_refs, &mut NoTransaction)
            .await?;
        let mut sources: Vec<MallPaymentSource> = Vec::with_capacity(payment.payment_sources.len());
        for source in &payment.payment_sources {
            let card_instance = match source.source_type {
                PaymentSourceType::Card => source
                    .source_card_instance_ref
                    .as_deref()
                    .and_then(|reference| card_map.get(reference)),
                PaymentSourceType::Wechat => None,
            };
            sources.push(MallPaymentSource::new(
                MallPaymentSourceId::new(next_id()),
                MallPaymentSourceData {
                    mall_order_id: order_id.clone(),
                    source_no: source.source_no,
                    source_type: source.source_type,
                    amount: Amount::from_str(&source.amount)?,
                    source_card_instance_ref: source.source_card_instance_ref.clone(),
                    mall_card_instance_id: card_instance.map(|card| card.base.id.clone().into()),
                    wechat_payment_ref: source.wechat_payment_ref.clone(),
                    attribution_status: MallPaymentSource::decide_attribution(
                        source.source_type,
                        card_instance.is_some(),
                    ),
                },
            )?);
        }

        let order_amounts = FundingOrderAmounts {
            gross: Amount::from_str(&payment.gross_amount)?,
            discount: Amount::from_str(&payment.discount_amount)?,
            paid: Amount::from_str(&payment.paid_amount)?,
        };
        let mut allocation_requests: Vec<PlannedAllocationRequest> =
            Vec::with_capacity(payment.funding_allocations.len());
        for allocation in &payment.funding_allocations {
            allocation_requests.push(PlannedAllocationRequest {
                external_item_id: allocation.external_item_id.clone(),
                source_no: allocation.source_no,
                allocated_payment_amount: Amount::from_str(&allocation.allocated_payment_amount)?,
                allocation_id: MallItemFundingAllocationId::new(next_id()),
            });
        }
        let entry_ids: Vec<MallConsumptionEntryId> = allocation_requests
            .iter()
            .map(|_| MallConsumptionEntryId::new(next_id()))
            .collect();
        let mut card_ids: Vec<entities::ids::MallCardInstanceId> = Vec::new();
        for source in &sources {
            if let Some(card_id) = source.mall_card_instance_id.as_ref() {
                if !card_ids.iter().any(|known| known == card_id) {
                    card_ids.push(card_id.clone());
                }
            }
        }
        let cards_by_id = self
            .db
            .mall_card_instances()
            .list_by_card_ids(&card_ids, &mut NoTransaction)
            .await?;
        let mut origins: HashMap<MallPaymentSourceId, entities::ids::SalesOrderId> = HashMap::new();
        for source in &sources {
            if let Some(card_id) = source.mall_card_instance_id.as_ref() {
                if let Some(card) = cards_by_id.get(&card_id.to_string()) {
                    origins.insert(
                        MallPaymentSourceId::new(source.base.id.clone()),
                        card.origin_sales_order_id.clone(),
                    );
                }
            }
        }
        let funding = FundingPlan::build(
            &items,
            &sources,
            &allocation_requests,
            order_amounts,
            &fact_id,
            occurred,
            &entry_ids,
            &origins,
        )
        .map_err(|error| Error::BusinessLogicError(error.to_string()))?;
        let allocations: Vec<MallItemFundingAllocation> = funding.allocations;
        let entries: Vec<MallConsumptionEntry> = funding.entries;

        let rollup = AttributionRollup::from_sources(&sources);
        let all_attributed = rollup.is_attributed();
        let order_attribution = if all_attributed {
            AttributionStatus::Attributed
        } else {
            AttributionStatus::PendingAttribution
        };
        let order = MallOrder::new(
            order_id.clone(),
            MallOrderData {
                mall_id: req.mall_id.clone(),
                external_order_no: req.external_order_no.clone(),
                payment_fact_id: fact_id.clone(),
                mall_user_ref: payment.mall_user_ref.clone(),
                source_customer_ref: payment.source_customer_ref.clone(),
                customer_id: payment
                    .customer_id
                    .as_deref()
                    .map(entities::ids::CustomerAccountId::new),
                ordered_at: Instant::from_unix_secs(payment.ordered_at as i64),
                paid_at: occurred,
                gross_amount: order_amounts.gross,
                discount_amount: order_amounts.discount,
                freight_amount: Amount::from_str(&payment.freight_amount)?,
                paid_amount: order_amounts.paid,
                fulfillment_chain: chain,
                attribution_status: order_attribution,
                address_snapshot_encrypted: payment.address_snapshot_encrypted.clone(),
            },
        )?;

        let (assessments, cost_entries, cost_allocations) = self.build_cost_assessments(
            &items,
            &sources,
            &allocations,
            &entries,
            occurred,
            actor.id().to_string(),
        )?;

        let processing = if all_attributed {
            ProcessingStatus::Attributed
        } else {
            ProcessingStatus::PendingAttribution
        };
        fact.update_processing_status(processing)?;

        Ok(PaymentPlan {
            order,
            items,
            sources,
            allocations,
            entries,
            assessments,
            cost_entries,
            cost_allocations,
        })
    }
}

#[cfg(test)]
mod tests {
    use entities::common::time::Instant;
    use entities::ids::{
        MallConsumptionEntryId, MallItemFundingAllocationId, MallOrderFactId, MallOrderId, MallOrderItemId,
        MallPaymentSourceId,
    };
    use entities::mall_order::types::AttributionStatus;
    use entities::mall_order::{
        FundingOrderAmounts, FundingPlan, MallOrderItem, MallOrderItemData, MallPaymentSource,
        MallPaymentSourceData, PaymentSourceType, PlannedAllocationRequest,
    };
    use entities::money::{Amount, Quantity, Rate, UnitPrice};
    use std::collections::HashMap;
    use std::str::FromStr;

    use crate::errors::Error;

    /// 构造守恒测试明细。
    ///
    /// # 参数
    /// * `id` - 明细主键
    /// * `external` - 来源明细身份
    /// * `paid` - 明细实付
    ///
    /// # 返回
    /// 返回通过实体校验的明细。
    fn item(id: &str, external: &str, paid: &str) -> MallOrderItem {
        MallOrderItem::new(
            MallOrderItemId::new(id),
            MallOrderItemData {
                mall_order_id: MallOrderId::new("order-1"),
                external_item_id: external.to_string(),
                sku_id: None,
                product_publication_revision_id: None,
                supplier_offering_revision_id: None,
                name_snapshot: format!("item-{id}"),
                spec_snapshot: None,
                quantity: Quantity::from_str("1.000000").unwrap(),
                unit_price_gross: UnitPrice::from_str(paid).unwrap(),
                line_gross_amount: Amount::from_str(paid).unwrap(),
                allocated_discount_amount: Amount::from_str("0.00").unwrap(),
                allocated_freight_amount: Amount::from_str("0.00").unwrap(),
                paid_amount: Amount::from_str(paid).unwrap(),
                sales_tax_rate: Rate::from_str("0.000000").unwrap(),
                unit_cost_snapshot: None,
                cost_snapshot_total: None,
                cost_tax_inclusion: None,
                cost_input_tax_rate: None,
            },
        )
        .unwrap()
    }

    /// 构造守恒测试来源。
    ///
    /// # 参数
    /// * `id` - 来源主键
    /// * `source_no` - 单内序号
    /// * `amount` - 来源金额
    ///
    /// # 返回
    /// 返回已归集的微信来源。
    fn source(id: &str, source_no: u32, amount: &str) -> MallPaymentSource {
        MallPaymentSource::new(
            MallPaymentSourceId::new(id),
            MallPaymentSourceData {
                mall_order_id: MallOrderId::new("order-1"),
                source_no,
                source_type: PaymentSourceType::Wechat,
                amount: Amount::from_str(amount).unwrap(),
                source_card_instance_ref: None,
                mall_card_instance_id: None,
                wechat_payment_ref: Some(format!("wx-{id}")),
                attribution_status: AttributionStatus::Attributed,
            },
        )
        .unwrap()
    }

    /// 构造分摊请求。
    ///
    /// # 参数
    /// * `id` - 分摊主键
    /// * `external` - 来源明细身份
    /// * `source_no` - 来源序号
    /// * `amount` - 分摊金额
    ///
    /// # 返回
    /// 返回类型化分摊请求。
    fn request(id: &str, external: &str, source_no: u32, amount: &str) -> PlannedAllocationRequest {
        PlannedAllocationRequest {
            external_item_id: external.to_string(),
            source_no,
            allocated_payment_amount: Amount::from_str(amount).unwrap(),
            allocation_id: MallItemFundingAllocationId::new(id),
        }
    }

    /// 构造订单金额快照。
    ///
    /// # 参数
    /// * `paid` - 订单实付
    ///
    /// # 返回
    /// 返回原价等于实付、优惠为零的快照。
    fn order_amounts(paid: &str) -> FundingOrderAmounts {
        FundingOrderAmounts {
            gross: Amount::from_str(paid).unwrap(),
            discount: Amount::from_str("0.00").unwrap(),
            paid: Amount::from_str(paid).unwrap(),
        }
    }

    /// 生产代码（`#[cfg(test)]` 之前部分），供分层守卫断言，避免字面量自匹配。
    ///
    /// # 返回
    /// 返回去掉测试模块后的生产代码全文。
    fn production_source() -> &'static str {
        include_str!("payment_plan.rs")
            .split("mod tests {")
            .next()
            .expect("必须存在生产代码")
    }

    /// 分层守卫（INT-E02）：行金额派生只归属实体工厂，Service 不复制舍入规则。
    ///
    /// 锁定 `from_line_primitives` 为唯一派生入口；`round_to_cent` 不得出现在 Service。
    #[test]
    fn line_derivation_is_owned_by_entity_factory() {
        let source = production_source();
        assert!(source.contains("from_line_primitives"));
        assert!(!source.contains("round_to_cent"));
    }

    /// 分层守卫（INT-R06）：CARD 解析只走批量接口，复用单次返回映射。
    ///
    /// 锁定引用批量与 ID 批量各调用一次；旧逐引用/逐 ID 单查不得回潮。
    #[test]
    fn card_resolution_uses_batch_lookups_only() {
        let source = production_source();
        assert!(source.contains("list_by_identity_refs"));
        assert!(source.contains("list_by_card_ids"));
        assert!(!source.contains("find_by_identity"));
    }

    /// 将领域构造失败映射为既有服务错误文案（与旧分支一致的分类与消息）。
    ///
    /// # 参数
    /// * `error` - 实体层返回的领域错误
    ///
    /// # 返回
    /// 返回服务层业务逻辑错误。
    fn service_error(error: entities::Error) -> Error {
        Error::BusinessLogicError(error.to_string())
    }

    /// 领域失败继续映射为既有商品、来源与订单汇总业务错误文案。
    ///
    /// 测试穷尽引用缺失与三类守恒违规，不执行外部 I/O；错误分类或冻结文案变化时失败。
    #[test]
    fn funding_plan_failures_keep_service_error_messages() {
        let items = vec![item("item-1", "line-1", "60.00")];
        let sources = vec![source("source-1", 1, "60.00")];
        let fact_id = MallOrderFactId::new("fact-1");
        let occurred = Instant::from_unix_secs(1_700_000_000);
        let entry_ids = vec![MallConsumptionEntryId::new("e-1")];

        let missing_item = FundingPlan::build(
            &items,
            &sources,
            &[request("a-1", "line-missing", 1, "60.00")],
            order_amounts("60.00"),
            &fact_id,
            occurred,
            &entry_ids,
            &HashMap::new(),
        )
        .map_err(service_error)
        .unwrap_err();
        assert!(
            matches!(missing_item, Error::BusinessLogicError(message) if message == "分摊引用的商品明细不存在: line-missing")
        );

        let missing_source = FundingPlan::build(
            &items,
            &sources,
            &[request("a-1", "line-1", 9, "60.00")],
            order_amounts("60.00"),
            &fact_id,
            occurred,
            &entry_ids,
            &HashMap::new(),
        )
        .map_err(service_error)
        .unwrap_err();
        assert!(
            matches!(missing_source, Error::BusinessLogicError(message) if message == "分摊引用的支付来源不存在: 9")
        );

        let item_row = FundingPlan::build(
            &items,
            &sources,
            &[request("a-1", "line-1", 1, "59.00")],
            order_amounts("60.00"),
            &fact_id,
            occurred,
            &entry_ids,
            &HashMap::new(),
        )
        .map_err(service_error)
        .unwrap_err();
        assert!(
            matches!(item_row, Error::BusinessLogicError(message) if message == "商品明细 line-1 分摊合计与实付不一致")
        );

        let source_column = FundingPlan::build(
            &[item("item-1", "line-1", "59.00")],
            &sources,
            &[request("a-1", "line-1", 1, "59.00")],
            order_amounts("59.00"),
            &fact_id,
            occurred,
            &entry_ids,
            &HashMap::new(),
        )
        .map_err(service_error)
        .unwrap_err();
        assert!(
            matches!(source_column, Error::BusinessLogicError(message) if message == "支付来源 1 分摊合计与支付金额不一致")
        );

        let order_total = FundingPlan::build(
            &items,
            &sources,
            &[request("a-1", "line-1", 1, "60.00")],
            order_amounts("59.00"),
            &fact_id,
            occurred,
            &entry_ids,
            &HashMap::new(),
        )
        .map_err(service_error)
        .unwrap_err();
        assert!(
            matches!(order_total, Error::BusinessLogicError(message) if message == "商品明细汇总与订单金额不一致")
        );
    }
}
