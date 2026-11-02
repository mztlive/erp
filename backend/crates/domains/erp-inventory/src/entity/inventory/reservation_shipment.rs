//! 仓发消费预占的最小领域事实与归属、数量规则。

use erp_core::ids::{SalesOrderLineId, SkuId, WarehouseId};
use erp_core::money::Quantity;

use crate::{Error, Result};

/// 从当前预占记录取得的仓发消费事实；状态和数量写入仍由仓储原子守卫校验。
#[derive(Clone)]
pub(crate) struct ReservationFact {
    /// 预占主键。
    pub(crate) id: String,
    /// 唯一归属的销售明细。
    pub(crate) sales_order_line_id: SalesOrderLineId,
    /// 当前记录中的剩余预占，不假定历史数据为正。
    pub(crate) reserved_quantity: Quantity,
    /// 预占所属仓库。
    pub(crate) warehouse_id: WarehouseId,
    /// 预占所属 SKU。
    pub(crate) sku_id: SkuId,
}

impl ReservationFact {
    /// 按销售行归属、剩余数量、发货仓存在性及仓库归属的顺序确定发货仓。
    ///
    /// # 参数
    /// * `sales_order_line_id` - 当前仓发行归属的销售明细。
    /// * `quantity` - 当前仓发行数量，流水构造仍负责数量正数约束。
    /// * `warehouse_id` - 发货单指定的仓库。
    ///
    /// # 返回
    /// 归属与剩余数量满足时返回发货仓。
    ///
    /// # 错误
    /// 销售行不匹配、数量不足、缺少发货仓或仓库不匹配时返回原业务错误。
    pub(crate) fn shipment_warehouse(
        &self,
        sales_order_line_id: &SalesOrderLineId,
        quantity: Quantity,
        warehouse_id: Option<&WarehouseId>,
    ) -> Result<WarehouseId> {
        if self.sales_order_line_id != *sales_order_line_id {
            return Err(Error::BusinessLogicError("库存预占不属于本销售明细，不能消耗".into()));
        }
        if self.reserved_quantity.to_decimal() < quantity.to_decimal() {
            return Err(Error::BusinessLogicError("为这单留的货不足，无法发货".into()));
        }
        let warehouse_id = warehouse_id.ok_or_else(|| Error::BusinessLogicError("仓发缺少发货仓".into()))?;
        if self.warehouse_id != *warehouse_id {
            return Err(Error::BusinessLogicError("库存预占不属于本发货仓，无法发货".into()));
        }
        Ok(warehouse_id.clone())
    }
}

#[cfg(test)]
mod tests {
    use std::str::FromStr;

    use super::*;

    fn fact() -> ReservationFact {
        ReservationFact {
            id: "reservation-1".into(),
            sales_order_line_id: SalesOrderLineId::new("sales-line-1"),
            reserved_quantity: Quantity::from_str("2").unwrap(),
            warehouse_id: WarehouseId::new("warehouse-1"),
            sku_id: SkuId::new("sku-1"),
        }
    }

    #[test]
    fn shipment_accepts_exact_remaining_quantity_and_original_warehouse() {
        let fact = fact();
        assert_eq!(
            fact.shipment_warehouse(
                &fact.sales_order_line_id,
                fact.reserved_quantity,
                Some(&fact.warehouse_id),
            )
            .unwrap(),
            fact.warehouse_id,
        );
    }

    #[test]
    fn shipment_checks_sales_ownership_before_quantity_and_missing_warehouse() {
        let fact = fact();
        let error = fact
            .shipment_warehouse(&SalesOrderLineId::new("other"), Quantity::from_str("3").unwrap(), None)
            .unwrap_err();
        assert!(matches!(error, Error::BusinessLogicError(message)
            if message == "库存预占不属于本销售明细，不能消耗"));
        let error = fact
            .shipment_warehouse(&fact.sales_order_line_id, Quantity::from_str("3").unwrap(), None)
            .unwrap_err();
        assert!(matches!(error, Error::BusinessLogicError(message)
            if message == "为这单留的货不足，无法发货"));
    }

    #[test]
    fn shipment_distinguishes_missing_and_different_warehouse() {
        let fact = fact();
        let error =
            fact.shipment_warehouse(&fact.sales_order_line_id, fact.reserved_quantity, None).unwrap_err();
        assert!(matches!(error, Error::BusinessLogicError(message) if message == "仓发缺少发货仓"));
        let error = fact
            .shipment_warehouse(
                &fact.sales_order_line_id,
                fact.reserved_quantity,
                Some(&WarehouseId::new("other")),
            )
            .unwrap_err();
        assert!(matches!(error, Error::BusinessLogicError(message)
            if message == "库存预占不属于本发货仓，无法发货"));
    }
}
