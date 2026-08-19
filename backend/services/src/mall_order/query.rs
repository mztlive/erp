//! 商城订单与关键事实查询用例：列表、详情与视图装配。

use database::{CardInstanceExt, MallOrderExt, NoTransaction};
use entities::card_instance::{MallCardInstance, MallConsumptionCutover};
use entities::common::time::Instant;
use entities::ids::{MallOrderId, MallOrderItemId};
use entities::mall_order::{
    AttributionStatus, CostBasis, DataSource, FactType, FulfillmentChain, MallConsumptionCostAssessment,
    MallConsumptionEntry, MallItemFundingAllocation, MallOrder, MallOrderFact, MallOrderItem,
    MallPaymentSource, PaymentSourceType,
};
use entities::money::Amount;
use std::str::FromStr;
use validator::Validate;

use super::dto;
use super::dto::{
    ConservationResultRow, ConservationView, ConsumptionEntryView, CostAssessmentView,
    CostBasisBreakdownItemView, FactSummaryItemView, FundingAllocationView, MallOrderAddressView,
    MallOrderAmountsView, MallOrderCustomerView, MallOrderDetailView, MallOrderFactListParams,
    MallOrderFactView, MallOrderFulfillmentView, MallOrderIdentityView, MallOrderItemView,
    MallOrderListParams, MallOrderListRow, PageView, PaymentCompositionView, PaymentSourceView,
    SupplierOrderSummaryView,
};
use super::MallOrderService;
use crate::errors::{Error, Result};

/// 商城订单列表筛选条件类型（经 `MallOrderExt` 关联类型跨 crate 可达）。
pub(super) type MallOrderFilter = <mongodb::Database as MallOrderExt>::MallOrderFilter;
/// 关键事实列表筛选条件类型。
pub(super) type MallOrderFactFilter = <mongodb::Database as MallOrderExt>::MallOrderFactFilter;

/// 订单详情装配所需的订单图集合。
///
/// # 用途
/// 将订单、明细、支付、事实、消费与评估一次性传入详情投影。
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
/// 评估映射按消费事实 ID 对齐当前评估。
struct MallOrderDetailGraph {
    /// 订单实体。
    order: MallOrder,
    /// 商品明细。
    items: Vec<MallOrderItem>,
    /// 支付来源。
    sources: Vec<MallPaymentSource>,
    /// 分摊记录。
    allocations: Vec<MallItemFundingAllocation>,
    /// 关键事实。
    facts: Vec<MallOrderFact>,
    /// 消费事实。
    entries: Vec<MallConsumptionEntry>,
    /// 消费 → 当前评估映射。
    assessments: std::collections::HashMap<String, MallConsumptionCostAssessment>,
    /// 该商城已启用的切换记录。
    cutover: Option<MallConsumptionCutover>,
}

impl MallOrderService {
    /// 分页查询商城订单列表（W25 列表页）。
    ///
    /// # 参数
    /// * `params` - 查询参数（`q`/`mall_id`/`external_order_no`/`customer_id`/
    ///   `fulfillment_chain`/`attribution_status`/`paid_at_from`/`paid_at_to` 扁平筛选）
    ///
    /// # 返回
    /// 返回契约形状的分页视图。
    ///
    /// # 错误
    /// * `ValidationError` - 分页参数非法或排序字段不在白名单
    /// * `RepositoryError` - 数据库查询失败
    pub async fn mall_order_list(&self, params: &MallOrderListParams) -> Result<PageView<MallOrderListRow>> {
        params.validate()?;
        let query = params.normalized()?;
        let filter = MallOrderFilter {
            mall_id: query.mall_id,
            external_order_no: query.external_order_no,
            customer_id: query
                .customer_id
                .as_deref()
                .map(entities::ids::CustomerAccountId::new),
            fulfillment_chain: query.fulfillment_chain,
            attribution_status: query.attribution_status,
            paid_at_from: query
                .paid_at_from
                .map(|secs| Instant::from_unix_secs(secs as i64)),
            paid_at_to: query.paid_at_to.map(|secs| Instant::from_unix_secs(secs as i64)),
            page: query.paging.page,
            page_size: query.paging.page_size,
            sort_by: Some(query.paging.sort_by.to_string()),
            sort_ascending: matches!(query.paging.sort_dir, dto::SortDir::Asc),
        };
        let page = self
            .db
            .mall_orders()
            .search_orders(&filter, &mut NoTransaction)
            .await?;
        // 行级聚合字段（事实摘要/支付构成/成本分项）按页内订单批量补齐：
        // 事实按（商城, 订单号）分组、支付来源按订单、消费事实沿支付来源取。
        let fact_map = self.facts_grouped_by_order(&filter.mall_id).await?;
        let mut rows = Vec::with_capacity(page.items.len());
        for row in page.items {
            rows.push(
                self.build_list_row(
                    OrderListRow {
                        id: row.id,
                        mall_id: row.mall_id,
                        external_order_no: row.external_order_no,
                        customer_id: row.customer_id,
                        paid_at: row.paid_at,
                        paid_amount: row.paid_amount,
                        fulfillment_chain: row.fulfillment_chain,
                        attribution_status: row.attribution_status,
                    },
                    &fact_map,
                )
                .await?,
            );
        }
        Ok(PageView {
            items: rows,
            total: page.total,
            page: filter.page,
            page_size: filter.page_size,
        })
    }

    /// 查询商城订单详情（W25 对象中心）。
    ///
    /// # 参数
    /// * `id` - 商城订单 ID
    ///
    /// # 返回
    /// 返回订单详情视图（事实/明细/支付来源/分摊/守恒/消费/成本）。
    ///
    /// # 错误
    /// * `NotFound` - 订单不存在
    /// * `RepositoryError` - 数据库查询失败
    pub async fn mall_order_detail(&self, id: &str) -> Result<MallOrderDetailView> {
        let order = self
            .db
            .mall_orders()
            .find_by_id(id, &mut NoTransaction)
            .await?
            .ok_or_else(|| Error::NotFound("商城订单不存在".to_string()))?;
        let order_id: MallOrderId = order.base.id.clone().into();
        let items = self
            .db
            .mall_order_items()
            .list_items_by_order(&order_id, &mut NoTransaction)
            .await?;
        let sources = self
            .db
            .mall_payment_sources()
            .list_by_order(&order_id, &mut NoTransaction)
            .await?;
        let item_ids: Vec<MallOrderItemId> = items.iter().map(|item| item.base.id.clone().into()).collect();
        let allocations = self
            .db
            .mall_item_funding_allocations()
            .list_by_items(&item_ids, &mut NoTransaction)
            .await?;
        let facts = self
            .load_facts_for_order(&order.mall_id, &order.external_order_no)
            .await?;
        let entries = self.load_entries_for_sources(&sources).await?;
        let assessments = self.load_current_assessments(&entries).await?;
        let cutover = self
            .db
            .mall_consumption_cutovers()
            .find_enabled_cutover_by_mall_id(&order.mall_id, &mut NoTransaction)
            .await?;

        Ok(self.build_detail_view(MallOrderDetailGraph {
            order,
            items,
            sources,
            allocations,
            facts,
            entries,
            assessments,
            cutover,
        }))
    }

    /// 分页查询关键事实列表。
    ///
    /// # 参数
    /// * `params` - 查询参数（`mall_id`/`fact_type`/`processing_status`/
    ///   `after_sales_request_id` 扁平筛选）
    ///
    /// # 返回
    /// 返回契约形状的分页视图。
    ///
    /// # 错误
    /// * `ValidationError` - 分页参数非法或排序字段不在白名单
    /// * `RepositoryError` - 数据库查询失败
    pub async fn mall_order_fact_list(
        &self,
        params: &MallOrderFactListParams,
    ) -> Result<PageView<MallOrderFactView>> {
        params.validate()?;
        let query = params.normalized()?;
        let filter = MallOrderFactFilter {
            mall_id: query.mall_id,
            fact_type: query.fact_type,
            processing_status: query.processing_status,
            after_sales_request_id: query.after_sales_request_id,
            page: query.paging.page,
            page_size: query.paging.page_size,
            sort_by: Some(query.paging.sort_by.to_string()),
            sort_ascending: matches!(query.paging.sort_dir, dto::SortDir::Asc),
        };
        let page = self
            .db
            .mall_order_facts()
            .search_facts(&filter, &mut NoTransaction)
            .await?;
        // 投影行不含扩展字段（售后请求/原支付），逐条加载完整事实后映射视图。
        let mut items = Vec::with_capacity(page.items.len());
        for row in page.items {
            if let Some(fact) = self
                .db
                .mall_order_facts()
                .find_by_id(&row.id, &mut NoTransaction)
                .await?
            {
                items.push(fact_view(&fact));
            }
        }
        Ok(PageView {
            items,
            total: page.total,
            page: filter.page,
            page_size: filter.page_size,
        })
    }
}

impl MallOrderService {
    /// 构建列表行视图（事实摘要/支付构成/成本分项聚合）。
    ///
    /// # 参数
    /// * `row` - 订单投影行（已按列表投影字段提取）
    /// * `fact_map` - （商城, 订单号）→ 事实摘要映射
    ///
    /// # 返回
    /// 返回列表行视图。
    ///
    /// # 错误
    /// 数据库查询失败时返回 `RepositoryError`。
    async fn build_list_row(&self, row: OrderListRow, fact_map: &OrderFactMap) -> Result<MallOrderListRow> {
        let order_id: MallOrderId = row.id.clone().into();
        let sources = self
            .db
            .mall_payment_sources()
            .list_by_order(&order_id, &mut NoTransaction)
            .await?;
        let entries = self.load_entries_for_sources(&sources).await?;
        let assessments = self.load_current_assessments(&entries).await?;
        let facts = fact_map
            .get(&(row.mall_id.clone(), row.external_order_no.clone()))
            .cloned()
            .unwrap_or_default();
        let facts = facts
            .into_iter()
            .map(|(fact_type, occurred_at, data_source)| OrderFactSummary {
                fact_type,
                occurred_at,
                data_source,
            })
            .collect::<Vec<_>>();

        let card_amount = sources
            .iter()
            .filter(|source| source.source_type == PaymentSourceType::Card)
            .fold(Amount::from_str("0.00")?, |acc, source| {
                acc.checked_add(source.amount)
            });
        let wechat_amount = sources
            .iter()
            .filter(|source| source.source_type == PaymentSourceType::Wechat)
            .fold(Amount::from_str("0.00")?, |acc, source| {
                acc.checked_add(source.amount)
            });
        let mut fact_summary = Vec::new();
        for fact_type in [
            FactType::PaymentSucceeded,
            FactType::OrderCanceled,
            FactType::RefundSucceeded,
            FactType::OrderCompleted,
            FactType::CardBalanceRestored,
        ] {
            let matched: Vec<_> = facts.iter().filter(|fact| fact.fact_type == fact_type).collect();
            if !matched.is_empty() {
                fact_summary.push(FactSummaryItemView {
                    fact_type,
                    latest_occurred_at: matched
                        .iter()
                        .map(|fact| fact.occurred_at.unix_secs())
                        .max()
                        .unwrap_or_default() as u64,
                    count: matched.len() as u64,
                });
            }
        }
        let data_source = facts
            .iter()
            .max_by_key(|fact| fact.occurred_at)
            .map(|fact| fact.data_source)
            .unwrap_or(DataSource::Realtime);

        let mut breakdown: Vec<CostBasisBreakdownItemView> = Vec::new();
        let mut distinct_bases: Vec<CostBasis> = Vec::new();
        for assessment in assessments.values() {
            if !distinct_bases.contains(&assessment.cost_basis) {
                distinct_bases.push(assessment.cost_basis);
            }
            let bucket = breakdown
                .iter_mut()
                .find(|item| item.basis == assessment.cost_basis);
            match bucket {
                Some(item) => {
                    item.line_count += 1;
                    if let Some(cost) = assessment.cost_amount_string() {
                        let current = item
                            .cost_amount
                            .as_deref()
                            .map(Amount::from_str)
                            .transpose()
                            .ok()
                            .flatten()
                            .unwrap_or_else(|| Amount::from_str("0.00").expect("零常量可解析"));
                        item.cost_amount = Some(current.checked_add(cost).to_string());
                    }
                }
                None => breakdown.push(CostBasisBreakdownItemView {
                    basis: assessment.cost_basis,
                    line_count: 1,
                    cost_amount: assessment.cost_amount_string().map(|amount| amount.to_string()),
                }),
            }
        }
        let normalized_cost_basis = match distinct_bases.len() {
            0 => None,
            1 => distinct_bases
                .into_iter()
                .next()
                .map(|basis| basis.as_str().to_string()),
            _ => Some("MIXED".to_string()),
        };

        let customer_id_label = row.customer_id.map(|id| id.to_string());
        Ok(MallOrderListRow {
            mall_order_id: row.id,
            mall_id: row.mall_id.clone(),
            mall_name: row.mall_id,
            external_order_no: row.external_order_no,
            customer_id: customer_id_label.clone(),
            customer_label: customer_id_label,
            paid_at: row.paid_at.unix_secs() as u64,
            paid_amount: row.paid_amount.to_string(),
            payment_composition: PaymentCompositionView {
                card_amount: card_amount.to_string(),
                wechat_amount: wechat_amount.to_string(),
                source_count: sources.len() as u32,
            },
            fact_summary,
            fulfillment_chain: row.fulfillment_chain,
            supplier_order_summary: SupplierOrderSummaryView {
                total: 0,
                statuses: Vec::new(),
                has_exception: false,
            },
            attribution_status: row.attribution_status,
            cost_basis_breakdown: breakdown,
            data_source,
            allowed_actions: Vec::new(),
            action_blockers: Vec::new(),
            cost_basis_policy_state: "CONFIGURED".to_string(),
            normalized_cost_basis,
        })
    }

    /// 组装订单详情视图（W25 §8.2 对象中心）。
    ///
    /// # 用途
    /// 将订单图集合投影为详情视图。
    ///
    /// # 参数
    /// * `graph` - 订单、明细、支付、事实与评估集合
    ///
    /// # 返回
    /// 返回订单详情视图。
    ///
    /// # 错误
    /// 无
    ///
    /// # 关键业务约束
    /// 支付来源任一待归属时明细归属状态为待归属。
    fn build_detail_view(&self, graph: MallOrderDetailGraph) -> MallOrderDetailView {
        let MallOrderDetailGraph {
            order,
            items,
            sources,
            allocations,
            facts,
            entries,
            assessments,
            cutover,
        } = graph;
        let source_views: Vec<PaymentSourceView> = sources
            .iter()
            .map(|source| PaymentSourceView {
                payment_source_id: source.base.id.clone(),
                source_no: source.source_no,
                source_type: source.source_type,
                amount: source.amount.to_string(),
                source_reference: source
                    .source_card_instance_ref
                    .clone()
                    .or_else(|| source.wechat_payment_ref.clone())
                    .map(|reference| mask_reference(&reference))
                    .unwrap_or_else(|| "已加密存储".to_string()),
                mall_card_instance_id: source.mall_card_instance_id.as_ref().map(|id| id.to_string()),
                attribution_status: source.attribution_status,
                origin: None,
            })
            .collect();
        let conservation = self.build_conservation(&order, &items, &sources, &allocations);
        let item_attribution = if sources
            .iter()
            .any(|source| source.attribution_status == AttributionStatus::PendingAttribution)
        {
            AttributionStatus::PendingAttribution
        } else {
            AttributionStatus::Attributed
        };
        let entry_views = entries
            .iter()
            .map(|entry| ConsumptionEntryView {
                consumption_entry_id: entry.base.id.clone(),
                fact_id: entry.mall_order_fact_id.to_string(),
                item_id: entry.mall_order_item_id.to_string(),
                payment_source_id: entry.mall_payment_source_id.to_string(),
                direction: entry.direction,
                amount: entry.amount.to_string(),
                occurred_at: entry.occurred_at.unix_secs() as u64,
                attribution_status: entry.attribution_status,
                origin_sales_order_id: entry.origin_sales_order_id.as_ref().map(|id| id.to_string()),
                reverses_consumption_entry_id: entry
                    .reverses_consumption_entry_id
                    .as_ref()
                    .map(|id| id.to_string()),
                current_cost_assessment: assessments.get(&entry.base.id).map(cost_assessment_view),
            })
            .collect();

        MallOrderDetailView {
            identity: MallOrderIdentityView {
                mall_order_id: order.base.id.clone(),
                mall_id: order.mall_id.clone(),
                mall_name: order.mall_id.clone(),
                external_order_no: order.external_order_no.clone(),
                payment_fact_id: order.payment_fact_id.to_string(),
            },
            customer: MallOrderCustomerView {
                source_customer_ref: order.source_customer_ref.clone(),
                customer_id: order.customer_id.as_ref().map(|id| id.to_string()),
                customer_label: order.customer_id.as_ref().map(|id| id.to_string()),
                attribution_status: order.attribution_status,
            },
            ordered_at: order.ordered_at.unix_secs() as u64,
            paid_at: order.paid_at.unix_secs() as u64,
            amounts: MallOrderAmountsView {
                gross: order.gross_amount.to_string(),
                discount: order.discount_amount.to_string(),
                freight: order.freight_amount.to_string(),
                paid: order.paid_amount.to_string(),
                conservation_status: if conservation.order_total.valid {
                    "VALID".to_string()
                } else {
                    "DIFFERENCE".to_string()
                },
            },
            fulfillment: MallOrderFulfillmentView {
                chain: order.fulfillment_chain,
                cutover_id: cutover.as_ref().map(|record| record.base.id.clone()),
                cutover_at: cutover
                    .as_ref()
                    .and_then(|record| record.enabled_at.map(|t| t.unix_secs() as u64)),
                decided_by_occurred_at: order.paid_at.unix_secs() as u64,
            },
            facts: facts.iter().map(fact_view).collect(),
            items: items
                .iter()
                .map(|item| MallOrderItemView {
                    mall_order_item_id: item.base.id.clone(),
                    external_item_id: item.external_item_id.clone(),
                    sku_id: item.sku_id.as_ref().map(|id| id.to_string()),
                    product_publication_revision_id: item
                        .product_publication_revision_id
                        .as_ref()
                        .map(|id| id.to_string()),
                    supplier_offering_revision_id: item
                        .supplier_offering_revision_id
                        .as_ref()
                        .map(|id| id.to_string()),
                    name_snapshot: item.name_snapshot.clone(),
                    spec_snapshot: item.spec_snapshot.clone(),
                    quantity: item.quantity.to_string(),
                    unit_price_gross: item.unit_price_gross.to_string(),
                    line_gross_amount: item.line_gross_amount.to_string(),
                    allocated_discount_amount: item.allocated_discount_amount.to_string(),
                    allocated_freight_amount: item.allocated_freight_amount.to_string(),
                    paid_amount: item.paid_amount.to_string(),
                    sales_tax_rate: item.sales_tax_rate.to_string(),
                    unit_cost_snapshot: item.unit_cost_snapshot.map(|value| value.to_string()),
                    cost_snapshot_total: item.cost_snapshot_total.map(|value| value.to_string()),
                    cost_tax_inclusion: item.cost_tax_inclusion,
                    cost_input_tax_rate: item.cost_input_tax_rate.map(|value| value.to_string()),
                    attribution_status: item_attribution,
                })
                .collect(),
            payment_sources: source_views,
            funding_allocations: allocations
                .iter()
                .map(|allocation| FundingAllocationView {
                    mall_order_item_id: allocation.mall_order_item_id.to_string(),
                    payment_source_id: allocation.mall_payment_source_id.to_string(),
                    allocated_payment_amount: allocation.allocated_payment_amount.to_string(),
                })
                .collect(),
            conservation,
            consumption_entries: entry_views,
            supplier_orders: Vec::new(),
            address: MallOrderAddressView {
                masked_summary: if order.address_snapshot_encrypted.is_some() {
                    "已加密存储，需受控揭示".to_string()
                } else {
                    "未记录".to_string()
                },
                reveal_allowed: false,
            },
            allowed_actions: Vec::new(),
            action_blockers: Vec::new(),
        }
    }

    /// 计算分摊矩阵守恒校验（§6.17 行/列守恒 + 订单总额）。
    ///
    /// # 参数
    /// * `order` - 订单实体
    /// * `items` - 商品明细
    /// * `sources` - 支付来源
    /// * `allocations` - 分摊记录
    ///
    /// # 返回
    /// 返回守恒校验视图。
    fn build_conservation(
        &self,
        order: &MallOrder,
        items: &[MallOrderItem],
        sources: &[MallPaymentSource],
        allocations: &[MallItemFundingAllocation],
    ) -> ConservationView {
        let zero = Amount::from_str("0.00").expect("零常量可解析");
        let item_rows = items
            .iter()
            .map(|item| {
                let actual = allocations
                    .iter()
                    .filter(|allocation| allocation.mall_order_item_id.as_ref() == item.base.id)
                    .fold(zero, |acc, allocation| {
                        acc.checked_add(allocation.allocated_payment_amount)
                    });
                ConservationResultRow {
                    id: item.base.id.clone(),
                    expected: item.paid_amount.to_string(),
                    actual: actual.to_string(),
                    valid: actual.to_decimal() == item.paid_amount.to_decimal(),
                }
            })
            .collect();
        let source_columns = sources
            .iter()
            .map(|source| {
                let actual = allocations
                    .iter()
                    .filter(|allocation| allocation.mall_payment_source_id.as_ref() == source.base.id)
                    .fold(zero, |acc, allocation| {
                        acc.checked_add(allocation.allocated_payment_amount)
                    });
                ConservationResultRow {
                    id: source.base.id.clone(),
                    expected: source.amount.to_string(),
                    actual: actual.to_string(),
                    valid: actual.to_decimal() == source.amount.to_decimal(),
                }
            })
            .collect();
        let actual_paid = allocations.iter().fold(zero, |acc, allocation| {
            acc.checked_add(allocation.allocated_payment_amount)
        });
        ConservationView {
            item_row_results: item_rows,
            source_column_results: source_columns,
            order_total: ConservationResultRow {
                id: order.base.id.clone(),
                expected: order.paid_amount.to_string(),
                actual: actual_paid.to_string(),
                valid: actual_paid.to_decimal() == order.paid_amount.to_decimal(),
            },
        }
    }
}

/// 从事实实体构造响应视图。
///
/// # 参数
/// * `fact` - 关键事实实体
///
/// # 返回
/// 返回响应视图。
pub(super) fn fact_view(fact: &MallOrderFact) -> MallOrderFactView {
    MallOrderFactView {
        fact_id: fact.base.id.clone(),
        fact_type: fact.fact_type,
        business_fact_key: fact.business_fact_key.clone(),
        external_order_version: fact.external_order_version.clone(),
        after_sales_request_id: fact.after_sales_request_id.as_ref().map(|id| id.to_string()),
        original_payment_fact_id: fact.original_payment_fact_id.as_ref().map(|id| id.to_string()),
        occurred_at: fact.occurred_at.unix_secs() as u64,
        received_at: fact.received_at.unix_secs() as u64,
        data_source: fact.data_source,
        processing_status: fact.processing_status,
    }
}

/// 从成本评估实体构造响应视图。
///
/// # 参数
/// * `assessment` - 成本评估实体
///
/// # 返回
/// 返回响应视图。
fn cost_assessment_view(assessment: &MallConsumptionCostAssessment) -> CostAssessmentView {
    CostAssessmentView {
        assessment_id: assessment.base.id.clone(),
        assessment_no: assessment.assessment_no,
        cost_basis: assessment.cost_basis,
        basis_source_label: assessment
            .basis_source_type
            .map(|source| source.label().to_string())
            .unwrap_or_else(|| "无可用成本来源".to_string()),
        gross_amount: assessment.gross_amount.map(|amount| amount.to_string()),
        net_amount: assessment.net_amount.map(|amount| amount.to_string()),
        tax_amount: assessment.tax_amount.map(|amount| amount.to_string()),
        tax_inclusion: assessment.tax_inclusion,
        input_tax_rate: assessment.input_tax_rate.map(|rate| rate.to_string()),
        assessed_at: assessment.assessed_at.unix_secs() as u64,
    }
}

/// 对敏感引用做脱敏展示（保留前后缀，中间以 `****` 掩盖）。
///
/// # 参数
/// * `reference` - 原始引用
///
/// # 返回
/// 返回脱敏后的展示串。
fn mask_reference(reference: &str) -> String {
    if reference.len() <= 8 {
        "****".to_string()
    } else {
        format!("{}****{}", &reference[..4], &reference[reference.len() - 4..])
    }
}

/// 归集状态判定：卡券来源映射到卡实例为已归集，否则待归集；微信恒为已归集。
///
/// # 参数
/// * `source_type` - 来源类型
/// * `card_instance` - 映射到的卡实例
///
/// # 返回
/// 返回归集状态。
pub(super) fn attribution_for(
    source_type: PaymentSourceType,
    card_instance: &Option<MallCardInstance>,
) -> AttributionStatus {
    match source_type {
        PaymentSourceType::Card if card_instance.is_some() => AttributionStatus::Attributed,
        PaymentSourceType::Card => AttributionStatus::PendingAttribution,
        PaymentSourceType::Wechat => AttributionStatus::Attributed,
    }
}

/// 从评估实体派生成本金额合计（`NONE` 为空）。
trait AssessmentAmountString {
    /// 返回成本金额展示串（`NONE` 为 `None`）。
    fn cost_amount_string(&self) -> Option<Amount>;
}

impl AssessmentAmountString for MallConsumptionCostAssessment {
    fn cost_amount_string(&self) -> Option<Amount> {
        if self.cost_basis == CostBasis::None {
            None
        } else {
            self.gross_amount
        }
    }
}

/// （商城, 订单号）→ 事实摘要列表的映射类型（列表行聚合用）。
pub(super) type OrderFactMap =
    std::collections::HashMap<(String, String), Vec<(FactType, Instant, DataSource)>>;

/// 商城订单列表投影行（Service 内私有，避免依赖仓储私有子树类型名）。
#[derive(Debug, Clone)]
struct OrderListRow {
    /// 实体主键。
    id: String,
    /// 商城订单身份。
    mall_id: String,
    /// 商城订单号。
    external_order_no: String,
    /// 映射后的企业客户。
    customer_id: Option<entities::ids::CustomerAccountId>,
    /// 支付成功时间。
    paid_at: Instant,
    /// 实付快照。
    paid_amount: Amount,
    /// 履约链归属。
    fulfillment_chain: FulfillmentChain,
    /// 归集进度状态。
    attribution_status: AttributionStatus,
}

/// 事实摘要（列表行聚合用）。
#[derive(Debug, Clone, Copy)]
struct OrderFactSummary {
    /// 事实类型。
    fact_type: FactType,
    /// 发生时间。
    occurred_at: Instant,
    /// 数据来源。
    data_source: DataSource,
}
