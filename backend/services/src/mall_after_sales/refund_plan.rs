//! 商城退款写入计划与原消费额度校验（INT-R11）。
//!
//! Repository 批量提供原消费事实与历史净额；Entity
//! [`ConsumptionRefundLimitPlan`] 聚合本请求 `APPLY`/`REVERSE` 并校验上限；
//! Service 保留行归属、跨聚合匹配，并以商城订单版本 CAS 串行化并发占用。

use database::{MallAfterSalesExt, NoTransaction};
use entities::common::time::Instant;
use entities::ids::{MallConsumptionEntryId, MallOrderFactId, MallRefundId, MallRefundLineId};
use entities::mall_after_sales::{
    AllocationAction, ConsumptionRefundLimitPlan, MallRefund, MallRefundAllocation, MallRefundAllocationData,
    MallRefundLine, MallRefundLineData, PendingConsumptionRefund,
};
use entities::mall_order::{
    AttributionStatus, ConsumptionDirection, MallConsumptionEntry, MallConsumptionEntryData,
};
use entities::money::Amount;
use id_generator::next_id;
use std::str::FromStr;

use super::dto::ReceiveRefundFactRequest;
use super::MallAfterSalesService;
use crate::errors::{Error, Result};

impl MallAfterSalesService {
    /// 构建退款写入计划（行/分配/消费反向事实），并做守恒与累计上限校验。
    ///
    /// # 参数
    /// * `req` - 退款事实接收请求
    /// * `order_items` - 原订单商品明细
    /// * `refund` - 已构造的退款头
    /// * `fact_id` - 事实 ID
    ///
    /// # 返回
    /// 返回 `(退款行, 退款分配, 消费反向事实)` 三元组。
    ///
    /// # 错误
    /// 行金额守恒、明细归属、原消费缺失或累计退款上限不满足时返回
    /// `BusinessLogicError`／实体 `LogicError`。
    ///
    /// # 约束
    /// 额度上限由 Entity 聚合校验；并发写偏斜由调用方在事务内对商城订单做 CAS。
    pub(super) async fn build_refund_plan(
        &self,
        req: &ReceiveRefundFactRequest,
        order_items: &[entities::mall_order::MallOrderItem],
        refund: &MallRefund,
        fact_id: &MallOrderFactId,
    ) -> Result<(
        Vec<MallRefundLine>,
        Vec<MallRefundAllocation>,
        Vec<MallConsumptionEntry>,
    )> {
        let occurred = Instant::from_unix_secs(req.occurred_at as i64);
        let refund_id = MallRefundId::new(refund.base.id.clone());
        let lines = self.build_refund_lines(req, order_items, refund, &refund_id)?;
        refund.ensure_line_total(&lines)?;
        self.ensure_item_cumulative_refund_limits(refund, order_items, &lines)
            .await?;

        let entry_ids: Vec<MallConsumptionEntryId> = req
            .allocations
            .iter()
            .map(|allocation| allocation.original_consumption_entry_id.clone())
            .collect();
        let scope = self
            .db
            .mall_after_sales()
            .consumption_refund_limit_scope(&entry_ids, &mut NoTransaction)
            .await?;
        let pending = self.pending_consumption_refunds(req)?;
        ConsumptionRefundLimitPlan::validate(&scope.entries, &scope.historical_nets, &pending)?;

        let mut allocations = Vec::with_capacity(req.allocations.len());
        let mut reversal_entries = Vec::with_capacity(req.allocations.len());
        for allocation in &req.allocations {
            let line = lines
                .iter()
                .find(|line| line.line_no == allocation.line_no)
                .ok_or_else(|| {
                    Error::BusinessLogicError(format!("分配引用的退款行不存在: {}", allocation.line_no))
                })?;
            let original_entry = scope
                .entries
                .get(&allocation.original_consumption_entry_id)
                .ok_or_else(|| Error::BusinessLogicError("原消费事实不存在".to_string()))?;
            if !original_entry
                .matches_refund_source(&line.mall_order_item_id, &allocation.original_payment_source_id)
            {
                return Err(Error::BusinessLogicError(
                    "退款分配必须引用原商品与原支付来源的消费事实".to_string(),
                ));
            }
            let amount = Amount::from_str(&allocation.allocated_refund_amount)?;
            let reversal = MallConsumptionEntry::new(
                MallConsumptionEntryId::new(next_id()),
                MallConsumptionEntryData {
                    mall_order_fact_id: fact_id.clone(),
                    mall_order_item_id: original_entry.mall_order_item_id.clone(),
                    mall_payment_source_id: original_entry.mall_payment_source_id.clone(),
                    direction: ConsumptionDirection::ConsumptionReversal,
                    amount,
                    customer_id: original_entry.customer_id.clone(),
                    origin_sales_order_id: original_entry.origin_sales_order_id.clone(),
                    sales_order_line_id: original_entry.sales_order_line_id.clone(),
                    occurred_at: occurred,
                    attribution_status: AttributionStatus::Attributed,
                    reverses_consumption_entry_id: Some(original_entry.base.id.clone().into()),
                },
            )?;
            allocations.push(MallRefundAllocation::new(
                entities::ids::MallRefundAllocationId::new(next_id()),
                MallRefundAllocationData {
                    mall_refund_line_id: line.base.id.clone().into(),
                    allocation_no: allocation.allocation_no,
                    original_consumption_entry_id: allocation.original_consumption_entry_id.clone(),
                    original_payment_source_id: allocation.original_payment_source_id.clone(),
                    allocated_refund_amount: amount,
                    allocation_action: AllocationAction::Apply,
                    reverses_allocation_id: None,
                    reversal_consumption_entry_id: Some(reversal.base.id.clone().into()),
                },
            )?);
            reversal_entries.push(reversal);
        }
        for line in &lines {
            line.ensure_allocation_total(&allocations)?;
        }
        Ok((lines, allocations, reversal_entries))
    }

    /// 校验商品行累计退款数量/金额上限（跨历史退款行）。
    ///
    /// # 参数
    /// * `refund` - 退款头
    /// * `order_items` - 原订单明细
    /// * `lines` - 本请求退款行
    ///
    /// # 返回
    /// 校验通过返回 `Ok(())`。
    ///
    /// # 错误
    /// 累计超过原支付数量或金额、退款行无对应原订单明细时返回 `BusinessLogicError`。
    async fn ensure_item_cumulative_refund_limits(
        &self,
        refund: &MallRefund,
        order_items: &[entities::mall_order::MallOrderItem],
        lines: &[MallRefundLine],
    ) -> Result<()> {
        let previous_lines = self
            .db
            .mall_after_sales()
            .list_refund_lines_by_order(&refund.mall_order_id, &mut NoTransaction)
            .await?;
        for line in lines {
            let item = find_order_item_for_line(order_items, line, &refund.mall_order_id)?;
            let refunded_amount = previous_lines
                .iter()
                .filter(|previous| previous.targets_item(&line.mall_order_item_id))
                .fold(line.line_refund_amount, |acc, previous| {
                    acc.checked_add(previous.line_refund_amount)
                });
            let refunded_quantity = previous_lines
                .iter()
                .filter(|previous| previous.targets_item(&line.mall_order_item_id))
                .fold(line.refunded_quantity.to_decimal(), |acc, previous| {
                    acc + previous.refunded_quantity.to_decimal()
                });
            let refunded_quantity = entities::money::Quantity::try_from(refunded_quantity)?;
            if !item.allows_cumulative_refund(refunded_quantity, refunded_amount) {
                return Err(Error::BusinessLogicError(format!(
                    "商品 {} 累计退款超过原支付数量或金额",
                    line.mall_order_item_id
                )));
            }
        }
        Ok(())
    }

    /// 构造退款行实体。
    ///
    /// # 参数
    /// * `req` - 退款请求
    /// * `order_items` - 原订单明细
    /// * `refund` - 退款头
    /// * `refund_id` - 退款头 ID
    ///
    /// # 返回
    /// 返回退款行列表。
    ///
    /// # 错误
    /// 明细不属于原订单或行构造失败时返回错误。
    fn build_refund_lines(
        &self,
        req: &ReceiveRefundFactRequest,
        order_items: &[entities::mall_order::MallOrderItem],
        refund: &MallRefund,
        refund_id: &MallRefundId,
    ) -> Result<Vec<MallRefundLine>> {
        let mut lines = Vec::with_capacity(req.lines.len());
        for line in &req.lines {
            let item_belongs_to_order = order_items.iter().any(|item| {
                item.base.id == line.mall_order_item_id.as_ref()
                    && item.belongs_to_order(&refund.mall_order_id)
            });
            if !item_belongs_to_order {
                return Err(Error::BusinessLogicError(format!(
                    "退款明细不属于原订单: {}",
                    line.mall_order_item_id
                )));
            }
            lines.push(MallRefundLine::new(
                MallRefundLineId::new(next_id()),
                MallRefundLineData {
                    mall_refund_id: refund_id.clone(),
                    line_no: line.line_no,
                    mall_order_item_id: line.mall_order_item_id.clone(),
                    refunded_quantity: entities::money::Quantity::from_str(&line.refunded_quantity)?,
                    line_refund_amount: Amount::from_str(&line.line_refund_amount)?,
                },
            )?);
        }
        Ok(lines)
    }

    /// 将请求分配转换为 Entity 额度行（当前接收路径全部为 `APPLY`）。
    ///
    /// # 参数
    /// * `req` - 退款请求
    ///
    /// # 返回
    /// 返回待校验额度行。
    ///
    /// # 错误
    /// 金额解析失败时返回错误。
    fn pending_consumption_refunds(
        &self,
        req: &ReceiveRefundFactRequest,
    ) -> Result<Vec<PendingConsumptionRefund>> {
        pending_consumption_refunds(req)
    }
}

/// 在事务会话下重读原消费额度范围并再次校验本请求净额。
///
/// # 参数
/// * `db` - 数据库
/// * `req` - 退款事实接收请求
/// * `executor` - 必须与写入位于同一事务会话
///
/// # 返回
/// 校验通过返回 `Ok(())`。
///
/// # 错误
/// 原消费缺失或累计超限时返回实体／业务错误。
///
/// # 约束
/// 必须在商城订单 CAS 成功之后调用，避免写偏斜窗口。
pub(super) async fn revalidate_consumption_refund_limits(
    db: &mongodb::Database,
    req: &ReceiveRefundFactRequest,
    executor: &mut dyn database::Executor,
) -> Result<()> {
    use database::MallAfterSalesExt;

    let entry_ids: Vec<MallConsumptionEntryId> = req
        .allocations
        .iter()
        .map(|allocation| allocation.original_consumption_entry_id.clone())
        .collect();
    let scope = db
        .mall_after_sales()
        .consumption_refund_limit_scope(&entry_ids, executor)
        .await?;
    let pending = pending_consumption_refunds(req)?;
    ConsumptionRefundLimitPlan::validate(&scope.entries, &scope.historical_nets, &pending)?;
    Ok(())
}

/// 将请求分配转换为 Entity 额度行（当前接收路径全部为 `APPLY`）。
///
/// # 参数
/// * `req` - 退款请求
///
/// # 返回
/// 返回待校验额度行。
///
/// # 错误
/// 金额解析失败时返回错误。
fn pending_consumption_refunds(req: &ReceiveRefundFactRequest) -> Result<Vec<PendingConsumptionRefund>> {
    req.allocations
        .iter()
        .map(|allocation| {
            Ok(PendingConsumptionRefund {
                original_consumption_entry_id: allocation.original_consumption_entry_id.clone(),
                amount: Amount::from_str(&allocation.allocated_refund_amount)?,
                action: AllocationAction::Apply,
            })
        })
        .collect()
}

/// 查找退款行对应的原订单明细。
///
/// # 参数
/// * `order_items` - 原订单商品明细快照
/// * `line` - 本请求退款行
/// * `mall_order_id` - 原商城订单
///
/// # 返回
/// 返回同时满足明细归属与订单归属的原订单明细引用。
///
/// # 错误
/// 无对应明细（缺失或跨订单引用）时返回 `BusinessLogicError`，永不 panic。
///
/// # 约束
/// 只做内存匹配，不访问仓储；跨聚合存在性仍由调用方保证。
fn find_order_item_for_line<'o>(
    order_items: &'o [entities::mall_order::MallOrderItem],
    line: &MallRefundLine,
    mall_order_id: &entities::ids::MallOrderId,
) -> Result<&'o entities::mall_order::MallOrderItem> {
    order_items
        .iter()
        .find(|item| {
            line.targets_item(&entities::ids::MallOrderItemId::new(item.base.id.clone()))
                && item.belongs_to_order(mall_order_id)
        })
        .ok_or_else(|| {
            Error::BusinessLogicError(format!("退款明细不属于原订单: {}", line.mall_order_item_id))
        })
}

#[cfg(test)]
mod tests {
    use super::find_order_item_for_line;
    use crate::errors::Error;
    use entities::ids::{MallOrderId, MallOrderItemId, MallRefundId, MallRefundLineId, SkuId};
    use entities::mall_after_sales::{MallRefundLine, MallRefundLineData};
    use entities::mall_order::{MallOrderItem, MallOrderItemData};
    use entities::money::{Amount, Quantity, Rate, UnitPrice};
    use std::str::FromStr;

    /// 构造归属 `order-1` 明细 `item-1` 的原订单明细。
    ///
    /// 复用实体行内恒等式（数量×单价=行总额，实付=总额-优惠+运费）；
    /// 不访问仓储或外部 I/O。
    fn order_item(order_id: &str, item_id: &str) -> MallOrderItem {
        MallOrderItem::new(
            MallOrderItemId::new(item_id),
            MallOrderItemData {
                mall_order_id: MallOrderId::new(order_id),
                external_item_id: "ext-1".to_string(),
                sku_id: Some(SkuId::new("sku-1")),
                product_publication_revision_id: None,
                supplier_offering_revision_id: None,
                name_snapshot: "测试商品".to_string(),
                spec_snapshot: None,
                quantity: Quantity::from_str("2.000000").unwrap(),
                unit_price_gross: UnitPrice::from_str("9.9900").unwrap(),
                line_gross_amount: Amount::from_str("19.98").unwrap(),
                allocated_discount_amount: Amount::from_str("0.98").unwrap(),
                allocated_freight_amount: Amount::from_str("1.00").unwrap(),
                paid_amount: Amount::from_str("20.00").unwrap(),
                sales_tax_rate: Rate::from_str("0.130000").unwrap(),
                unit_cost_snapshot: None,
                cost_snapshot_total: None,
                cost_tax_inclusion: None,
                cost_input_tax_rate: None,
            },
        )
        .unwrap()
    }

    /// 构造指向 `item-1` 的退款行。
    ///
    /// 行数量与金额为正以通过实体构造校验；不访问仓储或外部 I/O。
    fn refund_line() -> MallRefundLine {
        MallRefundLine::new(
            MallRefundLineId::new("rl-1"),
            MallRefundLineData {
                mall_refund_id: MallRefundId::new("refund-1"),
                line_no: 1,
                mall_order_item_id: MallOrderItemId::new("item-1"),
                refunded_quantity: Quantity::from_str("1.000000").unwrap(),
                line_refund_amount: Amount::from_str("10.00").unwrap(),
            },
        )
        .unwrap()
    }

    /// 命中明细时返回引用，不报错。
    #[test]
    fn find_order_item_returns_matching_item() {
        let items = vec![order_item("order-1", "item-1")];
        let line = refund_line();
        let found = find_order_item_for_line(&items, &line, &MallOrderId::new("order-1")).unwrap();
        assert_eq!(found.base.id, "item-1");
    }

    /// 缺失明细时返回 `BusinessLogicError` 而非 panic。
    ///
    /// 测试覆盖空快照与跨订单引用两种形态；任一形态不得 panic。
    #[test]
    fn find_order_item_rejects_missing_and_cross_order_without_panic() {
        let line = refund_line();
        let err =
            find_order_item_for_line(&[], &line, &MallOrderId::new("order-1")).expect_err("空快照必须拒绝");
        assert!(matches!(err, Error::BusinessLogicError(_)));

        let items = vec![order_item("order-2", "item-1")];
        let err = find_order_item_for_line(&items, &line, &MallOrderId::new("order-1"))
            .expect_err("跨订单引用必须拒绝");
        assert!(matches!(err, Error::BusinessLogicError(_)));
    }
}
