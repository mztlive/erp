//! 引用旧销售版本的采购草稿按当前销售版本刷新来源（第一期 §6.3“销售变更对采购的
//! 影响”第 5 条）。
//!
//! 刷新只改写销售来源引用与付款主体中的销售版本，保留单号、供应商、未受影响行和
//! 附件；预计交付日与收件分配随同一次保存调整，由保存流程按当前版本统一校验。

use std::str::FromStr;

use erp_core::ids::SalesOrderRevisionId;
use erp_core::money::Quantity;

use crate::dto::purchase_order::SavePurchaseOrderLine;
use crate::entity::purchase_order::{PaymentSubjectSnapshot, PurchaseLineType, SalesProcurementCoverage};
use crate::{Error, Result};

/// 移除数量填为零的商品行，表示刷新时不再采购该行。
///
/// # 参数
/// * `lines` - 已合并补丁的完整请求行
///
/// # 返回
/// 返回保留的请求行；非商品行与数量无法解析的行原样保留，交由后续校验报错。
///
/// # 错误
/// 无。
pub fn drop_cleared_item_lines(lines: Vec<SavePurchaseOrderLine>) -> Vec<SavePurchaseOrderLine> {
    lines
        .into_iter()
        .filter(|line| {
            line.line_type != PurchaseLineType::ItemService
                || !line
                    .quantity
                    .as_deref()
                    .and_then(|value| Quantity::from_str(value.trim()).ok())
                    .is_some_and(|quantity| quantity.to_decimal().is_zero())
        })
        .collect()
}

/// 把保留的商品行绑定到当前销售版本行。
///
/// # 参数
/// * `lines` - 已通过刷新校验的请求行
/// * `coverage` - 当前销售版本覆盖
///
/// # 返回
/// 绑定成功时返回 `Ok(())`。
///
/// # 错误
/// 当前版本缺少对应销售行时返回冲突。
pub fn rebind_draft_lines(
    lines: &mut [SavePurchaseOrderLine],
    coverage: &SalesProcurementCoverage,
) -> Result<()> {
    for line in lines.iter_mut().filter(|line| line.line_type == PurchaseLineType::ItemService) {
        let stable_id = line.sales_order_line_id.as_deref().map(str::trim).unwrap_or_default();
        let current = coverage
            .lines
            .iter()
            .find(|current| current.revision_line.sales_order_line_id.as_ref() == stable_id)
            .ok_or_else(|| Error::ConflictError("销售当前版本已移除采购来源行，请刷新后重试".to_string()))?;
        line.sales_order_revision_line_id = Some(current.revision_line.base.id.clone());
    }
    Ok(())
}

/// 以当前销售版本替换付款主体中的销售来源。
///
/// # 参数
/// * `subject` - 原草稿付款主体
/// * `revision_id` - 当前销售版本
///
/// # 返回
/// 返回我方主体不变、销售版本已更新的付款主体。
///
/// # 错误
/// 主体字段非法时返回校验错误。
pub fn rebased_payment_subject(
    subject: &PaymentSubjectSnapshot,
    revision_id: &str,
) -> Result<PaymentSubjectSnapshot> {
    PaymentSubjectSnapshot::new(
        subject.settlement_party_id.clone(),
        subject.settlement_party_name.clone(),
        subject.sales_order_id.clone(),
        SalesOrderRevisionId::new(revision_id),
    )
    .map_err(Error::Logic)
}

#[cfg(test)]
mod tests {
    use std::str::FromStr;

    use erp_core::common::time::Instant;
    use erp_core::ids::{
        PartyId, SalesOrderId, SalesOrderLineId, SalesOrderRevisionId, SalesOrderRevisionLineId, SkuId,
        SkuRevisionId,
    };
    use erp_core::money::Quantity;

    use super::{drop_cleared_item_lines, rebased_payment_subject, rebind_draft_lines};
    use crate::dto::purchase_order::SavePurchaseOrderLine;
    use crate::entity::facts::{
        FactIdentity, ProductKind, SalesCustomerSnapshotFact, SalesGoodsLineFact, SalesLineType,
        SalesRevisionFact, SalesRevisionLineFact,
    };
    use crate::entity::purchase_order::{
        PaymentSubjectSnapshot, ProcurementCoverageSummary, PurchaseDeliveryAllocation, PurchaseLineType,
        SalesProcurementCoverage, SalesProcurementCoverageLine,
    };

    /// 2026-10-20 00:00 上海时间。
    const DUE: i64 = 1_792_425_600;

    fn address(recipient: &str, quantity: &str) -> PurchaseDeliveryAllocation {
        PurchaseDeliveryAllocation {
            recipient: recipient.into(),
            phone: "13800000000".into(),
            region_code: erp_core::region::RegionCode::parse("310115").unwrap(),
            region_name: "上海市浦东新区".into(),
            address: "地址".into(),
            quantity: Quantity::from_str(quantity).unwrap(),
        }
    }

    fn request(
        quantity: &str,
        expected: Option<&str>,
        rows: Option<Vec<PurchaseDeliveryAllocation>>,
    ) -> SavePurchaseOrderLine {
        SavePurchaseOrderLine {
            delivery_allocations: rows,
            line_type: PurchaseLineType::ItemService,
            procurement_confirmation_line_id: None,
            sku_id: Some("sku-1".into()),
            sku_revision_id: Some("skur-1".into()),
            product_name: Some("礼盒".into()),
            specification: None,
            quantity: Some(quantity.into()),
            base_unit_code: Some("件".into()),
            unit_cost_gross: Some("5".into()),
            input_tax_rate: Some("0".into()),
            expected_delivery_date: expected.map(str::to_string),
            sales_order_line_id: Some("sol-1".into()),
            sales_order_revision_line_id: Some("old-1".into()),
            sales_order_submission_line_id: None,
            allocated_quantity: Some(quantity.into()),
            gross_amount: None,
            expense: None,
        }
    }

    fn coverage(rows: Vec<PurchaseDeliveryAllocation>) -> SalesProcurementCoverage {
        let total = Quantity::from_str("8").unwrap();
        let summary = ProcurementCoverageSummary::new(total, total).unwrap();
        SalesProcurementCoverage {
            revision: SalesRevisionFact {
                base: FactIdentity { id: "rev-2".into() },
                customer_snapshot: SalesCustomerSnapshotFact { customer_name: "客户".into() },
                contract_snapshot: None,
            },
            lines: vec![SalesProcurementCoverageLine {
                quantity_scale: Some(0),
                revision_line: SalesRevisionLineFact {
                    base: FactIdentity { id: "cur-1".into() },
                    sales_order_line_id: SalesOrderLineId::new("sol-1"),
                    line_no: 1,
                    line_type: SalesLineType::GoodsService,
                    item_name_snapshot: "礼盒".into(),
                    spec_snapshot: None,
                    unit_snapshot: None,
                },
                goods_line: SalesGoodsLineFact {
                    revision_line_id: SalesOrderRevisionLineId::new("cur-1"),
                    sku_id: SkuId::new("sku-1"),
                    sku_revision_id: SkuRevisionId::new("skur-1"),
                    quantity: total,
                    base_unit_code: "件".into(),
                    fulfillment_due_at: Instant::from_unix_secs(DUE),
                    service_region: None,
                    delivery_allocations: rows,
                    assigned_delivery_allocations: Vec::new(),
                },
                product_kind: ProductKind::Physical,
                summary,
            }],
            summary,
        }
    }

    /// 数量填零的商品行被移除，其余行保留。
    #[test]
    fn cleared_item_lines_are_dropped() {
        let kept = drop_cleared_item_lines(vec![request("0", None, None), request("3", None, None)]);

        assert_eq!(kept.len(), 1);
        assert_eq!(kept[0].quantity.as_deref(), Some("3"));
    }

    /// 刷新后商品行指向当前销售版本行。
    #[test]
    fn rebind_points_lines_to_current_revision() {
        let mut lines = vec![request("3", Some("2026-10-19"), Some(vec![address("张三", "3")]))];

        rebind_draft_lines(&mut lines, &coverage(vec![address("张三", "8")])).unwrap();

        assert_eq!(lines[0].sales_order_revision_line_id.as_deref(), Some("cur-1"));
    }

    /// 当前版本已移除来源行时拒绝刷新；交期与地址留给保存流程统一校验。
    #[test]
    fn rebind_rejects_removed_source_line() {
        let mut removed = vec![request("3", None, None)];
        removed[0].sales_order_line_id = Some("sol-removed".into());
        let mut late = vec![request("3", Some("2026-10-21"), Some(vec![address("张三", "3")]))];

        assert!(rebind_draft_lines(&mut removed, &coverage(Vec::new())).is_err());
        assert!(rebind_draft_lines(&mut late, &coverage(vec![address("李四", "8")])).is_ok());
    }

    /// 付款主体只替换销售版本。
    #[test]
    fn payment_subject_keeps_party_and_updates_revision() {
        let subject = PaymentSubjectSnapshot::new(
            PartyId::new("party-1"),
            "我方公司".into(),
            SalesOrderId::new("so-1"),
            SalesOrderRevisionId::new("rev-1"),
        )
        .unwrap();

        let rebased = rebased_payment_subject(&subject, "rev-2").unwrap();

        assert_eq!(rebased.sales_order_revision_id.as_ref(), "rev-2");
        assert_eq!(rebased.settlement_party_name, "我方公司");
    }
}
