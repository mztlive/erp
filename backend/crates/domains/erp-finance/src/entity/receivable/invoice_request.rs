//! 销项开票申请与授权额度。申请批准不形成发票或增加已开票金额。
use entity_core::BaseModel;
use entity_macros::Entity;
use erp_core::ids::{CustomerAccountId, PartyId, ReceivableAccountId, SalesInvoiceRequestId, SalesOrderId};
use erp_core::money::{Amount, Quantity, UnitPrice, round_to_cent};
use erp_core::validation::{normalize_optional_text, normalize_required_text};
use erp_core::{Error, Result};
use serde::{Deserialize, Serialize};

/// 申请业务状态；审批驳回沿流程运行，业务撤回回到草稿。
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum InvoiceRequestStatus {
    Draft,
    InApproval,
    Rejected,
    Approved,
    Completed,
}
impl InvoiceRequestStatus {
    /// 将开票申请状态转成用于持久化和查询的稳定代码，与中文展示文案分开。
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Draft => "draft",
            Self::InApproval => "in_approval",
            Self::Rejected => "rejected",
            Self::Approved => "approved",
            Self::Completed => "completed",
        }
    }
    /// 为界面或响应展示提供开票申请状态的中文名称；机器筛选和存储不使用该名称。
    pub fn label(self) -> &'static str {
        match self {
            Self::Draft => "草稿",
            Self::InApproval => "审批中",
            Self::Rejected => "已驳回",
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
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub seller_name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub seller_tax_number: Option<String>,
    pub invoice_content: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub lines: Vec<InvoiceRequestLine>,
    pub reason: String,
}

/// 一条拟开票内容，金额按含税单价乘数量后舍入到分。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct InvoiceRequestLine {
    pub content: String,
    pub quantity: Quantity,
    pub unit_price: UnitPrice,
}

impl InvoiceRequestLine {
    /// 返回本行含税金额；超出定点数值范围时拒绝。
    pub fn gross_amount(&self) -> Result<Amount> {
        let amount = self
            .quantity
            .to_decimal()
            .checked_mul(self.unit_price.to_decimal())
            .ok_or_else(|| Error::from("开票明细金额超出可用范围"))?;
        Amount::try_from(round_to_cent(amount))
    }
}

impl InvoiceRequestData {
    /// 规范化必填开票资料；金额必须为正且精确到分。
    /// 返回裁剪空白后的开票资料。
    /// 金额非正或超过两位小数，或抬头、税号、开票内容、申请事由为空或超长时返回错误。
    pub fn normalize(mut self) -> Result<Self> {
        if self.amount <= Amount::zero()
            || erp_core::money::round_to_cent(self.amount.to_decimal()) != self.amount.to_decimal()
        {
            return Err(Error::from("申请金额必须大于零且最多两位小数"));
        }
        self.invoice_title = required(self.invoice_title, "开票抬头", 256)?;
        self.tax_number = required(self.tax_number, "税号", 64)?;
        self.seller_name = normalize_optional_text(self.seller_name, "销方名称", 256)?;
        self.seller_tax_number = normalize_optional_text(self.seller_tax_number, "销方纳税人识别号", 64)?;
        if self.lines.is_empty() {
            self.invoice_content = required(self.invoice_content, "开票内容", 1000)?;
        } else {
            validate_lines(&mut self.lines, self.amount)?;
            self.invoice_content = self.lines[0].content.clone();
        }
        self.reason = required(self.reason, "申请事由", 1000)?;
        Ok(self)
    }
}

fn validate_lines(lines: &mut [InvoiceRequestLine], amount: Amount) -> Result<()> {
    if lines.len() > 100 {
        return Err(Error::from("一张申请最多填写 100 条开票明细"));
    }
    let mut total = Amount::zero().to_decimal();
    for line in lines {
        line.content = required(std::mem::take(&mut line.content), "开票内容", 1000)?;
        if line.quantity.to_decimal() <= Amount::zero().to_decimal()
            || line.unit_price.to_decimal() <= Amount::zero().to_decimal()
        {
            return Err(Error::from("开票明细的数量和含税单价必须大于零"));
        }
        let gross = line.gross_amount()?;
        if gross <= Amount::zero() {
            return Err(Error::from("开票明细金额舍入到分后必须大于零"));
        }
        total =
            total.checked_add(gross.to_decimal()).ok_or_else(|| Error::from("开票明细合计超出可用范围"))?;
    }
    if total != amount.to_decimal() {
        return Err(Error::from("开票明细含税合计必须等于本次申请金额"));
    }
    Ok(())
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
    /// 销售针对固定应收来源填写开票资料时创建草稿；申请未提交前不占用开票额度。
    /// 返回状态为 `Draft`、已开票金额为零、审批版本为 0 的申请。
    /// 开票资料无效，或申请人为空或超过 128 个字符时返回错误。
    ///
    /// 输入约定：
    /// - `id`：申请标识，同时用于生成申请单号
    /// - `account`：来源销售应收子账
    /// - `data`：开票资料，构造时再做规范化
    /// - `actor`：申请人
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
    /// 保存草稿或驳回后的开票资料，保留当前业务状态及审批版本，等待销售另行提交。
    /// 成功时替换开票资料，不占用额度。
    /// 失败条件：不可编辑状态或资料非法时返回错误且不改变实体。
    ///
    /// 输入约定：
    /// - `data`：待规范化的完整开票资料。
    pub fn update(&mut self, data: InvoiceRequestData) -> Result<()> {
        if !matches!(self.status, InvoiceRequestStatus::Draft | InvoiceRequestStatus::Rejected) {
            return Err(Error::from("只有草稿或已驳回的开票申请可以修改"));
        }
        self.data = data.normalize()?;
        Ok(())
    }
    /// 计算当前申请还可执行的授权或申请金额，供开票及额度展示使用。
    /// 返回申请金额减去 `invoiced_amount` 的精确差。
    pub fn remaining(&self) -> Amount {
        self.data.amount.checked_sub(self.invoiced_amount)
    }
    /// 分配应收可开票余额时计算申请实际占用额度，草稿和完成件不占额度。
    /// 结果：`InApproval` 或 `Approved` 时返回 [`Self::remaining`]，其余状态返回零。
    pub fn reserved(&self) -> Amount {
        match self.status {
            InvoiceRequestStatus::InApproval | InvoiceRequestStatus::Approved => self.remaining(),
            _ => Amount::zero(),
        }
    }
    /// 申请进入开票审批时冻结本轮新版本，不以原草稿状态占用审批额度。
    /// 结果：无返回值。成功时状态变为 `InApproval`，且 `approval_subject_version` 加一。
    /// 失败条件：非草稿、申请金额超过 `available` 或版本溢出时返回错误，且不改变实体。
    ///
    /// 输入约定：
    /// - `available`：当前可申请开票金额
    pub fn submit(&mut self, available: Amount) -> Result<()> {
        if !matches!(self.status, InvoiceRequestStatus::Draft | InvoiceRequestStatus::Rejected) {
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
    /// 开票的最终审批通过时仅授予本轮额度，不视作发票已经登记。
    /// 结果：无返回值。成功时状态变为 `Approved`。
    /// 状态不是 `InApproval` 时返回错误。
    pub fn approve(&mut self) -> Result<()> {
        if self.status != InvoiceRequestStatus::InApproval {
            return Err(Error::from("开票申请不在审批中"));
        }
        self.status = InvoiceRequestStatus::Approved;
        Ok(())
    }
    /// 撤回开票审批时使本轮不再占用额度，仍保留提交版本供下次递增。
    /// 结果：无返回值。成功时状态回到 `Draft`，`approval_subject_version` 不回退。
    /// 状态不是 `InApproval` 时返回错误。
    pub fn cancel_approval(&mut self) -> Result<()> {
        if self.status != InvoiceRequestStatus::InApproval {
            return Err(Error::from("只有审批中的开票申请可以撤回"));
        }
        self.status = InvoiceRequestStatus::Draft;
        Ok(())
    }
    /// 当前开票申请被驳回后释放审批额度，保留该轮历史资料与版本以便追溯。
    /// 结果：状态变为已驳回，`reserved` 返回零。
    /// 失败条件：非审批中状态拒绝，禁止重复释放及撤销已批准事实。
    pub fn reject(&mut self) -> Result<()> {
        if self.status != InvoiceRequestStatus::InApproval {
            return Err(Error::from("只有审批中的开票申请可以驳回"));
        }
        self.status = InvoiceRequestStatus::Rejected;
        Ok(())
    }

    /// 以附件确认已开票时读取须完成的剩余批准含税额度，供登记使用。
    /// 结果：当前已批准申请的正数剩余额度。
    /// 版本变化、尚未批准、已完成或额度无效时拒绝。
    ///
    /// 输入约定：用户核对附件时的申请版本。
    pub fn completion_amount(&self, expected_version: u64) -> Result<Amount> {
        if self.base.version != expected_version {
            return Err(Error::from("开票申请额度已变化，请刷新后重新核对"));
        }
        if self.status != InvoiceRequestStatus::Approved || self.remaining() <= Amount::zero() {
            return Err(Error::from("开票申请尚未批准或已完成"));
        }
        Ok(self.remaining())
    }

    /// 正式登记发票时扣减本次授权剩余金额，后续红冲不恢复原申请的开票授权。
    /// 结果：无返回值。成功时累加 `invoiced_amount`；剩余额度为零时状态变为 `Completed`。
    /// 状态不是 `Approved`、金额非正或超过剩余授权时返回错误。
    ///
    /// 输入约定：
    /// - `amount`：本次登记的开票金额
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
// 冻结开票申请资料前裁剪必填文本边界，拒绝只含空白或超出长度限制的值。
fn required(value: String, label: &str, max: usize) -> Result<String> {
    normalize_required_text(value, &format!("{label}不能为空"), max, &format!("{label}过长"))
}

#[cfg(test)]
mod tests;
