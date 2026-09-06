use std::collections::HashSet;
use std::str::FromStr;

use chrono::{Datelike, FixedOffset};
use entities::purchase_order::{
    basis_id_for, basis_scope_key, fulfillment_options, maximum_create_quantity,
    purchase_type_from_product_kind, stable_line_id, stock_basis_id_for, supply_cost, BasisGroup, BasisLine,
    BasisScope, CreationBasisFacts, FulfillmentResponsibility, LineSupply, PurchaseType,
    SalesProcurementCoverage, SalesProcurementCoverageLine, StockBasisGroup,
};
use entities::sales_order::{CommercialStatus, SalesOrder, SalesOrderRevision};
use entities::supplier_offering::{AvailabilityStatus, SupplierOffering};
use erp_core::common::time::{BusinessDate, Instant};
use erp_core::ids::SupplierAccountId;
use erp_core::money::{line_amounts, Amount, Quantity, UnitPrice};

use super::super::dto::{CreationBasisLineView, CreationBasisView, SupplySourceType};
use super::super::shared::zero_amount;
use crate::errors::{Error, Result};

/// 供应商当前商务资料中的付款条件与经营类目。
#[derive(Debug, Clone, PartialEq, Eq)]
struct SupplierSettlementTerms {
    /// 不含经营类目编码的付款条件代码。
    payment_term_code: String,
    /// 经营类目；未登记时为空。
    business_category: Option<String>,
}

impl SupplierSettlementTerms {
    /// 商务资料缺失时的缺省付款条件。
    ///
    /// # 参数
    /// 无。
    ///
    /// # 返回
    /// `NET-30` 且无经营类目。
    ///
    /// # 错误
    /// 无。
    fn net30() -> Self {
        Self {
            payment_term_code: "NET-30".to_string(),
            business_category: None,
        }
    }
}

/// 由销售当前版本、当前覆盖和批量供给事实形成精确依据集合（纯规则）。
///
/// # 参数
/// * `order` - 已生效销售单
/// * `coverage` - 当前销售版本采购覆盖
/// * `responsibility_scope_ids` - 当前采购任务冻结的稳定销售行 ID
/// * `facts` - 任务涉及 SKU 的批量供给事实
///
/// # 返回
/// 返回任务责任范围内按精确拆分维度分组并稳定排序的依据。
///
/// # 错误
/// 商品类型映射或可供数量非法时返回错误。
///
/// # 关键业务约束
/// 同一依据内供应商、采购类型、付款条件和履约责任完全一致；非生效销售单返回
/// 空集合；`min(remaining, available)` 为零时丢弃该供应商。
pub(super) fn basis_groups_from_facts(
    order: &SalesOrder,
    coverage: &SalesProcurementCoverage,
    responsibility_scope_ids: &[String],
    facts: &CreationBasisFacts,
) -> Result<Vec<BasisGroup>> {
    if order.commercial_status != CommercialStatus::Effective {
        return Ok(Vec::new());
    }
    let responsibility_scope_ids = responsibility_scope_ids
        .iter()
        .map(String::as_str)
        .collect::<HashSet<_>>();
    let mut groups: Vec<BasisGroup> = Vec::new();
    for line in &coverage.lines {
        if !responsibility_scope_ids.contains(line.revision_line.sales_order_line_id.as_ref())
            || line.summary.remaining_quantity <= zero_quantity()
        {
            continue;
        }
        let supplies = qualified_supplies_for_line(facts, line)?;
        append_line_supplies(&coverage.revision, line.clone(), supplies, facts, &mut groups)?;
    }
    for group in &mut groups {
        group
            .lines
            .sort_by(|left, right| stable_line_id(left).cmp(stable_line_id(right)));
    }
    groups.sort_by_key(|group| basis_scope_key(&group.scope));
    Ok(groups)
}

/// 将一条销售目标行的合格供给加入精确依据分组。
///
/// # 参数
/// * `revision` - 销售当前版本
/// * `line` - 当前销售版本目标行
/// * `supplies` - 每供应商一条确定供给
/// * `facts` - 批量供给与供应商结算事实
/// * `groups` - 待追加依据集合
///
/// # 返回
/// 追加完成返回 `Ok(())`。
///
/// # 错误
/// 商品类型映射或可供数量计算失败时返回错误。
///
/// # 关键业务约束
/// 有限可供量使用 `min(remaining, available)`，不因不足全量而丢弃供应商；付款
/// 条件与经营类目只从批量事实解释，不再逐供应商读取。
fn append_line_supplies(
    revision: &SalesOrderRevision,
    line: SalesProcurementCoverageLine,
    supplies: Vec<LineSupply>,
    facts: &CreationBasisFacts,
    groups: &mut Vec<BasisGroup>,
) -> Result<()> {
    for supply in supplies {
        let supplier_id = supply.offering.supplier_id.clone();
        let terms = settlement_terms_for(facts, &supplier_id);
        let purchase_type = purchase_type_from_product_kind(line.product_kind)?;
        let max_create_quantity = maximum_create_quantity(
            line.summary.remaining_quantity,
            supply.availability.available_quantity,
        )?;
        if max_create_quantity <= zero_quantity() {
            continue;
        }
        for &fulfillment_responsibility in fulfillment_options(line.product_kind)? {
            let scope = BasisScope {
                supplier_id: supplier_id.clone(),
                purchase_type,
                payment_term_code: terms.payment_term_code.clone(),
                fulfillment_responsibility,
            };
            let basis_line = BasisLine {
                coverage: line.clone(),
                supply: supply.clone(),
                max_create_quantity,
            };
            if let Some(group) = groups.iter_mut().find(|group| group.scope == scope) {
                group.lines.push(basis_line);
            } else {
                groups.push(BasisGroup {
                    revision: revision.clone(),
                    scope,
                    business_category: terms.business_category.clone(),
                    lines: vec![basis_line],
                });
            }
        }
    }
    Ok(())
}

/// 从批量事实解释供应商当前付款条件与经营类目。
///
/// # 参数
/// * `facts` - 批量供应商结算事实
/// * `supplier_id` - 供应商身份
///
/// # 返回
/// 返回该供应商已拆开的付款条件与经营类目；供应商、商务版本缺失或付款条件为
/// 空时付款条件回退 `NET-30`。
///
/// # 错误
/// 无。
///
/// # 关键业务约束
/// 付款条件是精确拆单维度的一部分；经营类目不得写入付款条件代码。
fn settlement_terms_for(
    facts: &CreationBasisFacts,
    supplier_id: &SupplierAccountId,
) -> SupplierSettlementTerms {
    let Some(supplier) = facts.suppliers.get(&supplier_id.to_string()) else {
        return SupplierSettlementTerms::net30();
    };
    let Some(revision_id) = supplier.current_commercial_profile_revision_id.clone() else {
        return SupplierSettlementTerms::net30();
    };
    let Some(revision) = facts.commercial_profiles.get(&revision_id.to_string()) else {
        return SupplierSettlementTerms::net30();
    };
    let payment_term_code = revision.effective_payment_term_code();
    SupplierSettlementTerms {
        payment_term_code: if payment_term_code.is_empty() {
            "NET-30".to_string()
        } else {
            payment_term_code
        },
        business_category: revision.effective_business_category(),
    }
}

/// 查询一条销售当前版本行的合格供给，并为每个供应商确定一条稳定供给。
///
/// # 参数
/// * `facts` - 批量 ACTIVE 供给、当前修订与可供投影
/// * `line` - 销售当前版本目标行
///
/// # 返回
/// 返回按供应商和供给 ID 稳定排序、每供应商最多一条的供给。
///
/// # 错误
/// 可供数量为负时返回业务错误。
///
/// # 关键业务约束
/// 仅 ACTIVE、条款当前有效且 availability 为 AVAILABLE 的供给合格；同一 SKU
/// 的供给顺序由批量事实保证，与逐 SKU 查询完全一致。
fn qualified_supplies_for_line(
    facts: &CreationBasisFacts,
    line: &SalesProcurementCoverageLine,
) -> Result<Vec<LineSupply>> {
    let mut seen_suppliers = HashSet::new();
    let mut supplies = Vec::new();
    for offering in facts
        .offerings
        .iter()
        .filter(|offering| offering.sku_id == line.goods_line.sku_id)
    {
        if seen_suppliers.contains(&offering.supplier_id.to_string()) {
            continue;
        }
        let Some(supply) = qualified_supply(facts, offering)? else {
            continue;
        };
        seen_suppliers.insert(supply.offering.supplier_id.to_string());
        supplies.push(supply);
    }
    Ok(supplies)
}

/// 复验单条供给当前修订与可供投影。
///
/// # 参数
/// * `facts` - 批量供给修订与可供投影
/// * `offering` - ACTIVE 供给稳定身份
///
/// # 返回
/// 当前合格时返回供给；缺少当前修订、条款失效或不可供时返回 `None`。
///
/// # 错误
/// 可供数量为负时返回业务错误。
///
/// # 关键业务约束
/// 可供数量为空表示供应商未给出上限，不等于不可供；条款有效期按当前业务日期
/// 判定，业务日期由 Service 注入。
fn qualified_supply(facts: &CreationBasisFacts, offering: &SupplierOffering) -> Result<Option<LineSupply>> {
    let Some(revision_id) = offering.stable.current_revision_id.clone() else {
        return Ok(None);
    };
    let Some(revision) = facts.revisions.get(&revision_id) else {
        return Ok(None);
    };
    let today = BusinessDate::today();
    if revision.valid_from > today || revision.valid_to.is_some_and(|valid_to| valid_to < today) {
        return Ok(None);
    }
    let Some(availability) = facts.availabilities.get(&offering.base.id.to_string()) else {
        return Ok(None);
    };
    if availability.availability_status != AvailabilityStatus::Available {
        return Ok(None);
    }
    if availability
        .available_quantity
        .is_some_and(|quantity| quantity < zero_quantity())
    {
        return Err(Error::BusinessLogicError("供应商可供数量不能为负".to_string()));
    }
    if availability
        .available_quantity
        .is_some_and(|quantity| quantity == zero_quantity())
    {
        return Ok(None);
    }
    Ok(Some(LineSupply {
        offering: offering.clone(),
        revision: revision.clone(),
        availability: availability.clone(),
    }))
}

/// 构造一条精确创建依据视图。
///
/// # 参数
/// * `order` - 销售稳定单
/// * `group` - 精确依据分组
/// * `facts` - 批量供应商结算事实
/// * `sales_owner_name` - 销售负责人展示名
/// * `work_item_id` - 冻结本依据责任范围的开放供给分配任务
///
/// # 返回
/// 返回前端可直接选择逐行数量的依据视图。
///
/// # 错误
/// 履约期限无法转换为业务日期时返回错误。
///
/// # 关键业务约束
/// 预计金额按 `max_create_quantity` 逐行舍入后汇总；供应商名称只从批量事实读取。
pub(super) fn build_basis_view(
    order: &SalesOrder,
    group: &BasisGroup,
    facts: &CreationBasisFacts,
    sales_owner_name: Option<String>,
    work_item_id: &str,
) -> Result<CreationBasisView> {
    let supplier_name = facts
        .supplier_names
        .get(&group.scope.supplier_id.to_string())
        .cloned()
        .unwrap_or_else(|| group.scope.supplier_id.to_string());
    let mut estimated = zero_amount();
    let mut lines = Vec::with_capacity(group.lines.len());
    for line in &group.lines {
        let cost = supply_cost(&line.supply.revision, group.scope.fulfillment_responsibility);
        let (gross, _, _) = line_amounts(
            cost,
            line.max_create_quantity,
            line.supply.revision.input_tax_rate,
        );
        estimated = estimated.checked_add(gross);
        lines.push(basis_line_view(line, &group.scope.supplier_id, cost, gross)?);
    }
    Ok(CreationBasisView {
        work_item_id: work_item_id.to_string(),
        basis_id: basis_id_for(order, group, work_item_id, None),
        source_type: SupplySourceType::Purchase,
        sales_order_id: order.base.id.clone(),
        sales_order_no: order.order_no.clone(),
        customer_name: group.revision.customer_snapshot.customer_name.clone(),
        contract_no: group
            .revision
            .contract_snapshot
            .as_ref()
            .map(|snapshot| snapshot.contract_no.clone()),
        sales_owner_name,
        sales_order_revision_id: group.revision.base.id.clone(),
        supplier_id: group.scope.supplier_id.to_string(),
        supplier_name,
        stock_balance_id: None,
        warehouse_id: None,
        warehouse_name: None,
        source_available_quantity: None,
        purchase_type: group.scope.purchase_type.as_str().to_string(),
        fulfillment_responsibility: group.scope.fulfillment_responsibility.as_str().to_string(),
        payment_term_code: group.scope.payment_term_code.clone(),
        business_category: group.business_category.clone(),
        lines,
        estimated_gross: estimated.to_string(),
    })
}

/// 构造一个现有库存供给依据视图。
pub(super) fn build_stock_basis_view(
    order: &SalesOrder,
    group: &StockBasisGroup,
    sales_owner_name: Option<String>,
    work_item_id: &str,
) -> Result<CreationBasisView> {
    let mut lines = Vec::with_capacity(group.lines.len());
    for line in &group.lines {
        let sales_delivery_deadline =
            business_date_of(line.coverage.goods_line.fulfillment_due_at)?.to_string();
        lines.push(CreationBasisLineView {
            sales_order_line_id: line.coverage.revision_line.sales_order_line_id.to_string(),
            sales_order_revision_line_id: line.coverage.revision_line.base.id.clone(),
            sales_line_no: line.coverage.revision_line.line_no,
            supplier_id: String::new(),
            sales_quantity: line.coverage.summary.total_quantity.to_string(),
            covered_quantity: line.coverage.summary.covered_quantity.to_string(),
            remaining_quantity: line.coverage.summary.remaining_quantity.to_string(),
            max_create_quantity: line.max_create_quantity.to_string(),
            confirmed_quantity: line.max_create_quantity.to_string(),
            latest_cost_gross: "0".to_string(),
            input_tax_rate: "0".to_string(),
            expected_delivery_date: sales_delivery_deadline.clone(),
            sales_delivery_deadline,
            product_name: Some(line.coverage.revision_line.item_name_snapshot.clone()),
            specification: line.coverage.revision_line.spec_snapshot.clone(),
            unit: line.coverage.revision_line.unit_snapshot.clone(),
            gross_amount: "0".to_string(),
        });
    }
    Ok(CreationBasisView {
        work_item_id: work_item_id.to_string(),
        basis_id: stock_basis_id_for(order, group, work_item_id),
        source_type: SupplySourceType::ExistingStock,
        sales_order_id: order.base.id.clone(),
        sales_order_no: order.order_no.clone(),
        customer_name: group.revision.customer_snapshot.customer_name.clone(),
        contract_no: group
            .revision
            .contract_snapshot
            .as_ref()
            .map(|snapshot| snapshot.contract_no.clone()),
        sales_owner_name,
        sales_order_revision_id: group.revision.base.id.clone(),
        supplier_id: String::new(),
        supplier_name: format!("现有库存 · {}", group.warehouse_name),
        stock_balance_id: Some(group.balance.base.id.clone()),
        warehouse_id: Some(group.balance.warehouse_id.to_string()),
        warehouse_name: Some(group.warehouse_name.clone()),
        source_available_quantity: Some(group.balance.available_quantity.to_string()),
        purchase_type: PurchaseType::Physical.as_str().to_string(),
        fulfillment_responsibility: FulfillmentResponsibility::Warehouse.as_str().to_string(),
        payment_term_code: String::new(),
        business_category: None,
        lines,
        estimated_gross: "0".to_string(),
    })
}

/// 构造单条依据行视图。
///
/// # 参数
/// * `line` - 精确依据行
/// * `supplier_id` - 当前依据供应商
/// * `cost` - 当前供给含税成本
/// * `gross` - 按最大可创建数量计算的含税行金额
///
/// # 返回
/// 返回销售目标、覆盖、剩余与可创建数量视图。
///
/// # 错误
/// 履约期限无法转换为业务日期时返回错误。
///
/// # 关键业务约束
/// 稳定销售行和当前销售版本行同时下发。
fn basis_line_view(
    line: &BasisLine,
    supplier_id: &SupplierAccountId,
    cost: UnitPrice,
    gross: Amount,
) -> Result<CreationBasisLineView> {
    let sales_delivery_deadline = business_date_of(line.coverage.goods_line.fulfillment_due_at)?.to_string();
    Ok(CreationBasisLineView {
        sales_order_line_id: stable_line_id(line).to_string(),
        sales_order_revision_line_id: line.coverage.revision_line.base.id.clone(),
        sales_line_no: line.coverage.revision_line.line_no,
        supplier_id: supplier_id.to_string(),
        sales_quantity: line.coverage.summary.total_quantity.to_string(),
        covered_quantity: line.coverage.summary.covered_quantity.to_string(),
        remaining_quantity: line.coverage.summary.remaining_quantity.to_string(),
        max_create_quantity: line.max_create_quantity.to_string(),
        confirmed_quantity: line.max_create_quantity.to_string(),
        latest_cost_gross: cost.to_string(),
        input_tax_rate: line.supply.revision.input_tax_rate.to_string(),
        expected_delivery_date: sales_delivery_deadline.clone(),
        sales_delivery_deadline,
        product_name: Some(line.coverage.revision_line.item_name_snapshot.clone()),
        specification: line.coverage.revision_line.spec_snapshot.clone(),
        unit: line.coverage.revision_line.unit_snapshot.clone(),
        gross_amount: gross.to_string(),
    })
}

/// 将精确时间转换为上海业务自然日。
///
/// # 参数
/// * `instant` - 销售履约期限
///
/// # 返回
/// 返回 Asia/Shanghai 自然日。
///
/// # 错误
/// 时区或日期构造失败时返回内部错误。
///
/// # 关键业务约束
/// 不按 UTC 日期截断。
pub(super) fn business_date_of(instant: Instant) -> Result<BusinessDate> {
    let business_tz = FixedOffset::east_opt(8 * 60 * 60)
        .ok_or_else(|| Error::Internal("无法形成 Asia/Shanghai 时区".to_string()))?;
    let naive = instant.as_utc().with_timezone(&business_tz).date_naive();
    BusinessDate::from_ymd(naive.year(), naive.month(), naive.day())
        .ok_or_else(|| Error::Internal("履约期限日期非法".to_string()))
}

/// 返回合法采购数量零值。
///
/// # 参数
/// 无。
///
/// # 返回
/// 返回六位精度数量零值。
///
/// # 错误
/// 无。
///
/// # 关键业务约束
/// 只用于边界比较，不代表缺失业务数量。
pub(super) fn zero_quantity() -> Quantity {
    Quantity::from_str("0").expect("零数量合法")
}
