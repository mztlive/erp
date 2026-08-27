//! `supplier_payment` 供应商付款事实（数据模型 §6.9、无独立审批的付款执行合同）。

use entity_core::BaseModel;
use entity_macros::Entity;
use serde::{Deserialize, Serialize};

use crate::common::state::{ensure_transition, DocumentState};
use crate::common::time::Instant;
use crate::errors::{Error, Result};
use crate::ids::{FileAssetId, PartyBankAccountId, PayableEntryId, SupplierAccountId, SupplierPaymentId};
use crate::money::Amount;
use crate::validation::{normalize_optional_text, normalize_required_text};

/// 付款单号最大长度。
const PAYMENT_NO_MAX_LEN: usize = 64;
/// 银行流水号最大长度。
const BANK_REFERENCE_MAX_LEN: usize = 256;

/// 付款单状态。
///
/// 付款由开放付款执行任务直接形成 `POSTED`，不得进入审批状态。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SupplierPaymentStatus {
    /// 草稿。
    Draft,
    /// 已过账。
    Posted,
    /// 已冲正（存在正式反向事实，原事实不删除）。
    Reversed,
}

impl SupplierPaymentStatus {
    /// 返回状态的中文展示名。
    ///
    /// # 返回
    /// 返回面向用户的中文标签。
    pub fn label(&self) -> &'static str {
        match self {
            Self::Draft => "草稿",
            Self::Posted => "已过账",
            Self::Reversed => "已冲正",
        }
    }

    /// 返回状态的稳定代码。
    ///
    /// # 返回
    /// 返回用于持久化与查询的稳定字符串。
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Draft => "draft",
            Self::Posted => "posted",
            Self::Reversed => "reversed",
        }
    }
}

impl DocumentState for SupplierPaymentStatus {
    /// 返回全部合法后继状态。
    ///
    /// 正常付款从草稿直接过账；`REVERSED` 是终态。
    fn allowed_next(self) -> &'static [Self] {
        match self {
            Self::Draft => &[Self::Posted],
            Self::Posted => &[Self::Reversed],
            Self::Reversed => &[],
        }
    }
}

/// 付款原子过账前的强类型核销分配行。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PendingPaymentAllocation {
    /// 被核销应付分录。
    pub payable_entry_id: PayableEntryId,
    /// 本次核销金额（正数）。
    pub allocated_amount: Amount,
}

impl PendingPaymentAllocation {
    /// 构造待过账核销行。
    ///
    /// # 参数
    /// * `payable_entry_id` - 被核销应付分录
    /// * `allocated_amount` - 核销金额
    ///
    /// # 错误
    /// 金额非正时返回错误。
    pub fn new(payable_entry_id: PayableEntryId, allocated_amount: Amount) -> Result<Self> {
        ensure_positive_amount(&allocated_amount)?;
        Ok(Self {
            payable_entry_id,
            allocated_amount,
        })
    }
}

/// 供应商付款单创建数据。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SupplierPaymentData {
    /// 付款单号（唯一）。
    pub payment_no: String,
    /// 收款供应商。
    pub supplier_id: SupplierAccountId,
    /// 本次付款冻结的供应商收款银行账户。
    pub payee_bank_account_id: PartyBankAccountId,
    /// 实际付款时间。
    pub paid_at: Instant,
    /// 含税付款金额。
    pub amount: Amount,
    /// 银行流水号。
    pub bank_reference: Option<String>,
    /// 银行回单图片资产。
    pub bank_receipt_asset_id: FileAssetId,
}

/// 供应商付款单实体（正式事实，数据模型 §6.9）。
///
/// `payment_no` 唯一；付款与应付供应商必须相同、净分配不得超过付款金额和应付
/// 开放余额是跨实体约束，由 P3 分配事务校验（§8.3）。状态机见合同 §4.4：
/// `POSTED` 后内容不可编辑，`REVERSED` 由 `payment_reversal` 过账形成；
/// 错付款使用 `payment_reversal` 或 `supplier_refund`，不删除原付款。
#[derive(Debug, Serialize, Deserialize, Clone, Entity, PartialEq, Eq)]
pub struct SupplierPayment {
    #[serde(flatten)]
    pub base: BaseModel,
    /// 付款单状态。
    pub status: SupplierPaymentStatus,
    /// 付款单号。
    pub payment_no: String,
    /// 收款供应商。
    pub supplier_id: SupplierAccountId,
    /// 本次付款冻结的供应商收款银行账户；异常旧数据可能为空。
    #[serde(default)]
    pub payee_bank_account_id: Option<PartyBankAccountId>,
    /// 实际付款时间。
    pub paid_at: Instant,
    /// 含税付款金额。
    pub amount: Amount,
    /// 银行流水号。
    pub bank_reference: Option<String>,
    /// 银行回单图片资产；异常旧数据可能为空，新付款必须提供。
    #[serde(default)]
    pub bank_receipt_asset_id: Option<FileAssetId>,
}

impl SupplierPayment {
    /// 创建供应商付款单（初始状态为草稿）。
    ///
    /// 完成付款单号与银行流水号的 trim/非空/长度校验和金额正数校验。
    ///
    /// # 参数
    /// * `id` - 实体主键（`entities::ids::SupplierPaymentId`）
    /// * `data` - 创建数据
    ///
    /// # 返回
    /// 返回新建的付款单实体。
    ///
    /// # 错误
    /// 当付款单号为空/超长、银行流水号超长或金额非正时返回错误。
    pub fn new(id: SupplierPaymentId, data: SupplierPaymentData) -> Result<Self> {
        let payment_no = normalize_required_text(
            data.payment_no,
            "付款单号不能为空",
            PAYMENT_NO_MAX_LEN,
            "付款单号过长",
        )?;
        let bank_reference =
            normalize_optional_text(data.bank_reference, "银行流水号", BANK_REFERENCE_MAX_LEN)?;
        ensure_positive_amount(&data.amount)?;

        Ok(Self {
            base: BaseModel::new(id.to_string()),
            status: SupplierPaymentStatus::Draft,
            payment_no,
            supplier_id: data.supplier_id,
            payee_bank_account_id: Some(data.payee_bank_account_id),
            paid_at: data.paid_at,
            amount: data.amount,
            bank_reference,
            bank_receipt_asset_id: Some(data.bank_receipt_asset_id),
        })
    }

    /// 返回付款执行所需的银行回单图片资产。
    ///
    /// # 错误
    /// 数据缺少银行回单时返回错误。
    pub fn require_bank_receipt(&self) -> Result<&FileAssetId> {
        self.bank_receipt_asset_id
            .as_ref()
            .ok_or_else(|| Error::from("请先上传银行回单图片"))
    }

    /// 迁移付款单状态。
    ///
    /// 按合同 §4.4 固定邻接矩阵校验并应用状态迁移。
    ///
    /// # 参数
    /// * `to` - 目标状态
    ///
    /// # 返回
    /// 迁移成功返回 `Ok(())`。
    ///
    /// # 错误
    /// 目标状态不在邻接矩阵中时返回 [`Error::InvalidStateTransition`]。
    pub fn transition(&mut self, to: SupplierPaymentStatus) -> Result<()> {
        ensure_transition(self.status, to)?;
        self.status = to;
        Ok(())
    }

    /// 由付款执行任务直接过账。
    ///
    /// # 参数
    /// * `allocations` - 本次原子写入的付款核销分配
    ///
    /// # 错误
    /// 非草稿或分配非法时返回冲突。
    pub fn post_from_execution(&mut self, allocations: &[PendingPaymentAllocation]) -> Result<()> {
        if self.status != SupplierPaymentStatus::Draft {
            return Err(Error::from("只有草稿状态的供应商付款单可以由付款任务过账"));
        }
        ensure_execution_allocations(&self.amount, allocations)?;
        self.transition(SupplierPaymentStatus::Posted)
    }

    /// 判断付款单是否已过账。
    ///
    /// # 返回
    /// 状态为 `Posted` 时返回 `true`。
    pub fn is_posted(&self) -> bool {
        self.status == SupplierPaymentStatus::Posted
    }
}

/// 校验金额为正数。
///
/// # 错误
/// 零或负数时返回错误。
fn ensure_positive_amount(amount: &Amount) -> Result<()> {
    if amount.to_decimal().is_sign_negative() || amount.to_decimal().is_zero() {
        return Err(Error::from("付款金额必须为正数"));
    }
    Ok(())
}

/// 校验付款任务核销分配合计等于实际付款金额。
///
/// # 错误
/// 无分配行或分配合计不等于付款金额时返回错误。
fn ensure_execution_allocations(amount: &Amount, allocations: &[PendingPaymentAllocation]) -> Result<()> {
    if allocations.is_empty() {
        return Err(Error::from("登记付款至少提供一条核销分配"));
    }
    let mut total = allocations[0].allocated_amount.to_decimal();
    for line in &allocations[1..] {
        total += line.allocated_amount.to_decimal();
    }
    if total != amount.to_decimal() {
        return Err(Error::from("核销分配合计必须等于付款金额"));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::str::FromStr;

    fn data() -> SupplierPaymentData {
        SupplierPaymentData {
            payment_no: " SP-2026-001 ".to_string(),
            supplier_id: SupplierAccountId::new("sup-1"),
            payee_bank_account_id: PartyBankAccountId::new("bank-1"),
            paid_at: Instant::from_unix_secs(1_700_000_000),
            amount: Amount::from_str("1000.00").unwrap(),
            bank_reference: Some(" BANK-1 ".to_string()),
            bank_receipt_asset_id: FileAssetId::new("asset-receipt-1"),
        }
    }

    #[test]
    fn new_trims_text_fields_and_starts_as_draft() {
        let payment = SupplierPayment::new(SupplierPaymentId::new("sp-1"), data()).unwrap();

        assert_eq!(payment.payment_no, "SP-2026-001");
        assert_eq!(payment.bank_reference.as_deref(), Some("BANK-1"));
        assert_eq!(
            payment.bank_receipt_asset_id.as_ref().map(AsRef::as_ref),
            Some("asset-receipt-1")
        );
        assert_eq!(payment.status, SupplierPaymentStatus::Draft);
        assert_eq!(
            payment.payee_bank_account_id.as_ref().map(AsRef::as_ref),
            Some("bank-1")
        );
        assert!(!payment.is_posted());
    }

    #[test]
    fn legacy_payment_without_bank_receipt_stays_readable_but_cannot_submit() {
        let payment = SupplierPayment::new(SupplierPaymentId::new("sp-legacy"), data()).unwrap();
        let mut value = serde_json::to_value(payment).unwrap();
        value.as_object_mut().unwrap().remove("bank_receipt_asset_id");

        let legacy: SupplierPayment = serde_json::from_value(value).unwrap();
        assert!(legacy.bank_receipt_asset_id.is_none());
        assert!(legacy.require_bank_receipt().is_err());
    }

    #[test]
    fn new_rejects_blank_no_overlong_reference_and_non_positive() {
        let blank_no = SupplierPaymentData {
            payment_no: "   ".to_string(),
            ..data()
        };
        assert!(SupplierPayment::new(SupplierPaymentId::new("sp-2"), blank_no).is_err());

        let overlong = SupplierPaymentData {
            bank_reference: Some("b".repeat(257)),
            ..data()
        };
        assert!(SupplierPayment::new(SupplierPaymentId::new("sp-3"), overlong).is_err());

        let non_positive = SupplierPaymentData {
            amount: Amount::from_str("0.00").unwrap(),
            ..data()
        };
        assert!(SupplierPayment::new(SupplierPaymentId::new("sp-4"), non_positive).is_err());
    }

    #[test]
    fn payment_is_posted_once_from_execution() {
        let mut payment = SupplierPayment::new(SupplierPaymentId::new("sp-1"), data()).unwrap();
        payment
            .post_from_execution(&[PendingPaymentAllocation::new(
                PayableEntryId::new("pe-1"),
                Amount::from_str("1000.00").unwrap(),
            )
            .unwrap()])
            .unwrap();
        assert!(payment.is_posted());
        assert!(payment
            .post_from_execution(&[PendingPaymentAllocation::new(
                PayableEntryId::new("pe-1"),
                Amount::from_str("1000.00").unwrap(),
            )
            .unwrap()])
            .is_err());
    }

    #[test]
    fn state_machine_edges_are_directed() {
        use crate::common::state::ensure_transition as tr;
        use SupplierPaymentStatus as S;

        assert!(tr(S::Draft, S::Posted).is_ok());
        assert!(tr(S::Posted, S::Reversed).is_ok());
        assert!(tr(S::Reversed, S::Reversed).is_ok(), "幂等迁移恒合法");

        assert!(tr(S::Draft, S::Reversed).is_err());
        assert!(tr(S::Posted, S::Draft).is_err());
        assert!(tr(S::Reversed, S::Posted).is_err());
        assert!(tr(S::Reversed, S::Draft).is_err());

        let mut payment = SupplierPayment::new(SupplierPaymentId::new("sp-1"), data()).unwrap();
        assert!(payment.transition(S::Reversed).is_err(), "实体迁移拒绝跨级");
    }

    #[test]
    fn direct_post_requires_allocations_and_cannot_repeat() {
        let mut payment = SupplierPayment::new(SupplierPaymentId::new("sp-1"), data()).unwrap();
        let allocations = [PendingPaymentAllocation::new(
            PayableEntryId::new("pe-1"),
            Amount::from_str("1000.00").unwrap(),
        )
        .unwrap()];
        payment.post_from_execution(&allocations).unwrap();
        assert_eq!(payment.status, SupplierPaymentStatus::Posted);
        assert!(payment.post_from_execution(&allocations).is_err());
    }

    #[test]
    fn direct_post_rejects_empty_under_or_over_allocated_lines() {
        let mut payment = SupplierPayment::new(SupplierPaymentId::new("sp-1"), data()).unwrap();
        assert!(payment.post_from_execution(&[]).is_err());
        assert!(payment
            .post_from_execution(&[PendingPaymentAllocation::new(
                PayableEntryId::new("pe-1"),
                Amount::from_str("999.99").unwrap(),
            )
            .unwrap()])
            .is_err());
        assert!(payment
            .post_from_execution(&[PendingPaymentAllocation::new(
                PayableEntryId::new("pe-1"),
                Amount::from_str("1000.01").unwrap(),
            )
            .unwrap()])
            .is_err());
    }

    #[test]
    fn rejected_business_status_is_unreachable() {
        let production = include_str!("supplier_payment.rs")
            .split("#[cfg(test)]")
            .next()
            .expect("生产代码");
        assert!(!production.contains("PendingReview"));
        assert!(!production.contains("fn reject"));
        assert!(!production.contains("SupplierPaymentStatus::Rejected"));
    }

    #[test]
    fn status_serializes_with_stable_codes_and_labels() {
        assert_eq!(SupplierPaymentStatus::Posted.label(), "已过账");
        assert_eq!(SupplierPaymentStatus::Reversed.as_str(), "reversed");
        assert_eq!(SupplierPaymentStatus::Draft.as_str(), "draft");
    }
}
