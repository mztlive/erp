//! `mall_item_funding_allocation`：商品明细 × 支付来源分摊矩阵（数据模型 §6.17）。
//!
//! 商城必须给出可守恒的商品 × 支付来源矩阵；ERP 不允许按订单总额推测分摊，
//! 不保留订单级比例兼容路径（§6.17）。本表只表达支付，不保存任何成本字段；
//! 成本金额、来源分摊和尾差只进入 `cost_entry + cost_allocation`（§6.17）。
//! 行合计（每个商品明细的来源分摊合计等于明细实付）与列合计（每个支付来源的
//! 商品分摊合计等于该来源支付金额）依赖聚合查询，由 P3 落实（P3 条目：§6.17
//! 分摊矩阵双向守恒）。本实体做单行不变式：分摊金额非负。

use entity_core::BaseModel;
use entity_macros::Entity;
use serde::{Deserialize, Serialize};

use crate::errors::{Error, Result};
use crate::ids::{MallItemFundingAllocationId, MallOrderItemId, MallPaymentSourceId};
use crate::money::Amount;

/// 分摊创建数据。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct MallItemFundingAllocationData {
    /// 商品明细。
    pub mall_order_item_id: MallOrderItemId,
    /// 支付来源。
    pub mall_payment_source_id: MallPaymentSourceId,
    /// 来源分摊到本商品的实付。
    pub allocated_payment_amount: Amount,
}

/// 商品 × 支付来源分摊实体（数据模型 §6.17）。
///
/// `(mall_order_item_id, mall_payment_source_id)` 唯一由 P2 唯一索引落实；
/// 行/列守恒由 P3 聚合校验。不可变，只提供 `new()`。
#[derive(Debug, Serialize, Deserialize, Clone, Entity, PartialEq, Eq)]
pub struct MallItemFundingAllocation {
    #[serde(flatten)]
    pub base: BaseModel,
    /// 商品明细。
    pub mall_order_item_id: MallOrderItemId,
    /// 支付来源。
    pub mall_payment_source_id: MallPaymentSourceId,
    /// 来源分摊到本商品的实付。
    pub allocated_payment_amount: Amount,
}

impl MallItemFundingAllocation {
    /// 创建分摊记录。
    ///
    /// 分摊金额必须非负（§6.17 支付矩阵不可出现负分摊）。
    ///
    /// # 参数
    /// * `id` - 实体主键（`entities::ids::MallItemFundingAllocationId`）
    /// * `data` - 创建数据
    ///
    /// # 返回
    /// 返回新建的分摊实体。
    ///
    /// # 错误
    /// 分摊金额为负时返回错误。
    pub fn new(id: MallItemFundingAllocationId, data: MallItemFundingAllocationData) -> Result<Self> {
        if data.allocated_payment_amount.to_decimal().is_sign_negative() {
            return Err(Error::from("分摊金额不能为负"));
        }

        Ok(Self {
            base: BaseModel::new(id.to_string()),
            mall_order_item_id: data.mall_order_item_id,
            mall_payment_source_id: data.mall_payment_source_id,
            allocated_payment_amount: data.allocated_payment_amount,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::{MallItemFundingAllocation, MallItemFundingAllocationData};
    use crate::ids::{MallItemFundingAllocationId, MallOrderItemId, MallPaymentSourceId};
    use crate::money::Amount;
    use std::str::FromStr;

    fn data() -> MallItemFundingAllocationData {
        MallItemFundingAllocationData {
            mall_order_item_id: MallOrderItemId::new("item-1"),
            mall_payment_source_id: MallPaymentSourceId::new("ps-1"),
            allocated_payment_amount: Amount::from_str("80.00").unwrap(),
        }
    }

    /// happy path：分摊金额按字典落库。
    #[test]
    fn new_keeps_allocation_amount() {
        let allocation =
            MallItemFundingAllocation::new(MallItemFundingAllocationId::new("ifa-1"), data()).unwrap();

        assert_eq!(allocation.mall_order_item_id, MallOrderItemId::new("item-1"));
        assert_eq!(
            allocation.mall_payment_source_id,
            MallPaymentSourceId::new("ps-1")
        );
        assert_eq!(
            allocation.allocated_payment_amount,
            Amount::from_str("80.00").unwrap()
        );
    }

    /// 失败路径 + 金额：负分摊金额拒绝；零分摊允许（矩阵单元格可为零）。
    #[test]
    fn new_rejects_negative_but_accepts_zero_allocation() {
        let negative = MallItemFundingAllocationData {
            allocated_payment_amount: Amount::from_str("-0.01").unwrap(),
            ..data()
        };
        assert!(MallItemFundingAllocation::new(MallItemFundingAllocationId::new("ifa-2"), negative).is_err());

        let zero = MallItemFundingAllocationData {
            allocated_payment_amount: Amount::from_str("0.00").unwrap(),
            ..data()
        };
        assert!(MallItemFundingAllocation::new(MallItemFundingAllocationId::new("ifa-3"), zero).is_ok());
    }
}
