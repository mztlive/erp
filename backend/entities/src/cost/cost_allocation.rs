//! `cost_allocation` 成本分配（数据模型 §6.10）。

use entity_core::BaseModel;
use entity_macros::Entity;
use serde::{Deserialize, Serialize};

use crate::errors::{Error, Result};
use crate::ids::{
    CostAllocationId, CostEntryId, MallConsumptionEntryId, MallPaymentSourceId, SalesOrderId,
    SalesOrderLineId,
};
use crate::money::Amount;

/// 成本分配创建数据。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CostAllocationData {
    /// 成本事实。
    pub cost_entry_id: CostEntryId,
    /// 经营归属销售单（非卡券成本必填；与 `mall_consumption_entry_id` 二选一）。
    pub sales_order_id: Option<SalesOrderId>,
    /// 经营归属销售明细（可空）。
    pub sales_order_line_id: Option<SalesOrderLineId>,
    /// 二期消费成本归属（可空；与 `sales_order_id` 二选一）。
    pub mall_consumption_entry_id: Option<MallConsumptionEntryId>,
    /// 卡券或微信来源（可空）。
    pub mall_payment_source_id: Option<MallPaymentSourceId>,
    /// 分配含税金额。
    pub allocated_gross_amount: Amount,
    /// 分配不含税金额。
    pub allocated_net_amount: Amount,
    /// 是否承担最后一笔尾差。
    pub rounding_residual_flag: bool,
}

/// 成本分配实体（正式事实，数据模型 §6.10）。
///
/// 归属目标在销售单与二期消费归集之间二选一；分配含税金额不得小于不含税金额
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
    /// 二期消费成本归属。
    pub mall_consumption_entry_id: Option<MallConsumptionEntryId>,
    /// 卡券或微信来源。
    pub mall_payment_source_id: Option<MallPaymentSourceId>,
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
    /// 完成归属目标互斥校验（销售单与二期消费归集必须且只能选一）、金额正数与
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
    /// 当归属目标缺失或重复、金额非正或含税金额小于不含税金额时返回错误。
    pub fn new(id: CostAllocationId, data: CostAllocationData) -> Result<Self> {
        validate_target(
            &data.sales_order_id,
            &data.sales_order_line_id,
            &data.mall_consumption_entry_id,
        )?;
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
            mall_consumption_entry_id: data.mall_consumption_entry_id,
            mall_payment_source_id: data.mall_payment_source_id,
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

/// 校验成本归属目标互斥。
///
/// 规则（数据模型 §6.10）：非卡券成本必须落到销售单或销售明细，二期消费成本
/// 归属 `mall_consumption_entry_id`；两者必须且只能选一。
///
/// # 参数
/// * `sales_order_id` - 销售单归属
/// * `sales_order_line_id` - 销售明细归属
/// * `mall_consumption_entry_id` - 二期消费归属
///
/// # 返回
/// 互斥成立返回 `Ok(())`。
///
/// # 错误
/// 归属目标缺失或重复时返回错误。
fn validate_target(
    sales_order_id: &Option<SalesOrderId>,
    sales_order_line_id: &Option<SalesOrderLineId>,
    mall_consumption_entry_id: &Option<MallConsumptionEntryId>,
) -> Result<()> {
    match (sales_order_id.is_some(), mall_consumption_entry_id.is_some()) {
        (true, true) => Err(Error::from("销售单归属与二期消费归属不得同时设置")),
        (false, false) => Err(Error::from("成本分配必须落到销售单或二期消费归集")),
        (false, true) if sales_order_line_id.is_some() => Err(Error::from("销售明细归属必须配合销售单归属")),
        _ => Ok(()),
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
            mall_consumption_entry_id: None,
            mall_payment_source_id: None,
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
    fn new_accepts_mall_consumption_target() {
        let mall = CostAllocationData {
            sales_order_id: None,
            sales_order_line_id: None,
            mall_consumption_entry_id: Some(MallConsumptionEntryId::new("mce-1")),
            mall_payment_source_id: Some(MallPaymentSourceId::new("mps-1")),
            ..data()
        };
        let allocation = CostAllocation::new(CostAllocationId::new("ca-2"), mall).unwrap();
        assert!(allocation.sales_order_id.is_none());
        assert_eq!(
            allocation.mall_consumption_entry_id,
            Some(MallConsumptionEntryId::new("mce-1"))
        );
    }

    #[test]
    fn new_rejects_missing_duplicate_and_dangling_targets() {
        let missing = CostAllocationData {
            sales_order_id: None,
            mall_consumption_entry_id: None,
            ..data()
        };
        assert!(CostAllocation::new(CostAllocationId::new("ca-3"), missing).is_err());

        let duplicated = CostAllocationData {
            mall_consumption_entry_id: Some(MallConsumptionEntryId::new("mce-1")),
            ..data()
        };
        assert!(CostAllocation::new(CostAllocationId::new("ca-4"), duplicated).is_err());

        let dangling_line = CostAllocationData {
            sales_order_id: None,
            mall_consumption_entry_id: None,
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
    fn formal_fact_rejects_update() {
        let mut allocation = CostAllocation::new(CostAllocationId::new("ca-1"), data()).unwrap();
        assert!(allocation.update(data(), "admin-2").is_err());
    }
}
