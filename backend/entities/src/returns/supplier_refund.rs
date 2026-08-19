//! `supplier_refund` 供应商退款（数据模型 §6.11 财务纠错表、§7.5 资金单据状态机）。

use entity_core::BaseModel;
use entity_macros::Entity;
use serde::{Deserialize, Serialize};

use crate::common::state::{ensure_transition, DocumentState};
use crate::common::time::Instant;
use crate::errors::{Error, Result};
use crate::ids::{
    FileAssetId, PayableEntryId, PurchaseReturnOrderId, SupplierAccountId, SupplierPaymentId,
    SupplierRefundId,
};
use crate::money::Amount;
use crate::validation::{normalize_optional_text, normalize_required_text};

use super::customer_refund::validate_actor_pair;

/// 退款单号最大长度。
const REFUND_NO_MAX_LEN: usize = 64;
/// 原因代码最大长度。
const REASON_CODE_MAX_LEN: usize = 32;
/// 原因文本最大长度。
const REASON_TEXT_MAX_LEN: usize = 512;

/// 退款状态（合同 §4.4.1 / §4.4.2：复核态收敛为唯一 `IN_APPROVAL`）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SupplierRefundStatus {
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

impl SupplierRefundStatus {
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

impl DocumentState for SupplierRefundStatus {
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

/// 供应商退款创建数据。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SupplierRefundData {
    /// 退款单号（唯一）。
    pub refund_no: String,
    /// 采购退货/错付款依据（可空）。
    pub purchase_return_order_id: Option<PurchaseReturnOrderId>,
    /// 供应商。
    pub supplier_id: SupplierAccountId,
    /// 原付款（与 `original_payable_entry_id` 必须且只能选一）。
    pub original_payment_id: Option<SupplierPaymentId>,
    /// 原应付分录（与 `original_payment_id` 必须且只能选一）。
    pub original_payable_entry_id: Option<PayableEntryId>,
    /// 原因代码（可空）。
    pub reason_code: Option<String>,
    /// 原因说明（必填）。
    pub reason_text: String,
    /// 退款金额（正数）。
    pub amount: Amount,
    /// 财务经办人。
    pub handled_by: String,
    /// 财务复核人（不得与经办人相同）。
    pub reviewed_by: String,
    /// 实际退款时间。
    pub occurred_at: Instant,
    /// 凭证附件。
    pub evidence_attachment_id: Option<FileAssetId>,
}

/// 供应商退款更新数据（仅草稿可编辑）。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct SupplierRefundUpdate {
    /// 原因代码；`None` 表示不修改，`Some("")` 清除。
    pub reason_code: Option<String>,
    /// 原因说明；`None` 表示不修改。
    pub reason_text: Option<String>,
    /// 退款金额；`None` 表示不修改。
    pub amount: Option<Amount>,
    /// 凭证附件；`None` 表示不修改。
    pub evidence_attachment_id: Option<FileAssetId>,
}

/// 供应商退款实体（正式事实，数据模型 §6.11）。
///
/// 财务经办人与复核人不得相同；审核通过并过账后，原事实保留，追加成本冲减、
/// 应付冲减和分录抵销；已付款部分同时追加付款分配 `REVERSE` 与通用供应商现金
/// 退款事实；不替代商城退款。同一原事实的累计有效冲正不得超过原金额是跨实体
/// 约束，由 P3 过账事务校验（§8.4）。状态机见 §7.5。
#[derive(Debug, Serialize, Deserialize, Clone, Entity, PartialEq, Eq)]
pub struct SupplierRefund {
    #[serde(flatten)]
    pub base: BaseModel,
    /// 退款状态。
    pub status: SupplierRefundStatus,
    /// 退款单号。
    pub refund_no: String,
    /// 采购退货/错付款依据。
    pub purchase_return_order_id: Option<PurchaseReturnOrderId>,
    /// 供应商。
    pub supplier_id: SupplierAccountId,
    /// 原付款。
    pub original_payment_id: Option<SupplierPaymentId>,
    /// 原应付分录。
    pub original_payable_entry_id: Option<PayableEntryId>,
    /// 原因代码。
    pub reason_code: Option<String>,
    /// 原因说明。
    pub reason_text: String,
    /// 退款金额。
    pub amount: Amount,
    /// 财务经办人。
    pub handled_by: String,
    /// 财务复核人。
    pub reviewed_by: String,
    /// 实际退款时间。
    pub occurred_at: Instant,
    /// 凭证附件。
    pub evidence_attachment_id: Option<FileAssetId>,
    /// 审批提交版本，初值 0。
    #[serde(default)]
    pub approval_subject_version: u32,
}

impl SupplierRefund {
    /// 创建供应商退款（初始状态为草稿）。
    ///
    /// 完成编号/原因/经办复核人的 trim/非空/长度校验、金额正数校验、经办人
    /// 与复核人分离校验，以及「原付款或原应付」二选一校验。
    ///
    /// # 参数
    /// * `id` - 实体主键（`entities::ids::SupplierRefundId`）
    /// * `data` - 创建数据
    ///
    /// # 返回
    /// 返回新建的退款实体。
    ///
    /// # 错误
    /// 当编号/原因/经办复核人为空或超长、金额非正、经办与复核人相同、原付款
    /// 与原应付同时或均未提供时返回错误。
    pub fn new(id: SupplierRefundId, data: SupplierRefundData) -> Result<Self> {
        let refund_no = normalize_required_text(
            data.refund_no,
            "退款单号不能为空",
            REFUND_NO_MAX_LEN,
            "退款单号过长",
        )?;
        let reason_text = normalize_required_text(
            data.reason_text,
            "退款原因不能为空",
            REASON_TEXT_MAX_LEN,
            "退款原因过长",
        )?;
        let reason_code = normalize_optional_text(data.reason_code, "原因代码", REASON_CODE_MAX_LEN)?;
        if data.amount.to_decimal().is_sign_negative() || data.amount.to_decimal().is_zero() {
            return Err(Error::from("退款金额必须为正数"));
        }
        let (handled_by, reviewed_by) = validate_actor_pair(data.handled_by, data.reviewed_by)?;
        validate_original_target(&data.original_payment_id, &data.original_payable_entry_id)?;

        Ok(Self {
            base: BaseModel::new(id.to_string()),
            status: SupplierRefundStatus::Draft,
            refund_no,
            purchase_return_order_id: data.purchase_return_order_id,
            supplier_id: data.supplier_id,
            original_payment_id: data.original_payment_id,
            original_payable_entry_id: data.original_payable_entry_id,
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

    /// 更新供应商退款草稿。
    ///
    /// 复用 `new` 的校验规则；仅草稿可编辑；退款单号、供应商与原事实引用
    /// 是固定字段不允许修改。
    ///
    /// # 参数
    /// * `update` - 更新数据
    ///
    /// # 返回
    /// 更新成功返回 `Ok(())`。
    ///
    /// # 错误
    /// 当状态非草稿或更新字段校验失败时返回错误。
    pub fn update(&mut self, update: SupplierRefundUpdate) -> Result<()> {
        if self.status != SupplierRefundStatus::Draft {
            return Err(Error::from("非草稿状态的退款不可编辑"));
        }
        if let Some(amount) = update.amount {
            if amount.to_decimal().is_sign_negative() || amount.to_decimal().is_zero() {
                return Err(Error::from("退款金额必须为正数"));
            }
            self.amount = amount;
        }
        if let Some(reason_text) = update.reason_text {
            self.reason_text = normalize_required_text(
                reason_text,
                "退款原因不能为空",
                REASON_TEXT_MAX_LEN,
                "退款原因过长",
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

    /// 迁移退款状态。
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
    pub fn transition(&mut self, to: SupplierRefundStatus) -> Result<()> {
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
        if self.status != SupplierRefundStatus::Draft {
            return Err(Error::from("只有草稿状态的供应商退款单可以提交审批"));
        }
        let next = self
            .approval_subject_version
            .checked_add(1)
            .ok_or_else(|| Error::from("审批提交版本溢出"))?;
        self.approval_subject_version = next;
        self.transition(SupplierRefundStatus::InApproval)?;
        Ok(next)
    }

    /// 撤回审批：回到草稿，且 `approval_subject_version` 不回退。
    ///
    /// # 错误
    /// 非审批中时返回冲突。
    pub fn cancel_approval(&mut self) -> Result<()> {
        if self.status != SupplierRefundStatus::InApproval {
            return Err(Error::from("只有审批中的供应商退款单可以撤回审批"));
        }
        self.transition(SupplierRefundStatus::Draft)
    }

    /// 最终通过过账：仅 `IN_APPROVAL` 可进入 `POSTED`。
    ///
    /// # 错误
    /// 状态不是审批中时返回冲突。
    pub fn mark_posted(&mut self) -> Result<()> {
        if self.status != SupplierRefundStatus::InApproval {
            return Err(Error::from("只有审批中的供应商退款单可以由最终通过动作过账"));
        }
        self.transition(SupplierRefundStatus::Posted)
    }

    /// 判断退款是否已过账。
    ///
    /// # 返回
    /// 状态为 `Posted` 时返回 `true`。
    pub fn is_posted(&self) -> bool {
        self.status == SupplierRefundStatus::Posted
    }
}

/// 校验退款原事实二选一。
///
/// 规则（数据模型 §6.11）：退款必须指向「原付款」或「原应付」之一。
///
/// # 参数
/// * `original_payment_id` - 原付款
/// * `original_payable_entry_id` - 原应付分录
///
/// # 返回
/// 二选一成立返回 `Ok(())`。
///
/// # 错误
/// 同时或均未提供时返回错误。
fn validate_original_target(
    original_payment_id: &Option<SupplierPaymentId>,
    original_payable_entry_id: &Option<PayableEntryId>,
) -> Result<()> {
    match (original_payment_id.is_some(), original_payable_entry_id.is_some()) {
        (true, true) => Err(Error::from("原付款与原应付只能指向其一")),
        (false, false) => Err(Error::from("退款必须指向原付款或原应付")),
        _ => Ok(()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::money::Amount;
    use std::str::FromStr;

    fn data() -> SupplierRefundData {
        SupplierRefundData {
            refund_no: " SRF-2026-001 ".to_string(),
            purchase_return_order_id: Some(PurchaseReturnOrderId::new("pro-1")),
            supplier_id: SupplierAccountId::new("sup-1"),
            original_payment_id: Some(SupplierPaymentId::new("sp-1")),
            original_payable_entry_id: None,
            reason_code: Some(" OVERPAY ".to_string()),
            reason_text: " 错付款退回 ".to_string(),
            amount: Amount::from_str("1000.00").unwrap(),
            handled_by: " handler-1 ".to_string(),
            reviewed_by: " reviewer-1 ".to_string(),
            occurred_at: Instant::from_unix_secs(1_700_000_000),
            evidence_attachment_id: None,
        }
    }

    #[test]
    fn new_trims_text_fields_and_starts_as_draft() {
        let refund = SupplierRefund::new(SupplierRefundId::new("srf-1"), data()).unwrap();

        assert_eq!(refund.refund_no, "SRF-2026-001");
        assert_eq!(refund.reason_code.as_deref(), Some("OVERPAY"));
        assert_eq!(refund.handled_by, "handler-1");
        assert_eq!(refund.status, SupplierRefundStatus::Draft);
        assert!(!refund.is_posted());
    }

    #[test]
    fn new_rejects_blank_no_same_actor_and_bad_target() {
        let blank_no = SupplierRefundData {
            refund_no: "   ".to_string(),
            ..data()
        };
        assert!(SupplierRefund::new(SupplierRefundId::new("srf-2"), blank_no).is_err());

        let overlong = SupplierRefundData {
            reason_text: "r".repeat(513),
            ..data()
        };
        assert!(SupplierRefund::new(SupplierRefundId::new("srf-3"), overlong).is_err());

        let same_actor = SupplierRefundData {
            reviewed_by: "handler-1".to_string(),
            ..data()
        };
        assert!(SupplierRefund::new(SupplierRefundId::new("srf-4"), same_actor).is_err());

        let no_target = SupplierRefundData {
            original_payment_id: None,
            ..data()
        };
        assert!(SupplierRefund::new(SupplierRefundId::new("srf-5"), no_target).is_err());

        let both_targets = SupplierRefundData {
            original_payable_entry_id: Some(PayableEntryId::new("pe-1")),
            ..data()
        };
        assert!(SupplierRefund::new(SupplierRefundId::new("srf-6"), both_targets).is_err());

        let non_positive = SupplierRefundData {
            amount: Amount::from_str("0.00").unwrap(),
            ..data()
        };
        assert!(SupplierRefund::new(SupplierRefundId::new("srf-7"), non_positive).is_err());
    }

    #[test]
    fn update_applies_draft_changes_and_rejects_posted() {
        let mut refund = SupplierRefund::new(SupplierRefundId::new("srf-1"), data()).unwrap();

        refund
            .update(SupplierRefundUpdate {
                reason_text: Some(" 更换原因 ".to_string()),
                amount: Some(Amount::from_str("800.00").unwrap()),
                ..Default::default()
            })
            .unwrap();
        assert_eq!(refund.reason_text, "更换原因");
        assert_eq!(refund.amount, Amount::from_str("800.00").unwrap());
        assert_eq!(refund.refund_no, "SRF-2026-001", "关键字段不改");

        refund.start_approval().unwrap();
        refund.mark_posted().unwrap();
        assert!(refund.is_posted());
        assert!(refund
            .update(SupplierRefundUpdate {
                amount: Some(Amount::from_str("1.00").unwrap()),
                ..Default::default()
            })
            .is_err());
    }

    #[test]
    fn state_machine_forces_in_approval_before_posting() {
        use crate::common::state::ensure_transition as tr;
        use SupplierRefundStatus as S;

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

        let mut refund = SupplierRefund::new(SupplierRefundId::new("srf-1"), data()).unwrap();
        assert!(refund.transition(S::Posted).is_err(), "草稿不得直接过账");
        refund.start_approval().unwrap();
        assert!(refund.mark_posted().is_ok());
    }

    #[test]
    fn start_approval_increments_version_and_cancel_does_not_rollback() {
        let mut refund = SupplierRefund::new(SupplierRefundId::new("srf-1"), data()).unwrap();
        assert_eq!(refund.approval_subject_version, 0);
        let version = refund.start_approval().unwrap();
        assert_eq!(version, 1);
        assert_eq!(refund.status, SupplierRefundStatus::InApproval);
        refund.cancel_approval().unwrap();
        assert_eq!(refund.status, SupplierRefundStatus::Draft);
        assert_eq!(refund.approval_subject_version, 1);
        assert!(refund.mark_posted().is_err());
    }

    #[test]
    fn status_serializes_with_stable_codes_and_labels() {
        assert_eq!(
            serde_json::to_string(&SupplierRefundStatus::InApproval).unwrap(),
            "\"IN_APPROVAL\""
        );
        assert_eq!(SupplierRefundStatus::Posted.label(), "已过账");
        assert_eq!(SupplierRefundStatus::Reversed.as_str(), "reversed");
        assert_eq!(SupplierRefundStatus::Draft.as_str(), "draft");
        let production = include_str!("supplier_refund.rs")
            .split("#[cfg(test)]")
            .next()
            .expect("生产代码");
        assert!(!production.contains("PendingReview"));
        assert!(!production.contains("pending_review"));
    }
}
