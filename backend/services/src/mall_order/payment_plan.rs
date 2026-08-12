//! 消费入账写入计划与金额守恒校验。

use database::{CardInstanceExt, NoTransaction};
use entities::common::time::Instant;
use entities::cost::{CostAllocation, CostEntry};
use entities::ids::{
    MallConsumptionEntryId, MallItemFundingAllocationId, MallOrderFactId, MallOrderId, MallOrderItemId,
    MallPaymentSourceId,
};
use entities::mall_order::{
    AttributionStatus, ConsumptionDirection, FulfillmentChain, MallConsumptionCostAssessment,
    MallConsumptionEntry, MallConsumptionEntryData, MallItemFundingAllocation, MallItemFundingAllocationData,
    MallOrder, MallOrderData, MallOrderFact, MallOrderItem, MallOrderItemData, MallPaymentSource,
    MallPaymentSourceData, PaymentSourceType, ProcessingStatus,
};
use entities::money::{round_to_cent, Amount, Quantity, Rate, UnitPrice};
use id_generator::next_id;
use std::str::FromStr;

use super::dto;
use super::dto::ReceiveMallOrderFactRequest;
use super::query::attribution_for;
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
    /// # 参数
    /// * `req` - 事实接收请求
    /// * `payment` - 付款载荷
    /// * `fact_id` - 事实 ID
    /// * `fact` - 待写入的支付事实（归集结果写入处理状态）
    /// * `order_id` - 订单 ID
    ///
    /// # 返回
    /// 返回全部待写实体。
    ///
    /// # 错误
    /// 金额守恒不成立、明细行/来源引用缺失或实体校验失败时返回错误。
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
        let chain = match cutover.as_ref().and_then(|c| c.enabled_at) {
            Some(t) if occurred >= t => FulfillmentChain::ErpAutomated,
            _ => FulfillmentChain::LegacyManual,
        };

        let mut items: Vec<MallOrderItem> = Vec::with_capacity(payment.items.len());
        for line in &payment.items {
            let quantity = Quantity::from_str(&line.quantity)?;
            let unit_price = UnitPrice::from_str(&line.unit_price_gross)?;
            let line_gross =
                Amount::try_from(round_to_cent(quantity.to_decimal() * unit_price.to_decimal()))?;
            let paid = line_gross
                .checked_sub(Amount::from_str(&line.allocated_discount_amount)?)
                .checked_add(Amount::from_str(&line.allocated_freight_amount)?);
            items.push(MallOrderItem::new(
                MallOrderItemId::new(next_id()),
                MallOrderItemData {
                    mall_order_id: order_id.clone(),
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
                    line_gross_amount: line_gross,
                    allocated_discount_amount: Amount::from_str(&line.allocated_discount_amount)?,
                    allocated_freight_amount: Amount::from_str(&line.allocated_freight_amount)?,
                    paid_amount: paid,
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

        let mut sources: Vec<MallPaymentSource> = Vec::with_capacity(payment.payment_sources.len());
        for source in &payment.payment_sources {
            let card_instance = match source.source_type {
                PaymentSourceType::Card => {
                    self.db
                        .mall_card_instances()
                        .find_by_identity(
                            &req.mall_id,
                            source.source_card_instance_ref.as_deref().unwrap_or_default(),
                            &mut NoTransaction,
                        )
                        .await?
                }
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
                    mall_card_instance_id: card_instance.as_ref().map(|c| c.base.id.clone().into()),
                    wechat_payment_ref: source.wechat_payment_ref.clone(),
                    attribution_status: attribution_for(source.source_type, &card_instance),
                },
            )?);
        }

        let mut allocations: Vec<MallItemFundingAllocation> =
            Vec::with_capacity(payment.funding_allocations.len());
        for allocation in &payment.funding_allocations {
            let item = items
                .iter()
                .find(|item| item.external_item_id == allocation.external_item_id)
                .ok_or_else(|| {
                    Error::BusinessLogicError(format!(
                        "分摊引用的商品明细不存在: {}",
                        allocation.external_item_id
                    ))
                })?;
            let source = sources
                .iter()
                .find(|source| source.source_no == allocation.source_no)
                .ok_or_else(|| {
                    Error::BusinessLogicError(format!("分摊引用的支付来源不存在: {}", allocation.source_no))
                })?;
            allocations.push(MallItemFundingAllocation::new(
                MallItemFundingAllocationId::new(next_id()),
                MallItemFundingAllocationData {
                    mall_order_item_id: item.base.id.clone().into(),
                    mall_payment_source_id: source.base.id.clone().into(),
                    allocated_payment_amount: Amount::from_str(&allocation.allocated_payment_amount)?,
                },
            )?);
        }
        self.ensure_conservation(&payment, &items, &sources, &allocations)?;

        let all_attributed = sources
            .iter()
            .all(|source| source.attribution_status == AttributionStatus::Attributed);
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
                gross_amount: Amount::from_str(&payment.gross_amount)?,
                discount_amount: Amount::from_str(&payment.discount_amount)?,
                freight_amount: Amount::from_str(&payment.freight_amount)?,
                paid_amount: Amount::from_str(&payment.paid_amount)?,
                fulfillment_chain: chain,
                attribution_status: order_attribution,
                address_snapshot_encrypted: payment.address_snapshot_encrypted.clone(),
            },
        )?;

        let mut entries: Vec<MallConsumptionEntry> = Vec::with_capacity(allocations.len());
        for allocation in &allocations {
            let source = sources
                .iter()
                .find(|source| source.base.id == allocation.mall_payment_source_id.as_ref())
                .expect("分摊来源已校验存在");
            let item = items
                .iter()
                .find(|item| item.base.id == allocation.mall_order_item_id.as_ref())
                .expect("分摊明细已校验存在");
            let origin_sales_order_id = match source.mall_card_instance_id.as_ref() {
                Some(card_id) => self
                    .db
                    .mall_card_instances()
                    .find_by_id(card_id, &mut NoTransaction)
                    .await?
                    .map(|instance| instance.origin_sales_order_id),
                None => None,
            };
            entries.push(MallConsumptionEntry::new(
                MallConsumptionEntryId::new(next_id()),
                MallConsumptionEntryData {
                    mall_order_fact_id: fact_id.clone(),
                    mall_order_item_id: allocation.mall_order_item_id.clone(),
                    mall_payment_source_id: allocation.mall_payment_source_id.clone(),
                    direction: ConsumptionDirection::Consumption,
                    amount: allocation.allocated_payment_amount,
                    customer_id: None,
                    origin_sales_order_id,
                    sales_order_line_id: None,
                    occurred_at: occurred,
                    attribution_status: source.attribution_status,
                    reverses_consumption_entry_id: None,
                },
            )?);
            let _ = item;
        }
        let (assessments, cost_entries, cost_allocations) = self.build_cost_assessments(
            &items,
            &sources,
            &allocations,
            &entries,
            occurred,
            actor.id().to_string(),
        );

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

    /// 校验分摊矩阵守恒（§6.17）：行合计 = 明细实付、列合计 = 来源金额、
    /// 订单汇总恒等。任一不成立即拒绝接收。
    ///
    /// # 参数
    /// * `payment` - 付款载荷
    /// * `items` - 商品明细
    /// * `sources` - 支付来源
    /// * `allocations` - 分摊记录
    ///
    /// # 返回
    /// 守恒成立返回 `Ok(())`。
    ///
    /// # 错误
    /// 守恒不成立返回 `BusinessLogicError`。
    fn ensure_conservation(
        &self,
        payment: &dto::PaymentFactData,
        items: &[MallOrderItem],
        sources: &[MallPaymentSource],
        allocations: &[MallItemFundingAllocation],
    ) -> Result<()> {
        let zero = Amount::from_str("0.00")?;
        for item in items {
            let allocated = allocations
                .iter()
                .filter(|allocation| allocation.mall_order_item_id.as_ref() == item.base.id)
                .fold(zero, |acc, allocation| {
                    acc.checked_add(allocation.allocated_payment_amount)
                });
            if allocated.to_decimal() != item.paid_amount.to_decimal() {
                return Err(Error::BusinessLogicError(format!(
                    "商品明细 {} 分摊合计与实付不一致",
                    item.external_item_id
                )));
            }
        }
        for source in sources {
            let allocated = allocations
                .iter()
                .filter(|allocation| allocation.mall_payment_source_id.as_ref() == source.base.id)
                .fold(zero, |acc, allocation| {
                    acc.checked_add(allocation.allocated_payment_amount)
                });
            if allocated.to_decimal() != source.amount.to_decimal() {
                return Err(Error::BusinessLogicError(format!(
                    "支付来源 {} 分摊合计与支付金额不一致",
                    source.source_no
                )));
            }
        }
        let totals = items.iter().fold((zero, zero, zero), |acc, item| {
            (
                acc.0.checked_add(item.line_gross_amount),
                acc.1.checked_add(item.allocated_discount_amount),
                acc.2.checked_add(item.paid_amount),
            )
        });
        if totals.0.to_decimal() != Amount::from_str(&payment.gross_amount)?.to_decimal()
            || totals.1.to_decimal() != Amount::from_str(&payment.discount_amount)?.to_decimal()
            || totals.2.to_decimal() != Amount::from_str(&payment.paid_amount)?.to_decimal()
        {
            return Err(Error::BusinessLogicError(
                "商品明细汇总与订单金额不一致".to_string(),
            ));
        }
        Ok(())
    }
}
