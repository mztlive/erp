//! 销项开票申请与授权额度。申请批准不形成发票或增加已开票金额。
use entity_core::BaseModel;
use entity_macros::Entity;
use erp_core::ids::{CustomerAccountId, PartyId, ReceivableAccountId, SalesInvoiceRequestId, SalesOrderId};
use erp_core::money::Amount;
use erp_core::validation::normalize_required_text;
use erp_core::{Error, Result};
use serde::{Deserialize, Serialize};

/// 申请业务状态；审批驳回沿流程运行，业务撤回回到草稿。
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum InvoiceRequestStatus {
    Draft,
    InApproval,
    Approved,
    Completed,
}
impl InvoiceRequestStatus {
    /// 返回稳定状态代码。
    ///
    /// # 参数
    /// 无。
    ///
    /// # 返回
    /// `Draft`、`InApproval`、`Approved`、`Completed` 分别返回
    /// `draft`、`in_approval`、`approved`、`completed`。
    ///
    /// # 错误
    /// 不返回错误。
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Draft => "draft",
            Self::InApproval => "in_approval",
            Self::Approved => "approved",
            Self::Completed => "completed",
        }
    }
    /// 返回面向经办人的状态名称。
    ///
    /// # 参数
    /// 无。
    ///
    /// # 返回
    /// 返回「草稿」「审批中」「待开票」或「已开票」。
    ///
    /// # 错误
    /// 不返回错误。
    pub fn label(self) -> &'static str {
        match self {
            Self::Draft => "草稿",
            Self::InApproval => "审批中",
            Self::Approved => "待开票",
            Self::Completed => "已开票",
        }
    }
}

/// 经办人填写并随审批提交冻结的开票要求。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct InvoiceRequestData {
    pub amount: Amount,
    pub invoice_title: String,
    pub tax_number: String,
    pub invoice_content: String,
    pub reason: String,
}
impl InvoiceRequestData {
    /// 规范化必填开票资料；金额必须为正且精确到分。
    ///
    /// # 参数
    /// 无。
    ///
    /// # 返回
    /// 返回裁剪空白后的开票资料。
    ///
    /// # 错误
    /// 金额非正或超过两位小数，或抬头、税号、开票内容、申请事由为空或超长时返回错误。
    pub fn normalize(mut self) -> Result<Self> {
        if self.amount <= Amount::zero()
            || erp_core::money::round_to_cent(self.amount.to_decimal()) != self.amount.to_decimal()
        {
            return Err(Error::from("申请金额必须大于零且最多两位小数"));
        }
        self.invoice_title = required(self.invoice_title, "开票抬头", 256)?;
        self.tax_number = required(self.tax_number, "税号", 64)?;
        self.invoice_content = required(self.invoice_content, "开票内容", 1000)?;
        self.reason = required(self.reason, "申请事由", 1000)?;
        Ok(self)
    }
}

/// 一张申请固定关联一笔销售应收；已开票额度不因红冲恢复原授权。
#[derive(Debug, Clone, Serialize, Deserialize, Entity, PartialEq, Eq)]
pub struct SalesInvoiceRequest {
    #[serde(flatten)]
    pub base: BaseModel,
    pub request_no: String,
    pub receivable_account_id: ReceivableAccountId,
    pub sales_order_id: SalesOrderId,
    pub customer_id: CustomerAccountId,
    pub counterparty_party_id: PartyId,
    pub created_by: String,
    pub status: InvoiceRequestStatus,
    pub approval_subject_version: u32,
    pub data: InvoiceRequestData,
    pub invoiced_amount: Amount,
    pub work_item_id: Option<String>,
}
impl SalesInvoiceRequest {
    /// 创建固定销售应收来源的申请草稿。
    ///
    /// # 参数
    /// * `id` - 申请标识，同时用于生成申请单号
    /// * `account` - 来源销售应收子账
    /// * `data` - 开票资料，构造时再做规范化
    /// * `actor` - 申请人
    ///
    /// # 返回
    /// 返回状态为 `Draft`、已开票金额为零、审批版本为 0 的申请。
    ///
    /// # 错误
    /// 开票资料无效，或申请人为空或超过 128 个字符时返回错误。
    pub fn new(
        id: SalesInvoiceRequestId,
        account: &super::ReceivableAccount,
        data: InvoiceRequestData,
        actor: &str,
    ) -> Result<Self> {
        let request_no = format!("KP-{}", id.as_ref());
        Ok(Self {
            base: BaseModel::new(id.to_string()),
            request_no,
            receivable_account_id: ReceivableAccountId::new(account.base.id.clone()),
            sales_order_id: account.sales_order_id.clone(),
            customer_id: account.customer_id.clone(),
            counterparty_party_id: account.counterparty_party_id.clone(),
            created_by: required(actor.to_owned(), "申请人", 128)?,
            status: InvoiceRequestStatus::Draft,
            approval_subject_version: 0,
            data: data.normalize()?,
            invoiced_amount: Amount::zero(),
            work_item_id: None,
        })
    }
    /// 返回尚未执行的本次授权或申请额度。
    ///
    /// # 参数
    /// 无。
    ///
    /// # 返回
    /// 返回申请金额减去 `invoiced_amount` 的精确差。
    ///
    /// # 错误
    /// 不返回错误。
    pub fn remaining(&self) -> Amount {
        self.data.amount.checked_sub(self.invoiced_amount)
    }
    /// 返回占用的额度。草稿和已完成申请不占用未开票额度。
    ///
    /// # 参数
    /// 无。
    ///
    /// # 返回
    /// `InApproval` 或 `Approved` 时返回 [`Self::remaining`]，其余状态返回零。
    ///
    /// # 错误
    /// 不返回错误。
    pub fn reserved(&self) -> Amount {
        match self.status {
            InvoiceRequestStatus::InApproval | InvoiceRequestStatus::Approved => self.remaining(),
            _ => Amount::zero(),
        }
    }
    /// 提交申请并冻结新审批版本。
    ///
    /// # 参数
    /// * `available` - 当前可申请开票金额
    ///
    /// # 返回
    /// 无返回值。成功时状态变为 `InApproval`，且 `approval_subject_version` 加一。
    ///
    /// # 错误
    /// 非草稿、申请金额超过 `available` 或版本溢出时返回错误，且不改变实体。
    pub fn submit(&mut self, available: Amount) -> Result<()> {
        if self.status != InvoiceRequestStatus::Draft {
            return Err(Error::from("只有草稿可以提交开票申请"));
        }
        if self.data.amount > available {
            return Err(Error::from("可申请开票金额不足，请刷新后调整申请金额"));
        }
        let version =
            self.approval_subject_version.checked_add(1).ok_or_else(|| Error::from("审批版本已达上限"))?;
        self.approval_subject_version = version;
        self.status = InvoiceRequestStatus::InApproval;
        Ok(())
    }
    /// 最终审批通过，仅授予开票额度。
    ///
    /// # 参数
    /// 无。
    ///
    /// # 返回
    /// 无返回值。成功时状态变为 `Approved`。
    ///
    /// # 错误
    /// 状态不是 `InApproval` 时返回错误。
    pub fn approve(&mut self) -> Result<()> {
        if self.status != InvoiceRequestStatus::InApproval {
            return Err(Error::from("开票申请不在审批中"));
        }
        self.status = InvoiceRequestStatus::Approved;
        Ok(())
    }
    /// 撤回审批并释放额度，提交版本保持递增。
    ///
    /// # 参数
    /// 无。
    ///
    /// # 返回
    /// 无返回值。成功时状态回到 `Draft`，`approval_subject_version` 不回退。
    ///
    /// # 错误
    /// 状态不是 `InApproval` 时返回错误。
    pub fn cancel_approval(&mut self) -> Result<()> {
        if self.status != InvoiceRequestStatus::InApproval {
            return Err(Error::from("只有审批中的开票申请可以撤回"));
        }
        self.status = InvoiceRequestStatus::Draft;
        Ok(())
    }
    /// 消耗实际登记发票的授权金额；红冲不恢复原申请授权。
    ///
    /// # 参数
    /// * `amount` - 本次登记的开票金额
    ///
    /// # 返回
    /// 无返回值。成功时累加 `invoiced_amount`；剩余额度为零时状态变为 `Completed`。
    ///
    /// # 错误
    /// 状态不是 `Approved`、金额非正或超过剩余授权时返回错误。
    pub fn record_invoice(&mut self, amount: Amount) -> Result<()> {
        if self.status != InvoiceRequestStatus::Approved {
            return Err(Error::from("开票申请尚未批准或已完成"));
        }
        if amount <= Amount::zero() || amount > self.remaining() {
            return Err(Error::from("本次开票金额超过申请剩余批准金额"));
        }
        self.invoiced_amount = self.invoiced_amount.checked_add(amount);
        if self.remaining() == Amount::zero() {
            self.status = InvoiceRequestStatus::Completed;
        }
        Ok(())
    }
}
/// 统一规范化申请资料，不允许空白和无界文本。
fn required(value: String, label: &str, max: usize) -> Result<String> {
    normalize_required_text(value, &format!("{label}不能为空"), max, &format!("{label}过长"))
}

#[cfg(test)]
mod tests;
