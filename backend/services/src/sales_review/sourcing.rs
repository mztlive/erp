//! W07 采购推荐方案查询。
//!
//! 本模块负责把当前供给、供应商能力与销售提交快照组装成领域候选；最低成本选择由
//! `entities::sales_review::sourcing` 完成，避免在 HTTP 或页面侧计算正式采购方案。

use std::collections::{BTreeMap, HashMap, HashSet};
use std::str::FromStr;

use database::{NoTransaction, PartyExt, SalesOrderExt, SalesReviewExt, SupplierExt, SupplierOfferingExt};
use entities::common::time::{BusinessDate, Instant};
use entities::ids::{SkuId, SupplierAccountId, SupplierCapabilityRevisionId, SupplierOfferingId};
use entities::money::{line_amounts, Amount, Quantity};
use entities::sales_order::{FulfillmentMode as SalesFulfillmentMode, SalesOrderSubmissionLine};
use entities::sales_review::{
    recommend_sourcing_line, FulfillmentMode, SourcingAllocation, SourcingCandidate, SourcingLine,
    SourcingLinePlan,
};
use entities::supplier::CapabilityCode;
use entities::supplier_offering::{
    OfferingStatus, SupplierOffering, SupplierOfferingAvailability, SupplierOfferingRevision,
};

use super::{
    ProcurementRecommendationIssueView, ProcurementRecommendationLineView,
    ProcurementRecommendationOrderView, ProcurementRecommendationView, SalesReviewService,
};
use crate::errors::{Error, Result};

/// 推荐规则版本；变更成本目标或可行性过滤规则时必须递增。
const SOURCING_POLICY_VERSION: &str = "LOWEST_FEASIBLE_LANDED_COST_V4";

impl SalesReviewService {
    /// 计算采购二次确认的最低可执行成本方案。
    ///
    /// 只使用当前启用供给修订、可供状态、有效期和当前启用供应商能力。结果携带精确
    /// 供给/能力修订，采购保存与审批仍会再次校验，防止计算后基础资料发生变化。
    ///
    /// # 参数
    /// * `id` - 采购确认批次 ID
    ///
    /// # 返回
    /// 返回推荐分配、预计采购单草稿分组、落地成本与阻断问题。
    ///
    /// # 错误
    /// 确认批次或销售提交不存在、数据库查询失败时返回错误。
    pub async fn procurement_recommendation(&self, id: &str) -> Result<ProcurementRecommendationView> {
        let _ = (self, id);
        return Err(Error::ConflictError(
            "采购二次确认已停止新写入，选源改由采购单创建路径承担".to_string(),
        ));
        #[allow(unreachable_code)]
        let confirmation = self
            .db
            .procurement_confirmations()
            .find_by_id(id, &mut NoTransaction)
            .await?
            .ok_or_else(|| Error::NotFound("采购确认不存在".to_string()))?;
        let submission = self
            .db
            .sales_order_submissions()
            .find_by_id(&confirmation.submission_id, &mut NoTransaction)
            .await?
            .ok_or_else(|| Error::NotFound("销售提交不存在".to_string()))?;
        let submission_lines = self
            .db
            .sales_order_submission_lines()
            .list_lines_by_submissions(
                std::slice::from_ref(&confirmation.submission_id),
                &mut NoTransaction,
            )
            .await?;
        let offerings = self.active_offerings_for_submission(&submission_lines).await?;
        let revisions = self.current_offering_revisions(&offerings).await?;
        let availabilities = self.current_offering_availabilities(&offerings).await?;
        let today = BusinessDate::today();
        let mut capability_cache = HashMap::new();
        let mut plans = Vec::new();
        let mut blocking_issues = Vec::new();

        for group in group_submission_lines(&submission_lines) {
            match self
                .recommend_submission_group(
                    &group,
                    &offerings,
                    &revisions,
                    &availabilities,
                    today,
                    &mut capability_cache,
                )
                .await
            {
                Ok(plan) => plans.extend(split_group_plan(&group, &plan)?),
                Err(error) => {
                    blocking_issues.extend(group.iter().map(|line| ProcurementRecommendationIssueView {
                        code: "NO_FEASIBLE_SUPPLY".to_string(),
                        message: format!("{}：{}", line.item_name_snapshot, error),
                        sales_order_submission_line_id: Some(line.base.id.clone()),
                    }));
                }
            }
        }

        let supplier_names = self.supplier_names(&plans).await?;
        let lines = recommendation_lines(&submission_lines, &plans, &supplier_names)?;
        let purchase_orders = recommendation_orders(&lines)?;
        let estimated_purchase = plans
            .iter()
            .fold(zero_amount()?, |total, plan| total.checked_add(plan.landed_gross));
        let margin = submission.gross_amount.checked_sub(estimated_purchase);
        let warnings = recommendation_warnings(&submission_lines);

        Ok(ProcurementRecommendationView {
            confirmation_id: confirmation.base.id,
            policy_version: SOURCING_POLICY_VERSION.to_string(),
            calculated_at: Instant::now().unix_secs() as u64,
            ready: blocking_issues.is_empty() && !lines.is_empty(),
            lines,
            purchase_orders,
            estimated_purchase_gross: estimated_purchase.to_string(),
            sales_gross: submission.gross_amount.to_string(),
            estimated_gross_margin: margin.to_string(),
            blocking_issues,
            warnings,
        })
    }

    /// 批量加载销售提交涉及的启用供给身份。
    async fn active_offerings_for_submission(
        &self,
        lines: &[SalesOrderSubmissionLine],
    ) -> Result<Vec<SupplierOffering>> {
        let sku_ids = lines
            .iter()
            .filter_map(|line| line.sku_id.clone())
            .collect::<Vec<SkuId>>();
        let offerings = self
            .db
            .supplier_offerings()
            .find_by_sku_ids(&sku_ids, &mut NoTransaction)
            .await?;
        Ok(offerings
            .into_iter()
            .filter(|offering| {
                offering.stable.status == OfferingStatus::Active
                    && offering.stable.current_revision_id.is_some()
            })
            .collect())
    }

    /// 批量加载供给身份指向的当前修订。
    async fn current_offering_revisions(
        &self,
        offerings: &[SupplierOffering],
    ) -> Result<HashMap<String, SupplierOfferingRevision>> {
        let offering_ids = offerings
            .iter()
            .map(|offering| SupplierOfferingId::new(offering.base.id.clone()))
            .collect::<Vec<_>>();
        let revisions = self
            .db
            .supplier_offering_revisions()
            .find_revisions_by_offering_ids(&offering_ids, &mut NoTransaction)
            .await?;
        let current_ids = offerings
            .iter()
            .filter_map(|offering| {
                offering
                    .stable
                    .current_revision_id
                    .as_ref()
                    .map(|revision_id| (revision_id.clone(), offering.base.id.clone()))
            })
            .collect::<HashMap<_, _>>();
        Ok(revisions
            .into_iter()
            .filter_map(|revision| {
                current_ids
                    .get(&revision.base.id)
                    .cloned()
                    .map(|offering_id| (offering_id, revision))
            })
            .collect())
    }

    /// 批量加载供给的实时可供投影。
    async fn current_offering_availabilities(
        &self,
        offerings: &[SupplierOffering],
    ) -> Result<HashMap<String, SupplierOfferingAvailability>> {
        let offering_ids = offerings
            .iter()
            .map(|offering| SupplierOfferingId::new(offering.base.id.clone()))
            .collect::<Vec<_>>();
        Ok(self
            .db
            .supplier_offering_availabilities()
            .find_by_offering_ids(&offering_ids, &mut NoTransaction)
            .await?
            .into_iter()
            .map(|availability| (availability.supplier_offering_id.to_string(), availability))
            .collect())
    }

    /// 为同 SKU、同交付类别的销售提交行组构造候选并调用领域最低成本规则。
    async fn recommend_submission_group(
        &self,
        lines: &[&SalesOrderSubmissionLine],
        offerings: &[SupplierOffering],
        revisions: &HashMap<String, SupplierOfferingRevision>,
        availabilities: &HashMap<String, SupplierOfferingAvailability>,
        today: BusinessDate,
        capability_cache: &mut HashMap<(String, String), Option<SupplierCapabilityRevisionId>>,
    ) -> Result<SourcingLinePlan> {
        let line = lines
            .first()
            .ok_or_else(|| Error::ValidationError("销售提交行组为空".to_string()))?;
        let sku_id = line
            .sku_id
            .as_ref()
            .ok_or_else(|| Error::ValidationError("销售提交行缺少公司 SKU".to_string()))?;
        let zero_quantity = Quantity::from_str("0")?;
        let required =
            Quantity::try_from(lines.iter().try_fold(zero_quantity.to_decimal(), |total, line| {
                line.quantity
                    .map(|quantity| total + quantity.to_decimal())
                    .ok_or_else(|| Error::ValidationError("销售提交行缺少采购数量".to_string()))
            })?)?;
        let mut candidates = Vec::new();
        for offering in offerings.iter().filter(|offering| &offering.sku_id == sku_id) {
            let Some(revision) = revisions.get(&offering.base.id) else {
                continue;
            };
            let Some(availability) = availabilities.get(&offering.base.id) else {
                continue;
            };
            if !offering_is_available(revision, availability, today) {
                continue;
            }
            for mode in candidate_modes(line.fulfillment_mode) {
                let capability_code = capability_for_mode(mode);
                let Some(capability_revision_id) = self
                    .current_capability_revision(
                        &offering.supplier_id,
                        capability_code,
                        today,
                        capability_cache,
                    )
                    .await?
                else {
                    continue;
                };
                candidates.push(candidate_from_offering(
                    offering,
                    revision,
                    availability,
                    mode,
                    capability_revision_id,
                )?);
            }
        }
        recommend_sourcing_line(&SourcingLine {
            submission_line_id: line.base.id.clone(),
            required_quantity: required,
            candidates,
        })
        .map_err(|error| Error::ValidationError(error.to_string()))
    }

    /// 读取并缓存供应商当前有效能力修订。
    async fn current_capability_revision(
        &self,
        supplier_id: &SupplierAccountId,
        capability_code: CapabilityCode,
        today: BusinessDate,
        cache: &mut HashMap<(String, String), Option<SupplierCapabilityRevisionId>>,
    ) -> Result<Option<SupplierCapabilityRevisionId>> {
        let key = (supplier_id.to_string(), capability_code.as_str().to_string());
        if let Some(cached) = cache.get(&key) {
            return Ok(cached.clone());
        }
        let capability = self
            .db
            .supplier_capabilities()
            .find_by_supplier_and_code(supplier_id, capability_code, &mut NoTransaction)
            .await?;
        let revision_id = capability.and_then(|capability| {
            let in_window = capability.valid_from <= today
                && capability.valid_to.is_none_or(|valid_to| today <= valid_to);
            (capability.is_active() && in_window)
                .then_some(capability.stable.current_revision_id)
                .flatten()
                .map(SupplierCapabilityRevisionId::new)
        });
        cache.insert(key, revision_id.clone());
        Ok(revision_id)
    }

    /// 解析推荐方案涉及的供应商名称。
    async fn supplier_names(&self, plans: &[SourcingLinePlan]) -> Result<HashMap<String, String>> {
        let supplier_ids = plans
            .iter()
            .flat_map(|plan| plan.allocations.iter())
            .map(|allocation| allocation.supplier_id.clone())
            .collect::<HashSet<_>>();
        let mut names = HashMap::with_capacity(supplier_ids.len());
        for supplier_id in supplier_ids {
            let name = self
                .supplier_name(&supplier_id)
                .await?
                .unwrap_or_else(|| "供应商名称缺失".to_string());
            names.insert(supplier_id.to_string(), name);
        }
        Ok(names)
    }

    /// 解析供应商主体当前名称。
    async fn supplier_name(&self, supplier_id: &SupplierAccountId) -> Result<Option<String>> {
        let supplier = self
            .db
            .supplier_accounts()
            .find_by_id(supplier_id, &mut NoTransaction)
            .await?;
        let Some(supplier) = supplier else { return Ok(None) };
        let party = self
            .db
            .parties()
            .find_by_id(&supplier.party_id, &mut NoTransaction)
            .await?;
        let Some(party) = party else { return Ok(None) };
        let Some(revision_id) = party.stable.current_revision_id else {
            return Ok(None);
        };
        let revision = self
            .db
            .party_revisions()
            .find_by_id(&revision_id, &mut NoTransaction)
            .await?;
        Ok(revision.map(|revision| revision.legal_name))
    }
}

/// 按公司 SKU 与销售交付类别合并需求，统一占用可供量并判断集采起订量。
fn group_submission_lines(lines: &[SalesOrderSubmissionLine]) -> Vec<Vec<&SalesOrderSubmissionLine>> {
    let mut grouped: BTreeMap<(String, &'static str), Vec<&SalesOrderSubmissionLine>> = BTreeMap::new();
    for line in lines {
        let sku_key = line
            .sku_id
            .as_ref()
            .map(ToString::to_string)
            .unwrap_or_else(|| format!("missing:{}", line.base.id));
        grouped
            .entry((sku_key, fulfillment_family(line.fulfillment_mode)))
            .or_default()
            .push(line);
    }
    grouped.into_values().collect()
}

/// 销售承诺的采购比较类别；实物入仓与供应商直发可在同一候选池比较。
fn fulfillment_family(mode: Option<SalesFulfillmentMode>) -> &'static str {
    match mode {
        Some(SalesFulfillmentMode::ElectronicDelivery) => "ELECTRONIC",
        Some(SalesFulfillmentMode::OfflineService) => "SERVICE",
        _ => "PHYSICAL",
    }
}

/// 将聚合选源结果按原销售提交行顺序回分，固定费用只落在首次使用该供给的分行。
fn split_group_plan(
    lines: &[&SalesOrderSubmissionLine],
    plan: &SourcingLinePlan,
) -> Result<Vec<SourcingLinePlan>> {
    let mut remaining = plan
        .allocations
        .iter()
        .cloned()
        .map(|allocation| {
            let quantity = allocation.quantity.to_decimal();
            (allocation, quantity, true)
        })
        .collect::<Vec<_>>();
    let mut result = Vec::with_capacity(lines.len());

    for line in lines {
        let mut needed = line
            .quantity
            .ok_or_else(|| Error::ValidationError("销售提交行缺少采购数量".to_string()))?
            .to_decimal();
        let mut allocations = Vec::new();
        let mut landed_gross = zero_amount()?;
        for (source, available, fee_pending) in &mut remaining {
            if needed.is_zero() {
                break;
            }
            let assigned = needed.min(*available);
            if assigned.is_zero() {
                continue;
            }
            let quantity = Quantity::try_from(assigned)?;
            let allocation = split_allocation(source, quantity, *fee_pending)?;
            landed_gross = landed_gross.checked_add(allocation.landed_gross);
            allocations.push(allocation);
            *available -= assigned;
            needed -= assigned;
            *fee_pending = false;
        }
        if !needed.is_zero() {
            return Err(Error::BusinessLogicError(
                "聚合采购方案无法回分到全部销售提交行".to_string(),
            ));
        }
        result.push(SourcingLinePlan {
            submission_line_id: line.base.id.clone(),
            allocations,
            landed_gross,
        });
    }
    Ok(result)
}

/// 构造聚合分配的一段销售行分配，并按采购草稿口径重新计算行金额。
fn split_allocation(
    source: &SourcingAllocation,
    quantity: Quantity,
    include_fixed_fees: bool,
) -> Result<SourcingAllocation> {
    let (product_gross, _, _) = line_amounts(source.unit_cost_gross, quantity, source.input_tax_rate);
    let freight_amount = include_fixed_fees.then_some(source.freight_amount).flatten();
    let service_fee_amount = include_fixed_fees.then_some(source.service_fee_amount).flatten();
    let landed_gross = freight_amount
        .into_iter()
        .chain(service_fee_amount)
        .fold(product_gross, Amount::checked_add);
    Ok(SourcingAllocation {
        supplier_id: source.supplier_id.clone(),
        offering_revision_id: source.offering_revision_id.clone(),
        capability_revision_id: source.capability_revision_id.clone(),
        fulfillment_mode: source.fulfillment_mode,
        quantity,
        unit_cost_gross: source.unit_cost_gross,
        uses_bulk_price: source.uses_bulk_price,
        input_tax_rate: source.input_tax_rate,
        freight_amount,
        service_fee_amount,
        landed_gross,
    })
}

/// 判断商业条款和实时可供投影当前是否都可参与推荐。
fn offering_is_available(
    revision: &SupplierOfferingRevision,
    availability: &SupplierOfferingAvailability,
    today: BusinessDate,
) -> bool {
    availability.is_available()
        && revision.valid_from <= today
        && revision.valid_to.is_none_or(|valid_to| today <= valid_to)
}

/// 按销售承诺类型给出允许采购比较的履约方式。
fn candidate_modes(mode: Option<SalesFulfillmentMode>) -> Vec<FulfillmentMode> {
    match mode {
        Some(SalesFulfillmentMode::ElectronicDelivery) => vec![FulfillmentMode::ElectronicDelivery],
        Some(SalesFulfillmentMode::OfflineService) => vec![FulfillmentMode::OfflineService],
        _ => vec![FulfillmentMode::CompanyWarehouse, FulfillmentMode::SupplierDirect],
    }
}

/// 将履约方式映射到供应商准入能力。
fn capability_for_mode(mode: FulfillmentMode) -> CapabilityCode {
    match mode {
        FulfillmentMode::CompanyWarehouse | FulfillmentMode::SupplierDirect => CapabilityCode::Physical,
        FulfillmentMode::ElectronicDelivery => CapabilityCode::Virtual,
        FulfillmentMode::OfflineService => CapabilityCode::OfflineService,
    }
}

/// 将当前供给修订转换为指定履约方式候选。
fn candidate_from_offering(
    offering: &SupplierOffering,
    revision: &SupplierOfferingRevision,
    availability: &SupplierOfferingAvailability,
    mode: FulfillmentMode,
    capability_revision_id: SupplierCapabilityRevisionId,
) -> Result<SourcingCandidate> {
    let is_warehouse = mode == FulfillmentMode::CompanyWarehouse;
    Ok(SourcingCandidate {
        supplier_id: offering.supplier_id.clone(),
        offering_revision_id: revision.base.id.clone().into(),
        capability_revision_id,
        fulfillment_mode: mode,
        dropship_unit_cost_gross: revision.dropship_supply_price_gross,
        bulk_unit_cost_gross: revision.bulk_supply_price_gross,
        input_tax_rate: revision.input_tax_rate,
        bulk_minimum_quantity: revision.bulk_minimum_order_quantity,
        available_quantity: availability.available_quantity,
        freight_amount: is_warehouse.then_some(revision.freight_amount).flatten(),
        service_fee_amount: revision.service_fee_amount,
    })
}

/// 将领域推荐转换为客户端可直接保存的确认分行。
fn recommendation_lines(
    submission_lines: &[SalesOrderSubmissionLine],
    plans: &[SourcingLinePlan],
    supplier_names: &HashMap<String, String>,
) -> Result<Vec<ProcurementRecommendationLineView>> {
    let submission_by_id = submission_lines
        .iter()
        .map(|line| (line.base.id.as_str(), line))
        .collect::<HashMap<_, _>>();
    let mut result = Vec::new();
    for plan in plans {
        let line = submission_by_id
            .get(plan.submission_line_id.as_str())
            .ok_or_else(|| Error::BusinessLogicError("推荐方案引用了不存在的销售提交行".to_string()))?;
        let sku_id = line
            .sku_id
            .as_ref()
            .ok_or_else(|| Error::BusinessLogicError("销售提交行缺少 SKU".to_string()))?;
        let expected_delivery_date = fulfillment_business_date(line)?;
        for allocation in &plan.allocations {
            result.push(ProcurementRecommendationLineView {
                line_no: (result.len() + 1) as u32,
                sales_order_submission_line_id: line.base.id.clone(),
                item_name: line.item_name_snapshot.clone(),
                sku_id: sku_id.to_string(),
                supplier_id: allocation.supplier_id.to_string(),
                supplier_name: supplier_names
                    .get(allocation.supplier_id.as_ref())
                    .cloned()
                    .unwrap_or_else(|| "供应商名称缺失".to_string()),
                supplier_offering_revision_id: allocation.offering_revision_id.to_string(),
                confirmed_quantity: allocation.quantity.to_string(),
                latest_cost_gross: allocation.unit_cost_gross.to_string(),
                input_tax_rate: allocation.input_tax_rate.to_string(),
                expected_delivery_date: expected_delivery_date.to_string(),
                fulfillment_mode: allocation.fulfillment_mode,
                supplier_capability_revision_id: allocation.capability_revision_id.to_string(),
                landed_gross: allocation.landed_gross.to_string(),
                freight_amount: allocation.freight_amount.map(|amount| amount.to_string()),
                service_fee_amount: allocation.service_fee_amount.map(|amount| amount.to_string()),
                recommendation_reason: if allocation.uses_bulk_price {
                    "本供应商承接数量已达到集采起订量，按集采价计算；综合商品价和费用后成本最低".to_string()
                } else {
                    "本供应商承接数量未达到集采起订量，按一件代发价计算；综合商品价和费用后成本最低"
                        .to_string()
                },
            });
        }
    }
    Ok(result)
}

/// 按供应商与履约方式汇总预计采购单草稿。
fn recommendation_orders(
    lines: &[ProcurementRecommendationLineView],
) -> Result<Vec<ProcurementRecommendationOrderView>> {
    let mut grouped: HashMap<(String, FulfillmentMode), (String, u32, Amount)> = HashMap::new();
    for line in lines {
        let key = (line.supplier_id.clone(), line.fulfillment_mode);
        let landed = Amount::from_str(&line.landed_gross)?;
        let entry = grouped.entry(key).or_insert_with(|| {
            (
                line.supplier_name.clone(),
                0,
                zero_amount().expect("静态零值合法"),
            )
        });
        entry.1 += 1;
        entry.2 = entry.2.checked_add(landed);
    }
    let mut result = grouped
        .into_iter()
        .map(
            |((supplier_id, fulfillment_mode), (supplier_name, line_count, estimated))| {
                ProcurementRecommendationOrderView {
                    supplier_id,
                    supplier_name,
                    fulfillment_mode,
                    line_count,
                    estimated_gross: estimated.to_string(),
                }
            },
        )
        .collect::<Vec<_>>();
    result.sort_by(|left, right| {
        (left.supplier_name.as_str(), left.fulfillment_mode.as_str())
            .cmp(&(right.supplier_name.as_str(), right.fulfillment_mode.as_str()))
    });
    Ok(result)
}

/// 把销售承诺时间转换为采购确认目标自然日。
fn fulfillment_business_date(line: &SalesOrderSubmissionLine) -> Result<BusinessDate> {
    let due = line
        .fulfillment_due_at
        .ok_or_else(|| Error::ValidationError("销售提交行缺少履约期限".to_string()))?;
    BusinessDate::from_str(&due.as_utc().date_naive().to_string()).map_err(Into::into)
}

/// 当前供给模型没有结构化提前期，推荐结果必须显式提醒采购复核交期。
fn recommendation_warnings(lines: &[SalesOrderSubmissionLine]) -> Vec<ProcurementRecommendationIssueView> {
    lines
        .iter()
        .map(|line| ProcurementRecommendationIssueView {
            code: "DELIVERY_DATE_REQUIRES_CONFIRMATION".to_string(),
            message: format!(
                "{}：系统已按销售承诺日期填入采购目标交期，请在通过前确认供应商能够按期履约",
                line.item_name_snapshot
            ),
            sales_order_submission_line_id: Some(line.base.id.clone()),
        })
        .collect()
}

/// 构造金额零值。
fn zero_amount() -> Result<Amount> {
    Amount::from_str("0.00").map_err(Into::into)
}

#[cfg(test)]
mod tests {
    use std::str::FromStr;

    use super::{candidate_modes, capability_for_mode, split_allocation};
    use entities::ids::{SupplierAccountId, SupplierCapabilityRevisionId, SupplierOfferingRevisionId};
    use entities::money::{Amount, Quantity, Rate, UnitPrice};
    use entities::sales_order::FulfillmentMode as SalesFulfillmentMode;
    use entities::sales_review::{FulfillmentMode, SourcingAllocation};
    use entities::supplier::CapabilityCode;

    #[test]
    fn physical_sales_can_compare_warehouse_and_supplier_direct() {
        assert_eq!(
            candidate_modes(Some(SalesFulfillmentMode::CompanyWarehouse)),
            vec![FulfillmentMode::CompanyWarehouse, FulfillmentMode::SupplierDirect]
        );
        assert_eq!(
            capability_for_mode(FulfillmentMode::SupplierDirect),
            CapabilityCode::Physical
        );
    }

    #[test]
    fn electronic_and_service_keep_their_business_capability() {
        assert_eq!(
            candidate_modes(Some(SalesFulfillmentMode::ElectronicDelivery)),
            vec![FulfillmentMode::ElectronicDelivery]
        );
        assert_eq!(
            capability_for_mode(FulfillmentMode::OfflineService),
            CapabilityCode::OfflineService
        );
    }

    #[test]
    fn grouped_allocation_charges_fixed_fee_only_once_when_split_back() {
        let source = SourcingAllocation {
            supplier_id: SupplierAccountId::new("supplier-1"),
            offering_revision_id: SupplierOfferingRevisionId::new("offering-revision-1"),
            capability_revision_id: SupplierCapabilityRevisionId::new("capability-revision-1"),
            fulfillment_mode: FulfillmentMode::CompanyWarehouse,
            quantity: Quantity::from_str("10").unwrap(),
            unit_cost_gross: UnitPrice::from_str("9").unwrap(),
            uses_bulk_price: true,
            input_tax_rate: Rate::from_str("0.13").unwrap(),
            freight_amount: Some(Amount::from_str("5").unwrap()),
            service_fee_amount: None,
            landed_gross: Amount::from_str("95").unwrap(),
        };

        let first = split_allocation(&source, Quantity::from_str("4").unwrap(), true).unwrap();
        let second = split_allocation(&source, Quantity::from_str("6").unwrap(), false).unwrap();

        assert_eq!(first.landed_gross, Amount::from_str("41").unwrap());
        assert_eq!(second.landed_gross, Amount::from_str("54").unwrap());
        assert_eq!(
            first.landed_gross.checked_add(second.landed_gross),
            source.landed_gross
        );
    }
}
