//! 采购调整交付安排时的约束（第一期 §6.3“销售变更对采购的影响”第 5 条）。
//!
//! 修改原单或办理采购变更时，预计交付日不得晚于销售当前履约期限；采购只能在销售
//! 当前版本的收件资料中重新分配，每个地址可分配量为销售要求扣除本采购之外其他有效
//! 采购已承担的数量。

use std::collections::HashMap;

use erp_core::common::time::BusinessDate;
use erp_core::money::Quantity;
use erp_core::{Error, Result};
use rust_decimal::Decimal;

use super::business_date::business_date_of;
use super::coverage::ProcurementCoverageFacts;
use super::delivery_allocation::PurchaseDeliveryAllocation;
use super::line_amounts::PurchaseLineInput;
use super::order::PurchaseOrder;
use super::purchase_revision::PurchaseOrderRevisionLine;
use super::purchase_submission::PurchaseOrderSubmissionLine;
use super::types::PurchaseLineType;
use crate::entity::facts::{SalesGoodsLineFact, SalesRevisionLineFact};

impl ProcurementCoverageFacts {
    /// 读取销售当前版本中指定稳定行的公共行与商品子类型行。
    ///
    /// # 参数
    /// * `sales_order_line_id` - 销售稳定行
    ///
    /// # 返回
    /// 当前版本仍有该商品行时返回公共行与子类型行；否则返回 `None`。
    ///
    /// # 错误
    /// 无。
    pub fn current_goods_line(
        &self,
        sales_order_line_id: &str,
    ) -> Option<(&SalesRevisionLineFact, &SalesGoodsLineFact)> {
        let line = self
            .revision_lines
            .iter()
            .find(|line| line.sales_order_line_id.as_ref() == sales_order_line_id)?;
        let goods = self.goods_lines.iter().find(|goods| goods.revision_line_id.as_ref() == line.base.id)?;
        Some((line, goods))
    }

    /// 汇总本采购之外其他有效采购在指定销售稳定行上承担的收件分配。
    ///
    /// # 参数
    /// * `order` - 正在调整的采购单
    /// * `sales_order_line_id` - 销售稳定行
    ///
    /// # 返回
    /// 返回其他采购当前提交与当前版本中该行的交付安排。
    ///
    /// # 错误
    /// 无。
    pub fn other_delivery_allocations(
        &self,
        order: &PurchaseOrder,
        sales_order_line_id: &str,
    ) -> Vec<PurchaseDeliveryAllocation> {
        let same_line = |id: Option<&str>| id == Some(sales_order_line_id);
        let submissions = self
            .submission_lines
            .iter()
            .filter(|line| {
                same_line(line.sales_order_line_id.as_ref().map(|id| id.as_ref()))
                    && order.current_submission_id.as_deref()
                        != Some(line.purchase_order_submission_id.as_ref())
            })
            .flat_map(|line| line.delivery_allocations.iter().cloned());
        let revisions = self
            .purchase_revision_lines
            .iter()
            .filter(|line| {
                same_line(line.sales_order_line_id.as_ref().map(|id| id.as_ref()))
                    && order.stable.current_revision_id.as_deref()
                        != Some(line.purchase_order_revision_id.as_ref())
            })
            .flat_map(|line| line.delivery_allocations.iter().cloned());
        submissions.chain(revisions).collect()
    }
}

/// 计算销售收件地址扣除已承担数量后的剩余可分配量。
///
/// # 参数
/// * `source` - 销售当前版本的收件要求
/// * `assigned` - 其他采购已承担的交付安排
///
/// # 返回
/// 按销售顺序返回每个地址的剩余数量；同一地址合并，承担超过要求时剩余为零。
///
/// # 错误
/// 数量合计溢出或精度非法时返回错误。
pub fn remaining_delivery_capacity(
    source: &[PurchaseDeliveryAllocation],
    assigned: &[PurchaseDeliveryAllocation],
) -> Result<Vec<PurchaseDeliveryAllocation>> {
    let mut used = HashMap::<_, Decimal>::new();
    for row in assigned {
        *used.entry(row.key()).or_default() += row.quantity.to_decimal();
    }
    let mut remaining: Vec<(&PurchaseDeliveryAllocation, Decimal)> = Vec::new();
    for row in source {
        match remaining.iter_mut().find(|(seen, _)| seen.key() == row.key()) {
            Some((_, total)) => *total += row.quantity.to_decimal(),
            None => remaining.push((row, row.quantity.to_decimal())),
        }
    }
    remaining
        .into_iter()
        .map(|(row, total)| {
            let left = (total - used.get(&row.key()).copied().unwrap_or_default()).max(Decimal::ZERO);
            let quantity = Quantity::try_from(left).map_err(|error| Error::from(error.to_string()))?;
            Ok(PurchaseDeliveryAllocation { quantity, ..row.clone() })
        })
        .collect()
}

/// 一行采购内容的交付字段。
#[derive(Debug, Clone, Copy)]
pub struct DeliveryFields<'a> {
    /// 销售稳定行。
    pub sales_order_line_id: &'a str,
    /// 采购数量。
    pub quantity: Quantity,
    /// 预计交付日。
    pub expected_delivery_date: Option<BusinessDate>,
    /// 收件分配。
    pub delivery_allocations: &'a [PurchaseDeliveryAllocation],
}

impl<'a> DeliveryFields<'a> {
    /// 读取采购提交行的交付字段。
    ///
    /// # 参数
    /// * `line` - 采购提交行
    ///
    /// # 返回
    /// 商品行且有销售来源与数量时返回字段；否则返回 `None`。
    ///
    /// # 错误
    /// 无。
    pub fn of_submission(line: &'a PurchaseOrderSubmissionLine) -> Option<Self> {
        (line.line_type == PurchaseLineType::ItemService).then_some(())?;
        Some(Self {
            sales_order_line_id: line.sales_order_line_id.as_ref()?.as_ref(),
            quantity: line.quantity?,
            expected_delivery_date: line.expected_delivery_date,
            delivery_allocations: &line.delivery_allocations,
        })
    }

    /// 读取采购版本行的交付字段。
    ///
    /// # 参数
    /// * `line` - 采购版本行
    ///
    /// # 返回
    /// 商品行且有销售来源与数量时返回字段；否则返回 `None`。
    ///
    /// # 错误
    /// 无。
    pub fn of_revision(line: &'a PurchaseOrderRevisionLine) -> Option<Self> {
        (line.line_type == PurchaseLineType::ItemService).then_some(())?;
        Some(Self {
            sales_order_line_id: line.sales_order_line_id.as_ref()?.as_ref(),
            quantity: line.quantity?,
            expected_delivery_date: line.expected_delivery_date,
            delivery_allocations: &line.delivery_allocations,
        })
    }

    /// 读取采购行输入的交付字段。
    ///
    /// # 参数
    /// * `line` - 采购行输入
    ///
    /// # 返回
    /// 商品行且有销售来源与数量时返回字段；否则返回 `None`。
    ///
    /// # 错误
    /// 无。
    pub fn of_input(line: &'a PurchaseLineInput) -> Option<Self> {
        (line.line_type == PurchaseLineType::ItemService).then_some(())?;
        Some(Self {
            sales_order_line_id: line.sales_order_line_id.as_ref()?.as_ref(),
            quantity: line.quantity?,
            expected_delivery_date: line.expected_delivery_date,
            delivery_allocations: &line.delivery_allocations,
        })
    }
}

/// 收件分配的复验范围。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RowCheck {
    /// 只复验相对原行变化的收件分配或数量。
    Changed,
    /// 复验全部收件分配。
    All,
}

/// 一行待复验的交付调整；为空的字段不参与复验。
#[derive(Debug, Clone, Copy)]
pub struct DeliveryEdit<'a> {
    /// 销售稳定行。
    pub sales_order_line_id: &'a str,
    /// 采购数量。
    pub quantity: Quantity,
    /// 新填写的预计交付日。
    pub expected_delivery_date: Option<BusinessDate>,
    /// 需复验的收件分配。
    pub delivery_allocations: Option<&'a [PurchaseDeliveryAllocation]>,
}

/// 相对原行找出需复验的交付调整。
///
/// # 参数
/// * `lines` - 待写入的采购行
/// * `previous` - 原行；按销售稳定行对应
/// * `rows` - 收件分配的复验范围
///
/// # 返回
/// 返回调整了预计交付日或需复验收件分配的行；未改动的预计交付日不复验，已登记维持
/// 的旧交期不阻断其他调整。
///
/// # 错误
/// 无。
pub fn delivery_edits<'a>(
    lines: &[DeliveryFields<'a>],
    previous: &[DeliveryFields<'_>],
    rows: RowCheck,
) -> Vec<DeliveryEdit<'a>> {
    lines
        .iter()
        .filter(|line| !line.quantity.to_decimal().is_zero())
        .filter_map(|line| {
            let before = previous.iter().find(|old| old.sales_order_line_id == line.sales_order_line_id);
            let date_changed =
                before.is_none_or(|old| old.expected_delivery_date != line.expected_delivery_date);
            let rows_changed = rows == RowCheck::All
                || before.is_none_or(|old| {
                    old.delivery_allocations != line.delivery_allocations || old.quantity != line.quantity
                });
            let edit = DeliveryEdit {
                sales_order_line_id: line.sales_order_line_id,
                quantity: line.quantity,
                expected_delivery_date: line.expected_delivery_date.filter(|_| date_changed),
                delivery_allocations: rows_changed.then_some(line.delivery_allocations),
            };
            (edit.expected_delivery_date.is_some()
                || edit.delivery_allocations.is_some_and(|rows| !rows.is_empty()))
            .then_some(edit)
        })
        .collect()
}

impl ProcurementCoverageFacts {
    /// 按销售当前版本复验交付调整。
    ///
    /// # 参数
    /// * `order` - 正在调整的采购单；其当前提交与版本不计入其他采购承担量
    /// * `edits` - 待复验的交付调整
    ///
    /// # 返回
    /// 预计交付日不晚于销售履约期限、收件分配属于销售当前收件资料且不超过地址剩余量时
    /// 返回 `Ok(())`。已移出销售当前版本的行不在此复验。
    ///
    /// # 错误
    /// 预计交付日晚于履约期限、地址不一致、超量、精度非法或期限无法换算时返回错误。
    pub fn validate_delivery_edits(&self, order: &PurchaseOrder, edits: &[DeliveryEdit<'_>]) -> Result<()> {
        for edit in edits {
            let Some((line, goods)) = self.current_goods_line(edit.sales_order_line_id) else {
                continue;
            };
            if let Some(expected) = edit.expected_delivery_date {
                let due = business_date_of(goods.fulfillment_due_at)?;
                if expected > due {
                    return Err(Error::from(format!(
                        "第 {} 行 {} 预计交付日 {expected} 晚于销售履约期限 {due}，请调整预计交付日",
                        line.line_no, line.item_name_snapshot
                    )));
                }
            }
            if let Some(rows) = edit.delivery_allocations.filter(|rows| !rows.is_empty()) {
                PurchaseDeliveryAllocation::validate_source(
                    rows,
                    &goods.delivery_allocations,
                    &self.other_delivery_allocations(order, edit.sales_order_line_id),
                    edit.quantity,
                    order.fulfillment_responsibility,
                    self.quantity_scales.get(goods.sku_id.as_ref()).copied(),
                    &goods.base_unit_code,
                )?;
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use std::str::FromStr;

    use erp_core::common::time::{BusinessDate, Instant};
    use erp_core::ids::{
        PurchaseOrderId, SalesOrderId, SalesOrderLineId, SalesOrderRevisionId, SalesOrderRevisionLineId,
        SkuId, SkuRevisionId, SupplierAccountId,
    };
    use erp_core::money::Quantity;

    use super::{DeliveryFields, RowCheck, delivery_edits, remaining_delivery_capacity};
    use crate::entity::facts::{FactIdentity, SalesGoodsLineFact, SalesLineType, SalesRevisionLineFact};
    use crate::entity::purchase_order::{
        FulfillmentResponsibility, ProcurementCoverageFacts, PurchaseDeliveryAllocation, PurchaseOrder,
        PurchaseOrderData, PurchaseType,
    };

    fn qty(value: &str) -> Quantity {
        Quantity::from_str(value).unwrap()
    }

    fn date(value: &str) -> BusinessDate {
        BusinessDate::from_str(value).unwrap()
    }

    fn row(address: &str, quantity: &str) -> PurchaseDeliveryAllocation {
        PurchaseDeliveryAllocation {
            recipient: "张三".into(),
            phone: "13800000000".into(),
            address: address.into(),
            quantity: qty(quantity),
        }
    }

    fn fields<'a>(
        quantity: &str,
        expected: Option<&str>,
        rows: &'a [PurchaseDeliveryAllocation],
    ) -> DeliveryFields<'a> {
        DeliveryFields {
            sales_order_line_id: "sol-1",
            quantity: qty(quantity),
            expected_delivery_date: expected.map(date),
            delivery_allocations: rows,
        }
    }

    fn order() -> PurchaseOrder {
        PurchaseOrder::new(
            PurchaseOrderId::new("po-1"),
            PurchaseOrderData {
                business_org_unit_id: "org".into(),
                purchase_no: "PO-1".into(),
                sales_order_id: SalesOrderId::new("so-1"),
                sales_order_revision_id: SalesOrderRevisionId::new("rev-1"),
                creation_basis_id: "basis".into(),
                supplier_id: SupplierAccountId::new("sup-1"),
                purchase_type: PurchaseType::Physical,
                payment_term_code: "NET-30".into(),
                fulfillment_responsibility: FulfillmentResponsibility::SupplierDirect,
                owner_user_id: "buyer".into(),
                target_warehouse_id: None,
            },
            "buyer",
            crate::entity::test_support::payment_term,
        )
        .unwrap()
    }

    /// 当前版本第 3 行履约期限 2026-10-20（上海时间），要求地址A 6 件、地址B 4 件。
    fn facts() -> ProcurementCoverageFacts {
        ProcurementCoverageFacts {
            quantity_scales: [("sku-1".to_string(), 0)].into(),
            revision_lines: vec![SalesRevisionLineFact {
                base: FactIdentity { id: "cur-1".into() },
                sales_order_line_id: SalesOrderLineId::new("sol-1"),
                line_no: 3,
                line_type: SalesLineType::GoodsService,
                item_name_snapshot: "礼盒".into(),
                spec_snapshot: None,
                unit_snapshot: None,
            }],
            goods_lines: vec![SalesGoodsLineFact {
                revision_line_id: SalesOrderRevisionLineId::new("cur-1"),
                sku_id: SkuId::new("sku-1"),
                sku_revision_id: SkuRevisionId::new("skur-1"),
                quantity: qty("10"),
                base_unit_code: "件".into(),
                fulfillment_due_at: Instant::from_unix_secs(1_792_425_600),
                service_region: None,
                delivery_allocations: vec![row("地址A", "6"), row("地址B", "4")],
                assigned_delivery_allocations: Vec::new(),
            }],
            ..ProcurementCoverageFacts::default()
        }
    }

    /// 扣除其他采购后按销售顺序给出剩余量，超额承担时剩余为零。
    #[test]
    fn remaining_capacity_subtracts_other_purchases() {
        let remaining = remaining_delivery_capacity(
            &[row("地址A", "6"), row("地址B", "4"), row("地址A", "1")],
            &[row("地址A", "2"), row("地址B", "5"), row("地址C", "3")],
        )
        .unwrap();

        assert_eq!(remaining.len(), 2);
        assert_eq!((remaining[0].address.as_str(), remaining[0].quantity), ("地址A", qty("5")));
        assert_eq!((remaining[1].address.as_str(), remaining[1].quantity), ("地址B", qty("0")));
    }

    /// 没有其他采购时剩余量等于销售要求；没有销售要求时为空。
    #[test]
    fn remaining_capacity_without_assignments() {
        assert_eq!(remaining_delivery_capacity(&[row("地址A", "2")], &[]).unwrap(), vec![row("地址A", "2")]);
        assert!(remaining_delivery_capacity(&[], &[row("地址A", "2")]).unwrap().is_empty());
    }

    /// 只复验改动的交付日与收件分配；全部复验时收件分配始终进入复验。
    #[test]
    fn edits_cover_only_changed_delivery_fields() {
        let old_rows = [row("地址A", "4")];
        let new_rows = [row("地址B", "2")];
        let previous = [fields("4", Some("2026-10-25"), &old_rows)];

        let unchanged =
            delivery_edits(&[fields("4", Some("2026-10-25"), &old_rows)], &previous, RowCheck::Changed);
        let changed =
            delivery_edits(&[fields("2", Some("2026-10-19"), &new_rows)], &previous, RowCheck::Changed);
        let all = delivery_edits(&[fields("4", Some("2026-10-25"), &old_rows)], &previous, RowCheck::All);

        assert!(unchanged.is_empty());
        assert_eq!(changed[0].expected_delivery_date, Some(date("2026-10-19")));
        assert_eq!(changed[0].delivery_allocations, Some(&new_rows[..]));
        assert_eq!(all[0].expected_delivery_date, None);
        assert_eq!(all[0].delivery_allocations, Some(&old_rows[..]));
    }

    /// 改单移除的零数量行不再复验交付日与收件分配。
    #[test]
    fn zero_quantity_lines_are_not_checked() {
        let edits = delivery_edits(&[fields("0", Some("2099-01-01"), &[])], &[], RowCheck::All);

        assert!(edits.is_empty());
    }

    /// 改动的交付日不得晚于履约期限；收件分配须属于销售要求且不超过剩余量。
    #[test]
    fn validate_edits_checks_due_and_address_capacity() {
        let mut facts = facts();
        let order = order();
        let ok_rows = [row("地址A", "2"), row("地址B", "2")];
        let late = delivery_edits(&[fields("4", Some("2026-10-21"), &ok_rows)], &[], RowCheck::Changed);
        let on_due = delivery_edits(&[fields("4", Some("2026-10-20"), &ok_rows)], &[], RowCheck::Changed);
        let forged_rows = [row("伪造地址", "4")];
        let forged = delivery_edits(&[fields("4", None, &forged_rows)], &[], RowCheck::Changed);

        assert!(facts.validate_delivery_edits(&order, &on_due).is_ok());
        let error = facts.validate_delivery_edits(&order, &late).unwrap_err();
        assert!(error.to_string().contains("第 3 行 礼盒 预计交付日 2026-10-21 晚于销售履约期限 2026-10-20"));
        assert!(facts.validate_delivery_edits(&order, &forged).is_err());

        facts.revision_lines.clear();
        assert!(facts.validate_delivery_edits(&order, &late).is_ok());
    }
}
