//! 消费入账资金计划聚合构造器（INT-E04 领域所有权）。
//!
//! 外部 item/source 编号匹配、分摊确定性连接、唯一性与守恒只归属本模块；
//! Service 只注入已解析卡事实与显式 ID，不使用 `expect` 做关联假设。

use std::collections::{BTreeMap, HashMap};

use crate::common::time::Instant;
use crate::errors::{Error, Result};
use crate::ids::{
    MallConsumptionEntryId, MallItemFundingAllocationId, MallOrderFactId, MallOrderItemId,
    MallPaymentSourceId, SalesOrderId,
};
use crate::mall_order::{
    ConsumptionDirection, FundingConservation, FundingConservationViolation, FundingOrderAmounts,
    MallConsumptionEntry, MallConsumptionEntryData, MallItemFundingAllocation, MallItemFundingAllocationData,
    MallOrderItem, MallPaymentSource,
};
use crate::money::Amount;

/// 分摊请求行（Service 已校验 wire 形态后的类型化输入）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlannedAllocationRequest {
    /// 来源明细身份（引用商品 `external_item_id`）。
    pub external_item_id: String,
    /// 单内支付来源序号（引用来源 `source_no`）。
    pub source_no: u32,
    /// 分摊实付。
    pub allocated_payment_amount: Amount,
    /// 新分摊主键，由调用方显式注入。
    pub allocation_id: MallItemFundingAllocationId,
}

/// 消费事实请求行（分摊确定性连接后的派生输入）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlannedEntryRequest {
    /// 新消费主键，由调用方显式注入。
    pub entry_id: MallConsumptionEntryId,
    /// 分摊主键（引用本次计划内分摊）。
    pub allocation_id: MallItemFundingAllocationId,
}

/// 资金计划构造结果（分摊与消费一一对应，顺序确定）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FundingPlan {
    /// 分摊记录（按 `(external_item_id, source_no)` 确定性排序）。
    pub allocations: Vec<MallItemFundingAllocation>,
    /// 消费事实（与分摊一一对应，同顺序）。
    pub entries: Vec<MallConsumptionEntry>,
}

impl FundingPlan {
    /// 从商品、来源与分摊请求构造资金计划并校验守恒（INT-E04 唯一构造点）。
    ///
    /// # 用途
    /// 拥有唯一性（外部明细 ID 与来源序号各自唯一）、引用匹配（分摊两端必须存在）、
    /// 确定性连接（分摊与消费按稳定键排序）与守恒（行列与订单总额精确一致）。
    ///
    /// # 参数
    /// * `items` - 已规范化商品明细（外部明细 ID 必须唯一）
    /// * `sources` - 已判定归属支付来源（单内序号必须唯一）
    /// * `requests` - 分摊请求（含调用方注入的分摊主键）
    /// * `order_amounts` - 订单原价、优惠与实付快照
    /// * `fact_id` - 所引支付事实，由调用方显式注入
    /// * `occurred_at` - 业务发生时间，由调用方显式注入
    /// * `entry_ids` - 与请求一一对应的新消费主键，由调用方显式注入
    /// * `origins` - 来源 ID 到原销售单的已解析映射，由调用方显式注入
    ///
    /// # 返回
    /// 返回确定性排序的分摊与一一对应的消费事实。
    ///
    /// # 错误
    /// 重复明细/来源编号、缺失引用、分摊请求重复、守恒不成立或实体校验失败时返回错误。
    ///
    /// # 关键约束
    /// 不做字符串解析、不访问 I/O、不使用全局时钟、ID 生成器或 `expect`；
    /// 卡归属事实只接受已解析映射，不自行查询。
    #[allow(clippy::too_many_arguments)]
    pub fn build(
        items: &[MallOrderItem],
        sources: &[MallPaymentSource],
        requests: &[PlannedAllocationRequest],
        order_amounts: FundingOrderAmounts,
        fact_id: &MallOrderFactId,
        occurred_at: Instant,
        entry_ids: &[MallConsumptionEntryId],
        origins: &HashMap<MallPaymentSourceId, SalesOrderId>,
    ) -> Result<Self> {
        ensure_unique_item_identities(items)?;
        ensure_unique_source_numbers(sources)?;
        let item_index = index_items_by_external_id(items)?;
        let source_index = index_sources_by_number(sources)?;
        if entry_ids.len() != requests.len() {
            return Err(Error::from("消费主键数量必须与分摊请求一一对应"));
        }
        let mut ordered: Vec<(&PlannedAllocationRequest, &MallOrderItem, &MallPaymentSource)> =
            Vec::with_capacity(requests.len());
        let mut seen_links = std::collections::HashSet::new();
        for request in requests {
            let item = item_index.get(&request.external_item_id).ok_or_else(|| {
                Error::from(format!("分摊引用的商品明细不存在: {}", request.external_item_id))
            })?;
            let source = source_index
                .get(&request.source_no)
                .ok_or_else(|| Error::from(format!("分摊引用的支付来源不存在: {}", request.source_no)))?;
            if !seen_links.insert((request.external_item_id.clone(), request.source_no)) {
                return Err(Error::from(format!(
                    "分摊引用的商品明细与支付来源重复: {}:{}",
                    request.external_item_id, request.source_no
                )));
            }
            ordered.push((request, item, source));
        }
        ordered.sort_by(|left, right| {
            left.0
                .external_item_id
                .cmp(&right.0.external_item_id)
                .then_with(|| left.0.source_no.cmp(&right.0.source_no))
        });

        let mut allocations = Vec::with_capacity(ordered.len());
        for (request, item, source) in &ordered {
            allocations.push(MallItemFundingAllocation::new(
                request.allocation_id.clone(),
                MallItemFundingAllocationData {
                    mall_order_item_id: MallOrderItemId::new(item.base.id.clone()),
                    mall_payment_source_id: MallPaymentSourceId::new(source.base.id.clone()),
                    allocated_payment_amount: request.allocated_payment_amount,
                },
            )?);
        }
        FundingConservation::evaluate(order_amounts, items, sources, &allocations)
            .ensure_valid()
            .map_err(funding_violation_message)?;

        let mut entries = Vec::with_capacity(allocations.len());
        for (allocation, entry_id) in allocations.iter().zip(entry_ids.iter()) {
            let source = sources
                .iter()
                .find(|source| source.base.id == allocation.mall_payment_source_id.as_ref())
                .ok_or_else(|| Error::from("分摊来源不存在"))?;
            entries.push(MallConsumptionEntry::new(
                entry_id.clone(),
                MallConsumptionEntryData {
                    mall_order_fact_id: fact_id.clone(),
                    mall_order_item_id: allocation.mall_order_item_id.clone(),
                    mall_payment_source_id: allocation.mall_payment_source_id.clone(),
                    direction: ConsumptionDirection::Consumption,
                    amount: allocation.allocated_payment_amount,
                    customer_id: None,
                    origin_sales_order_id: origins
                        .get(&MallPaymentSourceId::new(source.base.id.clone()))
                        .cloned(),
                    sales_order_line_id: None,
                    occurred_at,
                    attribution_status: source.attribution_status,
                    reverses_consumption_entry_id: None,
                },
            )?);
        }
        Ok(Self { allocations, entries })
    }

    /// 返回分摊金额合计（精确，不舍入）。
    ///
    /// # 参数
    /// * `self` - 已构造的资金计划
    ///
    /// # 返回
    /// 返回全部分摊的精确合计；空计划返回 `0.00`。
    pub fn allocated_total(&self) -> Amount {
        self.allocations.iter().fold(Amount::zero(), |total, allocation| {
            total.checked_add(allocation.allocated_payment_amount)
        })
    }

    /// 返回按稳定键排序的分摊连接视图。
    ///
    /// # 参数
    /// * `self` - 已构造的资金计划
    ///
    /// # 返回
    /// 返回 `(明细 ID, 来源 ID, 金额)` 的确定性序列。
    pub fn links(&self) -> Vec<(String, String, Amount)> {
        let mut links: Vec<(String, String, Amount)> = self
            .allocations
            .iter()
            .map(|allocation| {
                (
                    allocation.mall_order_item_id.to_string(),
                    allocation.mall_payment_source_id.to_string(),
                    allocation.allocated_payment_amount,
                )
            })
            .collect();
        links.sort();
        links
    }
}

/// 校验商品外部明细身份唯一。
///
/// # 参数
/// * `items` - 商品明细集合
///
/// # 返回
/// 身份唯一时返回 `Ok(())`。
///
/// # 错误
/// 存在重复来源明细身份时返回错误。
fn ensure_unique_item_identities(items: &[MallOrderItem]) -> Result<()> {
    let mut seen = std::collections::HashSet::new();
    for item in items {
        if !seen.insert(item.external_item_id.clone()) {
            return Err(Error::from(format!(
                "商品明细编号重复: {}",
                item.external_item_id
            )));
        }
    }
    Ok(())
}

/// 校验支付来源单内序号唯一。
///
/// # 参数
/// * `sources` - 支付来源集合
///
/// # 返回
/// 序号唯一时返回 `Ok(())`。
///
/// # 错误
/// 存在重复来源序号时返回错误。
fn ensure_unique_source_numbers(sources: &[MallPaymentSource]) -> Result<()> {
    let mut seen = std::collections::HashSet::new();
    for source in sources {
        if !seen.insert(source.source_no) {
            return Err(Error::from(format!("支付来源序号重复: {}", source.source_no)));
        }
    }
    Ok(())
}

/// 按外部明细身份索引商品。
///
/// # 参数
/// * `items` - 商品明细集合
///
/// # 返回
/// 返回外部明细身份到明细的映射。
fn index_items_by_external_id(items: &[MallOrderItem]) -> Result<BTreeMap<String, &MallOrderItem>> {
    let mut index = BTreeMap::new();
    for item in items {
        index.insert(item.external_item_id.clone(), item);
    }
    Ok(index)
}

/// 按单内序号索引支付来源。
///
/// # 参数
/// * `sources` - 支付来源集合
///
/// # 返回
/// 返回来源序号到来源的映射。
fn index_sources_by_number(
    sources: &[MallPaymentSource],
) -> Result<BTreeMap<u32, &MallPaymentSource>> {
    let mut index = BTreeMap::new();
    for source in sources {
        index.insert(source.source_no, source);
    }
    Ok(index)
}

/// 将资金守恒领域违规映射为既有中文错误文案。
///
/// # 参数
/// * `violation` - 实体层返回的首个守恒违规
///
/// # 返回
/// 返回对应的领域错误。
fn funding_violation_message(violation: FundingConservationViolation) -> Error {
    match violation {
        FundingConservationViolation::ItemRow { external_item_id } => {
            Error::from(format!("商品明细 {external_item_id} 分摊合计与实付不一致"))
        }
        FundingConservationViolation::SourceColumn { source_no } => {
            Error::from(format!("支付来源 {source_no} 分摊合计与支付金额不一致"))
        }
        FundingConservationViolation::OrderAmounts | FundingConservationViolation::OrderPaid => {
            Error::from("商品明细汇总与订单金额不一致".to_string())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{FundingPlan, PlannedAllocationRequest};
    use crate::common::time::Instant;
    use crate::ids::{
        MallConsumptionEntryId, MallItemFundingAllocationId, MallOrderFactId, MallOrderId, MallOrderItemId,
        MallPaymentSourceId, SalesOrderId,
    };
    use crate::mall_order::types::AttributionStatus;
    use crate::mall_order::{
        FundingOrderAmounts, MallOrderItem, MallOrderItemData, MallPaymentSource, MallPaymentSourceData,
        PaymentSourceType,
    };
    use crate::money::{Amount, Quantity, Rate, UnitPrice};
    use std::collections::HashMap;
    use std::str::FromStr;

    /// 构造测试商品明细。
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

    /// 构造测试支付来源。
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

    /// 构造测试订单金额快照。
    fn order_amounts() -> FundingOrderAmounts {
        FundingOrderAmounts {
            gross: Amount::from_str("100.00").unwrap(),
            discount: Amount::from_str("0.00").unwrap(),
            paid: Amount::from_str("100.00").unwrap(),
        }
    }

    /// 正常路径：引用匹配、确定性连接、总额守恒且消费一一对应。
    #[test]
    fn build_links_allocations_and_entries_deterministically() {
        let items = vec![
            item("item-2", "line-b", "40.00"),
            item("item-1", "line-a", "60.00"),
        ];
        let sources = vec![source("source-2", 2, "30.00"), source("source-1", 1, "70.00")];
        let requests = vec![
            request("a-4", "line-b", 2, "20.00"),
            request("a-1", "line-a", 1, "50.00"),
            request("a-3", "line-b", 1, "20.00"),
            request("a-2", "line-a", 2, "10.00"),
        ];
        let entry_ids = vec![
            MallConsumptionEntryId::new("e-1"),
            MallConsumptionEntryId::new("e-2"),
            MallConsumptionEntryId::new("e-3"),
            MallConsumptionEntryId::new("e-4"),
        ];
        let plan = FundingPlan::build(
            &items,
            &sources,
            &requests,
            order_amounts(),
            &MallOrderFactId::new("fact-1"),
            Instant::from_unix_secs(1_700_000_000),
            &entry_ids,
            &HashMap::new(),
        )
        .unwrap();
        assert_eq!(plan.allocations.len(), 4);
        assert_eq!(plan.entries.len(), 4);
        assert_eq!(plan.allocated_total(), Amount::from_str("100.00").unwrap());
        let links = plan.links();
        let mut sorted = links.clone();
        sorted.sort();
        assert_eq!(links, sorted);
        for entry in &plan.entries {
            assert_eq!(entry.occurred_at, Instant::from_unix_secs(1_700_000_000));
        }
    }

    /// 失败路径：重复明细/来源编号、缺失引用、分摊重复引用均拒绝且无 panic。
    #[test]
    fn build_rejects_duplicate_numbers_missing_refs_and_repeated_links() {
        let items = vec![
            item("item-1", "line-a", "60.00"),
            item("item-2", "line-a", "40.00"),
        ];
        let sources = vec![source("source-1", 1, "100.00")];
        let requests = vec![request("a-1", "line-a", 1, "60.00")];
        assert!(FundingPlan::build(
            &items,
            &sources,
            &requests,
            order_amounts(),
            &MallOrderFactId::new("fact-1"),
            Instant::from_unix_secs(1_700_000_000),
            &[MallConsumptionEntryId::new("e-1")],
            &HashMap::new(),
        )
        .is_err());

        let items = vec![item("item-1", "line-a", "60.00")];
        let sources = vec![source("source-1", 1, "60.00")];
        let missing_item = vec![request("a-1", "line-missing", 1, "60.00")];
        assert!(FundingPlan::build(
            &items,
            &sources,
            &missing_item,
            order_amounts(),
            &MallOrderFactId::new("fact-1"),
            Instant::from_unix_secs(1_700_000_000),
            &[MallConsumptionEntryId::new("e-1")],
            &HashMap::new(),
        )
        .is_err());

        let missing_source = vec![request("a-1", "line-a", 9, "60.00")];
        assert!(FundingPlan::build(
            &items,
            &sources,
            &missing_source,
            order_amounts(),
            &MallOrderFactId::new("fact-1"),
            Instant::from_unix_secs(1_700_000_000),
            &[MallConsumptionEntryId::new("e-1")],
            &HashMap::new(),
        )
        .is_err());

        let repeated = vec![
            request("a-1", "line-a", 1, "30.00"),
            request("a-2", "line-a", 1, "30.00"),
        ];
        assert!(FundingPlan::build(
            &items,
            &sources,
            &repeated,
            FundingOrderAmounts {
                gross: Amount::from_str("60.00").unwrap(),
                discount: Amount::from_str("0.00").unwrap(),
                paid: Amount::from_str("60.00").unwrap(),
            },
            &MallOrderFactId::new("fact-1"),
            Instant::from_unix_secs(1_700_000_000),
            &[
                MallConsumptionEntryId::new("e-1"),
                MallConsumptionEntryId::new("e-2")
            ],
            &HashMap::new(),
        )
        .is_err());
    }

    /// 已解析卡归属注入：来源映射的原销售单进入消费事实。
    #[test]
    fn build_injects_resolved_card_origins_into_entries() {
        let items = vec![item("item-1", "line-a", "60.00")];
        let sources = vec![source("source-1", 1, "60.00")];
        let requests = vec![request("a-1", "line-a", 1, "60.00")];
        let mut origins = HashMap::new();
        origins.insert(
            MallPaymentSourceId::new("source-1"),
            SalesOrderId::new("so-origin"),
        );
        let plan = FundingPlan::build(
            &items,
            &sources,
            &requests,
            FundingOrderAmounts {
                gross: Amount::from_str("60.00").unwrap(),
                discount: Amount::from_str("0.00").unwrap(),
                paid: Amount::from_str("60.00").unwrap(),
            },
            &MallOrderFactId::new("fact-1"),
            Instant::from_unix_secs(1_700_000_000),
            &[MallConsumptionEntryId::new("e-1")],
            &origins,
        )
        .unwrap();
        assert_eq!(
            plan.entries[0].origin_sales_order_id,
            Some(SalesOrderId::new("so-origin"))
        );
    }
}
