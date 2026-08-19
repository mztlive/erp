//! `customer_refund` 客户退款（数据模型 §6.11 财务纠错表、§7.5 资金单据状态机）。

use entity_core::BaseModel;
use entity_macros::Entity;
use serde::{Deserialize, Serialize};

use crate::common::state::{ensure_transition, DocumentState};
use crate::common::time::Instant;
use crate::errors::{Error, Result};
use crate::ids::{
    CustomerAccountId, CustomerReceiptId, CustomerRefundId, FileAssetId, ReceivableEntryId, SalesReturnCaseId,
};
use crate::money::Amount;
use crate::validation::{normalize_optional_text, normalize_required_text};

/// 退款单号最大长度。
const REFUND_NO_MAX_LEN: usize = 64;
/// 原因代码最大长度。
const REASON_CODE_MAX_LEN: usize = 32;
/// 原因文本最大长度。
const REASON_TEXT_MAX_LEN: usize = 512;
/// 经办人/复核人标识最大长度。
const ACTOR_MAX_LEN: usize = 128;

/// 退款状态（合同 §4.4.1 / §4.4.2：复核态收敛为唯一 `IN_APPROVAL`）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CustomerRefundStatus {
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

impl CustomerRefundStatus {
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

impl DocumentState for CustomerRefundStatus {
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

/// 客户退款创建数据。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CustomerRefundData {
    /// 退款单号（唯一）。
    pub refund_no: String,
    /// 销售退货/拒收处理单（可空）。
    pub sales_return_case_id: Option<SalesReturnCaseId>,
    /// 客户。
    pub customer_id: CustomerAccountId,
    /// 原回款（与 `original_receivable_entry_id` 必须且只能选一）。
    pub original_receipt_id: Option<CustomerReceiptId>,
    /// 原应收分录（与 `original_receipt_id` 必须且只能选一）。
    pub original_receivable_entry_id: Option<ReceivableEntryId>,
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

/// 客户退款更新数据（仅草稿可编辑）。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct CustomerRefundUpdate {
    /// 原因代码；`None` 表示不修改，`Some("")` 清除。
    pub reason_code: Option<String>,
    /// 原因说明；`None` 表示不修改。
    pub reason_text: Option<String>,
    /// 退款金额；`None` 表示不修改。
    pub amount: Option<Amount>,
    /// 凭证附件；`None` 表示不修改。
    pub evidence_attachment_id: Option<FileAssetId>,
}

/// 客户退款实体（正式事实，数据模型 §6.11）。
///
/// 财务经办人与复核人不得相同；审核通过并过账后，原事实保留，新增反向分录和
/// 反向核销；退款、冲正和红票之间不相互替代。同一原事实的累计有效冲正不得
/// 超过原金额、原回款或应收与客户归属一致是跨实体约束，由 P3 过账事务校验
/// （§8.3）。状态机见 §7.5。
#[derive(Debug, Serialize, Deserialize, Clone, Entity, PartialEq, Eq)]
pub struct CustomerRefund {
    #[serde(flatten)]
    pub base: BaseModel,
    /// 退款状态。
    pub status: CustomerRefundStatus,
    /// 退款单号。
    pub refund_no: String,
    /// 销售退货/拒收处理单。
    pub sales_return_case_id: Option<SalesReturnCaseId>,
    /// 客户。
    pub customer_id: CustomerAccountId,
    /// 原回款。
    pub original_receipt_id: Option<CustomerReceiptId>,
    /// 原应收分录。
    pub original_receivable_entry_id: Option<ReceivableEntryId>,
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

impl CustomerRefund {
    /// 创建客户退款（初始状态为草稿）。
    ///
    /// 完成编号/原因/经办复核人的 trim/非空/长度校验、金额正数校验、经办人
    /// 与复核人分离校验，以及「原回款或原应收」二选一校验。
    ///
    /// # 参数
    /// * `id` - 实体主键（`entities::ids::CustomerRefundId`）
    /// * `data` - 创建数据
    ///
    /// # 返回
    /// 返回新建的退款实体。
    ///
    /// # 错误
    /// 当编号/原因/经办复核人为空或超长、金额非正、经办与复核人相同、原回款
    /// 与原应收同时或均未提供时返回错误。
    pub fn new(id: CustomerRefundId, data: CustomerRefundData) -> Result<Self> {
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
        validate_original_target(&data.original_receipt_id, &data.original_receivable_entry_id)?;
        Ok(Self {
            base: BaseModel::new(id.to_string()),
            status: CustomerRefundStatus::Draft,
            refund_no,
            sales_return_case_id: data.sales_return_case_id,
            customer_id: data.customer_id,
            original_receipt_id: data.original_receipt_id,
            original_receivable_entry_id: data.original_receivable_entry_id,
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

    /// 更新客户退款草稿。
    ///
    /// 复用 `new` 的校验规则；仅草稿可编辑；退款单号、客户与原事实引用是
    /// 固定字段不允许修改。
    ///
    /// # 参数
    /// * `update` - 更新数据
    ///
    /// # 返回
    /// 更新成功返回 `Ok(())`。
    ///
    /// # 错误
    /// 当状态非草稿或更新字段校验失败时返回错误。
    pub fn update(&mut self, update: CustomerRefundUpdate) -> Result<()> {
        if self.status != CustomerRefundStatus::Draft {
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
    pub fn transition(&mut self, to: CustomerRefundStatus) -> Result<()> {
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
        if self.status != CustomerRefundStatus::Draft {
            return Err(Error::from("只有草稿状态的客户退款单可以提交审批"));
        }
        let next = self
            .approval_subject_version
            .checked_add(1)
            .ok_or_else(|| Error::from("审批提交版本溢出"))?;
        self.approval_subject_version = next;
        self.transition(CustomerRefundStatus::InApproval)?;
        Ok(next)
    }

    /// 撤回审批：回到草稿，且 `approval_subject_version` 不回退。
    ///
    /// # 错误
    /// 非审批中时返回冲突。
    pub fn cancel_approval(&mut self) -> Result<()> {
        if self.status != CustomerRefundStatus::InApproval {
            return Err(Error::from("只有审批中的客户退款单可以撤回审批"));
        }
        self.transition(CustomerRefundStatus::Draft)
    }

    /// 最终通过过账：仅 `IN_APPROVAL` 可进入 `POSTED`。
    ///
    /// # 错误
    /// 状态不是审批中时返回冲突。
    pub fn mark_posted(&mut self) -> Result<()> {
        if self.status != CustomerRefundStatus::InApproval {
            return Err(Error::from("只有审批中的客户退款单可以由最终通过动作过账"));
        }
        self.transition(CustomerRefundStatus::Posted)
    }

    /// 判断退款是否已过账。
    ///
    /// # 返回
    /// 状态为 `Posted` 时返回 `true`。
    pub fn is_posted(&self) -> bool {
        self.status == CustomerRefundStatus::Posted
    }
}

/// 校验财务经办人与复核人分离。
///
/// 规则（数据模型 §6.11 共同不变量）：财务经办人与复核人不得相同。
///
/// # 参数
/// * `handled_by` - 经办人
/// * `reviewed_by` - 复核人
///
/// # 返回
/// 返回规范化后的经办人/复核人。
///
/// # 错误
/// 任一方为空/超长或两者相同时返回错误。
pub(crate) fn validate_actor_pair(handled_by: String, reviewed_by: String) -> Result<(String, String)> {
    let handled_by =
        normalize_required_text(handled_by, "财务经办人不能为空", ACTOR_MAX_LEN, "经办人标识过长")?;
    let reviewed_by =
        normalize_required_text(reviewed_by, "财务复核人不能为空", ACTOR_MAX_LEN, "复核人标识过长")?;
    if handled_by == reviewed_by {
        return Err(Error::from("财务经办人与复核人不得相同"));
    }
    Ok((handled_by, reviewed_by))
}

/// 校验退款原事实二选一。
///
/// 规则（数据模型 §6.11）：退款必须指向「原回款」或「原应收」之一。
///
/// # 参数
/// * `original_receipt_id` - 原回款
/// * `original_receivable_entry_id` - 原应收分录
///
/// # 返回
/// 二选一成立返回 `Ok(())`。
///
/// # 错误
/// 同时或均未提供时返回错误。
pub(crate) fn validate_original_target<T, U>(
    original_receipt_id: &Option<T>,
    original_receivable_entry_id: &Option<U>,
) -> Result<()> {
    match (
        original_receipt_id.is_some(),
        original_receivable_entry_id.is_some(),
    ) {
        (true, true) => Err(Error::from("原回款与原应收只能指向其一")),
        (false, false) => Err(Error::from("退款必须指向原回款或原应收")),
        _ => Ok(()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::money::Amount;
    use std::str::FromStr;

    fn data() -> CustomerRefundData {
        CustomerRefundData {
            refund_no: " RF-2026-001 ".to_string(),
            sales_return_case_id: Some(SalesReturnCaseId::new("src-1")),
            customer_id: CustomerAccountId::new("cust-1"),
            original_receipt_id: Some(CustomerReceiptId::new("cr-1")),
            original_receivable_entry_id: None,
            reason_code: Some(" QUALITY ".to_string()),
            reason_text: " 商品破损退款 ".to_string(),
            amount: Amount::from_str("1000.00").unwrap(),
            handled_by: " handler-1 ".to_string(),
            reviewed_by: " reviewer-1 ".to_string(),
            occurred_at: Instant::from_unix_secs(1_700_000_000),
            evidence_attachment_id: None,
        }
    }

    #[test]
    fn new_trims_text_fields_and_starts_as_draft() {
        let refund = CustomerRefund::new(CustomerRefundId::new("crf-1"), data()).unwrap();

        assert_eq!(refund.refund_no, "RF-2026-001");
        assert_eq!(refund.reason_code.as_deref(), Some("QUALITY"));
        assert_eq!(refund.handled_by, "handler-1");
        assert_eq!(refund.reviewed_by, "reviewer-1");
        assert_eq!(refund.status, CustomerRefundStatus::Draft);
        assert!(!refund.is_posted());
    }

    #[test]
    fn new_rejects_blank_no_same_actor_and_bad_target() {
        let blank_no = CustomerRefundData {
            refund_no: "   ".to_string(),
            ..data()
        };
        assert!(CustomerRefund::new(CustomerRefundId::new("crf-2"), blank_no).is_err());

        let overlong = CustomerRefundData {
            reason_text: "r".repeat(513),
            ..data()
        };
        assert!(CustomerRefund::new(CustomerRefundId::new("crf-3"), overlong).is_err());

        let same_actor = CustomerRefundData {
            reviewed_by: "handler-1".to_string(),
            ..data()
        };
        assert!(CustomerRefund::new(CustomerRefundId::new("crf-4"), same_actor).is_err());

        let no_target = CustomerRefundData {
            original_receipt_id: None,
            ..data()
        };
        assert!(CustomerRefund::new(CustomerRefundId::new("crf-5"), no_target).is_err());

        let both_targets = CustomerRefundData {
            original_receivable_entry_id: Some(ReceivableEntryId::new("re-1")),
            ..data()
        };
        assert!(CustomerRefund::new(CustomerRefundId::new("crf-6"), both_targets).is_err());

        let non_positive = CustomerRefundData {
            amount: Amount::from_str("0.00").unwrap(),
            ..data()
        };
        assert!(CustomerRefund::new(CustomerRefundId::new("crf-7"), non_positive).is_err());
    }

    #[test]
    fn update_applies_draft_changes_and_rejects_posted() {
        let mut refund = CustomerRefund::new(CustomerRefundId::new("crf-1"), data()).unwrap();

        refund
            .update(CustomerRefundUpdate {
                reason_text: Some(" 更换原因 ".to_string()),
                amount: Some(Amount::from_str("800.00").unwrap()),
                ..Default::default()
            })
            .unwrap();
        assert_eq!(refund.reason_text, "更换原因");
        assert_eq!(refund.amount, Amount::from_str("800.00").unwrap());
        assert_eq!(refund.refund_no, "RF-2026-001", "关键字段不改");

        refund.start_approval().unwrap();
        refund.mark_posted().unwrap();
        assert!(refund.is_posted());
        assert!(refund
            .update(CustomerRefundUpdate {
                amount: Some(Amount::from_str("1.00").unwrap()),
                ..Default::default()
            })
            .is_err());
    }

    #[test]
    fn state_machine_forces_in_approval_before_posting() {
        use crate::common::state::ensure_transition as tr;
        use CustomerRefundStatus as S;

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

        let mut refund = CustomerRefund::new(CustomerRefundId::new("crf-1"), data()).unwrap();
        assert!(refund.transition(S::Posted).is_err(), "草稿不得直接过账");
        refund.start_approval().unwrap();
        assert!(refund.mark_posted().is_ok());
    }

    #[test]
    fn start_approval_increments_version_and_cancel_does_not_rollback() {
        let mut refund = CustomerRefund::new(CustomerRefundId::new("crf-1"), data()).unwrap();
        assert_eq!(refund.approval_subject_version, 0);
        let version = refund.start_approval().unwrap();
        assert_eq!(version, 1);
        assert_eq!(refund.status, CustomerRefundStatus::InApproval);
        refund.cancel_approval().unwrap();
        assert_eq!(refund.status, CustomerRefundStatus::Draft);
        assert_eq!(refund.approval_subject_version, 1);
        assert!(refund.mark_posted().is_err());
    }

    #[test]
    fn status_serializes_with_stable_codes_and_labels() {
        assert_eq!(
            serde_json::to_string(&CustomerRefundStatus::InApproval).unwrap(),
            "\"IN_APPROVAL\""
        );
        assert_eq!(CustomerRefundStatus::Posted.label(), "已过账");
        assert_eq!(CustomerRefundStatus::Reversed.as_str(), "reversed");
        assert_eq!(CustomerRefundStatus::Draft.as_str(), "draft");
        let production = include_str!("customer_refund.rs")
            .split("#[cfg(test)]")
            .next()
            .expect("生产代码");
        assert!(!production.contains("PendingReview"));
        assert!(!production.contains("pending_review"));
    }
}
