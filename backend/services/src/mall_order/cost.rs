//! 消费成本分摊计算与事实/分录/评估加载器。

use database::{MallOrderExt, NoTransaction};
use entities::common::time::Instant;
use entities::cost::{
    CostAllocation, CostAllocationData, CostBasis as CostBasisEntry, CostEntry, CostEntryData, CostScope,
    CostStage, CostType,
};
use entities::ids::{CostAllocationId, CostEntryId, MallConsumptionCostAssessmentId};
use entities::mall_order::{
    CostBasis, MallConsumptionCostAssessment, MallConsumptionCostAssessmentData, MallConsumptionEntry,
    MallItemFundingAllocation, MallOrderFact, MallOrderItem, MallPaymentSource, PaymentSourceType,
};
use entities::money::{round_to_cent, Amount, Rate};
use id_generator::next_id;
use std::str::FromStr;

use super::query::{MallOrderFactFilter, OrderFactMap};
use super::MallOrderService;
use crate::errors::Result;

impl MallOrderService {
    /// 构建消费成本评估（§8.4 第 7 条，P3 只落 `ACTUAL`/`NONE` 两级）。
    ///
    /// `ACTUAL`：明细商城成本快照含完整税额标识与进项税率；按支付来源金额
    /// 比例分摊，尾差计入最后一个来源。`NONE`：成本数据不全（`STANDARD`
    /// 依赖 D24 供给版本查询，未授予 D29，属闭环缺口）。
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
    pub(super) fn build_cost_assessments(
        &self,
        items: &[MallOrderItem],
        sources: &[MallPaymentSource],
        allocations: &[MallItemFundingAllocation],
        entries: &[MallConsumptionEntry],
        occurred: Instant,
        assessed_by: String,
    ) -> (
        Vec<MallConsumptionCostAssessment>,
        Vec<CostEntry>,
        Vec<CostAllocation>,
    ) {
        let mut assessments = Vec::new();
        let mut cost_entries = Vec::new();
        let mut cost_allocations = Vec::new();
        for item in items {
            // 同明细的分摊按来源序号稳定排序，成本尾差计入最后一个来源。
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
            let entry_of = |allocation: &MallItemFundingAllocation| -> &MallConsumptionEntry {
                entries
                    .iter()
                    .find(|entry| {
                        entry.mall_order_item_id.as_ref() == allocation.mall_order_item_id.as_ref()
                            && entry.mall_payment_source_id.as_ref()
                                == allocation.mall_payment_source_id.as_ref()
                    })
                    .expect("分摊与消费事实一一对应")
            };
            let source_of = |allocation: &MallItemFundingAllocation| -> &MallPaymentSource {
                sources
                    .iter()
                    .find(|source| source.base.id == allocation.mall_payment_source_id.as_ref())
                    .expect("分摊来源已校验存在")
            };
            let has_actual = item.cost_snapshot_total.is_some()
                && item.cost_tax_inclusion.is_some()
                && (!item.cost_tax_inclusion.unwrap_or(false) || item.cost_input_tax_rate.is_some());
            if !has_actual {
                for allocation in &item_allocations {
                    assessments.push(self.none_assessment(entry_of(allocation), occurred, &assessed_by));
                }
                continue;
            }
            let cost_total = item.cost_snapshot_total.expect("已校验存在");
            let paid = item.paid_amount;
            let mut accrued = Amount::from_str("0.00").expect("零常量可解析");
            let count = item_allocations.len();
            for (index, allocation) in item_allocations.iter().enumerate() {
                let entry = entry_of(allocation);
                let is_last = index + 1 == count;
                let gross = if is_last {
                    cost_total.checked_sub(accrued)
                } else {
                    let share = round_to_cent(
                        cost_total.to_decimal() * allocation.allocated_payment_amount.to_decimal()
                            / paid.to_decimal(),
                    );
                    Amount::try_from(share).expect("舍入后金额合法")
                };
                accrued = accrued.checked_add(gross);
                let (net, tax, input_rate) = match item.cost_tax_inclusion {
                    Some(true) => {
                        let rate = item.cost_input_tax_rate.expect("含税成本已校验税率");
                        let tax = Amount::try_from(round_to_cent(gross.to_decimal() * rate.to_decimal()))
                            .expect("舍入后金额合法");
                        (gross.checked_sub(tax), tax, Some(rate))
                    }
                    _ => (gross, Amount::from_str("0.00").expect("零常量可解析"), None),
                };
                let assessment = self.actual_assessment(
                    entry,
                    gross,
                    net,
                    tax,
                    input_rate,
                    item,
                    allocation,
                    occurred,
                    &assessed_by,
                );
                let cost_entry = CostEntry::new(
                    CostEntryId::new(next_id()),
                    CostEntryData {
                        cost_type: CostType::Product,
                        cost_stage: CostStage::Actual,
                        cost_scope: if source_of(allocation).source_type == PaymentSourceType::Card {
                            CostScope::MallConsumption
                        } else {
                            CostScope::WechatCost
                        },
                        cost_basis: Some(CostBasisEntry::Actual),
                        supplier_id: None,
                        gross_amount: gross,
                        net_amount: net,
                        tax_amount: tax,
                        tax_inclusion: item.cost_tax_inclusion.unwrap_or(false),
                        input_tax_rate: input_rate
                            .unwrap_or_else(|| Rate::from_str("0").expect("税率可解析")),
                        occurred_at: occurred,
                        source_fact_type: "mall_consumption_entry".to_string(),
                        source_document_id: entry.base.id.clone(),
                        source_line_id: item.base.id.clone(),
                        source_version: "1".to_string(),
                        adjusts_cost_entry_id: None,
                        evidence_attachment_id: None,
                    },
                )
                .expect("成本事实内容已校验");
                let cost_allocation = CostAllocation::new(
                    CostAllocationId::new(next_id()),
                    CostAllocationData {
                        cost_entry_id: cost_entry.base.id.clone().into(),
                        sales_order_id: None,
                        sales_order_line_id: None,
                        mall_consumption_entry_id: Some(entry.base.id.clone().into()),
                        mall_payment_source_id: Some(allocation.mall_payment_source_id.clone()),
                        allocated_gross_amount: gross,
                        allocated_net_amount: net,
                        rounding_residual_flag: is_last,
                    },
                )
                .expect("成本分配内容已校验");
                assessments.push(assessment);
                cost_entries.push(cost_entry);
                cost_allocations.push(cost_allocation);
            }
        }
        (assessments, cost_entries, cost_allocations)
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
    fn none_assessment(
        &self,
        entry: &MallConsumptionEntry,
        occurred: Instant,
        assessed_by: &str,
    ) -> MallConsumptionCostAssessment {
        MallConsumptionCostAssessment::new(
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
        )
        .expect("NONE 评估内容已校验")
    }

    /// 构造 `ACTUAL` 成本评估（商城成本快照来源，§12.1 第 5 项）。
    ///
    /// # 参数
    /// * `entry` - 消费事实
    /// * `gross` - 分摊含税成本
    /// * `net` - 分摊不含税成本
    /// * `tax` - 分摊税额
    /// * `input_rate` - 进项税率（不含税成本时为空）
    /// * `item` - 商品明细
    /// * `allocation` - 分摊记录
    /// * `occurred` - 评估时间
    /// * `assessed_by` - 评估人
    ///
    /// # 返回
    /// 返回链首 `ACTUAL` 评估。
    #[allow(clippy::too_many_arguments)]
    fn actual_assessment(
        &self,
        entry: &MallConsumptionEntry,
        gross: Amount,
        net: Amount,
        tax: Amount,
        input_rate: Option<Rate>,
        item: &MallOrderItem,
        allocation: &MallItemFundingAllocation,
        occurred: Instant,
        assessed_by: &str,
    ) -> MallConsumptionCostAssessment {
        MallConsumptionCostAssessment::new(
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
        )
        .expect("ACTUAL 评估内容已校验")
    }

    /// 分页加载指定商城的全部关键事实并按（商城, 订单号）分组。
    ///
    /// # 参数
    /// * `mall_id` - 商城筛选（`None` 表示全部商城）
    ///
    /// # 返回
    /// 返回 `(mall_id, external_order_no)` → 事实摘要 `(类型, 发生时间, 来源)` 映射。
    ///
    /// # 错误
    /// 数据库查询失败时返回 `RepositoryError`。
    pub(super) async fn facts_grouped_by_order(&self, mall_id: &Option<String>) -> Result<OrderFactMap> {
        let mut grouped = std::collections::HashMap::new();
        let mut page = 1u64;
        loop {
            let filter = MallOrderFactFilter {
                mall_id: mall_id.clone(),
                fact_type: None,
                processing_status: None,
                after_sales_request_id: None,
                page,
                page_size: 100,
                sort_by: Some("occurred_at".to_string()),
                sort_ascending: true,
            };
            let result = self
                .db
                .mall_order_facts()
                .search_facts(&filter, &mut NoTransaction)
                .await?;
            if result.items.is_empty() {
                break;
            }
            for row in result.items {
                grouped
                    .entry((row.mall_id.clone(), row.external_order_no.clone()))
                    .or_insert_with(Vec::new)
                    .push((row.fact_type, row.occurred_at, row.data_source));
            }
            if (result.total as u64) <= page * 100 {
                break;
            }
            page += 1;
        }
        Ok(grouped)
    }

    /// 加载指定（商城, 订单号）的全部关键事实实体（按发生时间升序）。
    ///
    /// # 参数
    /// * `mall_id` - 商城
    /// * `external_order_no` - 商城订单号
    ///
    /// # 返回
    /// 返回按发生时间升序的事实实体。
    ///
    /// # 错误
    /// 数据库查询失败时返回 `RepositoryError`。
    pub(super) async fn load_facts_for_order(
        &self,
        mall_id: &str,
        external_order_no: &str,
    ) -> Result<Vec<MallOrderFact>> {
        let mut facts = Vec::new();
        let mut page = 1u64;
        loop {
            let filter = MallOrderFactFilter {
                mall_id: Some(mall_id.to_string()),
                fact_type: None,
                processing_status: None,
                after_sales_request_id: None,
                page,
                page_size: 100,
                sort_by: Some("occurred_at".to_string()),
                sort_ascending: true,
            };
            let result = self
                .db
                .mall_order_facts()
                .search_facts(&filter, &mut NoTransaction)
                .await?;
            let mut hit = false;
            for row in result.items {
                if row.external_order_no != external_order_no {
                    continue;
                }
                hit = true;
                if let Some(fact) = self
                    .db
                    .mall_order_facts()
                    .find_by_id(&row.id, &mut NoTransaction)
                    .await?
                {
                    facts.push(fact);
                }
            }
            if !hit || (result.total as u64) <= page * 100 {
                break;
            }
            page += 1;
        }
        facts.sort_by_key(|fact| (fact.occurred_at, fact.base.id.clone()));
        Ok(facts)
    }

    /// 沿支付来源加载消费事实（去重后按发生时间升序）。
    ///
    /// # 参数
    /// * `sources` - 支付来源
    ///
    /// # 返回
    /// 返回消费事实列表。
    ///
    /// # 错误
    /// 数据库查询失败时返回 `RepositoryError`。
    pub(super) async fn load_entries_for_sources(
        &self,
        sources: &[MallPaymentSource],
    ) -> Result<Vec<MallConsumptionEntry>> {
        let mut entries = Vec::new();
        let mut seen = std::collections::HashSet::new();
        for source in sources {
            for entry in self
                .db
                .mall_consumption_entries()
                .list_by_original_payment_source(&source.base.id.clone().into(), &mut NoTransaction)
                .await?
            {
                if seen.insert(entry.base.id.clone()) {
                    entries.push(entry);
                }
            }
        }
        entries.sort_by_key(|entry| (entry.occurred_at, entry.base.id.clone()));
        Ok(entries)
    }

    /// 取每条消费的当前成本评估（链尾，即最大评估号）。
    ///
    /// # 参数
    /// * `entries` - 消费事实
    ///
    /// # 返回
    /// 返回 `消费ID → 当前评估` 映射。
    ///
    /// # 错误
    /// 数据库查询失败时返回 `RepositoryError`。
    pub(super) async fn load_current_assessments(
        &self,
        entries: &[MallConsumptionEntry],
    ) -> Result<std::collections::HashMap<String, MallConsumptionCostAssessment>> {
        let mut current = std::collections::HashMap::new();
        for entry in entries {
            let chain = self
                .db
                .mall_consumption_cost_assessments()
                .list_by_entry(&entry.base.id.clone().into(), &mut NoTransaction)
                .await?;
            if let Some(tail) = chain
                .into_iter()
                .max_by_key(|assessment| assessment.assessment_no)
            {
                current.insert(entry.base.id.clone(), tail);
            }
        }
        Ok(current)
    }
}
