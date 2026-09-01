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
/// 创建人标识最大长度。
const CREATOR_ID_MAX_LEN: usize = 128;

/// 冲正状态（合同 §4.4.1 / §4.4.2：复核态收敛为唯一 `IN_APPROVAL`）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PaymentReversalStatus {
    /// 草稿。
    Draft,
    /// 审批中。
    #[serde(rename = "IN_APPROVAL")]
    InApproval,
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
            Self::InApproval => "审批中",
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
            Self::InApproval => "IN_APPROVAL",
            Self::Posted => "posted",
            Self::Reversed => "reversed",
        }
    }
}

impl DocumentState for PaymentReversalStatus {
    /// 返回全部合法后继状态（合同 §4.4.1 / §4.4.2）。
    ///
    /// 复核态唯一为 `IN_APPROVAL`；草稿不得直接过账；审批中可过账或受控
    /// 撤回回草稿；`REVERSED` 是终态。审批导致的业务 `REJECTED` 已删除。
    fn allowed_next(self) -> &'static [Self] {
        match self {
            Self::Draft => &[Self::InApproval],
            Self::InApproval => &[Self::Posted, Self::Draft],
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
    /// 创建人。旧数据缺失时反序列化为空，但不得据此执行绑定升级。
    #[serde(default)]
    pub created_by: String,
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
    /// 审批提交版本，初值 0。
    #[serde(default)]
    pub approval_subject_version: u32,
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
    /// * `created_by` - 已认证创建人；创建后不得由更新命令覆盖
    ///
    /// # 返回
    /// 返回新建的冲正实体。
    ///
    /// # 错误
    /// 当编号/原因/经办复核人/创建人为空或超长、金额非正、经办与复核人相同时返回错误。
    pub fn new(
        id: PaymentReversalId,
        data: PaymentReversalData,
        created_by: impl Into<String>,
    ) -> Result<Self> {
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
        let created_by = normalize_required_text(
            created_by.into(),
            "创建人不能为空",
            CREATOR_ID_MAX_LEN,
            "创建人标识过长",
        )?;

        Ok(Self {
            base: BaseModel::new(id.to_string()),
            created_by,
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
            approval_subject_version: 0,
        })
    }

    /// 校验付款冲正仍是从未提交审批的初始草稿。
    ///
    /// # 错误
    /// 非草稿或审批主题版本已经递增时返回错误。
    pub fn ensure_initial_approval_state(&self) -> Result<()> {
        if self.status != PaymentReversalStatus::Draft || self.approval_subject_version != 0 {
            return Err(Error::from("付款冲正单已经提交或启动过审批"));
        }
        Ok(())
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
            return Err(Error::from("非草稿状态的冲正单不可编辑"));
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

    /// 提交并启动审批：递增 `approval_subject_version` 并进入 `IN_APPROVAL`。
    ///
    /// 版本使用 checked add，成功后不回退。不得改写 `BaseModel.version`。
    ///
    /// # 返回
    /// 返回冻结后的提交版本。
    ///
    /// # 错误
    /// 非草稿或版本溢出时返回冲突。
    pub fn start_approval(&mut self) -> Result<u32> {
        if self.status != PaymentReversalStatus::Draft {
            return Err(Error::from("只有草稿状态的付款冲正单可以提交审批"));
        }
        let next = self
            .approval_subject_version
            .checked_add(1)
            .ok_or_else(|| Error::from("审批提交版本溢出"))?;
        self.approval_subject_version = next;
        self.transition(PaymentReversalStatus::InApproval)?;
        Ok(next)
    }

    /// 撤回审批：回到草稿，且 `approval_subject_version` 不回退。
    ///
    /// # 错误
    /// 非审批中时返回冲突。
    pub fn cancel_approval(&mut self) -> Result<()> {
        if self.status != PaymentReversalStatus::InApproval {
            return Err(Error::from("只有审批中的付款冲正单可以撤回审批"));
        }
        self.transition(PaymentReversalStatus::Draft)
    }

    /// 最终通过过账：仅 `IN_APPROVAL` 可进入 `POSTED`。
    ///
    /// # 错误
    /// 状态不是审批中时返回冲突。
    pub fn mark_posted(&mut self) -> Result<()> {
        if self.status != PaymentReversalStatus::InApproval {
            return Err(Error::from("只有审批中的付款冲正单可以由最终通过动作过账"));
        }
        self.transition(PaymentReversalStatus::Posted)
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
        let reversal = PaymentReversal::new(PaymentReversalId::new("prr-1"), data(), "creator-1").unwrap();

        assert_eq!(reversal.reversal_no, "PRR-2026-001");
        assert_eq!(reversal.reason_code.as_deref(), Some("WRONG_ACCOUNT"));
        assert_eq!(reversal.handled_by, "handler-1");
        assert_eq!(reversal.status, PaymentReversalStatus::Draft);
        assert!(!reversal.is_posted());
    }

    #[test]
    fn creator_is_normalized_and_legacy_missing_field_defaults_empty() {
        let mut reversal =
            PaymentReversal::new(PaymentReversalId::new("creator-prr"), data(), " creator-1 ").unwrap();
        reversal.update(PaymentReversalUpdate::default()).unwrap();
        assert_eq!(reversal.created_by, "creator-1");
        assert!(PaymentReversal::new(PaymentReversalId::new("blank-creator-prr"), data(), "   ").is_err());

        let mut legacy = bson::serialize_to_document(&reversal).unwrap();
        legacy.remove("created_by");
        let legacy: PaymentReversal = bson::deserialize_from_document(legacy).unwrap();
        assert!(legacy.created_by.is_empty());
    }

    #[test]
    fn new_rejects_blank_no_same_actor_and_non_positive() {
        let blank_no = PaymentReversalData {
            reversal_no: "   ".to_string(),
            ..data()
        };
        assert!(PaymentReversal::new(PaymentReversalId::new("prr-2"), blank_no, "creator-1").is_err());

        let overlong = PaymentReversalData {
            reason_text: "r".repeat(513),
            ..data()
        };
        assert!(PaymentReversal::new(PaymentReversalId::new("prr-3"), overlong, "creator-1").is_err());

        let same_actor = PaymentReversalData {
            reviewed_by: "handler-1".to_string(),
            ..data()
        };
        assert!(PaymentReversal::new(PaymentReversalId::new("prr-4"), same_actor, "creator-1").is_err());

        let non_positive = PaymentReversalData {
            amount: Amount::from_str("0.00").unwrap(),
            ..data()
        };
        assert!(PaymentReversal::new(PaymentReversalId::new("prr-5"), non_positive, "creator-1").is_err());
    }

    #[test]
    fn update_applies_draft_changes_and_rejects_posted() {
        let mut reversal =
            PaymentReversal::new(PaymentReversalId::new("prr-1"), data(), "creator-1").unwrap();

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

        reversal.start_approval().unwrap();
        reversal.mark_posted().unwrap();
        assert!(reversal.is_posted());
        assert!(reversal
            .update(PaymentReversalUpdate {
                amount: Some(Amount::from_str("1.00").unwrap()),
                ..Default::default()
            })
            .is_err());
    }

    #[test]
    fn state_machine_forces_in_approval_before_posting() {
        use crate::common::state::ensure_transition as tr;
        use PaymentReversalStatus as S;

        assert!(tr(S::Draft, S::InApproval).is_ok());
        assert!(tr(S::InApproval, S::Posted).is_ok());
        assert!(tr(S::InApproval, S::Draft).is_ok());
        assert!(tr(S::Posted, S::Reversed).is_ok());
        assert!(tr(S::Reversed, S::Reversed).is_ok(), "幂等迁移恒合法");

        assert!(tr(S::Draft, S::Posted).is_err(), "草稿不得绕过审批直接过账");
        assert!(tr(S::Draft, S::Reversed).is_err());
        assert!(tr(S::Posted, S::Draft).is_err());
        assert!(tr(S::Posted, S::InApproval).is_err());
        assert!(tr(S::Reversed, S::Posted).is_err());
        assert!(tr(S::Reversed, S::Draft).is_err());

        let mut reversal =
            PaymentReversal::new(PaymentReversalId::new("prr-1"), data(), "creator-1").unwrap();
        assert!(reversal.transition(S::Posted).is_err(), "草稿不得直接过账");
        reversal.start_approval().unwrap();
        assert!(reversal.mark_posted().is_ok());
    }

    #[test]
    fn start_approval_increments_version_and_cancel_does_not_rollback() {
        let mut reversal =
            PaymentReversal::new(PaymentReversalId::new("prr-1"), data(), "creator-1").unwrap();
        reversal
            .ensure_initial_approval_state()
            .expect("新建草稿是初始未提交状态");
        assert_eq!(reversal.approval_subject_version, 0);
        let version = reversal.start_approval().unwrap();
        assert_eq!(version, 1);
        assert_eq!(reversal.status, PaymentReversalStatus::InApproval);
        reversal.cancel_approval().unwrap();
        assert_eq!(reversal.status, PaymentReversalStatus::Draft);
        assert_eq!(reversal.approval_subject_version, 1);
        assert!(reversal.ensure_initial_approval_state().is_err());
        assert!(reversal.mark_posted().is_err());
    }

    #[test]
    fn status_serializes_with_stable_codes_and_labels() {
        assert_eq!(
            serde_json::to_string(&PaymentReversalStatus::InApproval).unwrap(),
            "\"IN_APPROVAL\""
        );
        assert_eq!(PaymentReversalStatus::Posted.label(), "已过账");
        assert_eq!(PaymentReversalStatus::Reversed.as_str(), "reversed");
        assert_eq!(PaymentReversalStatus::Draft.as_str(), "draft");
        let production = include_str!("payment_reversal.rs")
            .split("#[cfg(test)]")
            .next()
            .expect("生产代码");
        assert!(!production.contains("PendingReview"));
        assert!(!production.contains("pending_review"));
    }
}
