use crate::errors::{Error, Result};
use entities::ids::{PayableEntryId, PaymentAllocationId, ReceiptAllocationId, ReceivableEntryId};
use entities::money::Amount;
use entities::payable::{AllocationAction as PayableAllocationAction, PaymentAllocation};
use entities::receivable::{AllocationAction as ReceivableAllocationAction, ReceiptAllocation};
use std::str::FromStr;

/// 回款核销反向计划行。
pub(super) struct ReceiptReversePlanRow {
    /// 被反向分配引用的原 `APPLY` 分配。
    pub(super) original_id: ReceiptAllocationId,
    /// 反向金额。
    pub(super) amount: Amount,
    /// 被核销应收分录。
    pub(super) entry_id: ReceivableEntryId,
}

/// 回款核销冲减计划块（按原分配逐条分摊）。
pub(super) struct ReceiptReverseChunk {
    /// 被冲减的增加分录。
    pub(super) increase_entry_id: ReceivableEntryId,
    /// 冲减金额。
    pub(super) amount: Amount,
}

/// 按原回款核销分配规划反向核销（§8.3-3）。
///
/// 按分配序号顺序分摊退款/冲正金额：每笔原 `APPLY` 分配先扣除既有 `REVERSE`
/// 再分摊；任一时刻累计反向不得超过原有效分配；金额不足时返回错误。
///
/// # 参数
/// * `allocations` - 原回款核销分配（`APPLY` + `REVERSE`）
/// * `amount` - 本次反向金额
///
/// # 返回
/// 返回 `(反向分配行计划, 冲减块计划)`。
///
/// # 错误
/// 原有效分配不足以覆盖反向金额时返回 `BusinessLogicError`。
pub(super) fn plan_receipt_reverse(
    allocations: &[ReceiptAllocation],
    amount: Amount,
) -> Result<(Vec<ReceiptReversePlanRow>, Vec<ReceiptReverseChunk>)> {
    let mut remaining = amount;
    let mut rows = Vec::new();
    let mut chunks = Vec::new();
    for allocation in allocations {
        if allocation.allocation_action != ReceivableAllocationAction::Apply {
            continue;
        }
        let reversed: Amount = allocations
            .iter()
            .filter(|other| {
                other.allocation_action == ReceivableAllocationAction::Reverse
                    && other.reverses_allocation_id.as_ref() == Some(&allocation.base.id.clone().into())
            })
            .fold(zero_amount(), |sum, other| {
                sum.checked_add(other.allocated_amount)
            });
        if reversed >= allocation.allocated_amount {
            continue;
        }
        let effective = allocation.allocated_amount.checked_sub(reversed);
        let chunk = if effective >= remaining {
            remaining
        } else {
            effective
        };
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
        return Err(Error::BusinessLogicError(
            "原回款有效分配不足，无法全额反向".to_string(),
        ));
    }
    Ok((rows, chunks))
}

/// 付款核销反向计划行。
pub(super) struct PaymentReversePlanRow {
    /// 被反向分配引用的原 `APPLY` 分配。
    pub(super) original_id: PaymentAllocationId,
    /// 反向金额。
    pub(super) amount: Amount,
    /// 被核销应付分录。
    pub(super) entry_id: PayableEntryId,
}

/// 付款核销冲减计划块。
pub(super) struct PaymentReverseChunk {
    /// 被冲减的增加分录。
    pub(super) increase_entry_id: PayableEntryId,
    /// 冲减金额。
    pub(super) amount: Amount,
}

/// 按原付款核销分配规划反向核销（§8.3-3，应付侧镜像）。
///
/// # 参数
/// * `allocations` - 原付款核销分配（`APPLY` + `REVERSE`）
/// * `amount` - 本次反向金额
///
/// # 返回
/// 返回 `(反向分配行计划, 冲减块计划)`。
///
/// # 错误
/// 原有效分配不足以覆盖反向金额时返回 `BusinessLogicError`。
pub(super) fn plan_payment_reverse(
    allocations: &[PaymentAllocation],
    amount: Amount,
) -> Result<(Vec<PaymentReversePlanRow>, Vec<PaymentReverseChunk>)> {
    let mut remaining = amount;
    let mut rows = Vec::new();
    let mut chunks = Vec::new();
    for allocation in allocations {
        if allocation.allocation_action != PayableAllocationAction::Apply {
            continue;
        }
        let reversed: Amount = allocations
            .iter()
            .filter(|other| {
                other.allocation_action == PayableAllocationAction::Reverse
                    && other.reverses_allocation_id.as_ref() == Some(&allocation.base.id.clone().into())
            })
            .fold(zero_amount(), |sum, other| {
                sum.checked_add(other.allocated_amount)
            });
        if reversed >= allocation.allocated_amount {
            continue;
        }
        let effective = allocation.allocated_amount.checked_sub(reversed);
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
        return Err(Error::BusinessLogicError(
            "原付款有效分配不足，无法全额反向".to_string(),
        ));
    }
    Ok((rows, chunks))
}

/// 返回固定零金额。
///
/// # 返回
/// 返回金额 `0.00`。
pub(super) fn zero_amount() -> Amount {
    Amount::from_str("0.00").expect("固定零金额必须可解析")
}
