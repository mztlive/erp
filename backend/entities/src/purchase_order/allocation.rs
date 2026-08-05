//! `purchase_line_sales_allocation` 采购行→销售行分配（数据模型 §6.6）。
//!
//! 采购行到销售行的数量归属必须显式分配（§9.2）；入库预占必须沿本分配关系
//! 回到原销售明细，禁止按 SKU 猜测归属（§6.6）。

use entity_core::BaseModel;
use entity_macros::Entity;
use serde::{Deserialize, Serialize};

use crate::errors::{Error, Result};
use crate::ids::{PurchaseLineSalesAllocationId, PurchaseOrderRevisionLineId, SalesOrderRevisionLineId};
use crate::money::{Amount, Quantity};

/// 采购行→销售行分配创建数据（不含系统字段）。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PurchaseLineSalesAllocationData {
    /// 采购版本明细。
    pub purchase_order_revision_line_id: PurchaseOrderRevisionLineId,
    /// 被满足的销售版本明细。
    pub sales_order_revision_line_id: SalesOrderRevisionLineId,
    /// 分配数量。
    pub allocated_quantity: Quantity,
    /// 分配采购成本（含税）。
    pub allocated_cost_gross: Amount,
    /// 分配采购成本（不含税）。
    pub allocated_cost_net: Amount,
}

/// 采购行→销售行分配实体（数据模型 §6.6）。
///
/// 实体层只做单行内非负与方向校验；「分配数量不超过采购数量」「不超过销售承诺
/// 数量」依赖聚合查询，由 P3 在形成分配的事务内校验（§6.6 必需约束）。
#[derive(Debug, Serialize, Deserialize, Clone, Entity, PartialEq, Eq)]
pub struct PurchaseLineSalesAllocation {
    #[serde(flatten)]
    pub base: BaseModel,
    /// 采购版本明细。
    pub purchase_order_revision_line_id: PurchaseOrderRevisionLineId,
    /// 被满足的销售版本明细。
    pub sales_order_revision_line_id: SalesOrderRevisionLineId,
    /// 分配数量（非负）。
    pub allocated_quantity: Quantity,
    /// 分配采购成本（含税，非负）。
    pub allocated_cost_gross: Amount,
    /// 分配采购成本（不含税，非负）。
    pub allocated_cost_net: Amount,
}

impl PurchaseLineSalesAllocation {
    /// 创建采购行→销售行分配。
    ///
    /// 校验分配数量、两端成本金额非负，且不含税成本不超过含税成本（方向校验）。
    ///
    /// # 参数
    /// * `id` - 实体主键（`entities::ids::PurchaseLineSalesAllocationId`）
    /// * `data` - 创建数据
    ///
    /// # 返回
    /// 返回新建的分配实体。
    ///
    /// # 错误
    /// 分配数量/成本为负，或不含税成本大于含税成本时返回错误。
    ///
    /// # 说明
    /// 跨行上限校验（不超采购行数量、不超销售承诺数量）依赖聚合查询，留 P3（§6.6）。
    pub fn new(id: PurchaseLineSalesAllocationId, data: PurchaseLineSalesAllocationData) -> Result<Self> {
        if data.allocated_quantity.to_decimal() < rust_decimal::Decimal::ZERO {
            return Err(Error::from("分配数量不能为负"));
        }
        if data.allocated_cost_gross.to_decimal() < rust_decimal::Decimal::ZERO
            || data.allocated_cost_net.to_decimal() < rust_decimal::Decimal::ZERO
        {
            return Err(Error::from("分配成本不能为负"));
        }
        if data.allocated_cost_net > data.allocated_cost_gross {
            return Err(Error::from("不含税分配成本不能超过含税分配成本"));
        }
        Ok(Self {
            base: BaseModel::new(id.to_string()),
            purchase_order_revision_line_id: data.purchase_order_revision_line_id,
            sales_order_revision_line_id: data.sales_order_revision_line_id,
            allocated_quantity: data.allocated_quantity,
            allocated_cost_gross: data.allocated_cost_gross,
            allocated_cost_net: data.allocated_cost_net,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::{PurchaseLineSalesAllocation, PurchaseLineSalesAllocationData};
    use crate::ids::{PurchaseLineSalesAllocationId, PurchaseOrderRevisionLineId, SalesOrderRevisionLineId};
    use crate::money::{Amount, Quantity};
    use std::str::FromStr;

    fn allocation_data() -> PurchaseLineSalesAllocationData {
        PurchaseLineSalesAllocationData {
            purchase_order_revision_line_id: PurchaseOrderRevisionLineId::new("porl-1"),
            sales_order_revision_line_id: SalesOrderRevisionLineId::new("sorl-1"),
            allocated_quantity: Quantity::from_str("3.000000").unwrap(),
            allocated_cost_gross: Amount::from_str("29.97").unwrap(),
            allocated_cost_net: Amount::from_str("26.07").unwrap(),
        }
    }

    #[test]
    fn allocation_happy_path() {
        let allocation = PurchaseLineSalesAllocation::new(
            PurchaseLineSalesAllocationId::new("alloc-1"),
            allocation_data(),
        )
        .unwrap();
        assert_eq!(
            allocation.allocated_quantity,
            Quantity::from_str("3.000000").unwrap()
        );
        assert_eq!(
            allocation.allocated_cost_gross,
            Amount::from_str("29.97").unwrap()
        );
    }

    #[test]
    fn allocation_rejects_negative_and_inverted_amounts() {
        let negative_quantity = PurchaseLineSalesAllocationData {
            allocated_quantity: Quantity::from_str("-1.000000").unwrap(),
            ..allocation_data()
        };
        assert!(PurchaseLineSalesAllocation::new(
            PurchaseLineSalesAllocationId::new("alloc-2"),
            negative_quantity,
        )
        .is_err());

        let inverted = PurchaseLineSalesAllocationData {
            allocated_cost_net: Amount::from_str("30.00").unwrap(),
            ..allocation_data()
        };
        assert!(
            PurchaseLineSalesAllocation::new(PurchaseLineSalesAllocationId::new("alloc-3"), inverted)
                .is_err()
        );

        let negative_cost = PurchaseLineSalesAllocationData {
            allocated_cost_gross: Amount::from_str("-1.00").unwrap(),
            ..allocation_data()
        };
        assert!(PurchaseLineSalesAllocation::new(
            PurchaseLineSalesAllocationId::new("alloc-4"),
            negative_cost
        )
        .is_err());
    }
}
