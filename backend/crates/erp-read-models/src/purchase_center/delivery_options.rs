//! 采购修改原单或办理采购变更时的交付安排范围（第一期 §6.3“销售变更对采购的影响”第 5 条）。
//!
//! 预计交付日以销售当前版本履约期限为上限；直发或服务采购只能在销售当前收件资料中
//! 重新分配，每个地址扣除本采购之外其他有效采购已承担的数量。

use std::collections::BTreeSet;

use application_core::AuditActor;
use erp_core::ids::SalesOrderRevisionId;
use erp_procurement::entity::purchase_order::{
    ChangeQuantityBound, FulfillmentResponsibility, ProcurementCoverageFacts, PurchaseOrder,
    business_date_of, remaining_delivery_capacity,
};
use erp_sales::repository::SalesOrderExt;
use persistence_core::NoTransaction;

use super::PurchaseOrderReadService;
use super::change_bounds::load_change_quantity_facts;
use super::dto::{
    PurchaseDeliveryAddressOptionView, PurchaseDeliveryLineOptionView, PurchaseDeliveryOptionsView,
    PurchaseQuantityBoundView,
};
use super::repository::load_procurement_coverage_facts;
use crate::support::data_scope_changed;
use crate::{Error, Result};

impl PurchaseOrderReadService {
    /// 读取采购各行可调整的预计交付日上限与收件地址剩余量。
    ///
    /// # 参数
    /// * `actor` - 当前账号
    /// * `id` - 采购单 ID
    ///
    /// # 返回
    /// 返回采购当前内容引用、且仍在销售当前版本中的商品行范围。
    ///
    /// # 错误
    /// 采购不可见、读取期间范围变化、来源销售缺少当前版本或事实读取失败时返回错误。
    pub async fn delivery_options(
        &self,
        actor: &AuditActor,
        id: &str,
    ) -> Result<PurchaseDeliveryOptionsView> {
        let (order, expected) = self.access().detail(actor, id).await?;
        let view = self.delivery_options_view(&order).await?;
        let (_, current) = self.access().detail(actor, id).await?;
        if current != expected {
            return Err(data_scope_changed("数据范围或采购单已变化，请刷新"));
        }
        Ok(view)
    }

    /// 按销售当前版本装配交付安排范围；已生效采购附带改单数量范围。
    async fn delivery_options_view(&self, order: &PurchaseOrder) -> Result<PurchaseDeliveryOptionsView> {
        if !order.stable.status.allows_change() {
            return self.draft_delivery_options_view(order).await;
        }
        let facts = load_change_quantity_facts(&self.db, order, &mut NoTransaction).await?;
        let mut view = delivery_options(order, &facts.state.facts)?;
        attach_quantity_bounds(&mut view, &facts.bounds)?;
        Ok(view)
    }

    /// 采购草稿只需交付安排范围。
    async fn draft_delivery_options_view(
        &self,
        order: &PurchaseOrder,
    ) -> Result<PurchaseDeliveryOptionsView> {
        let sales = self
            .db
            .sales_orders()
            .find_by_id(order.sales_order_id.as_ref(), &mut NoTransaction)
            .await?
            .ok_or_else(|| Error::NotFound("来源销售单不存在".into()))?;
        let revision_id = sales
            .current_revision_id()
            .ok_or_else(|| Error::BusinessLogicError("来源销售单缺少当前版本".into()))?;
        let facts = load_procurement_coverage_facts(
            &self.db,
            &SalesOrderRevisionId::new(revision_id),
            &order.sales_order_id,
            &mut NoTransaction,
        )
        .await?;
        delivery_options(order, &facts)
    }
}

/// 由覆盖事实计算采购各行的交付安排范围。
///
/// # 参数
/// * `order` - 正在调整的采购单
/// * `facts` - 销售当前版本的覆盖事实
///
/// # 返回
/// 按销售行号返回采购当前内容引用的商品行范围；已移出销售当前版本的行不返回。
///
/// # 错误
/// 履约期限无法换算或地址剩余量非法时返回错误。
fn delivery_options(
    order: &PurchaseOrder,
    facts: &ProcurementCoverageFacts,
) -> Result<PurchaseDeliveryOptionsView> {
    let mut lines = Vec::new();
    for stable in referenced_lines(order, facts) {
        let Some((line, goods)) = facts.current_goods_line(&stable) else {
            continue;
        };
        let requires_addresses = order.fulfillment_responsibility != FulfillmentResponsibility::Warehouse
            && !goods.delivery_allocations.is_empty();
        let addresses = if requires_addresses {
            remaining_delivery_capacity(
                &goods.delivery_allocations,
                &facts.other_delivery_allocations(order, &stable),
            )
            .map_err(Error::Logic)?
            .into_iter()
            .map(|row| PurchaseDeliveryAddressOptionView {
                recipient: row.recipient,
                phone: row.phone,
                address: row.address,
                available_quantity: row.quantity,
            })
            .collect()
        } else {
            Vec::new()
        };
        lines.push(PurchaseDeliveryLineOptionView {
            sales_order_line_id: stable,
            line_no: line.line_no,
            item_name: line.item_name_snapshot.clone(),
            sales_due_date: business_date_of(goods.fulfillment_due_at).map_err(Error::Logic)?.to_string(),
            requires_addresses,
            addresses,
            quantity_bound: None,
        });
    }
    lines.sort_by_key(|line| line.line_no);
    Ok(PurchaseDeliveryOptionsView { lines })
}

/// 按稳定销售行附上改单数量范围。
///
/// # 参数
/// * `view` - 交付安排范围
/// * `bounds` - 采购当前生效版本商品行的数量范围
///
/// # 返回
/// 成功时原地写入各行的数量范围。
///
/// # 错误
/// 数量越界时返回错误。
fn attach_quantity_bounds(
    view: &mut PurchaseDeliveryOptionsView,
    bounds: &[ChangeQuantityBound],
) -> Result<()> {
    for line in &mut view.lines {
        let Some(bound) = bounds.iter().find(|bound| bound.sales_order_line_id == line.sales_order_line_id)
        else {
            continue;
        };
        line.quantity_bound = Some(PurchaseQuantityBoundView {
            base_quantity: bound.base_quantity,
            sales_quantity: bound.sales_quantity,
            other_covered_quantity: bound.other_covered_quantity,
            fulfilled_quantity: bound.fulfilled_quantity,
            min_quantity: bound.min_quantity(),
            max_quantity: bound.max_quantity().map_err(Error::Logic)?,
            quantity_affected: bound.quantity_affected,
        });
    }
    Ok(())
}

/// 采购当前提交与当前版本引用的销售稳定行。
fn referenced_lines(order: &PurchaseOrder, facts: &ProcurementCoverageFacts) -> BTreeSet<String> {
    let submissions = facts
        .submission_lines
        .iter()
        .filter(|line| {
            order.current_submission_id.as_deref() == Some(line.purchase_order_submission_id.as_ref())
        })
        .filter_map(|line| line.sales_order_line_id.as_ref().map(ToString::to_string));
    let revisions = facts
        .purchase_revision_lines
        .iter()
        .filter(|line| {
            order.stable.current_revision_id.as_deref() == Some(line.purchase_order_revision_id.as_ref())
        })
        .filter_map(|line| line.sales_order_line_id.as_ref().map(ToString::to_string));
    submissions.chain(revisions).collect()
}

#[cfg(test)]
mod tests {
    use std::str::FromStr;

    use erp_core::common::time::Instant;
    use erp_core::ids::{
        ProcurementConfirmationLineId, PurchaseOrderId, PurchaseOrderSubmissionId,
        PurchaseOrderSubmissionLineId, SalesOrderId, SalesOrderLineId, SalesOrderRevisionId,
        SalesOrderRevisionLineId, SkuId, SkuRevisionId, SupplierAccountId, WarehouseId,
    };
    use erp_core::money::{Amount, Quantity, Rate, UnitPrice};
    use erp_procurement::entity::facts::{
        FactIdentity, PaymentTermFact, SalesGoodsLineFact, SalesLineType, SalesRevisionLineFact,
    };
    use erp_procurement::entity::purchase_order::{
        FulfillmentResponsibility, ProcurementCoverageFacts, PurchaseDeliveryAllocation, PurchaseLineType,
        PurchaseOrder, PurchaseOrderData, PurchaseOrderSubmissionLine, PurchaseOrderSubmissionLineData,
        PurchaseType,
    };

    use super::delivery_options;

    fn qty(value: &str) -> Quantity {
        Quantity::from_str(value).unwrap()
    }

    fn row(address: &str, quantity: &str) -> PurchaseDeliveryAllocation {
        PurchaseDeliveryAllocation {
            recipient: "张三".into(),
            phone: "13800000000".into(),
            address: address.into(),
            quantity: qty(quantity),
        }
    }

    fn purchase(id: &str, submission: &str, responsibility: FulfillmentResponsibility) -> PurchaseOrder {
        let mut order = PurchaseOrder::new(
            PurchaseOrderId::new(id),
            PurchaseOrderData {
                business_org_unit_id: "org".into(),
                purchase_no: format!("PO-{id}"),
                sales_order_id: SalesOrderId::new("so-1"),
                sales_order_revision_id: SalesOrderRevisionId::new("rev-1"),
                creation_basis_id: "basis".into(),
                supplier_id: SupplierAccountId::new("sup-1"),
                purchase_type: PurchaseType::Physical,
                payment_term_code: "POSTPAY_NET30".into(),
                fulfillment_responsibility: responsibility,
                owner_user_id: "buyer".into(),
                target_warehouse_id: (responsibility == FulfillmentResponsibility::Warehouse)
                    .then(|| WarehouseId::new("wh-1")),
            },
            "buyer",
            |_| Ok(PaymentTermFact::new("POSTPAY_NET30").with_days_after_delivery(30)),
        )
        .unwrap();
        order.current_submission_id = Some(submission.into());
        order
    }

    fn submission_line(
        submission: &str,
        stable: &str,
        rows: Vec<PurchaseDeliveryAllocation>,
    ) -> PurchaseOrderSubmissionLine {
        PurchaseOrderSubmissionLine::new(
            PurchaseOrderSubmissionLineId::new(format!("{submission}-{stable}")),
            PurchaseOrderSubmissionLineData {
                supplier_offering_source: None,
                delivery_allocations: rows,
                purchase_order_submission_id: PurchaseOrderSubmissionId::new(submission),
                line_no: 1,
                line_type: PurchaseLineType::ItemService,
                procurement_confirmation_line_id: Some(ProcurementConfirmationLineId::new("pcl-1")),
                sku_id: Some(SkuId::new("sku-1")),
                sku_revision_id: Some(SkuRevisionId::new("skur-1")),
                product_name_snapshot: Some("礼盒".into()),
                specification_snapshot: None,
                quantity: Some(qty("2")),
                base_unit_code: Some("件".into()),
                unit_cost_gross: Some(UnitPrice::from_str("5").unwrap()),
                gross_amount: Amount::from_str("10").unwrap(),
                net_amount: Amount::from_str("10").unwrap(),
                tax_amount: Amount::from_str("0").unwrap(),
                input_tax_rate: Some(Rate::from_str("0").unwrap()),
                expected_delivery_date: None,
                sales_order_line_id: Some(SalesOrderLineId::new(stable)),
                sales_order_revision_line_id: Some(SalesOrderRevisionLineId::new("cur-1")),
                sales_order_submission_line_id: None,
                allocated_quantity: Some(qty("2")),
                expense: None,
            },
        )
        .unwrap()
    }

    /// 当前版本第 2 行要求两个地址；另一张采购已在地址A承担 2 件；本采购还引用了已移除的行。
    fn facts() -> ProcurementCoverageFacts {
        ProcurementCoverageFacts {
            revision_lines: vec![SalesRevisionLineFact {
                base: FactIdentity { id: "cur-1".into() },
                sales_order_line_id: SalesOrderLineId::new("sol-1"),
                line_no: 2,
                line_type: SalesLineType::GoodsService,
                item_name_snapshot: "礼盒".into(),
                spec_snapshot: None,
                unit_snapshot: Some("件".into()),
            }],
            goods_lines: vec![SalesGoodsLineFact {
                revision_line_id: SalesOrderRevisionLineId::new("cur-1"),
                sku_id: SkuId::new("sku-1"),
                sku_revision_id: SkuRevisionId::new("skur-1"),
                quantity: qty("10"),
                base_unit_code: "件".into(),
                fulfillment_due_at: Instant::from_unix_secs(1_800_000_000),
                service_region: None,
                delivery_allocations: vec![row("地址A", "6"), row("地址B", "4")],
                assigned_delivery_allocations: Vec::new(),
            }],
            submission_lines: vec![
                submission_line("sub-1", "sol-1", vec![row("地址A", "2")]),
                submission_line("sub-1", "sol-removed", Vec::new()),
                submission_line("sub-2", "sol-1", vec![row("地址A", "2")]),
            ],
            ..ProcurementCoverageFacts::default()
        }
    }

    /// 直发采购按地址给出扣除其他采购后的剩余量，并以销售履约期限为交付日上限。
    #[test]
    fn direct_purchase_gets_remaining_address_capacity() {
        let order = purchase("po-1", "sub-1", FulfillmentResponsibility::SupplierDirect);

        let view = delivery_options(&order, &facts()).unwrap();

        assert_eq!(view.lines.len(), 1);
        let line = &view.lines[0];
        assert_eq!((line.sales_order_line_id.as_str(), line.line_no), ("sol-1", 2));
        assert_eq!(line.sales_due_date, "2027-01-15");
        assert!(line.requires_addresses);
        let remaining = line
            .addresses
            .iter()
            .map(|row| (row.address.as_str(), row.available_quantity))
            .collect::<Vec<_>>();
        assert_eq!(remaining, vec![("地址A", qty("4")), ("地址B", qty("4"))]);
    }

    /// 仓库履约采购只给交付日上限，不暴露客户地址。
    #[test]
    fn warehouse_purchase_hides_addresses() {
        let order = purchase("po-1", "sub-1", FulfillmentResponsibility::Warehouse);

        let view = delivery_options(&order, &facts()).unwrap();

        assert!(!view.lines[0].requires_addresses);
        assert!(view.lines[0].addresses.is_empty());
    }
}
