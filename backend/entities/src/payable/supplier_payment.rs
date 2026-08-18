//! `supplier_payment` 供应商付款单（数据模型 §6.9、§7.5 资金单据状态机）。

use entity_core::BaseModel;
use entity_macros::Entity;
use serde::{Deserialize, Serialize};

use crate::common::state::{ensure_transition, DocumentState};
use crate::common::time::Instant;
use crate::errors::{Error, Result};
use crate::ids::{SupplierAccountId, SupplierPaymentId};
use crate::money::Amount;
use crate::validation::{normalize_optional_text, normalize_required_text};

/// 付款单号最大长度。
const PAYMENT_NO_MAX_LEN: usize = 64;
/// 银行凭证引用最大长度。
const BANK_REFERENCE_MAX_LEN: usize = 256;

/// 付款单状态（数据模型 §6.9：草稿、已过账、已冲正；§7.5 增加 `PENDING_REVIEW`）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SupplierPaymentStatus {
    /// 草稿。
    Draft,
    /// 待复核（财务复核适用时经过）。
    PendingReview,
    /// 已过账。
    Posted,
    /// 已冲正（存在正式反向事实，原事实不删除）。
    Reversed,
    /// 审批中。
    #[serde(rename = "IN_APPROVAL")]
    InApproval,
}

impl SupplierPaymentStatus {
    /// 返回状态的中文展示名。
    ///
    /// # 返回
    /// 返回面向用户的中文标签。
    pub fn label(&self) -> &'static str {
        match self {
            Self::Draft => "草稿",
            Self::PendingReview => "待复核",
            Self::Posted => "已过账",
            Self::Reversed => "已冲正",
            Self::InApproval => "审批中",
        }
    }

    /// 返回状态的稳定代码。
    ///
    /// # 返回
    /// 返回用于持久化与查询的稳定字符串。
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Draft => "draft",
            Self::PendingReview => "pending_review",
            Self::Posted => "posted",
            Self::Reversed => "reversed",
            Self::InApproval => "IN_APPROVAL",
        }
    }
}

impl DocumentState for SupplierPaymentStatus {
    /// 返回全部合法后继状态（数据模型 §7.5 资金单据状态机）。
    ///
    /// 付款不是财务纠错，`PENDING_REVIEW` 仅适用时经过，允许草稿直接过账；
    /// `REVERSED` 是终态。
    fn allowed_next(self) -> &'static [Self] {
        match self {
            Self::Draft => &[Self::PendingReview, Self::Posted, Self::InApproval],
            Self::InApproval => &[Self::Posted, Self::Draft],
            Self::PendingReview => &[Self::Posted],
            Self::Posted => &[Self::Reversed],
            Self::Reversed => &[],
        }
    }
}

/// 供应商付款单创建数据。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SupplierPaymentData {
    /// 付款单号（唯一）。
    pub payment_no: String,
    /// 收款供应商。
    pub supplier_id: SupplierAccountId,
    /// 实际付款时间。
    pub paid_at: Instant,
    /// 含税付款金额。
    pub amount: Amount,
    /// 付款凭证引用。
    pub bank_reference: Option<String>,
}

/// 供应商付款单更新数据（仅草稿可编辑）。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct SupplierPaymentUpdate {
    /// 付款时间；`None` 表示不修改。
    pub paid_at: Option<Instant>,
    /// 付款金额；`None` 表示不修改。
    pub amount: Option<Amount>,
    /// 付款凭证引用；`None` 表示不修改，`Some("")` 清除。
    pub bank_reference: Option<String>,
}

/// 供应商付款单实体（正式事实，数据模型 §6.9）。
///
/// `payment_no` 唯一；付款与应付供应商必须相同、净分配不得超过付款金额和应付
/// 开放余额是跨实体约束，由 P3 分配事务校验（§8.3）。状态机见 §7.5：
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
    /// 实际付款时间。
    pub paid_at: Instant,
    /// 含税付款金额。
    pub amount: Amount,
    /// 付款凭证引用。
    pub bank_reference: Option<String>,
    /// 审批提交版本，初值 0。
    #[serde(default)]
    pub approval_subject_version: u32,
}

impl SupplierPayment {
    /// 创建供应商付款单（初始状态为草稿）。
    ///
    /// 完成付款单号与凭证引用的 trim/非空/长度校验和金额正数校验。
    ///
    /// # 参数
    /// * `id` - 实体主键（`entities::ids::SupplierPaymentId`）
    /// * `data` - 创建数据
    ///
    /// # 返回
    /// 返回新建的付款单实体。
    ///
    /// # 错误
    /// 当付款单号为空/超长、凭证引用超长或金额非正时返回错误。
    pub fn new(id: SupplierPaymentId, data: SupplierPaymentData) -> Result<Self> {
        let payment_no = normalize_required_text(
            data.payment_no,
            "付款单号不能为空",
            PAYMENT_NO_MAX_LEN,
            "付款单号过长",
        )?;
        let bank_reference =
            normalize_optional_text(data.bank_reference, "付款凭证引用", BANK_REFERENCE_MAX_LEN)?;
        if data.amount.to_decimal().is_sign_negative() || data.amount.to_decimal().is_zero() {
            return Err(Error::from("付款金额必须为正数"));
        }

        Ok(Self {
            base: BaseModel::new(id.to_string()),
            status: SupplierPaymentStatus::Draft,
            payment_no,
            supplier_id: data.supplier_id,
            paid_at: data.paid_at,
            amount: data.amount,
            bank_reference,
            approval_subject_version: 0,
        })
    }

    /// 更新付款单草稿。
    ///
    /// 复用 `new` 的校验规则；`POSTED` 后内容不可编辑（§7.5），草稿修改不改变
    /// 付款单号与收款供应商等关键字段。
    ///
    /// # 参数
    /// * `update` - 更新数据
    ///
    /// # 返回
    /// 更新成功返回 `Ok(())`。
    ///
    /// # 错误
    /// 当状态非草稿或更新字段校验失败时返回错误。
    pub fn update(&mut self, update: SupplierPaymentUpdate) -> Result<()> {
        self.ensure_editable()?;
        if let Some(amount) = update.amount {
            if amount.to_decimal().is_sign_negative() || amount.to_decimal().is_zero() {
                return Err(Error::from("付款金额必须为正数"));
            }
            self.amount = amount;
        }
        if let Some(paid_at) = update.paid_at {
            self.paid_at = paid_at;
        }
        if let Some(bank_reference) = update.bank_reference {
            self.bank_reference =
                normalize_optional_text(Some(bank_reference), "付款凭证引用", BANK_REFERENCE_MAX_LEN)?;
        }
        Ok(())
    }

    /// 迁移付款单状态。
    ///
    /// 按 §7.5 固定邻接矩阵校验并应用状态迁移。
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

    /// 判断付款单是否已过账。
    ///
    /// # 返回
    /// 状态为 `Posted` 时返回 `true`。
    pub fn is_posted(&self) -> bool {
        self.status == SupplierPaymentStatus::Posted
    }

    /// 校验付款单仍可编辑。
    ///
    /// # 返回
    /// 草稿状态返回 `Ok(())`。
    ///
    /// # 错误
    /// 状态非草稿时返回错误。
    fn ensure_editable(&self) -> Result<()> {
        if self.status != SupplierPaymentStatus::Draft {
            return Err(Error::from("已过账或已冲正的付款单不可编辑"));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::str::FromStr;

    fn data() -> SupplierPaymentData {
        SupplierPaymentData {
            payment_no: " SP-2026-001 ".to_string(),
            supplier_id: SupplierAccountId::new("sup-1"),
            paid_at: Instant::from_unix_secs(1_700_000_000),
            amount: Amount::from_str("1000.00").unwrap(),
            bank_reference: Some(" BANK-1 ".to_string()),
        }
    }

    #[test]
    fn new_trims_text_fields_and_starts_as_draft() {
        let payment = SupplierPayment::new(SupplierPaymentId::new("sp-1"), data()).unwrap();

        assert_eq!(payment.payment_no, "SP-2026-001");
        assert_eq!(payment.bank_reference.as_deref(), Some("BANK-1"));
        assert_eq!(payment.status, SupplierPaymentStatus::Draft);
        assert!(!payment.is_posted());
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
    fn update_applies_draft_changes_and_rejects_posted() {
        let mut payment = SupplierPayment::new(SupplierPaymentId::new("sp-1"), data()).unwrap();

        payment
            .update(SupplierPaymentUpdate {
                paid_at: Some(Instant::from_unix_secs(1_700_000_100)),
                amount: Some(Amount::from_str("1200.00").unwrap()),
                bank_reference: Some(" BANK-2 ".to_string()),
            })
            .unwrap();
        assert_eq!(payment.payment_no, "SP-2026-001", "关键字段不改");
        assert_eq!(payment.bank_reference.as_deref(), Some("BANK-2"));

        payment.transition(SupplierPaymentStatus::Posted).unwrap();
        assert!(payment.is_posted());
        assert!(payment
            .update(SupplierPaymentUpdate {
                amount: Some(Amount::from_str("1.00").unwrap()),
                ..Default::default()
            })
            .is_err());
    }

    #[test]
    fn state_machine_edges_are_directed() {
        use crate::common::state::ensure_transition as tr;
        use SupplierPaymentStatus as S;

        assert!(tr(S::Draft, S::PendingReview).is_ok());
        assert!(tr(S::Draft, S::Posted).is_ok(), "付款非财务纠错，草稿可直接过账");
        assert!(tr(S::PendingReview, S::Posted).is_ok());
        assert!(tr(S::Posted, S::Reversed).is_ok());
        assert!(tr(S::Reversed, S::Reversed).is_ok(), "幂等迁移恒合法");

        assert!(tr(S::Draft, S::Reversed).is_err());
        assert!(tr(S::PendingReview, S::Reversed).is_err());
        assert!(tr(S::PendingReview, S::Draft).is_err());
        assert!(tr(S::Posted, S::Draft).is_err());
        assert!(tr(S::Reversed, S::Posted).is_err());

        let mut payment = SupplierPayment::new(SupplierPaymentId::new("sp-1"), data()).unwrap();
        assert!(payment.transition(S::Reversed).is_err(), "实体迁移拒绝跨级");
    }

    #[test]
    fn status_serializes_with_stable_codes_and_labels() {
        assert_eq!(
            serde_json::to_string(&SupplierPaymentStatus::Reversed).unwrap(),
            "\"reversed\""
        );
        assert_eq!(SupplierPaymentStatus::PendingReview.label(), "待复核");
        assert_eq!(SupplierPaymentStatus::Posted.as_str(), "posted");
    }
}
