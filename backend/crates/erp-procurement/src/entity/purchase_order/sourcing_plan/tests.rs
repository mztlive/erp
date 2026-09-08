use crate::entity::facts::{
    AvailabilityFact as SupplierOfferingAvailability, AvailabilityStatus, CurrentRevisionFact, FactIdentity,
    OfferingFact as SupplierOffering, OfferingRevisionFact as SupplierOfferingRevision, ProductKind,
    SalesCustomerSnapshotFact, SalesGoodsLineFact as SalesOrderGoodsServiceLineRevision,
    SalesLineType as LineType, SalesOrderBasisFact, SalesRevisionFact as SalesOrderRevision,
    SalesRevisionLineFact as SalesOrderRevisionLine, StockBalanceFact as StockBalance, VersionedFactIdentity,
};
use crate::entity::purchase_order::ProcurementCoverageSummary;
use std::str::FromStr;

use erp_core::common::time::Instant;
use erp_core::ids::{SalesOrderRevisionLineId, SkuId, SupplierAccountId, SupplierOfferingId, WarehouseId};
use erp_core::money::{Quantity, Rate, UnitPrice};

use super::{
    stock_basis_id_for, SourcingAssignment, SourcingAssignmentSet, SourcingPlan, SourcingPlanError,
    StockBasisGroup, StockBasisLine, SupplySourceType,
};
use crate::entity::purchase_order::coverage::SalesProcurementCoverageLine;
use crate::entity::purchase_order::creation_basis::{
    basis_id_for, BasisGroup, BasisLine, BasisScope, LineSupply,
};
use crate::entity::purchase_order::{FulfillmentResponsibility, PurchaseType};

/// 构造销售当前版本头。
fn revision(id: &str) -> SalesOrderRevision {
    SalesOrderRevision {
        base: FactIdentity {
            id: format!("rev-{id}"),
        },
        customer_snapshot: SalesCustomerSnapshotFact {
            customer_name: "客户".to_string(),
        },
        contract_snapshot: None,
    }
}

/// 构造销售当前版本公共行。
fn revision_line(id: &str, stable_line_id: &str) -> SalesOrderRevisionLine {
    SalesOrderRevisionLine {
        base: FactIdentity { id: id.to_string() },
        sales_order_line_id: erp_core::ids::SalesOrderLineId::new(stable_line_id),
        line_no: 1,
        line_type: LineType::GoodsService,
        item_name_snapshot: "商品".to_string(),
        spec_snapshot: Some("规格".to_string()),
        unit_snapshot: Some("件".to_string()),
    }
}

/// 构造销售当前版本商品/服务子类型行。
fn goods_line(revision_line_id: &str) -> SalesOrderGoodsServiceLineRevision {
    SalesOrderGoodsServiceLineRevision {
        revision_line_id: SalesOrderRevisionLineId::new(revision_line_id),
        sku_id: SkuId::new("sku-1"),
        sku_revision_id: erp_core::ids::SkuRevisionId::new("skur-1"),
        fulfillment_due_at: Instant::from_unix_secs(1_800_000_000),
        quantity: Quantity::from_str("10").unwrap(),
        base_unit_code: "件".to_string(),
    }
}

/// 构造一条销售覆盖目标行；剩余量等于目标减覆盖。
fn coverage_line(stable_line_id: &str, total: &str, covered: &str) -> SalesProcurementCoverageLine {
    SalesProcurementCoverageLine {
        quantity_scale: Some(6),
        revision_line: revision_line(&format!("sorl-{stable_line_id}"), stable_line_id),
        goods_line: goods_line(&format!("sorl-{stable_line_id}")),
        product_kind: ProductKind::Physical,
        summary: ProcurementCoverageSummary::new(
            Quantity::from_str(total).unwrap(),
            Quantity::from_str(covered).unwrap(),
        )
        .unwrap(),
    }
}

/// 计算剩余量文本对应的数量。
fn remaining(total: &str, covered: &str) -> Quantity {
    Quantity::try_from(
        Quantity::from_str(total).unwrap().to_decimal() - Quantity::from_str(covered).unwrap().to_decimal(),
    )
    .unwrap()
}

/// 构造供给稳定身份。
fn offering(id: &str, supplier_id: &str) -> SupplierOffering {
    SupplierOffering {
        base: FactIdentity { id: id.to_string() },
        stable: CurrentRevisionFact::default(),
        sku_id: SkuId::new("sku-1"),
        supplier_id: SupplierAccountId::new(supplier_id),
    }
}

/// 构造供给商业条款修订。
fn offering_revision(offering_id: &str) -> SupplierOfferingRevision {
    SupplierOfferingRevision {
        base: FactIdentity {
            id: format!("offrev-{offering_id}"),
        },
        dropship_supply_price_gross: UnitPrice::from_str("6").unwrap(),
        bulk_supply_price_gross: UnitPrice::from_str("5").unwrap(),
        input_tax_rate: Rate::from_str("0.13").unwrap(),
        valid_from: erp_core::common::time::BusinessDate::from_str("2026-01-01").unwrap(),
        valid_to: None,
    }
}

/// 构造供给实时可供投影。
fn availability(offering_id: &str, quantity: &str) -> SupplierOfferingAvailability {
    SupplierOfferingAvailability {
        base: VersionedFactIdentity {
            id: format!("avail-{offering_id}"),
            version: 1,
        },
        supplier_offering_id: SupplierOfferingId::new(offering_id),
        availability_status: AvailabilityStatus::Available,
        available_quantity: Some(Quantity::from_str(quantity).unwrap()),
        source_revision_token: None,
    }
}

/// 构造一条合格供给。
fn line_supply(offering_id: &str, supplier_id: &str, available: &str) -> LineSupply {
    LineSupply {
        offering: offering(offering_id, supplier_id),
        revision: offering_revision(offering_id),
        availability: availability(offering_id, available),
    }
}

/// 构造销售稳定单。
fn sales_order(id: &str) -> SalesOrderBasisFact {
    SalesOrderBasisFact {
        base: FactIdentity { id: id.to_string() },
        order_no: format!("SO-{id}"),
        procurement_guard_version: 3,
        is_effective: true,
    }
}

/// 构造完整精确依据分组；行元组为 `(稳定销售行, 目标数量, 覆盖数量)`。
fn basis_group(
    supplier_id: &str,
    payment_term_code: &str,
    fulfillment: FulfillmentResponsibility,
    supply_available: &str,
    lines: &[(&str, &str, &str)],
) -> BasisGroup {
    let lines = lines
        .iter()
        .map(|(line_id, total, covered)| BasisLine {
            coverage: coverage_line(line_id, total, covered),
            supply: line_supply("offering-1", supplier_id, supply_available),
            max_create_quantity: remaining(total, covered),
        })
        .collect();
    BasisGroup {
        revision: revision("1"),
        scope: BasisScope {
            supplier_id: SupplierAccountId::new(supplier_id),
            purchase_type: PurchaseType::Physical,
            payment_term_code: payment_term_code.to_string(),
            fulfillment_responsibility: fulfillment,
        },
        business_category: None,
        lines,
    }
}

/// 构造现有库存余额依据；行元组为 `(稳定销售行, 目标数量, 覆盖数量)`。
fn stock_basis_group(
    balance_id: &str,
    warehouse_id: &str,
    available: &str,
    lines: &[(&str, &str, &str)],
) -> StockBasisGroup {
    let available = Quantity::from_str(available).unwrap();
    let lines = lines
        .iter()
        .map(|(line_id, total, covered)| StockBasisLine {
            coverage: coverage_line(line_id, total, covered),
            max_create_quantity: remaining(total, covered),
        })
        .collect();
    StockBasisGroup {
        revision: revision("1"),
        balance: StockBalance {
            base: VersionedFactIdentity {
                id: balance_id.to_string(),
                version: 1,
            },
            warehouse_id: WarehouseId::new(warehouse_id),
            sku_id: SkuId::new("sku-1"),
            available_quantity: available,
        },
        warehouse_name: warehouse_id.to_string(),
        lines,
    }
}

/// 构造一条已类型化的选源行。
fn assignment(
    line_id: &str,
    basis_id: &str,
    source_type: SupplySourceType,
    target_warehouse_id: Option<&str>,
    quantity: &str,
) -> SourcingAssignment {
    SourcingAssignment {
        sales_order_line_id: line_id.to_string(),
        basis_id: basis_id.to_string(),
        source_type,
        target_warehouse_id: target_warehouse_id.map(str::to_string),
        quantity: Quantity::from_str(quantity).unwrap(),
        expected_delivery_date: erp_core::common::time::BusinessDate::from_str("2026-09-01").unwrap(),
    }
}

/// 解析成功时去除首尾空白并类型化数量与日期。
#[test]
fn parse_trims_and_types_valid_assignment() {
    let parsed = SourcingAssignment::parse(
        " sol-1 ",
        " basis-1 ",
        SupplySourceType::Purchase,
        Some(" wh-1 "),
        " 10 ",
        " 2026-09-01 ",
    )
    .expect("合法选源行必须解析成功");

    assert_eq!(parsed.sales_order_line_id, "sol-1");
    assert_eq!(parsed.basis_id, "basis-1");
    assert_eq!(parsed.target_warehouse_id.as_deref(), Some("wh-1"));
    assert_eq!(parsed.quantity, Quantity::from_str("10").unwrap());
}

/// 空白销售行必须拒绝。
#[test]
fn parse_rejects_blank_sales_line() {
    let error = SourcingAssignment::parse(
        " ",
        "basis-1",
        SupplySourceType::Purchase,
        None,
        "10",
        "2026-09-01",
    )
    .expect_err("空白销售行必须失败");
    assert_eq!(error.to_string(), "销售行不能为空");
}

/// 空白依据必须拒绝。
#[test]
fn parse_rejects_blank_basis() {
    let error = SourcingAssignment::parse("sol-1", " ", SupplySourceType::Purchase, None, "10", "2026-09-01")
        .expect_err("空白依据必须失败");
    assert_eq!(error.to_string(), "履约方案不能为空");
}

/// 非法数量文本必须拒绝。
#[test]
fn parse_rejects_invalid_quantity() {
    let error = SourcingAssignment::parse(
        "sol-1",
        "basis-1",
        SupplySourceType::Purchase,
        None,
        "abc",
        "2026-09-01",
    )
    .expect_err("非法数量必须失败");
    assert!(error.to_string().starts_with("本次分配数量非法"));
}

/// 非法预计交付日必须拒绝。
#[test]
fn parse_rejects_invalid_delivery_date() {
    let error = SourcingAssignment::parse(
        "sol-1",
        "basis-1",
        SupplySourceType::Purchase,
        None,
        "10",
        "not-a-date",
    )
    .expect_err("非法日期必须失败");
    assert!(error.to_string().starts_with("预计交付日非法"));
}

/// 零数量与负数量必须拒绝。
#[test]
fn parse_rejects_zero_and_negative_quantity() {
    for quantity in ["0", "-1"] {
        let error = SourcingAssignment::parse(
            "sol-1",
            "basis-1",
            SupplySourceType::Purchase,
            None,
            quantity,
            "2026-09-01",
        )
        .expect_err("非正数量必须失败");
        assert_eq!(error.to_string(), "本次分配数量必须大于 0");
    }
}

/// 现有库存不能另行指定目标仓。
#[test]
fn parse_rejects_target_warehouse_for_existing_stock() {
    let error = SourcingAssignment::parse(
        "sol-1",
        "basis-1",
        SupplySourceType::ExistingStock,
        Some("wh-1"),
        "10",
        "2026-09-01",
    )
    .expect_err("现有库存指定目标仓必须失败");
    assert_eq!(
        error.to_string(),
        "现有库存由所选库存余额确定仓库，不能另行指定目标仓"
    );
}

/// 现有库存不携带目标仓时解析成功。
#[test]
fn parse_accepts_existing_stock_without_target_warehouse() {
    let parsed = SourcingAssignment::parse(
        "sol-1",
        "basis-1",
        SupplySourceType::ExistingStock,
        None,
        "10",
        "2026-09-01",
    )
    .expect("现有库存不指定目标仓必须成功");
    assert_eq!(parsed.source_type, SupplySourceType::ExistingStock);
    assert_eq!(parsed.target_warehouse_id, None);
}

/// 空白目标仓归一为未指定。
#[test]
fn parse_treats_blank_target_warehouse_as_none() {
    let parsed = SourcingAssignment::parse(
        "sol-1",
        "basis-1",
        SupplySourceType::Purchase,
        Some("  "),
        "10",
        "2026-09-01",
    )
    .expect("空白目标仓必须归一为空");
    assert_eq!(parsed.target_warehouse_id, None);
}

/// 同一销售行可拆到不同依据，但同一依据不能重复。
#[test]
fn normalize_allows_split_and_rejects_duplicate() {
    let set = SourcingAssignmentSet::normalize(&[
        assignment("sol-1", "basis-a", SupplySourceType::Purchase, None, "1"),
        assignment("sol-1", "basis-b", SupplySourceType::Purchase, None, "1"),
    ])
    .expect("不同依据允许拆分");
    assert_eq!(set.assignments().len(), 2);

    let error = SourcingAssignmentSet::normalize(&[
        assignment("sol-1", "basis-a", SupplySourceType::Purchase, None, "1"),
        assignment("sol-1", "basis-a", SupplySourceType::Purchase, None, "1"),
    ])
    .expect_err("重复分配必须失败");
    assert_eq!(error.to_string(), "同一销售行不能重复使用同一履约方案");
}

/// 规范化结果按销售行与依据稳定排序。
#[test]
fn normalize_sorts_stably_by_line_then_basis() {
    let set = SourcingAssignmentSet::normalize(&[
        assignment("sol-2", "basis-a", SupplySourceType::Purchase, None, "1"),
        assignment("sol-1", "basis-b", SupplySourceType::Purchase, None, "1"),
        assignment("sol-1", "basis-a", SupplySourceType::Purchase, None, "1"),
    ])
    .expect("合法集合必须成功");

    let ids = set
        .assignments()
        .iter()
        .map(|line| (line.sales_order_line_id.as_str(), line.basis_id.as_str()))
        .collect::<Vec<_>>();
    assert_eq!(
        ids,
        vec![("sol-1", "basis-a"), ("sol-1", "basis-b"), ("sol-2", "basis-a")]
    );
}

/// 空集合必须归一为空计划。
#[test]
fn normalize_accepts_empty_set() {
    let set = SourcingAssignmentSet::normalize(&[]).expect("空集合必须成功");
    assert!(set.assignments().is_empty());
}

/// 同一拆分维度与目标仓的多销售行合并为一张采购单。
#[test]
fn purchase_assignments_merge_into_one_plan_per_scope_and_warehouse() {
    let order = sales_order("so-1");
    let group = basis_group(
        "supplier-1",
        "NET-30",
        FulfillmentResponsibility::Warehouse,
        "8",
        &[("sol-1", "10", "2"), ("sol-2", "10", "2")],
    );
    let basis_id = basis_id_for(&order, &group, "wi-1", None);
    let assignments = SourcingAssignmentSet::normalize(&[
        assignment("sol-1", &basis_id, SupplySourceType::Purchase, Some("wh-1"), "3"),
        assignment("sol-2", &basis_id, SupplySourceType::Purchase, Some("wh-1"), "2"),
    ])
    .expect("合法集合必须成功");

    let plan = SourcingPlan::plan(&order, &[group], &[], "wi-1", &assignments).expect("计划必须成功");
    let drafts = plan.purchase_plans();
    assert_eq!(drafts.len(), 1);
    assert_eq!(
        drafts[0].target_warehouse_id.as_ref().map(ToString::to_string),
        Some("wh-1".to_string())
    );
    assert_eq!(drafts[0].requested_lines.len(), 2);
    assert!(plan.stock_plans().is_empty());
}

/// 同一依据的不同销售行指定不同目标仓时必须拆分为不同采购单。
#[test]
fn purchase_assignments_split_by_target_warehouse() {
    let order = sales_order("so-1");
    let group = basis_group(
        "supplier-1",
        "NET-30",
        FulfillmentResponsibility::Warehouse,
        "8",
        &[("sol-1", "10", "2"), ("sol-2", "10", "2")],
    );
    let basis_id = basis_id_for(&order, &group, "wi-1", None);
    let assignments = SourcingAssignmentSet::normalize(&[
        assignment("sol-1", &basis_id, SupplySourceType::Purchase, Some("wh-1"), "3"),
        assignment("sol-2", &basis_id, SupplySourceType::Purchase, Some("wh-2"), "2"),
    ])
    .expect("合法集合必须成功");

    let plan = SourcingPlan::plan(&order, &[group], &[], "wi-1", &assignments).expect("计划必须成功");
    let warehouses = plan
        .purchase_plans()
        .iter()
        .map(|plan| plan.target_warehouse_id.as_ref().map(ToString::to_string))
        .collect::<Vec<_>>();
    assert_eq!(
        warehouses,
        vec![Some("wh-1".to_string()), Some("wh-2".to_string())]
    );
}

/// 不同付款条件必须拆分为不同采购单。
#[test]
fn purchase_assignments_split_by_scope() {
    let order = sales_order("so-1");
    let first = basis_group(
        "supplier-1",
        "NET-30",
        FulfillmentResponsibility::SupplierDirect,
        "8",
        &[("sol-1", "10", "2")],
    );
    let second = basis_group(
        "supplier-1",
        "NET-60",
        FulfillmentResponsibility::SupplierDirect,
        "8",
        &[("sol-2", "10", "2")],
    );
    let first_basis = basis_id_for(&order, &first, "wi-1", None);
    let second_basis = basis_id_for(&order, &second, "wi-1", None);
    let assignments = SourcingAssignmentSet::normalize(&[
        assignment("sol-1", &first_basis, SupplySourceType::Purchase, None, "3"),
        assignment("sol-2", &second_basis, SupplySourceType::Purchase, None, "2"),
    ])
    .expect("合法集合必须成功");

    let plan = SourcingPlan::plan(&order, &[first, second], &[], "wi-1", &assignments).expect("计划必须成功");
    assert_eq!(plan.purchase_plans().len(), 2);
}

/// 选源行依据失效必须失败关闭。
#[test]
fn purchase_assignment_with_unknown_basis_fails_closed() {
    let order = sales_order("so-1");
    let group = basis_group(
        "supplier-1",
        "NET-30",
        FulfillmentResponsibility::SupplierDirect,
        "8",
        &[("sol-1", "10", "2")],
    );
    let assignments = SourcingAssignmentSet::normalize(&[assignment(
        "sol-1",
        "unknown-basis",
        SupplySourceType::Purchase,
        None,
        "3",
    )])
    .expect("合法集合必须成功");

    let error =
        SourcingPlan::plan(&order, &[group], &[], "wi-1", &assignments).expect_err("失效依据必须失败");
    assert_eq!(error, SourcingPlanError::StaleFacts);
}

/// 仓库履约必须先选择目标收货仓。
#[test]
fn warehouse_fulfillment_requires_target_warehouse() {
    let order = sales_order("so-1");
    let group = basis_group(
        "supplier-1",
        "NET-30",
        FulfillmentResponsibility::Warehouse,
        "8",
        &[("sol-1", "10", "2")],
    );
    let basis_id = basis_id_for(&order, &group, "wi-1", None);
    let assignments = SourcingAssignmentSet::normalize(&[assignment(
        "sol-1",
        &basis_id,
        SupplySourceType::Purchase,
        None,
        "3",
    )])
    .expect("合法集合必须成功");

    let error =
        SourcingPlan::plan(&order, &[group], &[], "wi-1", &assignments).expect_err("缺少目标仓必须失败");
    assert_eq!(
        error,
        SourcingPlanError::WarehouseContract("仓库履约必须先选择目标收货仓".to_string())
    );
}

/// 非仓库履约不能指定目标收货仓。
#[test]
fn non_warehouse_fulfillment_rejects_target_warehouse() {
    let order = sales_order("so-1");
    let group = basis_group(
        "supplier-1",
        "NET-30",
        FulfillmentResponsibility::SupplierDirect,
        "8",
        &[("sol-1", "10", "2")],
    );
    let basis_id = basis_id_for(&order, &group, "wi-1", None);
    let assignments = SourcingAssignmentSet::normalize(&[assignment(
        "sol-1",
        &basis_id,
        SupplySourceType::Purchase,
        Some("wh-1"),
        "3",
    )])
    .expect("合法集合必须成功");

    let error = SourcingPlan::plan(&order, &[group], &[], "wi-1", &assignments)
        .expect_err("非仓库履约指定目标仓必须失败");
    assert_eq!(
        error,
        SourcingPlanError::WarehouseContract("非仓库履约不能指定目标收货仓".to_string())
    );
}

/// 现有库存按余额归组，同余额的多销售行合并为一次预占。
#[test]
fn stock_assignments_group_by_balance() {
    let order = sales_order("so-1");
    let group = stock_basis_group(
        "bal-1",
        "wh-1",
        "8",
        &[("sol-1", "10", "2"), ("sol-2", "10", "2")],
    );
    let basis_id = stock_basis_id_for(&order, &group, "wi-1");
    let assignments = SourcingAssignmentSet::normalize(&[
        assignment("sol-1", &basis_id, SupplySourceType::ExistingStock, None, "3"),
        assignment("sol-2", &basis_id, SupplySourceType::ExistingStock, None, "2"),
    ])
    .expect("合法集合必须成功");

    let plan = SourcingPlan::plan(&order, &[], &[group], "wi-1", &assignments).expect("计划必须成功");
    let stock_plans = plan.stock_plans();
    assert_eq!(stock_plans.len(), 1);
    assert_eq!(stock_plans[0].group.balance.base.id, "bal-1");
    assert_eq!(stock_plans[0].requested_lines.len(), 2);
    assert!(plan.purchase_plans().is_empty());
}

/// 不同余额必须拆分，未知余额依据失败关闭。
#[test]
fn stock_assignments_split_by_balance_and_reject_unknown() {
    let order = sales_order("so-1");
    let first = stock_basis_group("bal-1", "wh-1", "8", &[("sol-1", "10", "2")]);
    let second = stock_basis_group("bal-2", "wh-2", "8", &[("sol-2", "10", "2")]);
    let first_basis = stock_basis_id_for(&order, &first, "wi-1");
    let second_basis = stock_basis_id_for(&order, &second, "wi-1");
    let assignments = SourcingAssignmentSet::normalize(&[
        assignment("sol-1", &first_basis, SupplySourceType::ExistingStock, None, "3"),
        assignment("sol-2", &second_basis, SupplySourceType::ExistingStock, None, "2"),
    ])
    .expect("合法集合必须成功");

    let plan = SourcingPlan::plan(&order, &[], &[first, second], "wi-1", &assignments).expect("计划必须成功");
    assert_eq!(plan.stock_plans().len(), 2);

    let unknown = SourcingAssignmentSet::normalize(&[assignment(
        "sol-1",
        "unknown-balance",
        SupplySourceType::ExistingStock,
        None,
        "3",
    )])
    .expect("合法集合必须成功");
    let error = SourcingPlan::plan(
        &order,
        &[],
        &[stock_basis_group("bal-1", "wh-1", "8", &[("sol-1", "10", "2")])],
        "wi-1",
        &unknown,
    )
    .expect_err("未知余额依据必须失败");
    assert_eq!(error, SourcingPlanError::StaleFacts);
}

/// 库存与采购合计不得超过同一份最新剩余量。
#[test]
fn combined_purchase_and_stock_totals_capped_by_latest_remaining() {
    let order = sales_order("so-1");
    let purchase = basis_group(
        "supplier-1",
        "NET-30",
        FulfillmentResponsibility::SupplierDirect,
        "8",
        &[("sol-1", "10", "2")],
    );
    let stock = stock_basis_group("bal-1", "wh-1", "8", &[("sol-1", "10", "2")]);
    let purchase_basis = basis_id_for(&order, &purchase, "wi-1", None);
    let stock_basis = stock_basis_id_for(&order, &stock, "wi-1");

    let within = SourcingAssignmentSet::normalize(&[
        assignment("sol-1", &purchase_basis, SupplySourceType::Purchase, None, "5"),
        assignment("sol-1", &stock_basis, SupplySourceType::ExistingStock, None, "3"),
    ])
    .expect("合法集合必须成功");
    let plan = SourcingPlan::plan(
        &order,
        std::slice::from_ref(&purchase),
        std::slice::from_ref(&stock),
        "wi-1",
        &within,
    )
    .expect("合计等于剩余量必须成功");
    assert_eq!(plan.purchase_plans().len(), 1);
    assert_eq!(plan.stock_plans().len(), 1);

    let excess = SourcingAssignmentSet::normalize(&[
        assignment("sol-1", &purchase_basis, SupplySourceType::Purchase, None, "6"),
        assignment("sol-1", &stock_basis, SupplySourceType::ExistingStock, None, "3"),
    ])
    .expect("合法集合必须成功");
    let error = SourcingPlan::plan(&order, &[purchase], &[stock], "wi-1", &excess).expect_err("超量必须失败");
    assert_eq!(error, SourcingPlanError::StaleFacts);
}

/// 无选源行时计划为空。
#[test]
fn plan_without_assignments_is_empty() {
    let order = sales_order("so-1");
    let assignments = SourcingAssignmentSet::normalize(&[]).expect("空集合必须成功");
    let plan = SourcingPlan::plan(&order, &[], &[], "wi-1", &assignments).expect("空计划必须成功");
    assert!(plan.purchase_plans().is_empty());
    assert!(plan.stock_plans().is_empty());
}

/// 最新余额重验接受恰好等于行剩余量与余额可用量的分配。
#[test]
fn validate_against_latest_stock_accepts_exact_cap() {
    let order = sales_order("so-1");
    let group = stock_basis_group("bal-1", "wh-1", "10", &[("sol-1", "10", "2")]);
    let basis_id = stock_basis_id_for(&order, &group, "wi-1");
    let assignments = SourcingAssignmentSet::normalize(&[assignment(
        "sol-1",
        &basis_id,
        SupplySourceType::ExistingStock,
        None,
        "8",
    )])
    .expect("合法集合必须成功");
    let plan = SourcingPlan::plan(&order, &[], std::slice::from_ref(&group), "wi-1", &assignments)
        .expect("计划必须成功");

    plan.validate_against_latest_stock(&[group])
        .expect("恰好等于上限必须成功");
}

/// 最新行剩余量下降时重验失败关闭。
#[test]
fn validate_against_latest_stock_rejects_line_excess() {
    let order = sales_order("so-1");
    let group = stock_basis_group("bal-1", "wh-1", "10", &[("sol-1", "10", "2")]);
    let basis_id = stock_basis_id_for(&order, &group, "wi-1");
    let assignments = SourcingAssignmentSet::normalize(&[assignment(
        "sol-1",
        &basis_id,
        SupplySourceType::ExistingStock,
        None,
        "8",
    )])
    .expect("合法集合必须成功");
    let plan = SourcingPlan::plan(&order, &[], std::slice::from_ref(&group), "wi-1", &assignments)
        .expect("计划必须成功");

    let shrunken = stock_basis_group("bal-1", "wh-1", "10", &[("sol-1", "10", "3")]);
    let error = plan
        .validate_against_latest_stock(&[shrunken])
        .expect_err("剩余量下降必须失败");
    assert_eq!(error, SourcingPlanError::StaleFacts);
}

/// 余额可用量下降时重验失败关闭。
#[test]
fn validate_against_latest_stock_rejects_balance_excess() {
    let order = sales_order("so-1");
    let group = stock_basis_group("bal-1", "wh-1", "10", &[("sol-1", "10", "2")]);
    let basis_id = stock_basis_id_for(&order, &group, "wi-1");
    let assignments = SourcingAssignmentSet::normalize(&[assignment(
        "sol-1",
        &basis_id,
        SupplySourceType::ExistingStock,
        None,
        "8",
    )])
    .expect("合法集合必须成功");
    let plan = SourcingPlan::plan(&order, &[], std::slice::from_ref(&group), "wi-1", &assignments)
        .expect("计划必须成功");

    let shrunken = stock_basis_group("bal-1", "wh-1", "5", &[("sol-1", "10", "2")]);
    let error = plan
        .validate_against_latest_stock(&[shrunken])
        .expect_err("余额可用量下降必须失败");
    assert_eq!(error, SourcingPlanError::StaleFacts);
}

/// 最新余额缺失时重验失败关闭。
#[test]
fn validate_against_latest_stock_missing_group_fails_closed() {
    let order = sales_order("so-1");
    let group = stock_basis_group("bal-1", "wh-1", "10", &[("sol-1", "10", "2")]);
    let basis_id = stock_basis_id_for(&order, &group, "wi-1");
    let assignments = SourcingAssignmentSet::normalize(&[assignment(
        "sol-1",
        &basis_id,
        SupplySourceType::ExistingStock,
        None,
        "8",
    )])
    .expect("合法集合必须成功");
    let plan = SourcingPlan::plan(&order, &[], &[group], "wi-1", &assignments).expect("计划必须成功");

    let error = plan
        .validate_against_latest_stock(&[])
        .expect_err("余额失效必须失败");
    assert_eq!(error, SourcingPlanError::StaleFacts);
}

/// 最新依据重验接受恰好等于剩余量与可供量的拆分。
#[test]
fn validate_against_latest_sourcing_accepts_exact_cap() {
    let order = sales_order("so-1");
    let group = basis_group(
        "supplier-1",
        "NET-30",
        FulfillmentResponsibility::SupplierDirect,
        "8",
        &[("sol-1", "10", "2")],
    );
    let basis_id = basis_id_for(&order, &group, "wi-1", None);
    let assignments = SourcingAssignmentSet::normalize(&[assignment(
        "sol-1",
        &basis_id,
        SupplySourceType::Purchase,
        None,
        "8",
    )])
    .expect("合法集合必须成功");
    let plan = SourcingPlan::plan(&order, std::slice::from_ref(&group), &[], "wi-1", &assignments)
        .expect("计划必须成功");

    plan.validate_against_latest_sourcing(&[group])
        .expect("恰好等于上限必须成功");
}

/// 最新行剩余量下降时重验失败关闭。
#[test]
fn validate_against_latest_sourcing_rejects_line_excess() {
    let order = sales_order("so-1");
    let group = basis_group(
        "supplier-1",
        "NET-30",
        FulfillmentResponsibility::SupplierDirect,
        "8",
        &[("sol-1", "10", "2")],
    );
    let basis_id = basis_id_for(&order, &group, "wi-1", None);
    let assignments = SourcingAssignmentSet::normalize(&[assignment(
        "sol-1",
        &basis_id,
        SupplySourceType::Purchase,
        None,
        "8",
    )])
    .expect("合法集合必须成功");
    let plan = SourcingPlan::plan(&order, std::slice::from_ref(&group), &[], "wi-1", &assignments)
        .expect("计划必须成功");

    let shrunken = basis_group(
        "supplier-1",
        "NET-30",
        FulfillmentResponsibility::SupplierDirect,
        "8",
        &[("sol-1", "10", "3")],
    );
    let error = plan
        .validate_against_latest_sourcing(&[shrunken])
        .expect_err("剩余量下降必须失败");
    assert_eq!(error, SourcingPlanError::StaleFacts);
}

/// 同一供给跨方案共享可用量，累计超量失败关闭。
#[test]
fn validate_against_latest_sourcing_shares_supply_cap_across_plans() {
    let order = sales_order("so-1");
    let first = basis_group(
        "supplier-1",
        "NET-30",
        FulfillmentResponsibility::SupplierDirect,
        "8",
        &[("sol-1", "10", "0")],
    );
    let second = basis_group(
        "supplier-1",
        "NET-60",
        FulfillmentResponsibility::SupplierDirect,
        "8",
        &[("sol-2", "10", "0")],
    );
    let first_basis = basis_id_for(&order, &first, "wi-1", None);
    let second_basis = basis_id_for(&order, &second, "wi-1", None);
    let assignments = SourcingAssignmentSet::normalize(&[
        assignment("sol-1", &first_basis, SupplySourceType::Purchase, None, "5"),
        assignment("sol-2", &second_basis, SupplySourceType::Purchase, None, "5"),
    ])
    .expect("合法集合必须成功");
    let plan = SourcingPlan::plan(&order, &[first, second], &[], "wi-1", &assignments).expect("计划必须成功");

    let latest = [
        basis_group(
            "supplier-1",
            "NET-30",
            FulfillmentResponsibility::SupplierDirect,
            "8",
            &[("sol-1", "10", "0")],
        ),
        basis_group(
            "supplier-1",
            "NET-60",
            FulfillmentResponsibility::SupplierDirect,
            "8",
            &[("sol-2", "10", "0")],
        ),
    ];
    let error = plan
        .validate_against_latest_sourcing(&latest)
        .expect_err("同一供给跨方案累计超量必须失败");
    assert_eq!(error, SourcingPlanError::StaleFacts);
}

/// 相同输入重复构造库存依据 ID 完全一致。
#[test]
fn stock_basis_id_is_deterministic() {
    let order = sales_order("so-1");
    let group = stock_basis_group("bal-1", "wh-1", "8", &[("sol-1", "10", "2")]);
    assert_eq!(
        stock_basis_id_for(&order, &group, "wi-1"),
        stock_basis_id_for(&order, &group, "wi-1")
    );
}

/// 逐行剩余量变化必须改变库存依据 ID。
#[test]
fn stock_basis_id_changes_with_quantity() {
    let order = sales_order("so-1");
    let first = stock_basis_group("bal-1", "wh-1", "8", &[("sol-1", "10", "2")]);
    let second = stock_basis_group("bal-1", "wh-1", "8", &[("sol-1", "10", "3")]);
    assert_ne!(
        stock_basis_id_for(&order, &first, "wi-1"),
        stock_basis_id_for(&order, &second, "wi-1")
    );
}

/// 余额依据可按稳定销售行查找。
#[test]
fn stock_group_line_for_finds_and_misses() {
    let group = stock_basis_group("bal-1", "wh-1", "8", &[("sol-1", "10", "2")]);
    assert!(group.line_for("sol-1").is_some());
    assert!(group.line_for("sol-2").is_none());
}

/// 计划错误必须保持稳定文案。
#[test]
fn sourcing_plan_error_messages_are_stable() {
    assert_eq!(
        SourcingPlanError::StaleFacts.to_string(),
        "可分配供给数量已更新，请刷新后重试"
    );
    assert_eq!(
        SourcingPlanError::WarehouseContract("仓库履约必须先选择目标收货仓".to_string()).to_string(),
        "仓库履约必须先选择目标收货仓"
    );
}

/// 采购与库存路径都不能绕过整件精度；最新单位配置也必须重新验证。
#[test]
fn sourcing_plan_enforces_unit_precision_for_purchase_and_stock() {
    let order = sales_order("1");
    let mut purchase = basis_group(
        "supplier-1",
        "NET30",
        FulfillmentResponsibility::SupplierDirect,
        "1",
        &[("line-1", "1", "0")],
    );
    purchase.lines[0].coverage.quantity_scale = Some(0);
    let mut stock = stock_basis_group("balance-1", "wh-1", "1", &[("line-1", "1", "0")]);
    stock.lines[0].coverage.quantity_scale = Some(0);
    let purchase_id = basis_id_for(&order, &purchase, "wi-1", None);
    let stock_id = stock_basis_id_for(&order, &stock, "wi-1");
    for (basis_id, source) in [
        (&purchase_id, SupplySourceType::Purchase),
        (&stock_id, SupplySourceType::ExistingStock),
    ] {
        let selected =
            SourcingAssignmentSet::normalize(&[assignment("line-1", basis_id, source, None, "0.5")]).unwrap();
        assert!(matches!(
            SourcingPlan::plan(&order, &[purchase.clone()], &[stock.clone()], "wi-1", &selected),
            Err(SourcingPlanError::QuantityContract(_))
        ));
    }
    stock.lines[0].coverage.quantity_scale = Some(1);
    let selected = SourcingAssignmentSet::normalize(&[assignment(
        "line-1",
        &stock_id,
        SupplySourceType::ExistingStock,
        None,
        "0.5",
    )])
    .unwrap();
    let plan = SourcingPlan::plan(&order, &[], &[stock.clone()], "wi-1", &selected).unwrap();
    stock.lines[0].coverage.quantity_scale = Some(0);
    assert!(matches!(
        plan.validate_against_latest_stock(&[stock]),
        Err(SourcingPlanError::QuantityContract(_))
    ));
}
