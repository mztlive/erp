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

/// W13 卡券票款登记的单行账户分配输入。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CardFundsRegistrationAllocationInput {
    /// 目标应收往来子账。
    pub target_account_id: crate::ids::ReceivableAccountId,
    /// 本行含税分配金额。
    pub amount: Amount,
}

/// W13 卡券票款登记分配集合的领域校验错误。
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum CardFundsRegistrationAllocationsError {
    /// 任一分配未指向当前任务账户，或账户 ID 不是规范形式。
    #[error("卡券票款登记只能分配到当前任务应收账户")]
    TargetAccountMismatch,
    /// 任一分配金额不是严格正数。
    #[error("分配金额必须大于零")]
    NonPositiveAmount,
    /// 全部分配之和不等于本次登记金额。
    #[error("票款分配合计必须等于本次登记金额")]
    TotalMismatch,
}

/// 已规范化且满足 W13 单账户与金额守恒不变量的分配集合。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CardFundsRegistrationAllocations {
    lines: Vec<CardFundsRegistrationAllocationInput>,
    total: Amount,
}

impl CardFundsRegistrationAllocations {
    /// 规范化并校验 W13 卡券票款登记分配集合。
    ///
    /// 所有目标账户必须是无首尾空白的同一任务账户，每行金额必须严格为正，
    /// 且行金额合计必须精确等于登记含税金额；不执行任何数据库读取。
    ///
    /// # 参数
    /// * `target_account_id` - 当前任务绑定的唯一应收子账
    /// * `expected_total` - 本次回款或发票登记的含税总额
    /// * `lines` - 从服务 DTO 转换后的账户分配输入
    ///
    /// # 返回
    /// 返回保存规范账户 ID、原行顺序和原登记金额表示的已验证值对象。
    ///
    /// # 错误
    /// 账户不一致或非规范时返回 `TargetAccountMismatch`；金额非正时返回
    /// `NonPositiveAmount`；合计不守恒时返回 `TotalMismatch`。
    ///
    /// # 约束
    /// 不去重或重排输入，不放宽带空白账户 ID 的拒绝行为，并在数值守恒后
    /// 保留 `expected_total` 的原始小数位表示，避免重算改变序列化结果。
    pub fn new(
        target_account_id: crate::ids::ReceivableAccountId,
        expected_total: Amount,
        lines: Vec<CardFundsRegistrationAllocationInput>,
    ) -> std::result::Result<Self, CardFundsRegistrationAllocationsError> {
        let target_account_id = canonical_registration_account_id(&target_account_id)
            .ok_or(CardFundsRegistrationAllocationsError::TargetAccountMismatch)?;
        let zero = expected_total.checked_sub(expected_total);
        let mut allocated_total = zero;
        let mut normalized_lines = Vec::with_capacity(lines.len());
        for line in lines {
            let line_account_id = canonical_registration_account_id(&line.target_account_id)
                .ok_or(CardFundsRegistrationAllocationsError::TargetAccountMismatch)?;
            if line_account_id != target_account_id {
                return Err(CardFundsRegistrationAllocationsError::TargetAccountMismatch);
            }
            if line.amount <= zero {
                return Err(CardFundsRegistrationAllocationsError::NonPositiveAmount);
            }
            allocated_total = allocated_total.checked_add(line.amount);
            normalized_lines.push(CardFundsRegistrationAllocationInput {
                target_account_id: line_account_id,
                amount: line.amount,
            });
        }
        if allocated_total != expected_total {
            return Err(CardFundsRegistrationAllocationsError::TotalMismatch);
        }
        Ok(Self {
            lines: normalized_lines,
            total: expected_total,
        })
    }

    /// 返回保持请求顺序的已验证分配行。
    ///
    /// # 参数
    /// 无。
    ///
    /// # 返回
    /// 返回由值对象持有的规范化分配行切片。
    ///
    /// # 错误
    /// 不返回错误。
    ///
    /// # 约束
    /// 调用方只能读取，不能绕过构造校验修改集合内容。
    pub fn as_slice(&self) -> &[CardFundsRegistrationAllocationInput] {
        &self.lines
    }

    /// 返回构造时验证通过的登记总额。
    ///
    /// # 参数
    /// 无。
    ///
    /// # 返回
    /// 返回与已验证分配数值相等、且保留调用方小数位表示的登记含税金额。
    ///
    /// # 错误
    /// 不返回错误。
    ///
    /// # 约束
    /// 该值直接保留构造参数 `expected_total`，不以分配求和结果替换其小数位表示。
    pub fn total(&self) -> Amount {
        self.total
    }
}

/// 将 W13 分配账户 ID 收窄为无首尾空白的规范值。
///
/// # 参数
/// * `account_id` - 待校验的透明账户 ID
///
/// # 返回
/// 非空且已处于 trim 后形式时返回重建的规范 ID，否则返回 `None`。
///
/// # 错误
/// 不返回错误。
///
/// # 约束
/// 不接受并静默修复带空白输入，以保持既有服务拒绝行为。
fn canonical_registration_account_id(
    account_id: &crate::ids::ReceivableAccountId,
) -> Option<crate::ids::ReceivableAccountId> {
    let normalized = account_id.as_ref().trim();
    (!normalized.is_empty() && normalized == account_id.as_ref())
        .then(|| crate::ids::ReceivableAccountId::new(normalized))
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

    /// 构建值对象测试使用的最小 W13 分配输入。
    ///
    /// 参数指定账户与金额字符串，返回可直接参与构造的输入；金额解析失败时
    /// 测试会 panic，且辅助函数不执行任何外部 I/O。
    fn registration_line(account_id: &str, amount: &str) -> CardFundsRegistrationAllocationInput {
        CardFundsRegistrationAllocationInput {
            target_account_id: crate::ids::ReceivableAccountId::new(account_id),
            amount: Amount::from_str(amount).unwrap(),
        }
    }

    /// 验证正常多行输入保持顺序、账户规范形式和精确金额守恒。
    ///
    /// 无参数且返回单元值；构造或断言失败时测试会 panic，并固定同一账户
    /// 多行合计必须精确等于登记总额的领域约束。
    #[test]
    fn card_funds_registration_allocations_accept_valid_lines() {
        let allocations = CardFundsRegistrationAllocations::new(
            crate::ids::ReceivableAccountId::new("ra-1"),
            Amount::from_str("100.00").unwrap(),
            vec![
                registration_line("ra-1", "40.00"),
                registration_line("ra-1", "60.00"),
            ],
        )
        .unwrap();

        assert_eq!(allocations.as_slice().len(), 2);
        assert_eq!(allocations.as_slice()[0].target_account_id.as_ref(), "ra-1");
        assert_eq!(allocations.total(), Amount::from_str("100.00").unwrap());
    }

    /// 验证混合小数位输入通过数值守恒后保留登记总额的原始表示。
    ///
    /// 测试固定登记额 `10` 与分配额 `10.00`，直接断言两者各自序列化尺度。
    #[test]
    fn card_funds_registration_allocations_preserve_expected_total_scale() {
        let allocations = CardFundsRegistrationAllocations::new(
            crate::ids::ReceivableAccountId::new("ra-1"),
            Amount::from_str("10").unwrap(),
            vec![registration_line("ra-1", "10.00")],
        )
        .unwrap();

        assert_eq!(allocations.total().to_string(), "10");
        assert_eq!(allocations.as_slice()[0].amount.to_string(), "10.00");
    }

    /// 验证跨账户和带空白账户 ID 都按既有单账户错误拒绝。
    ///
    /// 无参数且返回单元值；错误分类不匹配时测试会 panic，且测试明确约束
    /// 构造器不得通过 trim 放宽服务原有的账户匹配行为。
    #[test]
    fn card_funds_registration_allocations_reject_wrong_or_noncanonical_account() {
        for account_id in ["ra-2", " ra-1 "] {
            let error = CardFundsRegistrationAllocations::new(
                crate::ids::ReceivableAccountId::new("ra-1"),
                Amount::from_str("10.00").unwrap(),
                vec![registration_line(account_id, "10.00")],
            )
            .unwrap_err();
            assert_eq!(
                error,
                CardFundsRegistrationAllocationsError::TargetAccountMismatch
            );
        }
    }

    /// 验证零数、负数和不守恒合计分别触发稳定领域错误。
    ///
    /// 无参数且返回单元值；错误分类不匹配时测试会 panic，并覆盖严格正数
    /// 与精确总额两个失败约束而不依赖数据库。
    #[test]
    fn card_funds_registration_allocations_reject_amount_failures() {
        for amount in ["0.00", "-0.01"] {
            let error = CardFundsRegistrationAllocations::new(
                crate::ids::ReceivableAccountId::new("ra-1"),
                Amount::from_str("10.00").unwrap(),
                vec![registration_line("ra-1", amount)],
            )
            .unwrap_err();
            assert_eq!(error, CardFundsRegistrationAllocationsError::NonPositiveAmount);
        }

        let error = CardFundsRegistrationAllocations::new(
            crate::ids::ReceivableAccountId::new("ra-1"),
            Amount::from_str("10.00").unwrap(),
            vec![registration_line("ra-1", "9.99")],
        )
        .unwrap_err();
        assert_eq!(error, CardFundsRegistrationAllocationsError::TotalMismatch);
    }

    /// 验证最小分币金额作为严格正数边界可以完成守恒构造。
    ///
    /// 无参数且返回单元值；构造失败时测试会 panic，并固定 `0.01` 是 Amount
    /// 精度下可接受的最小正分配边界。
    #[test]
    fn card_funds_registration_allocations_accept_cent_boundary() {
        let allocations = CardFundsRegistrationAllocations::new(
            crate::ids::ReceivableAccountId::new("ra-1"),
            Amount::from_str("0.01").unwrap(),
            vec![registration_line("ra-1", "0.01")],
        )
        .unwrap();

        assert_eq!(allocations.total(), Amount::from_str("0.01").unwrap());
    }

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
