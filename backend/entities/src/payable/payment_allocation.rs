//! `payment_allocation` 付款核销分配（数据模型 §6.9，与 `receipt_allocation` 同构）。

use entity_core::BaseModel;
use entity_macros::Entity;
use serde::{Deserialize, Serialize};

use crate::common::time::Instant;
use crate::errors::{Error, Result};
use crate::ids::{PayableEntryId, PaymentAllocationId, SupplierPaymentId};
use crate::money::Amount;

/// 分配动作（数据模型 §6.9：`APPLY` 或 `REVERSE`）。
///
/// 全部金额存正数，方向只由动作表达；`REVERSE` 必须引用原 `APPLY` 分配。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AllocationAction {
    /// 正向核销分配。
    Apply,
    /// 反向冲减（引用原 `APPLY` 分配）。
    Reverse,
}

impl AllocationAction {
    /// 返回动作的中文展示名。
    ///
    /// # 返回
    /// 返回面向用户的中文标签。
    pub fn label(&self) -> &'static str {
        match self {
            Self::Apply => "核销",
            Self::Reverse => "冲减",
        }
    }

    /// 返回动作的稳定代码。
    ///
    /// # 返回
    /// 返回用于持久化与查询的稳定字符串。
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Apply => "apply",
            Self::Reverse => "reverse",
        }
    }
}

/// 付款核销分配创建数据。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PaymentAllocationData {
    /// 付款单。
    pub supplier_payment_id: SupplierPaymentId,
    /// 被核销应付分录。
    pub payable_entry_id: PayableEntryId,
    /// 付款单内追加序号（从 1 开始）。
    pub allocation_seq: u32,
    /// 分配动作。
    pub allocation_action: AllocationAction,
    /// 核销金额（正数）。
    pub allocated_amount: Amount,
    /// 核销发生时间。
    pub allocated_at: Instant,
    /// 反向分配引用的原 `APPLY`。
    pub reverses_allocation_id: Option<PaymentAllocationId>,
}

/// 付款核销分配实体（正式事实，数据模型 §6.9）。
///
/// `(supplier_payment_id, allocation_seq)` 唯一；`REVERSE` 累计不得超过原
/// `APPLY`、净分配不得超过付款金额和应付开放余额是跨行约束，由 P3 过账事务
/// 校验（§8.3）。分配行过账后不可更新或删除，冲减追加引用原行的 `REVERSE`。
#[derive(Debug, Serialize, Deserialize, Clone, Entity, PartialEq, Eq)]
pub struct PaymentAllocation {
    #[serde(flatten)]
    pub base: BaseModel,
    /// 付款单。
    pub supplier_payment_id: SupplierPaymentId,
    /// 被核销应付分录。
    pub payable_entry_id: PayableEntryId,
    /// 付款单内追加序号。
    pub allocation_seq: u32,
    /// 分配动作。
    pub allocation_action: AllocationAction,
    /// 核销金额。
    pub allocated_amount: Amount,
    /// 核销发生时间。
    pub allocated_at: Instant,
    /// 原 `APPLY` 分配。
    pub reverses_allocation_id: Option<PaymentAllocationId>,
}

impl PaymentAllocation {
    /// 创建付款核销分配。
    ///
    /// 完成金额正数、序号从 1 起与「动作 ↔ 原分配引用」一致性校验：
    /// `REVERSE` 必填 `reverses_allocation_id`，`APPLY` 不得携带。
    ///
    /// # 参数
    /// * `id` - 实体主键（`entities::ids::PaymentAllocationId`）
    /// * `data` - 创建数据
    ///
    /// # 返回
    /// 返回新建的分配实体。
    ///
    /// # 错误
    /// 当金额非正、序号为 0 或动作与引用不一致时返回错误。
    pub fn new(id: PaymentAllocationId, data: PaymentAllocationData) -> Result<Self> {
        if data.allocated_amount.to_decimal().is_sign_negative()
            || data.allocated_amount.to_decimal().is_zero()
        {
            return Err(Error::from("核销金额必须为正数"));
        }
        if data.allocation_seq == 0 {
            return Err(Error::from("分配序号必须从 1 开始"));
        }
        match data.allocation_action {
            AllocationAction::Apply if data.reverses_allocation_id.is_some() => {
                return Err(Error::from("APPLY 分配不得引用原分配"));
            }
            AllocationAction::Reverse if data.reverses_allocation_id.is_none() => {
                return Err(Error::from("REVERSE 分配必须引用原 APPLY 分配"));
            }
            _ => {}
        }

        Ok(Self {
            base: BaseModel::new(id.to_string()),
            supplier_payment_id: data.supplier_payment_id,
            payable_entry_id: data.payable_entry_id,
            allocation_seq: data.allocation_seq,
            allocation_action: data.allocation_action,
            allocated_amount: data.allocated_amount,
            allocated_at: data.allocated_at,
            reverses_allocation_id: data.reverses_allocation_id,
        })
    }

    /// 更新付款核销分配。
    ///
    /// 分配行过账后不可更新或删除（数据模型 §6.9 末尾），任何修改都被拒绝。
    ///
    /// # 参数
    /// * `update` - 更新数据（恒被拒绝）
    /// * `updated_by` - 更新执行人（恒被拒绝）
    ///
    /// # 返回
    /// 恒返回错误。
    pub fn update(&mut self, update: PaymentAllocationData, updated_by: impl Into<String>) -> Result<()> {
        let _ = (update, updated_by);
        Err(Error::from("正式事实过账后不可更新或删除"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::str::FromStr;

    fn data() -> PaymentAllocationData {
        PaymentAllocationData {
            supplier_payment_id: SupplierPaymentId::new("sp-1"),
            payable_entry_id: PayableEntryId::new("pe-1"),
            allocation_seq: 1,
            allocation_action: AllocationAction::Apply,
            allocated_amount: Amount::from_str("1000.00").unwrap(),
            allocated_at: Instant::from_unix_secs(1_700_000_000),
            reverses_allocation_id: None,
        }
    }

    #[test]
    fn new_accepts_valid_allocation() {
        let allocation = PaymentAllocation::new(PaymentAllocationId::new("pa-1"), data()).unwrap();
        assert_eq!(allocation.allocation_action, AllocationAction::Apply);
        assert_eq!(allocation.allocated_amount, Amount::from_str("1000.00").unwrap());
    }

    #[test]
    fn new_rejects_non_positive_amount_and_zero_seq() {
        let non_positive = PaymentAllocationData {
            allocated_amount: Amount::from_str("0.00").unwrap(),
            ..data()
        };
        assert!(PaymentAllocation::new(PaymentAllocationId::new("pa-2"), non_positive).is_err());

        let zero_seq = PaymentAllocationData {
            allocation_seq: 0,
            ..data()
        };
        assert!(PaymentAllocation::new(PaymentAllocationId::new("pa-3"), zero_seq).is_err());
    }

    #[test]
    fn new_enforces_action_reference_consistency() {
        let apply_with_reverse = PaymentAllocationData {
            reverses_allocation_id: Some(PaymentAllocationId::new("pa-1")),
            ..data()
        };
        assert!(PaymentAllocation::new(PaymentAllocationId::new("pa-4"), apply_with_reverse).is_err());

        let reverse_without_ref = PaymentAllocationData {
            allocation_action: AllocationAction::Reverse,
            reverses_allocation_id: None,
            ..data()
        };
        assert!(PaymentAllocation::new(PaymentAllocationId::new("pa-5"), reverse_without_ref).is_err());

        let reverse_valid = PaymentAllocationData {
            allocation_action: AllocationAction::Reverse,
            reverses_allocation_id: Some(PaymentAllocationId::new("pa-1")),
            ..data()
        };
        assert!(PaymentAllocation::new(PaymentAllocationId::new("pa-6"), reverse_valid).is_ok());
    }

    #[test]
    fn formal_fact_rejects_update() {
        let mut allocation = PaymentAllocation::new(PaymentAllocationId::new("pa-1"), data()).unwrap();
        assert!(allocation.update(data(), "admin-2").is_err());
    }
}
