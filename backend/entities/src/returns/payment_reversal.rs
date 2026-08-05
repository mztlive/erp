//! `payment_reversal` 付款冲正（数据模型 §6.11 财务纠错表、§7.5 资金单据状态机）。

use entity_core::BaseModel;
use entity_macros::Entity;
use serde::{Deserialize, Serialize};

use crate::common::state::{ensure_transition, DocumentState};
use crate::common::time::Instant;
use crate::errors::{Error, Result};
use crate::ids::{FileAssetId, PaymentReversalId, SupplierPaymentId};
use crate::money::Amount;
use crate::validation::{normalize_optional_text, normalize_required_text};

use super::customer_refund::validate_actor_pair;

/// 冲正单号最大长度。
const REVERSAL_NO_MAX_LEN: usize = 64;
/// 原因代码最大长度。
const REASON_CODE_MAX_LEN: usize = 32;
/// 原因文本最大长度。
const REASON_TEXT_MAX_LEN: usize = 512;

/// 冲正状态（数据模型 §6.11；§7.5 资金单据状态机：财务纠错强制经过复核）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PaymentReversalStatus {
    /// 草稿。
    Draft,
    /// 待复核（财务纠错必过）。
    PendingReview,
    /// 已过账。
    Posted,
    /// 已冲正（存在正式反向事实，原事实不删除）。
    Reversed,
}

impl PaymentReversalStatus {
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
        }
    }
}

impl DocumentState for PaymentReversalStatus {
    /// 返回全部合法后继状态（数据模型 §7.5 资金单据状态机）。
    ///
    /// 财务纠错必须经过 `PENDING_REVIEW`（经办/复核分离，§7.5），草稿不得
    /// 直接过账；`REVERSED` 是终态。
    fn allowed_next(self) -> &'static [Self] {
        match self {
            Self::Draft => &[Self::PendingReview],
            Self::PendingReview => &[Self::Posted],
            Self::Posted => &[Self::Reversed],
            Self::Reversed => &[],
        }
    }
}

/// 付款冲正创建数据。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PaymentReversalData {
    /// 冲正单号（唯一）。
    pub reversal_no: String,
    /// 被冲正的原供应商付款。
    pub original_supplier_payment_id: SupplierPaymentId,
    /// 原因代码（可空）。
    pub reason_code: Option<String>,
    /// 原因说明（必填）。
    pub reason_text: String,
    /// 冲正金额（正数；累计有效冲正不得超过原付款金额，跨实体约束归 P3）。
    pub amount: Amount,
    /// 财务经办人。
    pub handled_by: String,
    /// 财务复核人（不得与经办人相同）。
    pub reviewed_by: String,
    /// 冲正实际发生时间。
    pub occurred_at: Instant,
    /// 凭证附件。
    pub evidence_attachment_id: Option<FileAssetId>,
}

/// 付款冲正更新数据（仅草稿可编辑）。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct PaymentReversalUpdate {
    /// 原因代码；`None` 表示不修改，`Some("")` 清除。
    pub reason_code: Option<String>,
    /// 原因说明；`None` 表示不修改。
    pub reason_text: Option<String>,
    /// 冲正金额；`None` 表示不修改。
    pub amount: Option<Amount>,
    /// 凭证附件；`None` 表示不修改。
    pub evidence_attachment_id: Option<FileAssetId>,
}

/// 付款冲正实体（正式事实，数据模型 §6.11）。
///
/// 财务经办人与复核人不得相同；冲正过账时锁定付款和子账，追加必要的反向分配，
/// 不删除原付款（§6.9、§8.3）。同一原事实的累计有效冲正不得超过原金额是跨
/// 实体约束，由 P3 过账事务校验。状态机见 §7.5。
#[derive(Debug, Serialize, Deserialize, Clone, Entity, PartialEq, Eq)]
pub struct PaymentReversal {
    #[serde(flatten)]
    pub base: BaseModel,
    /// 冲正状态。
    pub status: PaymentReversalStatus,
    /// 冲正单号。
    pub reversal_no: String,
    /// 被冲正的原供应商付款。
    pub original_supplier_payment_id: SupplierPaymentId,
    /// 原因代码。
    pub reason_code: Option<String>,
    /// 原因说明。
    pub reason_text: String,
    /// 冲正金额。
    pub amount: Amount,
    /// 财务经办人。
    pub handled_by: String,
    /// 财务复核人。
    pub reviewed_by: String,
    /// 冲正实际发生时间。
    pub occurred_at: Instant,
    /// 凭证附件。
    pub evidence_attachment_id: Option<FileAssetId>,
}

impl PaymentReversal {
    /// 创建付款冲正（初始状态为草稿）。
    ///
    /// 完成编号/原因/经办复核人的 trim/非空/长度校验、金额正数校验与经办人
    /// 复核人分离校验。
    ///
    /// # 参数
    /// * `id` - 实体主键（`entities::ids::PaymentReversalId`）
    /// * `data` - 创建数据
    ///
    /// # 返回
    /// 返回新建的冲正实体。
    ///
    /// # 错误
    /// 当编号/原因/经办复核人为空或超长、金额非正、经办与复核人相同时返回错误。
    #[allow(clippy::too_many_arguments)]
    pub fn new(id: PaymentReversalId, data: PaymentReversalData) -> Result<Self> {
        let reversal_no = normalize_required_text(
            data.reversal_no,
            "冲正单号不能为空",
            REVERSAL_NO_MAX_LEN,
            "冲正单号过长",
        )?;
        let reason_text = normalize_required_text(
            data.reason_text,
            "冲正原因不能为空",
            REASON_TEXT_MAX_LEN,
            "冲正原因过长",
        )?;
        let reason_code = normalize_optional_text(data.reason_code, "原因代码", REASON_CODE_MAX_LEN)?;
        if data.amount.to_decimal().is_sign_negative() || data.amount.to_decimal().is_zero() {
            return Err(Error::from("冲正金额必须为正数"));
        }
        let (handled_by, reviewed_by) = validate_actor_pair(data.handled_by, data.reviewed_by)?;

        Ok(Self {
            base: BaseModel::new(id.to_string()),
            status: PaymentReversalStatus::Draft,
            reversal_no,
            original_supplier_payment_id: data.original_supplier_payment_id,
            reason_code,
            reason_text,
            amount: data.amount,
            handled_by,
            reviewed_by,
            occurred_at: data.occurred_at,
            evidence_attachment_id: data.evidence_attachment_id,
        })
    }

    /// 更新付款冲正草稿。
    ///
    /// 复用 `new` 的校验规则；`POSTED` 后内容不可编辑（§7.5）；冲正单号与
    /// 原付款引用是固定字段不允许修改。
    ///
    /// # 参数
    /// * `update` - 更新数据
    ///
    /// # 返回
    /// 更新成功返回 `Ok(())`。
    ///
    /// # 错误
    /// 当状态非草稿或更新字段校验失败时返回错误。
    pub fn update(&mut self, update: PaymentReversalUpdate) -> Result<()> {
        if self.status != PaymentReversalStatus::Draft {
            return Err(Error::from("已过账或已冲正的冲正单不可编辑"));
        }
        if let Some(amount) = update.amount {
            if amount.to_decimal().is_sign_negative() || amount.to_decimal().is_zero() {
                return Err(Error::from("冲正金额必须为正数"));
            }
            self.amount = amount;
        }
        if let Some(reason_text) = update.reason_text {
            self.reason_text = normalize_required_text(
                reason_text,
                "冲正原因不能为空",
                REASON_TEXT_MAX_LEN,
                "冲正原因过长",
            )?;
        }
        if let Some(reason_code) = update.reason_code {
            self.reason_code = normalize_optional_text(Some(reason_code), "原因代码", REASON_CODE_MAX_LEN)?;
        }
        if let Some(evidence) = update.evidence_attachment_id {
            self.evidence_attachment_id = Some(evidence);
        }
        Ok(())
    }

    /// 迁移冲正状态。
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
    pub fn transition(&mut self, to: PaymentReversalStatus) -> Result<()> {
        ensure_transition(self.status, to)?;
        self.status = to;
        Ok(())
    }

    /// 判断冲正是否已过账。
    ///
    /// # 返回
    /// 状态为 `Posted` 时返回 `true`。
    pub fn is_posted(&self) -> bool {
        self.status == PaymentReversalStatus::Posted
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::money::Amount;
    use std::str::FromStr;

    fn data() -> PaymentReversalData {
        PaymentReversalData {
            reversal_no: " PRR-2026-001 ".to_string(),
            original_supplier_payment_id: SupplierPaymentId::new("sp-1"),
            reason_code: Some(" WRONG_ACCOUNT ".to_string()),
            reason_text: " 错付款冲正 ".to_string(),
            amount: Amount::from_str("1000.00").unwrap(),
            handled_by: " handler-1 ".to_string(),
            reviewed_by: " reviewer-1 ".to_string(),
            occurred_at: Instant::from_unix_secs(1_700_000_000),
            evidence_attachment_id: None,
        }
    }

    #[test]
    fn new_trims_text_fields_and_starts_as_draft() {
        let reversal = PaymentReversal::new(PaymentReversalId::new("prr-1"), data()).unwrap();

        assert_eq!(reversal.reversal_no, "PRR-2026-001");
        assert_eq!(reversal.reason_code.as_deref(), Some("WRONG_ACCOUNT"));
        assert_eq!(reversal.handled_by, "handler-1");
        assert_eq!(reversal.status, PaymentReversalStatus::Draft);
        assert!(!reversal.is_posted());
    }

    #[test]
    fn new_rejects_blank_no_same_actor_and_non_positive() {
        let blank_no = PaymentReversalData {
            reversal_no: "   ".to_string(),
            ..data()
        };
        assert!(PaymentReversal::new(PaymentReversalId::new("prr-2"), blank_no).is_err());

        let overlong = PaymentReversalData {
            reason_text: "r".repeat(513),
            ..data()
        };
        assert!(PaymentReversal::new(PaymentReversalId::new("prr-3"), overlong).is_err());

        let same_actor = PaymentReversalData {
            reviewed_by: "handler-1".to_string(),
            ..data()
        };
        assert!(PaymentReversal::new(PaymentReversalId::new("prr-4"), same_actor).is_err());

        let non_positive = PaymentReversalData {
            amount: Amount::from_str("0.00").unwrap(),
            ..data()
        };
        assert!(PaymentReversal::new(PaymentReversalId::new("prr-5"), non_positive).is_err());
    }

    #[test]
    fn update_applies_draft_changes_and_rejects_posted() {
        let mut reversal = PaymentReversal::new(PaymentReversalId::new("prr-1"), data()).unwrap();

        reversal
            .update(PaymentReversalUpdate {
                reason_text: Some(" 更换原因 ".to_string()),
                amount: Some(Amount::from_str("500.00").unwrap()),
                ..Default::default()
            })
            .unwrap();
        assert_eq!(reversal.reason_text, "更换原因");
        assert_eq!(reversal.amount, Amount::from_str("500.00").unwrap());
        assert_eq!(reversal.reversal_no, "PRR-2026-001", "关键字段不改");

        reversal.transition(PaymentReversalStatus::PendingReview).unwrap();
        reversal.transition(PaymentReversalStatus::Posted).unwrap();
        assert!(reversal.is_posted());
        assert!(reversal
            .update(PaymentReversalUpdate {
                amount: Some(Amount::from_str("1.00").unwrap()),
                ..Default::default()
            })
            .is_err());
    }

    #[test]
    fn state_machine_forces_review_before_posting() {
        use crate::common::state::ensure_transition as tr;
        use PaymentReversalStatus as S;

        assert!(tr(S::Draft, S::PendingReview).is_ok());
        assert!(tr(S::PendingReview, S::Posted).is_ok());
        assert!(tr(S::Posted, S::Reversed).is_ok());

        assert!(tr(S::Draft, S::Posted).is_err(), "财务纠错必须经过复核");
        assert!(tr(S::Draft, S::Reversed).is_err());
        assert!(tr(S::Posted, S::Draft).is_err());
        assert!(tr(S::Reversed, S::Posted).is_err());

        let mut reversal = PaymentReversal::new(PaymentReversalId::new("prr-1"), data()).unwrap();
        assert!(
            reversal.transition(PaymentReversalStatus::Posted).is_err(),
            "草稿不得直接过账"
        );
        reversal.transition(PaymentReversalStatus::PendingReview).unwrap();
        assert!(reversal.transition(PaymentReversalStatus::Posted).is_ok());
    }

    #[test]
    fn status_serializes_with_stable_codes_and_labels() {
        assert_eq!(
            serde_json::to_string(&PaymentReversalStatus::Reversed).unwrap(),
            "\"reversed\""
        );
        assert_eq!(PaymentReversalStatus::PendingReview.label(), "待复核");
        assert_eq!(PaymentReversalStatus::Posted.as_str(), "posted");
    }
}
