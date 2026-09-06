use std::str::FromStr;

use crate::inventory::{StockBalance, StockBalanceData};
use crate::sales_order::revision::{
    SalesOrderGoodsServiceLineRevision, SalesOrderGoodsServiceLineRevisionData, SalesOrderRevision,
    SalesOrderRevisionData, SalesOrderRevisionLine, SalesOrderRevisionLineData,
};
use crate::sales_order::snapshot::HeaderSnapshotData;
use crate::sales_order::{CommercialStatus, LineType, ProcurementCoverageSummary, RevisionSource};
use crate::supplier_offering::{
    AvailabilityStatus, FromGrossPricesParams, OfferingSourceType, PrefillSourceRefs, SupplierOffering,
    SupplierOfferingAvailability, SupplierOfferingAvailabilityData, SupplierOfferingData,
    SupplierOfferingRevision, SupplierOfferingRevisionData,
};
use erp_core::common::time::Instant;
use erp_core::ids::{
    SalesOrderId, SalesOrderRevisionId, SalesOrderRevisionLineId, SkuId, StockBalanceId, SupplierAccountId,
    SupplierOfferingAvailabilityId, SupplierOfferingId, SupplierOfferingRevisionId, WarehouseId,
};
use erp_core::money::{Amount, Quantity, Rate, UnitPrice};

use super::{
    stock_basis_id_for, SourcingAssignment, SourcingAssignmentSet, SourcingPlan, SourcingPlanError,
    StockBasisGroup, StockBasisLine, SupplySourceType,
};
use crate::purchase_order::coverage::SalesProcurementCoverageLine;
use crate::purchase_order::creation_basis::{basis_id_for, BasisGroup, BasisLine, BasisScope, LineSupply};
use crate::purchase_order::{FulfillmentResponsibility, PurchaseType};

/// 构造销售当前版本头。
fn revision(id: &str) -> SalesOrderRevision {
    SalesOrderRevision::new(
        SalesOrderRevisionId::new(format!("rev-{id}")),
        SalesOrderRevisionData {
            sales_order_id: SalesOrderId::new("so-1"),
            revision_no: 1,
            revision_source: RevisionSource::ErpApproval,
            previous_revision_id: None,
            content_hash: format!("hash-{id}"),
            customer_revision_id: None,
            contract_revision_id: None,
            snapshot: HeaderSnapshotData {
                customer_name: "客户".to_string(),
                contract_no: None,
                settlement_party_name: None,
                payment_term_code: "NET-30".to_string(),
                payment_term_name: "净30天".to_string(),
                invoice_type: "增值税专用发票".to_string(),
                tax_point: "13".to_string(),
            },
            project_name: None,
            business_remark: None,
            voucher_category_sku_id: None,
            voucher_expiry_at: None,
            gross_amount: Amount::from_str("100").unwrap(),
            net_amount: Amount::from_str("100").unwrap(),
            tax_amount: Amount::from_str("0").unwrap(),
            effective_at: Instant::from_unix_secs(1_800_000_000),
            recorded_at: Instant::from_unix_secs(1_800_000_000),
        },
    )
    .unwrap()
}

/// 构造销售当前版本公共行。
fn revision_line(id: &str, stable_line_id: &str) -> SalesOrderRevisionLine {
    SalesOrderRevisionLine::new(
        SalesOrderRevisionLineId::new(id),
        SalesOrderRevisionLineData {
            sales_order_revision_id: SalesOrderRevisionId::new("rev-1"),
            sales_order_line_id: erp_core::ids::SalesOrderLineId::new(stable_line_id),
            line_no: 1,
            line_type: LineType::GoodsService,
            gross_amount: Amount::from_str("10").unwrap(),
            net_amount: Amount::from_str("10").unwrap(),
            tax_amount: Amount::from_str("0").unwrap(),
            sales_tax_rate: Rate::from_str("0").unwrap(),
            item_name_snapshot: "商品".to_string(),
            spec_snapshot: Some("规格".to_string()),
            unit_snapshot: Some("件".to_string()),
        },
    )
    .unwrap()
}

/// 构造销售当前版本商品/服务子类型行。
fn goods_line(revision_line_id: &str) -> SalesOrderGoodsServiceLineRevision {
    SalesOrderGoodsServiceLineRevision::new(
        erp_core::ids::SalesOrderGoodsServiceLineRevisionId::new(format!("goods-{revision_line_id}")),
        SalesOrderGoodsServiceLineRevisionData {
            revision_line_id: SalesOrderRevisionLineId::new(revision_line_id),
            sku_id: SkuId::new("sku-1"),
            sku_revision_id: erp_core::ids::SkuRevisionId::new("skur-1"),
            welfare_scenario: None,
            service_region: None,
            fulfillment_due_at: Instant::from_unix_secs(1_800_000_000),
            quantity: Quantity::from_str("10").unwrap(),
            base_unit_code: "件".to_string(),
            unit_price_gross: UnitPrice::from_str("5").unwrap(),
        },
    )
    .unwrap()
}

/// 构造一条销售覆盖目标行；剩余量等于目标减覆盖。
fn coverage_line(stable_line_id: &str, total: &str, covered: &str) -> SalesProcurementCoverageLine {
    SalesProcurementCoverageLine {
        revision_line: revision_line(&format!("sorl-{stable_line_id}"), stable_line_id),
        goods_line: goods_line(&format!("sorl-{stable_line_id}")),
        product_kind: erp_catalog::ProductKind::Physical,
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
    SupplierOffering::new(
        SupplierOfferingId::new(id),
        SupplierOfferingData {
            sku_id: SkuId::new("sku-1"),
            supplier_id: SupplierAccountId::new(supplier_id),
            supplier_product_code: None,
            supplier_sku_code: format!("SKU-{id}"),
            source_type: OfferingSourceType::Manual,
            source_connection_id: None,
        },
        "test",
    )
    .unwrap()
}

/// 构造供给商业条款修订。
fn offering_revision(offering_id: &str) -> SupplierOfferingRevision {
    SupplierOfferingRevision::new(
        SupplierOfferingRevisionId::new(format!("offrev-{offering_id}")),
        SupplierOfferingRevisionData::from_gross_prices(FromGrossPricesParams {
            supplier_offering_id: SupplierOfferingId::new(offering_id),
            revision_no: 1,
            dropship_supply_price_gross: UnitPrice::from_str("6").unwrap(),
            bulk_supply_price_gross: UnitPrice::from_str("5").unwrap(),
            input_tax_rate: Rate::from_str("0.13").unwrap(),
            dropship_express: None,
            freight_amount: None,
            service_fee_amount: None,
            bulk_minimum_order_quantity: Quantity::from_str("1").unwrap(),
            supply_region: vec!["全国".to_string()],
            product_capabilities: Vec::new(),
            valid_from: erp_core::common::time::BusinessDate::from_str("2026-01-01").unwrap(),
            valid_to: None,
            prefill_source_refs: PrefillSourceRefs {
                input_tax_rate: None,
                supply_region: None,
                valid_from_date: None,
                valid_from_timezone: None,
                valid_from_calendar_version: None,
            },
        }),
    )
    .unwrap()
}

/// 构造供给实时可供投影。
fn availability(offering_id: &str, quantity: &str) -> SupplierOfferingAvailability {
    SupplierOfferingAvailability::new(
        SupplierOfferingAvailabilityId::new(format!("avail-{offering_id}")),
        SupplierOfferingAvailabilityData {
            supplier_offering_id: SupplierOfferingId::new(offering_id),
            availability_status: AvailabilityStatus::Available,
            available_quantity: Some(Quantity::from_str(quantity).unwrap()),
            source_updated_at: Instant::from_unix_secs(1_800_000_000),
            received_at: Instant::from_unix_secs(1_800_000_000),
            source_revision_token: None,
            updated_by: "test".to_string(),
        },
    )
    .unwrap()
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
fn sales_order(id: &str) -> crate::sales_order::SalesOrder {
    let mut order = crate::sales_order::SalesOrder::new(
        SalesOrderId::new(id),
        crate::sales_order::SalesOrderData {
            order_no: format!("SO-{id}"),
            business_type: crate::sales_order::BusinessType::GoodsService,
            origin_system: crate::sales_order::OriginSystem::Erp,
            source_identity_id: None,
            customer_id: erp_core::ids::CustomerAccountId::new("customer-1"),
            contract_id: None,
            settlement_party_id: erp_core::ids::PartyId::new("party-1"),
            source_status_code: None,
        },
        "seller-1",
    )
    .unwrap();
    order.commercial_status = CommercialStatus::Effective;
    order.procurement_guard_version = 3;
    order
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
        balance: StockBalance::new(
            StockBalanceId::new(balance_id),
            StockBalanceData {
                warehouse_id: WarehouseId::new(warehouse_id),
                sku_id: SkuId::new("sku-1"),
                on_hand_quantity: available,
                reserved_quantity: Quantity::from_str("0").unwrap(),
                available_quantity: available,
                last_movement_id: None,
            },
        )
        .unwrap(),
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
