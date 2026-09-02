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
    /// 累计超过原支付数量或金额时返回 `BusinessLogicError`。
    async fn ensure_item_cumulative_refund_limits(
        &self,
        refund: &MallRefund,
        order_items: &[entities::mall_order::MallOrderItem],
        lines: &[MallRefundLine],
    ) -> Result<()> {
        let previous_refunds = self
            .db
            .mall_refunds()
            .list_by_order(&refund.mall_order_id, &mut NoTransaction)
            .await?;
        let previous_ids: Vec<entities::ids::MallRefundId> = previous_refunds
            .iter()
            .map(|refund| refund.base.id.clone().into())
            .collect();
        let previous_lines = self
            .db
            .mall_refund_lines()
            .list_by_refunds(&previous_ids, &mut NoTransaction)
            .await?;
        for line in lines {
            let item = order_items
                .iter()
                .find(|item| {
                    line.targets_item(&entities::ids::MallOrderItemId::new(item.base.id.clone()))
                        && item.belongs_to_order(&refund.mall_order_id)
                })
                .expect("退款行实体已确认明细属于原订单");
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
