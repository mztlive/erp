//! `receipt_allocation` 回款核销分配（数据模型 §6.8）。

use entity_core::BaseModel;
use entity_macros::Entity;
use serde::{Deserialize, Serialize};

use crate::common::time::Instant;
use crate::errors::{Error, Result};
use crate::ids::{CustomerReceiptId, ReceiptAllocationId, ReceivableEntryId};
use crate::money::Amount;

/// 分配动作（数据模型 §6.8：`APPLY` 或 `REVERSE`）。
///
/// 全部金额存正数，方向只由动作表达；`REVERSE` 必须引用原 `APPLY` 分配。
/// 本域内 `sales_invoice_allocation` 复用同一枚举（跨域不共享枚举，D19 各自定义，
/// 见 A-G7 报告「地基修订候选」）。
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

/// 回款核销分配创建数据。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ReceiptAllocationData {
    /// 回款单。
    pub customer_receipt_id: CustomerReceiptId,
    /// 被核销应收分录。
    pub receivable_entry_id: ReceivableEntryId,
    /// 回款单内追加序号（从 1 开始）。
    pub allocation_seq: u32,
    /// 分配动作。
    pub allocation_action: AllocationAction,
    /// 本次核销金额（正数）。
    pub allocated_amount: Amount,
    /// 核销时间。
    pub allocated_at: Instant,
    /// `REVERSE` 必填的原 `APPLY` 分配。
    pub reverses_allocation_id: Option<ReceiptAllocationId>,
}

/// 回款核销分配实体（正式事实，数据模型 §6.8）。
///
/// `(customer_receipt_id, allocation_seq)` 唯一；`REVERSE` 必须引用同一回款的
/// 有效 `APPLY` 且累计反向不超过原分配、净分配合计不得超过已过账回款金额是
/// 跨行约束，由 P3 过账事务校验（§8.3）。分配行过账后不可更新或删除，冲减
/// 追加引用原行的 `REVERSE`。
#[derive(Debug, Serialize, Deserialize, Clone, Entity, PartialEq, Eq)]
pub struct ReceiptAllocation {
    #[serde(flatten)]
    pub base: BaseModel,
    /// 回款单。
    pub customer_receipt_id: CustomerReceiptId,
    /// 被核销应收分录。
    pub receivable_entry_id: ReceivableEntryId,
    /// 回款单内追加序号。
    pub allocation_seq: u32,
    /// 分配动作。
    pub allocation_action: AllocationAction,
    /// 本次核销金额。
    pub allocated_amount: Amount,
    /// 核销时间。
    pub allocated_at: Instant,
    /// 原 `APPLY` 分配。
    pub reverses_allocation_id: Option<ReceiptAllocationId>,
}

impl ReceiptAllocation {
    /// 创建回款核销分配。
    ///
    /// 完成金额正数、序号从 1 起与「动作 ↔ 原分配引用」一致性校验：
    /// `REVERSE` 必填 `reverses_allocation_id`，`APPLY` 不得携带。
    ///
    /// # 参数
    /// * `id` - 实体主键（`entities::ids::ReceiptAllocationId`）
    /// * `data` - 创建数据
    ///
    /// # 返回
    /// 返回新建的分配实体。
    ///
    /// # 错误
    /// 当金额非正、序号为 0 或动作与引用不一致时返回错误。
    pub fn new(id: ReceiptAllocationId, data: ReceiptAllocationData) -> Result<Self> {
        if data.allocated_amount.to_decimal().is_sign_negative()
            || data.allocated_amount.to_decimal().is_zero()
        {
            return Err(Error::from("核销金额必须为正数"));
        }
        if data.allocation_seq == 0 {
            return Err(Error::from("分配序号必须从 1 开始"));
        }
        validate_action_reference(data.allocation_action, &data.reverses_allocation_id)?;

        Ok(Self {
            base: BaseModel::new(id.to_string()),
            customer_receipt_id: data.customer_receipt_id,
            receivable_entry_id: data.receivable_entry_id,
            allocation_seq: data.allocation_seq,
            allocation_action: data.allocation_action,
            allocated_amount: data.allocated_amount,
            allocated_at: data.allocated_at,
            reverses_allocation_id: data.reverses_allocation_id,
        })
    }

    /// 更新回款核销分配。
    ///
    /// 分配行过账后不可更新或删除（数据模型 §6.9 末尾），任何修改都被拒绝。
    ///
    /// # 参数
    /// * `update` - 更新数据（恒被拒绝）
    /// * `updated_by` - 更新执行人（恒被拒绝）
    ///
    /// # 返回
    /// 恒返回错误。
    pub fn update(&mut self, update: ReceiptAllocationData, updated_by: impl Into<String>) -> Result<()> {
        let _ = (update, updated_by);
        Err(Error::from("正式事实过账后不可更新或删除"))
    }
}

/// 分配引用 ID 的可泛化标记。
///
/// 本域内 `receipt_allocation` 与 `sales_invoice_allocation` 的反向引用 ID 类型
/// 不同，共享校验逻辑用空 trait 泛化。
pub(crate) trait AllocationIdRef {}

impl AllocationIdRef for ReceiptAllocationId {}
impl AllocationIdRef for crate::ids::SalesInvoiceAllocationId {}

/// 校验分配动作与反向引用的一致性。
///
/// # 参数
/// * `action` - 分配动作
/// * `reverses_allocation_id` - 原 `APPLY` 分配引用
///
/// # 返回
/// 一致返回 `Ok(())`。
///
/// # 错误
/// `REVERSE` 未携带引用或 `APPLY` 携带引用时返回错误。
pub(crate) fn validate_action_reference<T: AllocationIdRef>(
    action: AllocationAction,
    reverses_allocation_id: &Option<T>,
) -> Result<()> {
    match action {
        AllocationAction::Apply if reverses_allocation_id.is_some() => {
            Err(Error::from("APPLY 分配不得引用原分配"))
        }
        AllocationAction::Reverse if reverses_allocation_id.is_none() => {
            Err(Error::from("REVERSE 分配必须引用原 APPLY 分配"))
        }
        _ => Ok(()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::str::FromStr;

    fn data() -> ReceiptAllocationData {
        ReceiptAllocationData {
            customer_receipt_id: CustomerReceiptId::new("cr-1"),
            receivable_entry_id: ReceivableEntryId::new("re-1"),
            allocation_seq: 1,
            allocation_action: AllocationAction::Apply,
            allocated_amount: Amount::from_str("1000.00").unwrap(),
            allocated_at: Instant::from_unix_secs(1_700_000_000),
            reverses_allocation_id: None,
        }
    }

    #[test]
    fn new_accepts_valid_allocation() {
        let allocation = ReceiptAllocation::new(ReceiptAllocationId::new("rc-1"), data()).unwrap();
        assert_eq!(allocation.allocation_action, AllocationAction::Apply);
        assert_eq!(allocation.allocated_amount, Amount::from_str("1000.00").unwrap());
    }

    #[test]
    fn new_rejects_non_positive_amount_and_zero_seq() {
        let non_positive = ReceiptAllocationData {
            allocated_amount: Amount::from_str("0.00").unwrap(),
            ..data()
        };
        assert!(ReceiptAllocation::new(ReceiptAllocationId::new("rc-2"), non_positive).is_err());

        let zero_seq = ReceiptAllocationData {
            allocation_seq: 0,
            ..data()
        };
        assert!(ReceiptAllocation::new(ReceiptAllocationId::new("rc-3"), zero_seq).is_err());
    }

    #[test]
    fn new_enforces_action_reference_consistency() {
        let apply_with_reverse = ReceiptAllocationData {
            reverses_allocation_id: Some(ReceiptAllocationId::new("rc-1")),
            ..data()
        };
        assert!(ReceiptAllocation::new(ReceiptAllocationId::new("rc-4"), apply_with_reverse).is_err());

        let reverse_without_ref = ReceiptAllocationData {
            allocation_action: AllocationAction::Reverse,
            reverses_allocation_id: None,
            ..data()
        };
        assert!(ReceiptAllocation::new(ReceiptAllocationId::new("rc-5"), reverse_without_ref).is_err());

        let reverse_valid = ReceiptAllocationData {
            allocation_action: AllocationAction::Reverse,
            reverses_allocation_id: Some(ReceiptAllocationId::new("rc-1")),
            ..data()
        };
        assert!(ReceiptAllocation::new(ReceiptAllocationId::new("rc-6"), reverse_valid).is_ok());
    }

    #[test]
    fn formal_fact_rejects_update() {
        let mut allocation = ReceiptAllocation::new(ReceiptAllocationId::new("rc-1"), data()).unwrap();
        assert!(allocation.update(data(), "admin-2").is_err());
    }
}
