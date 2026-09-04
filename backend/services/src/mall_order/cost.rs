//! 消费成本评估编排。
//!
//! 比例分摊、尾差归尾、含税拆分与金额守恒由 [`CostSharePlan`] 拥有；本模块只
//! 将跨域事实转换为评估/`CostEntry`/`CostAllocation`。

use entities::common::time::Instant;
use entities::cost::{
    CostAllocation, CostAllocationData, CostBasis as CostBasisEntry, CostEntry, CostEntryData, CostScope,
    CostStage, CostType,
};
use entities::ids::{CostAllocationId, CostEntryId, MallConsumptionCostAssessmentId};
use entities::mall_order::{
    CostBasis, CostSharePlan, MallConsumptionCostAssessment, MallConsumptionCostAssessmentData,
    MallConsumptionEntry, MallItemFundingAllocation, MallOrderItem, MallPaymentSource, PaymentSourceType,
};
use entities::money::{Amount, Rate};
use id_generator::next_id;
use std::str::FromStr;

use super::MallOrderService;
use crate::errors::Result;

/// `ACTUAL` 成本评估的金额、税率与来源。
///
/// # 用途
/// 将分摊金额、进项税率与明细/分摊/消费事实打包。
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
/// 金额与税额必须已由调用方按支付比例舍入完成。
struct ActualAssessmentInput<'a> {
    /// 消费事实。
    entry: &'a MallConsumptionEntry,
    /// 分摊含税成本。
    gross: Amount,
    /// 分摊不含税成本。
    net: Amount,
    /// 分摊税额。
    tax: Amount,
    /// 进项税率（不含税成本时为空）。
    input_rate: Option<Rate>,
    /// 商品明细。
    item: &'a MallOrderItem,
    /// 分摊记录。
    allocation: &'a MallItemFundingAllocation,
}

impl MallOrderService {
    /// 构建消费成本评估（§8.4 第 7 条，P3 只落 `ACTUAL`/`NONE` 两级）。
    ///
    /// `ACTUAL`：明细商城成本快照完整时，由 [`CostSharePlan`] 完成比例分摊与
    /// 含税拆分；Service 仅注入 ID、成本范围并组装持久化实体。`NONE`：成本
    /// 数据不全（`STANDARD` 依赖 D24 供给版本查询，未授予 D29）。
    ///
    /// # 参数
    /// * `items` - 商品明细
    /// * `sources` - 支付来源
    /// * `allocations` - 分摊记录
    /// * `entries` - 消费事实（与分摊一一对应）
    /// * `occurred` - 事实发生时间
    /// * `assessed_by` - 评估人
    ///
    /// # 返回
    /// 返回 `(评估, 成本事实, 成本分配)` 三元组。
    ///
    /// # 错误
    /// 分摊计划或成本实体构造失败时返回领域/业务错误。
    pub(super) fn build_cost_assessments(
        &self,
        items: &[MallOrderItem],
        sources: &[MallPaymentSource],
        allocations: &[MallItemFundingAllocation],
        entries: &[MallConsumptionEntry],
        occurred: Instant,
        assessed_by: String,
    ) -> Result<(
        Vec<MallConsumptionCostAssessment>,
        Vec<CostEntry>,
        Vec<CostAllocation>,
    )> {
        let mut assessments = Vec::new();
        let mut cost_entries = Vec::new();
        let mut cost_allocations = Vec::new();
        for item in items {
            let mut item_allocations: Vec<&MallItemFundingAllocation> = allocations
                .iter()
                .filter(|allocation| allocation.mall_order_item_id.as_ref() == item.base.id)
                .collect();
            item_allocations.sort_by_key(|allocation| {
                sources
                    .iter()
                    .find(|source| source.base.id == allocation.mall_payment_source_id.as_ref())
                    .map(|source| source.source_no)
                    .unwrap_or_default()
            });
            let entry_of = |allocation: &MallItemFundingAllocation| -> Result<&MallConsumptionEntry> {
                entries
                    .iter()
                    .find(|entry| {
                        entry.mall_order_item_id.as_ref() == allocation.mall_order_item_id.as_ref()
                            && entry.mall_payment_source_id.as_ref()
                                == allocation.mall_payment_source_id.as_ref()
                    })
                    .ok_or_else(|| {
                        crate::errors::Error::BusinessLogicError("分摊与消费事实必须一一对应".to_string())
                    })
            };
            let source_of = |allocation: &MallItemFundingAllocation| -> Result<&MallPaymentSource> {
                sources
                    .iter()
                    .find(|source| source.base.id == allocation.mall_payment_source_id.as_ref())
                    .ok_or_else(|| crate::errors::Error::BusinessLogicError("分摊来源不存在".to_string()))
            };
            if !CostSharePlan::has_actual_cost(
                item.cost_snapshot_total,
                item.cost_tax_inclusion,
                item.cost_input_tax_rate,
            ) {
                for allocation in &item_allocations {
                    assessments.push(self.none_assessment(entry_of(allocation)?, occurred, &assessed_by)?);
                }
                continue;
            }
            let cost_total = item.cost_snapshot_total.ok_or_else(|| {
                crate::errors::Error::BusinessLogicError("ACTUAL 成本缺少成本合计".to_string())
            })?;
            let tax_inclusion = item.cost_tax_inclusion.unwrap_or(false);
            let payment_amounts: Vec<Amount> = item_allocations
                .iter()
                .map(|allocation| allocation.allocated_payment_amount)
                .collect();
            let plan = CostSharePlan::share(
                cost_total,
                item.paid_amount,
                &payment_amounts,
                tax_inclusion,
                item.cost_input_tax_rate,
            )?;
            for (allocation, leg) in item_allocations.iter().zip(plan.legs()) {
                let entry = entry_of(allocation)?;
                let assessment = self.actual_assessment(
                    ActualAssessmentInput {
                        entry,
                        gross: leg.gross_amount,
                        net: leg.net_amount,
                        tax: leg.tax_amount,
                        input_rate: leg.input_tax_rate,
                        item,
                        allocation,
                    },
                    occurred,
                    &assessed_by,
                )?;
                let input_tax_rate = match leg.input_tax_rate {
                    Some(rate) => rate,
                    None => Rate::from_str("0")
                        .map_err(|error| crate::errors::Error::BusinessLogicError(error.to_string()))?,
                };
                let cost_entry = CostEntry::new(
                    CostEntryId::new(next_id()),
                    CostEntryData {
                        cost_type: CostType::Product,
                        cost_stage: CostStage::Actual,
                        cost_scope: if source_of(allocation)?.source_type == PaymentSourceType::Card {
                            CostScope::MallConsumption
                        } else {
                            CostScope::WechatCost
                        },
                        cost_basis: Some(CostBasisEntry::Actual),
                        supplier_id: None,
                        gross_amount: leg.gross_amount,
                        net_amount: leg.net_amount,
                        tax_amount: leg.tax_amount,
                        tax_inclusion,
                        input_tax_rate,
                        occurred_at: occurred,
                        source_fact_type: "mall_consumption_entry".to_string(),
                        source_document_id: entry.base.id.clone(),
                        source_line_id: item.base.id.clone(),
                        source_version: "1".to_string(),
                        adjusts_cost_entry_id: None,
                        evidence_attachment_id: None,
                    },
                )?;
                let cost_allocation = CostAllocation::new(
                    CostAllocationId::new(next_id()),
                    CostAllocationData {
                        cost_entry_id: cost_entry.base.id.clone().into(),
                        sales_order_id: None,
                        sales_order_line_id: None,
                        mall_consumption_entry_id: Some(entry.base.id.clone().into()),
                        mall_payment_source_id: Some(allocation.mall_payment_source_id.clone()),
                        allocated_gross_amount: leg.gross_amount,
                        allocated_net_amount: leg.net_amount,
                        rounding_residual_flag: leg.rounding_residual_flag,
                    },
                )?;
                assessments.push(assessment);
                cost_entries.push(cost_entry);
                cost_allocations.push(cost_allocation);
            }
        }
        Ok((assessments, cost_entries, cost_allocations))
    }

    /// 构造 `NONE` 成本评估（无来源依据、金额与税字段）。
    ///
    /// # 参数
    /// * `entry` - 消费事实
    /// * `occurred` - 评估时间（= 事实发生时间）
    /// * `assessed_by` - 评估人
    ///
    /// # 返回
    /// 返回链首 `NONE` 评估。
    ///
    /// # 错误
    /// 评估实体构造失败时返回领域错误。
    fn none_assessment(
        &self,
        entry: &MallConsumptionEntry,
        occurred: Instant,
        assessed_by: &str,
    ) -> Result<MallConsumptionCostAssessment> {
        Ok(MallConsumptionCostAssessment::new(
            MallConsumptionCostAssessmentId::new(next_id()),
            MallConsumptionCostAssessmentData {
                mall_consumption_entry_id: entry.base.id.clone().into(),
                assessment_no: 1,
                cost_basis: CostBasis::None,
                basis_source_type: None,
                basis_source_id: None,
                basis_source_line_id: None,
                basis_source_version: None,
                source_snapshot_hash: None,
                gross_amount: None,
                net_amount: None,
                tax_amount: None,
                tax_inclusion: None,
                input_tax_rate: None,
                delta_cost_entry_id: None,
                supersedes_assessment_id: None,
                assessed_at: occurred,
                assessed_by: assessed_by.to_string(),
            },
        )?)
    }

    /// 构造 `ACTUAL` 成本评估（商城成本快照来源，§12.1 第 5 项）。
    ///
    /// # 用途
    /// 按明细与分摊金额生成链首 `ACTUAL` 评估。
    ///
    /// # 参数
    /// * `input` - 消费事实、分摊金额与明细来源
    /// * `occurred` - 评估时间
    /// * `assessed_by` - 评估人
    ///
    /// # 返回
    /// 返回链首 `ACTUAL` 评估。
    ///
    /// # 错误
    /// 评估实体构造失败时返回领域错误。
    ///
    /// # 关键业务约束
    /// 来源固定为商城成本快照；不含税成本不写进项税率。
    fn actual_assessment(
        &self,
        input: ActualAssessmentInput<'_>,
        occurred: Instant,
        assessed_by: &str,
    ) -> Result<MallConsumptionCostAssessment> {
        let ActualAssessmentInput {
            entry,
            gross,
            net,
            tax,
            input_rate,
            item,
            allocation,
        } = input;
        Ok(MallConsumptionCostAssessment::new(
            MallConsumptionCostAssessmentId::new(next_id()),
            MallConsumptionCostAssessmentData {
                mall_consumption_entry_id: entry.base.id.clone().into(),
                assessment_no: 1,
                cost_basis: CostBasis::Actual,
                basis_source_type: Some(entities::mall_order::CostBasisSourceType::MallCostSnapshot),
                basis_source_id: Some(item.base.id.clone()),
                basis_source_line_id: Some(allocation.mall_payment_source_id.to_string()),
                basis_source_version: Some("1".to_string()),
                source_snapshot_hash: Some(format!(
                    "mall_item:{}:{}",
                    item.base.id,
                    item.cost_snapshot_total
                        .map(|amount| amount.to_string())
                        .unwrap_or_default()
                )),
                gross_amount: Some(gross),
                net_amount: Some(net),
                tax_amount: Some(tax),
                tax_inclusion: Some(item.cost_tax_inclusion.unwrap_or(false)),
                input_tax_rate: input_rate,
                delta_cost_entry_id: None,
                supersedes_assessment_id: None,
                assessed_at: occurred,
                assessed_by: assessed_by.to_string(),
            },
        )?)
    }
}
