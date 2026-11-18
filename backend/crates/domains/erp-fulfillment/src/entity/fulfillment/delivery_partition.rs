//! 已冻结直发草稿按可证明数量拆分，原数量与本次交付保持守恒。

use erp_core::ids::{DeliveryId, DeliveryLineId};
use erp_core::money::Quantity;
use erp_core::{Error, Result};
use rust_decimal::Decimal;

use super::{Delivery, DeliveryData, DeliveryLine, DeliveryLineData, DeliveryState, DeliveryType};

impl Delivery {
    /// 为同一采购、销售及地址创建独立的部分发货草稿。
    /// 未发货的独立草稿，数量须由 split_quantity 提供。
    /// `id` 与 `number` 为服务端生成的新发货身份及业务编号。
    /// 原单不是有效供应商直发草稿时拒绝。
    pub fn observed_partition(&self, id: DeliveryId, number: String) -> Result<Self> {
        if self.delivery_type != DeliveryType::SupplierDirect
            || self.status != DeliveryState::Draft
            || self.base.deleted_at != 0
        {
            return Err(Error::from("只能拆分尚未发货的供应商直发草稿"));
        }
        Self::new(
            id,
            DeliveryData {
                delivery_no: number,
                delivery_type: self.delivery_type,
                sales_order_id: self.sales_order_id.clone(),
                purchase_order_id: self.purchase_order_id.clone(),
                warehouse_id: None,
                tracking_entries: Vec::new(),
                address_snapshot_encrypted: self.address_snapshot_encrypted.clone(),
                address_snapshot_fingerprint: self.address_snapshot_fingerprint.clone(),
            },
        )
    }
}

impl DeliveryLine {
    /// 从原行中取得确定数量，新行与剩余行数量之和等于原行。
    /// 本次交付行和剩余草稿行；恰好用尽时剩余为 None。
    /// 新发货及新行身份、来源能够证明的本次发货数量。
    /// 非直发、数量不正、超过剩余、身份复用或算术溢出时拒绝。
    pub fn split_quantity(
        &self,
        delivery_id: DeliveryId,
        line_id: DeliveryLineId,
        quantity: Quantity,
    ) -> Result<(Self, Option<Self>)> {
        if quantity.to_decimal() <= Decimal::ZERO
            || quantity > self.quantity
            || self.stock_reservation_id.is_some()
            || self.purchase_line_sales_allocation_id.is_none()
            || delivery_id == self.delivery_id
            || line_id.as_ref() == self.base.id
        {
            return Err(Error::from("部分直发数量或采购销售归属无效"));
        }

        let shipped = Self::new(
            line_id,
            DeliveryLineData {
                delivery_id,
                line_no: self.line_no,
                sales_order_line_id: self.sales_order_line_id.clone(),
                quantity,
                stock_reservation_id: None,
                purchase_line_sales_allocation_id: self.purchase_line_sales_allocation_id.clone(),
            },
            DeliveryType::SupplierDirect,
        )?;

        let rest = self
            .quantity
            .to_decimal()
            .checked_sub(quantity.to_decimal())
            .ok_or_else(|| Error::from("部分发货数量超限"))?;
        let remaining = if rest == Decimal::ZERO {
            None
        } else {
            let mut line = self.clone();
            line.quantity = Quantity::try_from(rest)?;
            Some(line)
        };
        Ok((shipped, remaining))
    }
}

#[cfg(test)]
mod tests {
    use erp_core::ids::{PurchaseLineSalesAllocationId, SalesOrderLineId};

    use super::*;

    fn line() -> DeliveryLine {
        DeliveryLine::new(
            DeliveryLineId::new("line"),
            DeliveryLineData {
                delivery_id: DeliveryId::new("draft"),
                line_no: 1,
                sales_order_line_id: SalesOrderLineId::new("sale"),
                quantity: "3".parse().unwrap(),
                stock_reservation_id: None,
                purchase_line_sales_allocation_id: Some(PurchaseLineSalesAllocationId::new("allocation")),
            },
            DeliveryType::SupplierDirect,
        )
        .unwrap()
    }
    #[test]
    fn partial_and_final_delivery_preserve_allocation_and_exact_quantity() {
        let original = line();
        let (first, rest) = original
            .split_quantity(DeliveryId::new("part1"), DeliveryLineId::new("line1"), "1".parse().unwrap())
            .unwrap();
        let rest = rest.unwrap();
        assert_eq!(first.quantity.to_decimal() + rest.quantity.to_decimal(), original.quantity.to_decimal());
        assert_eq!(first.purchase_line_sales_allocation_id, original.purchase_line_sales_allocation_id);

        let (last, rest) = rest
            .split_quantity(DeliveryId::new("part2"), DeliveryLineId::new("line2"), "2".parse().unwrap())
            .unwrap();
        assert!(rest.is_none());
        assert_eq!(last.quantity, "2".parse().unwrap());
    }
    #[test]
    fn over_dispatch_zero_and_reused_identity_fail_without_mutation() {
        let original = line();
        let before = original.clone();
        for quantity in ["0", "-1", "4"] {
            assert!(
                original
                    .split_quantity(
                        DeliveryId::new("part"),
                        DeliveryLineId::new("line2"),
                        quantity.parse().unwrap()
                    )
                    .is_err()
            );
        }
        assert!(
            original
                .split_quantity(DeliveryId::new("draft"), DeliveryLineId::new("line2"), "1".parse().unwrap())
                .is_err()
        );
        assert_eq!(original, before);
    }
}
