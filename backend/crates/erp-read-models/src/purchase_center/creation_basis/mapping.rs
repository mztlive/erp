//! 采购领域已形成的精确依据到 HTTP 展示视图的映射。
use super::super::dto::{CreationBasisLineView, CreationBasisView};
use erp_core::ids::SupplierAccountId;
use erp_core::money::{line_amounts, Amount, UnitPrice};
use erp_procurement::entity::facts::SalesOrderBasisFact;
use erp_procurement::entity::purchase_order::{
    basis_id_for, stable_line_id, stock_basis_id_for, supply_cost, BasisGroup, BasisLine, CreationBasisFacts,
    FulfillmentResponsibility, PurchaseType, StockBasisGroup, SupplySourceType,
};
use erp_procurement::service::purchase_order::creation_basis::business_date_of;
use erp_procurement::service::purchase_order::shared::zero_amount;
use services::Result;
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
    order: &SalesOrderBasisFact,
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
    order: &SalesOrderBasisFact,
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
