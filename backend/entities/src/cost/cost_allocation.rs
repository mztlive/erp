//! `cost_allocation` 成本分配（数据模型 §6.10）。

use entity_core::BaseModel;
use entity_macros::Entity;
use serde::{Deserialize, Serialize};

use crate::errors::{Error, Result};
use crate::ids::{CostAllocationId, CostEntryId, SalesOrderId, SalesOrderLineId};
use crate::money::Amount;

/// 成本分配创建数据。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CostAllocationData {
    /// 成本事实。
    pub cost_entry_id: CostEntryId,
    /// 经营归属销售单（非卡券成本必填）。
    pub sales_order_id: Option<SalesOrderId>,
    /// 经营归属销售明细（可空）。
    pub sales_order_line_id: Option<SalesOrderLineId>,
    /// 分配含税金额。
    pub allocated_gross_amount: Amount,
    /// 分配不含税金额。
    pub allocated_net_amount: Amount,
    /// 是否承担最后一笔尾差。
    pub rounding_residual_flag: bool,
}

/// 成本分配实体（正式事实，数据模型 §6.10）。
///
/// 归属目标为销售单；分配含税金额不得小于不含税金额
/// 且均为正数。成本分配合计等于成本事实金额、非卡券成本必须落到销售单或销售
/// 明细、尾差只落到该商品明细最后一个支付来源是跨事实约束，由 P3 过账事务
/// 校验（§8.4）。分配行过账后不可更新或删除。
#[derive(Debug, Serialize, Deserialize, Clone, Entity, PartialEq, Eq)]
pub struct CostAllocation {
    #[serde(flatten)]
    pub base: BaseModel,
    /// 成本事实。
    pub cost_entry_id: CostEntryId,
    /// 经营归属销售单。
    pub sales_order_id: Option<SalesOrderId>,
    /// 经营归属销售明细。
    pub sales_order_line_id: Option<SalesOrderLineId>,
    /// 分配含税金额。
    pub allocated_gross_amount: Amount,
    /// 分配不含税金额。
    pub allocated_net_amount: Amount,
    /// 是否承担最后一笔尾差。
    pub rounding_residual_flag: bool,
}

impl CostAllocation {
    /// 创建成本分配。
    ///
    /// 完成销售单归属校验、金额正数与
    /// 含税 ≥ 不含税校验。
    ///
    /// # 参数
    /// * `id` - 实体主键（`entities::ids::CostAllocationId`）
    /// * `data` - 创建数据
    ///
    /// # 返回
    /// 返回新建的分配实体。
    ///
    /// # 错误
    /// 当销售单归属缺失、金额非正或含税金额小于不含税金额时返回错误。
    pub fn new(id: CostAllocationId, data: CostAllocationData) -> Result<Self> {
        validate_target(&data.sales_order_id, &data.sales_order_line_id)?;
        if data.allocated_gross_amount.to_decimal().is_sign_negative()
            || data.allocated_gross_amount.to_decimal().is_zero()
            || data.allocated_net_amount.to_decimal().is_sign_negative()
            || data.allocated_net_amount.to_decimal().is_zero()
        {
            return Err(Error::from("分配金额必须为正数"));
        }
        if data.allocated_gross_amount < data.allocated_net_amount {
            return Err(Error::from("分配含税金额不得小于分配不含税金额"));
        }

        Ok(Self {
            base: BaseModel::new(id.to_string()),
            cost_entry_id: data.cost_entry_id,
            sales_order_id: data.sales_order_id,
            sales_order_line_id: data.sales_order_line_id,
            allocated_gross_amount: data.allocated_gross_amount,
            allocated_net_amount: data.allocated_net_amount,
            rounding_residual_flag: data.rounding_residual_flag,
        })
    }

    /// 更新成本分配。
    ///
    /// 分配行过账后不可更新或删除（数据模型 §6.9 末尾），任何修改都被拒绝。
    ///
    /// # 参数
    /// * `update` - 更新数据（恒被拒绝）
    /// * `updated_by` - 更新执行人（恒被拒绝）
    ///
    /// # 返回
    /// 恒返回错误。
    pub fn update(&mut self, update: CostAllocationData, updated_by: impl Into<String>) -> Result<()> {
        let _ = (update, updated_by);
        Err(Error::from("正式事实过账后不可更新或删除"))
    }
}

/// 校验成本归属目标。
///
/// 规则（数据模型 §6.10）：非卡券成本必须落到销售单或销售明细。
///
/// # 参数
/// * `sales_order_id` - 销售单归属
/// * `sales_order_line_id` - 销售明细归属
///
/// # 返回
/// 归属成立返回 `Ok(())`。
///
/// # 错误
/// 销售单归属缺失时返回错误。
fn validate_target(
    sales_order_id: &Option<SalesOrderId>,
    sales_order_line_id: &Option<SalesOrderLineId>,
) -> Result<()> {
    match (sales_order_id.is_some(), sales_order_line_id.is_some()) {
        (true, _) => Ok(()),
        (false, _) => Err(Error::from("成本分配必须落到销售单")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::money::Amount;
    use std::str::FromStr;

    fn data() -> CostAllocationData {
        CostAllocationData {
            cost_entry_id: CostEntryId::new("ce-1"),
            sales_order_id: Some(SalesOrderId::new("so-1")),
            sales_order_line_id: Some(SalesOrderLineId::new("so-1-l1")),
            allocated_gross_amount: Amount::from_str("113.00").unwrap(),
            allocated_net_amount: Amount::from_str("100.00").unwrap(),
            rounding_residual_flag: false,
        }
    }

    #[test]
    fn new_accepts_sales_order_target_and_keeps_amounts() {
        let allocation = CostAllocation::new(CostAllocationId::new("ca-1"), data()).unwrap();
        assert_eq!(allocation.sales_order_id, Some(SalesOrderId::new("so-1")));
        assert_eq!(
            allocation.allocated_gross_amount,
            Amount::from_str("113.00").unwrap()
        );
        assert!(!allocation.rounding_residual_flag);
    }

    #[test]
    fn new_rejects_missing_target() {
        let missing = CostAllocationData {
            sales_order_id: None,
            ..data()
        };
        assert!(CostAllocation::new(CostAllocationId::new("ca-3"), missing).is_err());

        let dangling_line = CostAllocationData {
            sales_order_id: None,
            ..data()
        };
        assert!(CostAllocation::new(CostAllocationId::new("ca-5"), dangling_line).is_err());
    }

    #[test]
    fn new_rejects_non_positive_and_net_over_gross() {
        let non_positive = CostAllocationData {
            allocated_gross_amount: Amount::from_str("0.00").unwrap(),
            ..data()
        };
        assert!(CostAllocation::new(CostAllocationId::new("ca-6"), non_positive).is_err());

        let net_over_gross = CostAllocationData {
            allocated_net_amount: Amount::from_str("114.00").unwrap(),
            ..data()
        };
        assert!(CostAllocation::new(CostAllocationId::new("ca-7"), net_over_gross).is_err());
    }

    #[test]
    fn new_rejects_negative_amounts() {
        // 负含税金额走 is_sign_negative 分支，错误文案与零金额一致。
        let negative_gross = CostAllocationData {
            allocated_gross_amount: Amount::from_str("-1.00").unwrap(),
            ..data()
        };
        let error = CostAllocation::new(CostAllocationId::new("ca-8"), negative_gross).unwrap_err();
        assert_eq!(error.to_string(), "分配金额必须为正数");

        // 负不含税金额同样拒绝。
        let negative_net = CostAllocationData {
            allocated_net_amount: Amount::from_str("-1.00").unwrap(),
            ..data()
        };
        let error = CostAllocation::new(CostAllocationId::new("ca-9"), negative_net).unwrap_err();
        assert_eq!(error.to_string(), "分配金额必须为正数");
    }

    #[test]
    fn formal_fact_rejects_update() {
        let mut allocation = CostAllocation::new(CostAllocationId::new("ca-1"), data()).unwrap();
        assert!(allocation.update(data(), "admin-2").is_err());
    }
}
