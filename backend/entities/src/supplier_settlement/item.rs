//! `supplier_settlement_item`（数据模型 §6.20 供应商结算订单明细）。
//!
//! 正式结算事实行，只 `new` 不 `update`：完成、取消和退款事实均参与结算，
//! 不按可变当前状态猜测历史金额（§6.20）。`erp_calculated_amount` 由结算构成
//! 派生并强制恒等，全部结算金额非负。

use entity_core::BaseModel;
use entity_macros::Entity;
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};

use crate::errors::{Error, Result};
use crate::ids::{
    SupplierFulfillmentItemId, SupplierFulfillmentOrderId, SupplierSettlementItemId,
    SupplierSettlementStatementId,
};
use crate::money::Amount;

/// 结算明细创建数据（不含系统字段；`erp_calculated_amount` 由结算构成派生）。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SupplierSettlementItemData {
    /// 所属结算单。
    pub statement_id: SupplierSettlementStatementId,
    /// 供应商子订单。
    pub supplier_fulfillment_order_id: SupplierFulfillmentOrderId,
    /// 供应商履约明细。
    pub supplier_fulfillment_item_id: SupplierFulfillmentItemId,
    /// 订单结算金额。
    pub order_amount: Amount,
    /// 运费金额。
    pub freight_amount: Amount,
    /// 服务费金额。
    pub service_fee_amount: Amount,
    /// 供应商退款金额。
    pub refund_amount: Amount,
    /// ERP 计算金额（= 订单 + 运费 + 服务费 − 退款）。
    pub erp_calculated_amount: Amount,
    /// 供应商账单金额。
    pub supplier_billed_amount: Amount,
}

/// 供应商结算明细实体（数据模型 §6.20，正式结算事实行，不可变）。
#[derive(Debug, Serialize, Deserialize, Clone, Entity, PartialEq, Eq)]
pub struct SupplierSettlementItem {
    #[serde(flatten)]
    pub base: BaseModel,
    /// 所属结算单。
    pub statement_id: SupplierSettlementStatementId,
    /// 供应商子订单。
    pub supplier_fulfillment_order_id: SupplierFulfillmentOrderId,
    /// 供应商履约明细。
    pub supplier_fulfillment_item_id: SupplierFulfillmentItemId,
    /// 订单结算金额。
    pub order_amount: Amount,
    /// 运费金额。
    pub freight_amount: Amount,
    /// 服务费金额。
    pub service_fee_amount: Amount,
    /// 供应商退款金额。
    pub refund_amount: Amount,
    /// ERP 计算金额。
    pub erp_calculated_amount: Amount,
    /// 供应商账单金额。
    pub supplier_billed_amount: Amount,
}

impl SupplierSettlementItem {
    /// 创建供应商结算明细。
    ///
    /// 校验全部结算金额非负，并强制构成恒等（§6.20）：
    /// `erp_calculated_amount = order + freight + service_fee − refund_amount`。
    ///
    /// # 参数
    /// * `id` - 实体主键（`entities::ids::SupplierSettlementItemId`）
    /// * `data` - 创建数据
    ///
    /// # 返回
    /// 返回新建的结算明细实体。
    ///
    /// # 错误
    /// 任一金额为负或构成恒等不成立时返回错误。
    pub fn new(id: SupplierSettlementItemId, data: SupplierSettlementItemData) -> Result<Self> {
        ensure_non_negative(data.order_amount, "订单金额不得为负")?;
        ensure_non_negative(data.freight_amount, "运费金额不得为负")?;
        ensure_non_negative(data.service_fee_amount, "服务费金额不得为负")?;
        ensure_non_negative(data.refund_amount, "退款金额不得为负")?;
        ensure_non_negative(data.erp_calculated_amount, "ERP 计算金额不得为负")?;
        ensure_non_negative(data.supplier_billed_amount, "供应商账单金额不得为负")?;
        let expected = data
            .order_amount
            .checked_add(data.freight_amount)
            .checked_add(data.service_fee_amount)
            .checked_sub(data.refund_amount);
        if expected != data.erp_calculated_amount {
            return Err(Error::from(
                "ERP 计算金额必须等于订单金额加运费加服务费减退款金额",
            ));
        }

        Ok(Self {
            base: BaseModel::new(id.to_string()),
            statement_id: data.statement_id,
            supplier_fulfillment_order_id: data.supplier_fulfillment_order_id,
            supplier_fulfillment_item_id: data.supplier_fulfillment_item_id,
            order_amount: data.order_amount,
            freight_amount: data.freight_amount,
            service_fee_amount: data.service_fee_amount,
            refund_amount: data.refund_amount,
            erp_calculated_amount: data.erp_calculated_amount,
            supplier_billed_amount: data.supplier_billed_amount,
        })
    }
}

/// 校验金额非负。
///
/// # 参数
/// * `value` - 金额
/// * `message` - 失败时的错误信息
///
/// # 错误
/// 金额为负时返回错误。
fn ensure_non_negative(value: Amount, message: &str) -> Result<()> {
    if value.to_decimal() < Decimal::ZERO {
        return Err(Error::from(message));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ids::{SupplierFulfillmentItemId, SupplierFulfillmentOrderId, SupplierSettlementItemId};
    use std::str::FromStr;

    fn sample_data() -> SupplierSettlementItemData {
        SupplierSettlementItemData {
            statement_id: SupplierSettlementStatementId::new("statement-1"),
            supplier_fulfillment_order_id: SupplierFulfillmentOrderId::new("order-1"),
            supplier_fulfillment_item_id: SupplierFulfillmentItemId::new("item-1"),
            order_amount: Amount::from_str("100.00").unwrap(),
            freight_amount: Amount::from_str("10.00").unwrap(),
            service_fee_amount: Amount::from_str("5.00").unwrap(),
            refund_amount: Amount::from_str("15.00").unwrap(),
            erp_calculated_amount: Amount::from_str("100.00").unwrap(),
            supplier_billed_amount: Amount::from_str("99.50").unwrap(),
        }
    }

    #[test]
    fn new_accepts_valid_item_with_identity() {
        let item =
            SupplierSettlementItem::new(SupplierSettlementItemId::new("settlement-item-1"), sample_data())
                .unwrap();

        assert_eq!(item.erp_calculated_amount, Amount::from_str("100.00").unwrap());
        assert_eq!(item.supplier_billed_amount, Amount::from_str("99.50").unwrap());
    }

    #[test]
    fn new_rejects_negative_amounts() {
        for field in [
            ("order_amount", -1.0),
            ("freight_amount", -0.01),
            ("service_fee_amount", -0.01),
            ("refund_amount", -0.01),
            ("erp_calculated_amount", -0.01),
            ("supplier_billed_amount", -0.01),
        ] {
            let amount = Amount::from_str(&format!("{:.2}", field.1)).unwrap();
            let mut data = sample_data();
            match field.0 {
                "order_amount" => data.order_amount = amount,
                "freight_amount" => data.freight_amount = amount,
                "service_fee_amount" => data.service_fee_amount = amount,
                "refund_amount" => data.refund_amount = amount,
                "erp_calculated_amount" => data.erp_calculated_amount = amount,
                "supplier_billed_amount" => data.supplier_billed_amount = amount,
                _ => unreachable!(),
            }
            assert!(
                SupplierSettlementItem::new(SupplierSettlementItemId::new("settlement-item-2"), data)
                    .is_err(),
                "{} 为负必须被拒绝",
                field.0
            );
        }
    }

    #[test]
    fn new_rejects_inconsistent_erp_calculated_amount() {
        let data = SupplierSettlementItemData {
            erp_calculated_amount: Amount::from_str("99.99").unwrap(),
            ..sample_data()
        };
        assert!(
            SupplierSettlementItem::new(SupplierSettlementItemId::new("settlement-item-3"), data).is_err()
        );
    }
}
