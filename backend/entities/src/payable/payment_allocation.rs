//! `payment_allocation` 付款核销分配（数据模型 §6.9，与 `receipt_allocation` 同构）。

use entity_core::BaseModel;
use entity_macros::Entity;
use serde::{Deserialize, Serialize};

use std::str::FromStr;

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

/// 付款核销反向计划行（SALES-E16，应付侧镜像）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PaymentReversePlanRow {
    /// 被反向的原 `APPLY` 分配 ID。
    pub original_id: PaymentAllocationId,
    /// 本次反向金额。
    pub amount: Amount,
    /// 被核销应付分录。
    pub entry_id: PayableEntryId,
}

/// 付款核销冲减计划块（SALES-E16）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PaymentReverseChunk {
    /// 被冲减的增加分录。
    pub increase_entry_id: PayableEntryId,
    /// 冲减金额。
    pub amount: Amount,
}

impl PaymentAllocation {
    /// 计算下一可用分配序号（SALES-E17，应付侧镜像）。
    ///
    /// # 参数
    /// * `allocations` - 同一付款单已持久化分配集合
    ///
    /// # 返回
    /// 返回 `max(allocation_seq) + 1`，空集合返回 `1`。
    ///
    /// # 错误
    /// 当最大序号已为 `u32::MAX` 时返回错误。
    ///
    /// # 约束
    /// 仅基于传入切片计算，不读取持久化；并发唯一性仍由唯一索引保证。
    pub fn next_allocation_seq(allocations: &[Self]) -> Result<u32> {
        let max = allocations.iter().map(|a| a.allocation_seq).max();
        match max {
            None => Ok(1),
            Some(value) if value == u32::MAX => Err(Error::from("付款核销分配序号已达上限")),
            Some(value) => Ok(value + 1),
        }
    }

    /// 为批量反向分配分配连续序号区间（SALES-E17）。
    ///
    /// # 参数
    /// * `allocations` - 同一付款单已持久化分配集合
    /// * `count` - 本次需新增的反向行数
    ///
    /// # 返回
    /// 返回长度为 `count` 的连续序号向量，起点为 `next_allocation_seq`。
    ///
    /// # 错误
    /// 当 `count == 0` 返回空向量；当区间末值超过 `u32::MAX` 时返回错误。
    ///
    /// # 约束
    /// 不触及持久化或全局 ID 生成器；调用方需在同一事务内写入并依赖唯一索引检测并发冲突。
    pub fn next_allocation_seq_range(allocations: &[Self], count: usize) -> Result<Vec<u32>> {
        if count == 0 {
            return Ok(Vec::new());
        }
        let start = Self::next_allocation_seq(allocations)?;
        let end = start
            .checked_add((count as u32).saturating_sub(1))
            .ok_or_else(|| Error::from("付款核销分配序号区间溢出"))?;
        Ok((start..=end).collect())
    }

    /// 按原付款核销分配规划反向核销（SALES-E16，§8.3-3，应付侧镜像）。
    ///
    /// 按 `allocation_seq` 确定性排序并预聚合既有 `REVERSE`，逐笔有效 `APPLY` 扣除已反向后再分摊；金额不足时拒绝。
    ///
    /// # 参数
    /// * `allocations` - 原付款核销分配（任意顺序）
    /// * `amount` - 本次反向金额
    ///
    /// # 返回
    /// 返回 `(反向分配行计划, 冲减块计划)`，两者一一对应且按序号顺序排列。
    ///
    /// # 错误
    /// 当有效 `APPLY` 净额不足时返回 `BusinessLogicError`。
    ///
    /// # 约束
    /// 纯内存计算，不依赖 MongoDB 自然顺序或外部 I/O；调用方负责在同一事务内持久化并通过条件更新保证并发正确性。
    pub fn plan_reverse(
        allocations: &[Self],
        amount: Amount,
    ) -> Result<(Vec<PaymentReversePlanRow>, Vec<PaymentReverseChunk>)> {
        use std::collections::HashMap;

        if amount.to_decimal().is_zero() {
            return Ok((Vec::new(), Vec::new()));
        }
        if amount.to_decimal().is_sign_negative() {
            return Err(Error::from("反向金额必须为正数"));
        }
        let mut reverse_sums: HashMap<String, Amount> = HashMap::new();
        let zero = Amount::from_str("0.00").expect("固定零金额必须可解析");
        for allocation in allocations {
            if allocation.allocation_action == AllocationAction::Reverse {
                if let Some(original) = &allocation.reverses_allocation_id {
                    let entry = reverse_sums.entry(original.to_string()).or_insert(zero);
                    *entry = entry.checked_add(allocation.allocated_amount);
                }
            }
        }
        let mut applies: Vec<&PaymentAllocation> = allocations
            .iter()
            .filter(|a| a.allocation_action == AllocationAction::Apply)
            .collect();
        applies.sort_by(|a, b| {
            a.allocation_seq
                .cmp(&b.allocation_seq)
                .then_with(|| a.base.id.cmp(&b.base.id))
        });
        let mut remaining = amount;
        let mut rows = Vec::new();
        let mut chunks = Vec::new();
        for allocation in applies {
            let reversed = reverse_sums.get(&allocation.base.id).copied().unwrap_or(zero);
            if reversed >= allocation.allocated_amount {
                continue;
            }
            let effective = allocation.allocated_amount.checked_sub(reversed);
            if effective.to_decimal().is_zero() {
                continue;
            }
            let chunk = if effective >= remaining {
                remaining
            } else {
                effective
            };
            if chunk.to_decimal().is_zero() {
                continue;
            }
            rows.push(PaymentReversePlanRow {
                original_id: allocation.base.id.clone().into(),
                amount: chunk,
                entry_id: allocation.payable_entry_id.clone(),
            });
            chunks.push(PaymentReverseChunk {
                increase_entry_id: allocation.payable_entry_id.clone(),
                amount: chunk,
            });
            remaining = remaining.checked_sub(chunk);
            if remaining.to_decimal().is_zero() {
                break;
            }
        }
        if !remaining.to_decimal().is_zero() {
            return Err(Error::from("原付款有效分配不足，无法全额反向"));
        }
        Ok((rows, chunks))
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

    fn payment_allocation_for_plan(
        id: &str,
        seq: u32,
        action: AllocationAction,
        amount: &str,
        entry: &str,
        reverses: Option<&str>,
    ) -> PaymentAllocation {
        PaymentAllocation::new(
            PaymentAllocationId::new(id),
            PaymentAllocationData {
                supplier_payment_id: SupplierPaymentId::new("sp-1"),
                payable_entry_id: PayableEntryId::new(entry),
                allocation_seq: seq,
                allocation_action: action,
                allocated_amount: Amount::from_str(amount).unwrap(),
                allocated_at: Instant::from_unix_secs(1_700_000_000),
                reverses_allocation_id: reverses.map(PaymentAllocationId::new),
            },
        )
        .unwrap()
    }

    #[test]
    fn payment_plan_sorts_by_seq_deterministically() {
        let a2 = payment_allocation_for_plan("pa-2", 2, AllocationAction::Apply, "100.00", "pe-2", None);
        let a1 = payment_allocation_for_plan("pa-1", 1, AllocationAction::Apply, "100.00", "pe-1", None);
        let allocations = vec![a2.clone(), a1.clone()];
        let (rows, chunks) =
            PaymentAllocation::plan_reverse(&allocations, Amount::from_str("150.00").unwrap()).unwrap();
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0].original_id.to_string(), "pa-1");
        assert_eq!(rows[0].amount, Amount::from_str("100.00").unwrap());
        assert_eq!(chunks[0].increase_entry_id.to_string(), "pe-1");
    }

    #[test]
    fn payment_plan_partial_and_full_reverse() {
        let a1 = payment_allocation_for_plan("pa-1", 1, AllocationAction::Apply, "100.00", "pe-1", None);
        let a2 = payment_allocation_for_plan("pa-2", 2, AllocationAction::Apply, "50.00", "pe-2", None);
        let (rows, _) =
            PaymentAllocation::plan_reverse(&[a1.clone(), a2.clone()], Amount::from_str("60.00").unwrap())
                .unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].amount, Amount::from_str("60.00").unwrap());
        let (rows, _) =
            PaymentAllocation::plan_reverse(&[a1, a2], Amount::from_str("150.00").unwrap()).unwrap();
        assert_eq!(rows.len(), 2);
    }

    #[test]
    fn payment_plan_deducts_multiple_reverses_and_insufficient_fails() {
        let a1 = payment_allocation_for_plan("pa-1", 1, AllocationAction::Apply, "100.00", "pe-1", None);
        let r1 = payment_allocation_for_plan(
            "pa-r1",
            2,
            AllocationAction::Reverse,
            "30.00",
            "pe-1",
            Some("pa-1"),
        );
        let r2 = payment_allocation_for_plan(
            "pa-r2",
            3,
            AllocationAction::Reverse,
            "20.00",
            "pe-1",
            Some("pa-1"),
        );
        let allocations = vec![a1.clone(), r1, r2];
        let (rows, _) =
            PaymentAllocation::plan_reverse(&allocations, Amount::from_str("50.00").unwrap()).unwrap();
        assert_eq!(rows[0].amount, Amount::from_str("50.00").unwrap());
        let err =
            PaymentAllocation::plan_reverse(&allocations, Amount::from_str("60.00").unwrap()).unwrap_err();
        assert!(err.to_string().contains("不足"));
    }

    #[test]
    fn payment_plan_zero_amount_and_duplicate_seq() {
        let a1 = payment_allocation_for_plan("pa-1", 1, AllocationAction::Apply, "100.00", "pe-1", None);
        let a2 = payment_allocation_for_plan("pa-2", 1, AllocationAction::Apply, "100.00", "pe-2", None);
        let (rows, chunks) =
            PaymentAllocation::plan_reverse(&[a1.clone(), a2.clone()], Amount::from_str("0.00").unwrap())
                .unwrap();
        assert!(rows.is_empty() && chunks.is_empty());
        let (rows, _) =
            PaymentAllocation::plan_reverse(&[a2.clone(), a1.clone()], Amount::from_str("100.00").unwrap())
                .unwrap();
        assert_eq!(rows[0].original_id.to_string(), "pa-1");
        assert_eq!(rows.len(), 1);
    }

    #[test]
    fn payment_plan_insufficient_and_skip_fully_reversed() {
        let a1 = payment_allocation_for_plan("pa-1", 1, AllocationAction::Apply, "50.00", "pe-1", None);
        let r1 = payment_allocation_for_plan(
            "pa-r1",
            2,
            AllocationAction::Reverse,
            "50.00",
            "pe-1",
            Some("pa-1"),
        );
        let a2 = payment_allocation_for_plan("pa-2", 3, AllocationAction::Apply, "30.00", "pe-2", None);
        let (rows, _) =
            PaymentAllocation::plan_reverse(&[a1, r1, a2], Amount::from_str("30.00").unwrap()).unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].original_id.to_string(), "pa-2");
    }

    #[test]
    fn payment_next_seq_empty_and_sorted_and_duplicate() {
        assert_eq!(PaymentAllocation::next_allocation_seq(&[]).unwrap(), 1);
        let a1 = payment_allocation_for_plan("pa-1", 5, AllocationAction::Apply, "10.00", "pe-1", None);
        let a2 = payment_allocation_for_plan("pa-2", 2, AllocationAction::Apply, "10.00", "pe-2", None);
        let a3 = payment_allocation_for_plan("pa-3", 5, AllocationAction::Apply, "10.00", "pe-3", None);
        assert_eq!(PaymentAllocation::next_allocation_seq(&[a1, a2, a3]).unwrap(), 6);
        let range = PaymentAllocation::next_allocation_seq_range(
            &[payment_allocation_for_plan(
                "pa-1",
                3,
                AllocationAction::Apply,
                "10.00",
                "pe-1",
                None,
            )],
            3,
        )
        .unwrap();
        assert_eq!(range, vec![4, 5, 6]);
        assert!(PaymentAllocation::next_allocation_seq_range(&[], 0)
            .unwrap()
            .is_empty());
    }

    #[test]
    fn payment_next_seq_u32_max_fails() {
        let max_alloc =
            payment_allocation_for_plan("pa-max", u32::MAX, AllocationAction::Apply, "10.00", "pe-1", None);
        assert!(PaymentAllocation::next_allocation_seq(std::slice::from_ref(&max_alloc)).is_err());
        assert!(PaymentAllocation::next_allocation_seq_range(std::slice::from_ref(&max_alloc), 1).is_err());
        let near_max = payment_allocation_for_plan(
            "pa-near",
            u32::MAX - 1,
            AllocationAction::Apply,
            "10.00",
            "pe-1",
            None,
        );
        assert!(PaymentAllocation::next_allocation_seq_range(&[near_max], 3).is_err());
    }

    #[test]
    fn payment_and_receipt_mirror_consistency() {
        let r_a1 = crate::receivable::ReceiptAllocation::new(
            crate::ids::ReceiptAllocationId::new("rc-1"),
            crate::receivable::ReceiptAllocationData {
                customer_receipt_id: crate::ids::CustomerReceiptId::new("cr-1"),
                receivable_entry_id: crate::ids::ReceivableEntryId::new("re-1"),
                allocation_seq: 2,
                allocation_action: crate::receivable::AllocationAction::Apply,
                allocated_amount: Amount::from_str("100.00").unwrap(),
                allocated_at: Instant::from_unix_secs(1_700_000_000),
                reverses_allocation_id: None,
            },
        )
        .unwrap();
        let r_a2 = crate::receivable::ReceiptAllocation::new(
            crate::ids::ReceiptAllocationId::new("rc-2"),
            crate::receivable::ReceiptAllocationData {
                customer_receipt_id: crate::ids::CustomerReceiptId::new("cr-1"),
                receivable_entry_id: crate::ids::ReceivableEntryId::new("re-2"),
                allocation_seq: 1,
                allocation_action: crate::receivable::AllocationAction::Apply,
                allocated_amount: Amount::from_str("100.00").unwrap(),
                allocated_at: Instant::from_unix_secs(1_700_000_000),
                reverses_allocation_id: None,
            },
        )
        .unwrap();
        let p_a1 = payment_allocation_for_plan("pa-1", 2, AllocationAction::Apply, "100.00", "pe-1", None);
        let p_a2 = payment_allocation_for_plan("pa-2", 1, AllocationAction::Apply, "100.00", "pe-2", None);
        let (r_rows, _) = crate::receivable::ReceiptAllocation::plan_reverse(
            &[r_a1, r_a2],
            Amount::from_str("150.00").unwrap(),
        )
        .unwrap();
        let (p_rows, _) =
            PaymentAllocation::plan_reverse(&[p_a1, p_a2], Amount::from_str("150.00").unwrap()).unwrap();
        // Both must produce same count and amounts regardless of original order
        assert_eq!(r_rows.len(), p_rows.len());
        assert_eq!(r_rows[0].amount, p_rows[0].amount);
        assert_eq!(r_rows[1].amount, p_rows[1].amount);
    }
}
