//! 采购领域已形成的精确依据到 HTTP 展示视图的映射。
use std::collections::BTreeSet;

use erp_core::ids::SupplierAccountId;
use erp_core::money::{Amount, UnitPrice, line_amounts};
use erp_procurement::entity::facts::SalesOrderBasisFact;
use erp_procurement::entity::purchase_order::{
    BasisGroup, BasisLine, BasisScope, CreationBasisFacts, FulfillmentResponsibility, PurchaseType,
    StockBasisGroup, StockBasisLine, SupplyPriceBasis, basis_id_for, stable_line_id, stock_basis_id_for,
};
use erp_procurement::service::purchase_order::creation_basis::business_date_of;
use erp_procurement::service::purchase_order::shared::zero_amount;

use super::super::dto::{
    CreationBasisDeliveryView, CreationBasisLineView, CreationBasisView, DeliveryRegionTermView,
    WarehouseDeliveryTermView,
};
use super::super::query;
use super::super::repository::WarehouseRegionFact;
use crate::Result;
/// 构造一条精确创建依据视图。
///
/// # 参数
/// * `order` - 销售稳定单
/// * `group` - 精确依据分组
/// * `facts` - 批量供应商结算事实
/// * `sales_owner_name` - 销售负责人展示名
/// * `work_item_id` - 冻结本依据责任范围的开放供给分配任务
/// * `warehouses` - 启用仓库所在地区，用于集采入仓判定
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
    order: &SalesOrderBasisFact,
    group: &BasisGroup,
    facts: &CreationBasisFacts,
    sales_owner_name: Option<String>,
    work_item_id: &str,
    warehouses: &[WarehouseRegionFact],
) -> Result<CreationBasisView> {
    let supplier_name = query::supplier_display(group.scope.supplier_id.as_ref(), &facts.supplier_names);
    let mut estimated = zero_amount();
    let mut lines = Vec::with_capacity(group.lines.len());
    for line in &group.lines {
        let cost = line.unit_cost(group.scope.price_basis);
        let (gross, _, _) = line_amounts(cost, line.max_create_quantity, line.tax_rate());
        estimated = estimated.checked_add(gross);
        let delivery = delivery_view(line, &group.scope, warehouses);
        lines
            .push(basis_line_view(line, &group.scope.supplier_id, cost, gross)?.with_delivery_view(delivery));
    }
    Ok(CreationBasisView::new(
        work_item_id.to_string(),
        basis_id_for(order, group, work_item_id, None),
        order.base.id.clone(),
        order.order_no.clone(),
        group.revision.customer_snapshot.customer_name.clone(),
        group.revision.base.id.clone(),
        group.scope.supplier_id.to_string(),
        supplier_name,
        group.scope.purchase_type.as_str().to_string(),
        group.scope.fulfillment_responsibility.as_str().to_string(),
        group.scope.payment_term_code.clone(),
        estimated.to_string(),
    )
    .with_contract_no(group.revision.contract_snapshot.as_ref().map(|snapshot| snapshot.contract_no.clone()))
    .with_sales_owner_name(sales_owner_name)
    .with_business_category(group.business_category.clone())
    .with_price_basis(group.scope.price_basis.as_str())
    .with_inquiry_source(group.lines.first().and_then(|line| line.inquiry_quote.as_ref()))
    .with_inquiry_reselection_required(
        facts.source_inquiry_id.is_some() && group.scope.inquiry_quote_revision_id.is_none(),
    )
    .with_lines(lines))
}

/// 按依据范围计算一条依据行的配送判定视图。
///
/// # 参数
/// * `line` - 依据行
/// * `scope` - 依据范围
/// * `warehouses` - 启用仓库所在地区
///
/// # 返回
/// 直发返回逐销售收货地区判定，集采入仓返回逐仓库判定，集采直发附带起订量。
///
/// # 错误
/// 无。
fn delivery_view(
    line: &BasisLine,
    scope: &BasisScope,
    warehouses: &[WarehouseRegionFact],
) -> CreationBasisDeliveryView {
    let delivery = &line.supply.delivery;
    let mode = scope.price_basis.delivery_mode();
    let mut view = CreationBasisDeliveryView {
        delivery_template_name: delivery.template_name.clone(),
        supplier_offering_id: Some(line.supply.offering.base.id.clone()),
        ..CreationBasisDeliveryView::default()
    };
    match scope.fulfillment_responsibility {
        FulfillmentResponsibility::SupplierDirect => {
            let regions = line
                .coverage
                .goods_line
                .delivery_allocations
                .iter()
                .map(|allocation| allocation.region_code.clone())
                .collect::<BTreeSet<_>>();
            view.delivery_terms = regions
                .into_iter()
                .map(|region| DeliveryRegionTermView {
                    term: delivery.policy.judge(mode, &region),
                    region_code: region.to_string(),
                })
                .collect();
        },
        FulfillmentResponsibility::Warehouse => {
            view.warehouse_delivery_terms = warehouses
                .iter()
                .map(|warehouse| WarehouseDeliveryTermView {
                    warehouse_id: warehouse.warehouse_id.clone(),
                    region_name: warehouse.region_code.full_name(),
                    term: delivery.policy.judge(mode, &warehouse.region_code),
                })
                .collect();
        },
        _ => {},
    }
    if scope.fulfillment_responsibility == FulfillmentResponsibility::SupplierDirect
        && scope.price_basis == SupplyPriceBasis::Bulk
        && line.inquiry_quote.is_none()
    {
        view.bulk_minimum_order_quantity = Some(line.supply.revision.bulk_minimum_order_quantity.to_string());
    }
    view
}

/// 构造一个现有库存供给依据视图。
///
/// # 参数
/// * `order` - 销售稳定单。
/// * `group` - 现有库存供给分组。
/// * `sales_owner_name` - 销售负责人展示名。
/// * `work_item_id` - 冻结本依据责任范围的开放任务。
///
/// # 返回
/// 返回现有库存供给依据视图。
///
/// # 错误
/// 履约期限无法转换为业务日期时返回错误。
pub(super) fn build_stock_basis_view(
    order: &SalesOrderBasisFact,
    group: &StockBasisGroup,
    sales_owner_name: Option<String>,
    work_item_id: &str,
) -> Result<CreationBasisView> {
    let lines = group.lines.iter().map(stock_basis_line_view).collect::<Result<Vec<_>>>()?;
    Ok(CreationBasisView::new(
        work_item_id.to_string(),
        stock_basis_id_for(order, group, work_item_id),
        order.base.id.clone(),
        order.order_no.clone(),
        group.revision.customer_snapshot.customer_name.clone(),
        group.revision.base.id.clone(),
        String::new(),
        format!("现有库存 · {}", group.warehouse_name),
        PurchaseType::Physical.as_str().to_string(),
        FulfillmentResponsibility::Warehouse.as_str().to_string(),
        String::new(),
        "0".to_string(),
    )
    .with_contract_no(group.revision.contract_snapshot.as_ref().map(|snapshot| snapshot.contract_no.clone()))
    .with_sales_owner_name(sales_owner_name)
    .with_stock_source(
        Some(group.balance.base.id.clone()),
        Some(group.balance.warehouse_id.to_string()),
        Some(group.warehouse_name.clone()),
        Some(group.balance.available_quantity.to_string()),
    )
    .with_lines(lines))
}

/// 构造单条依据行视图。
fn basis_line_view(
    line: &BasisLine,
    supplier_id: &SupplierAccountId,
    cost: UnitPrice,
    gross: Amount,
) -> Result<CreationBasisLineView> {
    let sales_delivery_deadline = business_date_of(line.coverage.goods_line.fulfillment_due_at)?.to_string();
    Ok(CreationBasisLineView::new(
        stable_line_id(line).to_string(),
        line.coverage.revision_line.base.id.clone(),
        supplier_id.to_string(),
        line.coverage.summary.total_quantity.to_string(),
        line.coverage.summary.covered_quantity.to_string(),
        line.coverage.summary.remaining_quantity.to_string(),
        line.max_create_quantity.to_string(),
        line.max_create_quantity.to_string(),
        cost.to_string(),
        line.tax_rate().to_string(),
        sales_delivery_deadline.clone(),
        sales_delivery_deadline,
        gross.to_string(),
    )
    .with_delivery(
        line.coverage.goods_line.delivery_allocations.clone(),
        line.coverage.goods_line.assigned_delivery_allocations.clone(),
    )
    .with_quantity_scale(line.coverage.quantity_scale)
    .with_sales_line_no(line.coverage.revision_line.line_no)
    .with_product_name(Some(line.coverage.revision_line.item_name_snapshot.clone()))
    .with_specification(line.coverage.revision_line.spec_snapshot.clone())
    .with_unit(line.coverage.revision_line.unit_snapshot.clone()))
}

fn stock_basis_line_view(line: &StockBasisLine) -> Result<CreationBasisLineView> {
    let sales_delivery_deadline = business_date_of(line.coverage.goods_line.fulfillment_due_at)?.to_string();
    Ok(CreationBasisLineView::new(
        line.coverage.revision_line.sales_order_line_id.to_string(),
        line.coverage.revision_line.base.id.clone(),
        String::new(),
        line.coverage.summary.total_quantity.to_string(),
        line.coverage.summary.covered_quantity.to_string(),
        line.coverage.summary.remaining_quantity.to_string(),
        line.max_create_quantity.to_string(),
        line.max_create_quantity.to_string(),
        "0".to_string(),
        "0".to_string(),
        sales_delivery_deadline.clone(),
        sales_delivery_deadline,
        "0".to_string(),
    )
    .with_delivery(
        line.coverage.goods_line.delivery_allocations.clone(),
        line.coverage.goods_line.assigned_delivery_allocations.clone(),
    )
    .with_quantity_scale(line.coverage.quantity_scale)
    .with_sales_line_no(line.coverage.revision_line.line_no)
    .with_product_name(Some(line.coverage.revision_line.item_name_snapshot.clone()))
    .with_specification(line.coverage.revision_line.spec_snapshot.clone())
    .with_unit(line.coverage.revision_line.unit_snapshot.clone()))
}
