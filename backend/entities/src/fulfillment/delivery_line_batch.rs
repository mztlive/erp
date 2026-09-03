//! `DeliveryLine` 批量工厂（FUL-E01）。
//!
//! 仓发创建与入库预占补仓发两条路径共用同一编号与归属规则。

use serde::{Deserialize, Serialize};

use crate::errors::{Error, Result};
use crate::fulfillment::{DeliveryLine, DeliveryLineData, DeliveryType};
use crate::ids::{
    DeliveryId, DeliveryLineId, PurchaseLineSalesAllocationId, SalesOrderLineId, StockReservationId,
};
use crate::money::Quantity;

/// 单行领域输入（已验证形态，不含行号与表头）。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DeliveryLineSpec {
    /// 行主键（Service 注入系统 ID）。
    pub line_id: DeliveryLineId,
    /// 销售稳定明细。
    pub sales_order_line_id: SalesOrderLineId,
    /// 发货数量。
    pub quantity: Quantity,
    /// 仓发消耗的预占；直发为空。
    pub stock_reservation_id: Option<StockReservationId>,
    /// 供应商直发的采购到销售分配；仓发为空。
    pub purchase_line_sales_allocation_id: Option<PurchaseLineSalesAllocationId>,
}

/// `DeliveryLine` 批量构造入口。
pub struct DeliveryLineBatch;

impl DeliveryLineBatch {
    /// 按输入顺序从首行号连续编号批量构造发货行。
    ///
    /// # 参数
    /// * `delivery_id` - 所属发货单主键
    /// * `delivery_type` - 发货类型（决定行级 reservation/allocation 互斥）
    /// * `first_line_no` - 起始行号（1 为新建，>1 为追加）
    /// * `specs` - 与输入顺序一致的行规格（含 Service 注入的行 ID）
    ///
    /// # 返回
    /// 返回与输入顺序一致的行实体集合。
    ///
    /// # 错误
    /// 首行号为 0、行号溢出 `u32` 上限或行级归属/数量违规时返回错误。
    ///
    /// # 约束
    /// 无 I/O、无时钟、无 ID 生成；行号 `checked_add`，溢出 fail-closed。
    pub fn build(
        delivery_id: DeliveryId,
        delivery_type: DeliveryType,
        first_line_no: u32,
        specs: Vec<DeliveryLineSpec>,
    ) -> Result<Vec<DeliveryLine>> {
        if first_line_no < 1 {
            return Err(Error::from("行号必须从 1 开始"));
        }
        let mut lines = Vec::with_capacity(specs.len());
        for (index, spec) in specs.into_iter().enumerate() {
            let offset = u32::try_from(index).map_err(|_| Error::from("发货行号溢出"))?;
            let line_no = first_line_no
                .checked_add(offset)
                .ok_or_else(|| Error::from("发货行号溢出"))?;
            if line_no < 1 {
                return Err(Error::from("行号必须从 1 开始"));
            }
            lines.push(DeliveryLine::new(
                spec.line_id,
                DeliveryLineData {
                    delivery_id: delivery_id.clone(),
                    line_no,
                    sales_order_line_id: spec.sales_order_line_id,
                    quantity: spec.quantity,
                    stock_reservation_id: spec.stock_reservation_id,
                    purchase_line_sales_allocation_id: spec.purchase_line_sales_allocation_id,
                },
                delivery_type,
            )?);
        }
        Ok(lines)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::str::FromStr;

    fn spec(id: &str) -> DeliveryLineSpec {
        DeliveryLineSpec {
            line_id: DeliveryLineId::new(id),
            sales_order_line_id: SalesOrderLineId::new("so-line-1"),
            quantity: Quantity::from_str("2").unwrap(),
            stock_reservation_id: Some(StockReservationId::new("rsv-1")),
            purchase_line_sales_allocation_id: None,
        }
    }

    /// 仓发正常路径：输入顺序与行号一一对应。
    #[test]
    fn warehouse_build_preserves_order_and_numbers() {
        let lines = DeliveryLineBatch::build(
            DeliveryId::new("d-1"),
            DeliveryType::WarehouseShip,
            1,
            vec![spec("l-1"), spec("l-2")],
        )
        .unwrap();
        assert_eq!(lines.len(), 2);
        assert_eq!(lines[0].line_no, 1);
        assert_eq!(lines[1].line_no, 2);
        assert_eq!(lines[0].base.id, "l-1");
    }

    /// 非 1 首行号连续追加。
    #[test]
    fn non_one_first_line_no_appends_continuously() {
        let lines = DeliveryLineBatch::build(
            DeliveryId::new("d-1"),
            DeliveryType::WarehouseShip,
            4,
            vec![spec("l-1"), spec("l-2")],
        )
        .unwrap();
        assert_eq!(lines[0].line_no, 4);
        assert_eq!(lines[1].line_no, 5);
    }

    /// 直发行字段映射：分配必填、预占为空。
    #[test]
    fn direct_line_maps_allocation_fields() {
        let lines = DeliveryLineBatch::build(
            DeliveryId::new("d-1"),
            DeliveryType::SupplierDirect,
            1,
            vec![DeliveryLineSpec {
                stock_reservation_id: None,
                purchase_line_sales_allocation_id: Some(PurchaseLineSalesAllocationId::new("pla-1")),
                ..spec("l-1")
            }],
        )
        .unwrap();
        assert_eq!(
            lines[0]
                .purchase_line_sales_allocation_id
                .as_ref()
                .unwrap()
                .as_ref(),
            "pla-1"
        );
    }

    /// 错误组合 fail-closed：仓发行缺预占、直发行带预占。
    #[test]
    fn invalid_reservation_allocation_combo_fails_closed() {
        assert!(DeliveryLineBatch::build(
            DeliveryId::new("d-1"),
            DeliveryType::WarehouseShip,
            1,
            vec![DeliveryLineSpec {
                stock_reservation_id: None,
                ..spec("l-1")
            }],
        )
        .is_err());
        assert!(DeliveryLineBatch::build(
            DeliveryId::new("d-1"),
            DeliveryType::SupplierDirect,
            1,
            vec![spec("l-1")],
        )
        .is_err());
    }

    /// 行号溢出返回错误而非 panic：首行号 0 与 u32 上限追加。
    #[test]
    fn line_no_overflow_fails_without_panic() {
        assert!(DeliveryLineBatch::build(
            DeliveryId::new("d-1"),
            DeliveryType::WarehouseShip,
            0,
            vec![spec("l-1")],
        )
        .is_err());
        assert!(DeliveryLineBatch::build(
            DeliveryId::new("d-1"),
            DeliveryType::WarehouseShip,
            u32::MAX,
            vec![spec("l-1"), spec("l-2")],
        )
        .is_err());
    }
}
