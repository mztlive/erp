//! 付款申请合同的持久化结构与输入约束。

use std::collections::HashSet;

use entity_core::BaseModel;
use entity_macros::{Entity, id_type};
use erp_core::common::time::BusinessDate;
use erp_core::money::Amount;
use erp_core::validation::normalize_required_text;
use erp_core::{Error, Result};
use serde::{Deserialize, Serialize};
use serde_json::Value;

id_type!(PurchasePaymentRequestId);

/// 付款申请状态；驳回不会自动建立新的审批轮次。
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum PurchasePaymentRequestStatus {
    Draft,
    InApproval,
    Rejected,
    Withdrawn,
    Approved,
    PartiallyPaid,
    Paid,
    Closed,
}

impl PurchasePaymentRequestStatus {
    /// 返回稳定状态代码。
    ///
    /// # 参数
    /// 无。
    /// # 返回
    /// 返回持久化与接口共用的状态代码。
    /// # 错误
    /// 不返回错误。
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Draft => "draft",
            Self::InApproval => "in_approval",
            Self::Rejected => "rejected",
            Self::Withdrawn => "withdrawn",
            Self::Approved => "approved",
            Self::PartiallyPaid => "partially_paid",
            Self::Paid => "paid",
            Self::Closed => "closed",
        }
    }

    /// 返回经办人可读的状态名称。
    ///
    /// # 参数
    /// 无。
    /// # 返回
    /// 返回申请审批和实际付款分别表达的状态名称。
    /// # 错误
    /// 不返回错误。
    pub fn label(self) -> &'static str {
        match self {
            Self::Draft => "草稿",
            Self::InApproval => "审批中",
            Self::Rejected => "已驳回",
            Self::Withdrawn => "已撤回",
            Self::Approved => "待付款",
            Self::PartiallyPaid => "部分付款",
            Self::Paid => "已付清",
            Self::Closed => "已关闭",
        }
    }

    /// 判断原批准付款授权是否允许显式终止。
    ///
    /// # 参数
    /// 无。
    /// # 返回
    /// 待付款、部分付款和已付清允许关闭；已付清关闭不释放额度。
    /// # 错误
    /// 不返回错误。
    pub fn can_close(self) -> bool {
        matches!(self, Self::Approved | Self::PartiallyPaid | Self::Paid)
    }

    pub(super) fn occupies(self) -> bool {
        matches!(self, Self::InApproval | Self::Approved | Self::PartiallyPaid)
    }
}

/// 采购明确填写的逐单申请金额。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct PaymentRequestLineData {
    pub payable_account_id: String,
    pub amount: Amount,
}

/// 申请表单；主体及当前银行默认账户不能替代正式来源和实际选定账户。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct PurchasePaymentRequestData {
    pub supplier_id: String,
    pub payee_bank_account_id: String,
    pub lines: Vec<PaymentRequestLineData>,
    pub desired_payment_date: BusinessDate,
    pub reason: String,
    pub attachment_ids: Vec<String>,
}

impl PurchasePaymentRequestData {
    /// 规范化申请表单并拒绝重复采购和非正金额。
    ///
    /// # 参数
    /// 无。
    /// # 返回
    /// 返回已规范化且有界的表单。
    /// # 错误
    /// 缺少必要身份、说明、金额、重复采购或附件超限时返回错误。
    pub fn normalize(mut self) -> Result<Self> {
        self.supplier_id = required(self.supplier_id, "供应商", 128)?;
        self.payee_bank_account_id = required(self.payee_bank_account_id, "收款账户", 128)?;
        self.reason = required(self.reason, "申请说明", 2000)?;
        if self.lines.is_empty() || self.lines.len() > 100 {
            return Err(Error::from("付款申请须包含 1 至 100 张采购单"));
        }
        let mut purchases = HashSet::new();
        for line in &mut self.lines {
            line.payable_account_id = required(line.payable_account_id.clone(), "采购应付", 128)?;
            positive(line.amount)?;
            if !purchases.insert(line.payable_account_id.clone()) {
                return Err(Error::from("同一申请不得重复选择采购单"));
            }
        }
        if self.attachment_ids.len() > 50 {
            return Err(Error::from("付款申请附件不得超过 50 个"));
        }
        for id in &mut self.attachment_ids {
            *id = required(id.clone(), "附件身份", 128)?;
        }
        self.attachment_ids.sort();
        self.attachment_ids.dedup();
        Ok(self)
    }
}

/// 本次选定账户的审核快照，只保存脱敏账号。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PaymentRequestAccountSnapshot {
    pub account_id: String,
    pub version: u64,
    pub supplier_party_id: String,
    pub account_name: String,
    pub bank_name: String,
    pub masked_account_number: String,
}

/// 精确关联的正式销售身份和完整只读预览。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PaymentRequestSalesSource {
    pub sales_order_id: String,
    pub sales_revision_id: String,
    pub customer_id: String,
    pub payer_party_id: String,
    pub payer_party_name: String,
    pub preview: Value,
}

/// 提交时冻结的采购、销售及允许材料，不允许从后续草稿补读。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PaymentRequestSourceSnapshot {
    pub purchase_preview: Value,
    pub sales_sources: Vec<PaymentRequestSalesSource>,
    pub allowed_attachment_ids: Vec<String>,
}

/// 逐采购来源的申请、净已付和已关闭金额。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PurchasePaymentRequestLine {
    pub payable_account_id: String,
    pub purchase_order_id: String,
    pub purchase_revision_id: String,
    pub requested_amount: Amount,
    pub paid_amount: Amount,
    pub closed_amount: Amount,
    pub gross_total: Amount,
    pub settled_total: Amount,
    pub open_total: Amount,
    pub other_reserved_total: Amount,
    pub payment_terms: Value,
    pub source_snapshot: PaymentRequestSourceSnapshot,
}

impl PurchasePaymentRequestLine {
    /// 返回该行尚未支付且尚未关闭的批准金额。
    ///
    /// # 参数
    /// 无。
    /// # 返回
    /// 返回申请金额减去净已付及关闭金额。
    /// # 错误
    /// 不返回错误；实体构造和生命周期校验保持非负。
    pub fn remaining(&self) -> Amount {
        self.requested_amount.checked_sub(self.paid_amount).checked_sub(self.closed_amount)
    }
}

/// 服务端证明并冻结的完整来源。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PaymentRequestSubmission {
    pub supplier_id: String,
    pub payer_party_id: String,
    pub payer_party_name: String,
    pub org_unit_id: Option<String>,
    pub payee_account: PaymentRequestAccountSnapshot,
    pub lines: Vec<PurchasePaymentRequestLine>,
}

/// 实际付款或冲正按申请采购行分配；不得移动逐行批准额度。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PaymentRequestAllocation {
    pub payable_account_id: String,
    pub amount: Amount,
}

/// 采购付款申请实体，审批批准不产生付款事实。
#[derive(Debug, Clone, Serialize, Deserialize, Entity, PartialEq, Eq)]
pub struct PurchasePaymentRequest {
    #[serde(flatten)]
    pub base: BaseModel,
    pub request_no: String,
    pub status: PurchasePaymentRequestStatus,
    pub supplier_id: String,
    pub payer_party_id: String,
    pub payer_party_name: String,
    pub payee_account: Option<PaymentRequestAccountSnapshot>,
    pub org_unit_id: Option<String>,
    pub created_by: String,
    pub data: PurchasePaymentRequestData,
    pub lines: Vec<PurchasePaymentRequestLine>,
    pub total_amount: Amount,
    pub submission_version: u32,
    pub approval_subject_version: u32,
    pub approval_instance_id: Option<String>,
    pub work_item_id: Option<String>,
    pub last_reason: Option<String>,
}

pub(super) fn required(value: String, label: &str, max: usize) -> Result<String> {
    normalize_required_text(value, &format!("{label}不能为空"), max, &format!("{label}过长"))
}

pub(super) fn positive(amount: Amount) -> Result<()> {
    if amount <= Amount::zero() {
        return Err(Error::from("逐采购申请或付款金额必须大于零"));
    }
    Ok(())
}

pub(super) fn sum(values: impl IntoIterator<Item = Amount>) -> Result<Amount> {
    values.into_iter().try_fold(Amount::zero(), |total, value| {
        let decimal = total
            .to_decimal()
            .checked_add(value.to_decimal())
            .ok_or_else(|| Error::from("付款申请金额汇总溢出"))?;
        Amount::try_from(decimal)
    })
}
