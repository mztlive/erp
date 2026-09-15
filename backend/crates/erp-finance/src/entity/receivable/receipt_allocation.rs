//! `receipt_allocation` 回款核销分配（数据模型 §6.8）。

use std::str::FromStr;

use entity_core::BaseModel;
use entity_macros::Entity;
use erp_core::common::time::Instant;
use erp_core::ids::{CustomerReceiptId, ReceiptAllocationId, ReceivableEntryId};
use erp_core::money::Amount;
use erp_core::{Error, Result};
use serde::{Deserialize, Serialize};

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
    /// * `id` - 实体主键（`erp_core::ids::ReceiptAllocationId`）
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
impl AllocationIdRef for erp_core::ids::SalesInvoiceAllocationId {}

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
        },
        AllocationAction::Reverse if reverses_allocation_id.is_none() => {
            Err(Error::from("REVERSE 分配必须引用原 APPLY 分配"))
        },
        _ => Ok(()),
    }
}

/// 回款核销反向计划行（SALES-E16）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReceiptReversePlanRow {
    /// 被反向的原 `APPLY` 分配 ID。
    pub original_id: ReceiptAllocationId,
    /// 本次反向金额。
    pub amount: Amount,
    /// 被核销应收分录。
    pub entry_id: ReceivableEntryId,
}

/// 回款核销冲减计划块（按原分配逐条分摊，SALES-E16）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReceiptReverseChunk {
    /// 被冲减的增加分录。
    pub increase_entry_id: ReceivableEntryId,
    /// 冲减金额。
    pub amount: Amount,
}

impl ReceiptAllocation {
    /// 计算下一可用分配序号（SALES-E17）。
    ///
    /// # 参数
    /// * `allocations` - 同一回款单已持久化分配集合
    ///
    /// # 返回
    /// 返回 `max(allocation_seq) + 1`，空集合返回 `1`。
    ///
    /// # 错误
    /// 当最大序号已为 `u32::MAX` 时返回错误，调用方不得通过 `wrapping_add` 静默溢出。
    ///
    /// # 约束
    /// 仅基于传入切片计算，不读取持久化或全局状态；并发唯一性仍由唯一索引或等价约束保证。
    pub fn next_allocation_seq(allocations: &[Self]) -> Result<u32> {
        let max = allocations.iter().map(|a| a.allocation_seq).max();
        match max {
            None => Ok(1),
            Some(value) if value == u32::MAX => Err(Error::from("回款核销分配序号已达上限")),
            Some(value) => Ok(value + 1),
        }
    }

    /// 为批量反向分配分配连续序号区间（SALES-E17）。
    ///
    /// # 参数
    /// * `allocations` - 同一回款单已持久化分配集合
    /// * `count` - 本次需新增的反向行数
    ///
    /// # 返回
    /// 返回长度为 `count` 的连续序号向量，起点为 `next_allocation_seq`。
    ///
    /// # 错误
    /// 当空集合且 `count > 0` 时从 1 起算；当 `count == 0` 返回空向量；当区间末值超过 `u32::MAX` 或 `count` 导致溢出时返回错误。
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
            .ok_or_else(|| Error::from("回款核销分配序号区间溢出"))?;
        Ok((start..=end).collect())
    }

    /// 按原回款核销分配规划反向核销（SALES-E16，§8.3-3）。
    ///
    /// 按 `allocation_seq` 确定性排序并预聚合既有 `REVERSE`，逐笔有效 `APPLY` 扣除已反向后再分摊；金额不足时拒绝。
    ///
    /// # 参数
    /// * `allocations` - 原回款核销分配（`APPLY` + `REVERSE`，任意顺序）
    /// * `amount` - 本次反向金额
    ///
    /// # 返回
    /// 返回 `(反向分配行计划, 冲减块计划)`，两者一一对应且按序号顺序排列。
    ///
    /// # 错误
    /// 当有效 `APPLY` 净额不足以覆盖 `amount` 时返回 `BusinessLogicError`。
    ///
    /// # 约束
    /// 纯内存计算，不依赖 MongoDB 自然顺序或外部 I/O；调用方负责在同一事务内持久化并通过 `revert_settlement` 的条件更新保证并发正确性。
    pub fn plan_reverse(
        allocations: &[Self],
        amount: Amount,
    ) -> Result<(Vec<ReceiptReversePlanRow>, Vec<ReceiptReverseChunk>)> {
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
            if allocation.allocation_action == AllocationAction::Reverse
                && let Some(original) = &allocation.reverses_allocation_id
            {
                let entry = reverse_sums.entry(original.to_string()).or_insert(zero);
                *entry = entry.checked_add(allocation.allocated_amount);
            }
        }
        let mut applies: Vec<&ReceiptAllocation> =
            allocations.iter().filter(|a| a.allocation_action == AllocationAction::Apply).collect();
        applies
            .sort_by(|a, b| a.allocation_seq.cmp(&b.allocation_seq).then_with(|| a.base.id.cmp(&b.base.id)));
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
            let chunk = if effective >= remaining { remaining } else { effective };
            if chunk.to_decimal().is_zero() {
                continue;
            }
            rows.push(ReceiptReversePlanRow {
                original_id: allocation.base.id.clone().into(),
                amount: chunk,
                entry_id: allocation.receivable_entry_id.clone(),
            });
            chunks.push(ReceiptReverseChunk {
                increase_entry_id: allocation.receivable_entry_id.clone(),
                amount: chunk,
            });
            remaining = remaining.checked_sub(chunk);
            if remaining.to_decimal().is_zero() {
                break;
            }
        }
        if !remaining.to_decimal().is_zero() {
            return Err(Error::from("原回款有效分配不足，无法全额反向"));
        }
        Ok((rows, chunks))
    }
}

#[cfg(test)]
mod tests {
    use std::str::FromStr;

    use super::*;

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
        let non_positive =
            ReceiptAllocationData { allocated_amount: Amount::from_str("0.00").unwrap(), ..data() };
        assert!(ReceiptAllocation::new(ReceiptAllocationId::new("rc-2"), non_positive).is_err());

        let zero_seq = ReceiptAllocationData { allocation_seq: 0, ..data() };
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

    fn receipt_allocation_for_plan(
        id: &str,
        seq: u32,
        action: AllocationAction,
        amount: &str,
        entry: &str,
        reverses: Option<&str>,
    ) -> ReceiptAllocation {
        ReceiptAllocation::new(
            ReceiptAllocationId::new(id),
            ReceiptAllocationData {
                customer_receipt_id: CustomerReceiptId::new("cr-1"),
                receivable_entry_id: ReceivableEntryId::new(entry),
                allocation_seq: seq,
                allocation_action: action,
                allocated_amount: Amount::from_str(amount).unwrap(),
                allocated_at: Instant::from_unix_secs(1_700_000_000),
                reverses_allocation_id: reverses.map(ReceiptAllocationId::new),
            },
        )
        .unwrap()
    }

    #[test]
    fn receipt_plan_sorts_by_seq_deterministically() {
        let a2 = receipt_allocation_for_plan("rc-2", 2, AllocationAction::Apply, "100.00", "re-2", None);
        let a1 = receipt_allocation_for_plan("rc-1", 1, AllocationAction::Apply, "100.00", "re-1", None);
        let allocations = vec![a2.clone(), a1.clone()];
        let (rows, chunks) =
            ReceiptAllocation::plan_reverse(&allocations, Amount::from_str("150.00").unwrap()).unwrap();
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0].original_id.to_string(), "rc-1");
        assert_eq!(rows[0].amount, Amount::from_str("100.00").unwrap());
        assert_eq!(rows[1].original_id.to_string(), "rc-2");
        assert_eq!(chunks[0].increase_entry_id.to_string(), "re-1");
    }

    #[test]
    fn receipt_plan_partial_and_full_reverse() {
        let a1 = receipt_allocation_for_plan("rc-1", 1, AllocationAction::Apply, "100.00", "re-1", None);
        let a2 = receipt_allocation_for_plan("rc-2", 2, AllocationAction::Apply, "50.00", "re-2", None);
        // partial 60 -> only first row partially
        let (rows, _) =
            ReceiptAllocation::plan_reverse(&[a1.clone(), a2.clone()], Amount::from_str("60.00").unwrap())
                .unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].amount, Amount::from_str("60.00").unwrap());
        // full 150 -> both rows
        let (rows, _) =
            ReceiptAllocation::plan_reverse(&[a1, a2], Amount::from_str("150.00").unwrap()).unwrap();
        assert_eq!(rows.len(), 2);
    }

    #[test]
    fn receipt_plan_deducts_multiple_reverses_and_insufficient_fails() {
        let a1 = receipt_allocation_for_plan("rc-1", 1, AllocationAction::Apply, "100.00", "re-1", None);
        let r1 =
            receipt_allocation_for_plan("rc-r1", 2, AllocationAction::Reverse, "30.00", "re-1", Some("rc-1"));
        let r2 =
            receipt_allocation_for_plan("rc-r2", 3, AllocationAction::Reverse, "20.00", "re-1", Some("rc-1"));
        let allocations = vec![a1.clone(), r1, r2];
        let (rows, _) =
            ReceiptAllocation::plan_reverse(&allocations, Amount::from_str("50.00").unwrap()).unwrap();
        assert_eq!(rows[0].amount, Amount::from_str("50.00").unwrap());
        let err =
            ReceiptAllocation::plan_reverse(&allocations, Amount::from_str("60.00").unwrap()).unwrap_err();
        assert!(err.to_string().contains("不足"));
    }

    #[test]
    fn receipt_plan_zero_amount_and_duplicate_seq() {
        let a1 = receipt_allocation_for_plan("rc-1", 1, AllocationAction::Apply, "100.00", "re-1", None);
        let a2 = receipt_allocation_for_plan("rc-2", 1, AllocationAction::Apply, "100.00", "re-2", None);
        let (rows, chunks) =
            ReceiptAllocation::plan_reverse(&[a1.clone(), a2.clone()], Amount::from_str("0.00").unwrap())
                .unwrap();
        assert!(rows.is_empty() && chunks.is_empty());
        // duplicate seq should be deterministic by id
        let (rows, _) =
            ReceiptAllocation::plan_reverse(&[a2.clone(), a1.clone()], Amount::from_str("100.00").unwrap())
                .unwrap();
        assert_eq!(rows[0].original_id.to_string(), "rc-1");
        assert_eq!(rows.len(), 1);
    }

    #[test]
    fn receipt_plan_insufficient_and_zero_effective_skipped() {
        let a1 = receipt_allocation_for_plan("rc-1", 1, AllocationAction::Apply, "50.00", "re-1", None);
        let r1 =
            receipt_allocation_for_plan("rc-r1", 2, AllocationAction::Reverse, "50.00", "re-1", Some("rc-1"));
        let a2 = receipt_allocation_for_plan("rc-2", 3, AllocationAction::Apply, "30.00", "re-2", None);
        // a1 already fully reversed, should skip
        let (rows, _) =
            ReceiptAllocation::plan_reverse(&[a1, r1, a2], Amount::from_str("30.00").unwrap()).unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].original_id.to_string(), "rc-2");
    }

    #[test]
    fn receipt_next_seq_empty_and_sorted_and_duplicate() {
        assert_eq!(ReceiptAllocation::next_allocation_seq(&[]).unwrap(), 1);
        let a1 = receipt_allocation_for_plan("rc-1", 5, AllocationAction::Apply, "10.00", "re-1", None);
        let a2 = receipt_allocation_for_plan("rc-2", 2, AllocationAction::Apply, "10.00", "re-2", None);
        let a3 = receipt_allocation_for_plan("rc-3", 5, AllocationAction::Apply, "10.00", "re-3", None);
        assert_eq!(ReceiptAllocation::next_allocation_seq(&[a1, a2, a3]).unwrap(), 6);
        let range = ReceiptAllocation::next_allocation_seq_range(
            &[receipt_allocation_for_plan("rc-1", 3, AllocationAction::Apply, "10.00", "re-1", None)],
            3,
        )
        .unwrap();
        assert_eq!(range, vec![4, 5, 6]);
        assert!(ReceiptAllocation::next_allocation_seq_range(&[], 0).unwrap().is_empty());
    }

    #[test]
    fn receipt_next_seq_u32_max_fails() {
        let max_alloc =
            receipt_allocation_for_plan("rc-max", u32::MAX, AllocationAction::Apply, "10.00", "re-1", None);
        assert!(ReceiptAllocation::next_allocation_seq(std::slice::from_ref(&max_alloc)).is_err());
        assert!(ReceiptAllocation::next_allocation_seq_range(std::slice::from_ref(&max_alloc), 1).is_err());
        let near_max = receipt_allocation_for_plan(
            "rc-near",
            u32::MAX - 1,
            AllocationAction::Apply,
            "10.00",
            "re-1",
            None,
        );
        assert!(ReceiptAllocation::next_allocation_seq_range(&[near_max], 3).is_err());
    }
}
