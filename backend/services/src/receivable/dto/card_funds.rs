//! W13 卡券票款复核与历史票款登记 DTO。

use erp_core::common::time::{BusinessDate, Instant};
use erp_core::ids::{FileAssetId, ReceivableAccountId, WorkItemId};
use erp_core::money::Amount;
use serde::{Deserialize, Serialize};
use validator::Validate;

use application_core::non_blank;

/// W13 当前应收账户关联的正式回款事实投影。
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct ReceivableReceiptFactView {
    /// 回款单 ID。
    pub receipt_id: String,
    /// 回款单号。
    pub receipt_no: String,
    /// 实际到账时间（RFC 3339）。
    pub received_at: String,
    /// 回款含税金额。
    pub gross_amount: Amount,
    /// 当前应收账户的净核销金额。
    pub allocated_to_account: Amount,
    /// 分配到其它账户的说明；当前投影无法完整证明时为空。
    pub other_allocation_summary: Option<String>,
    /// 回款单是否已经冲正。
    pub reversed: bool,
}

/// W13 当前应收账户关联的正式销项发票事实投影。
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct ReceivableInvoiceFactView {
    /// 发票 ID。
    pub invoice_id: String,
    /// 发票号码。
    pub invoice_no: String,
    /// 蓝票或红票稳定代码（`BLUE` / `RED`）。
    pub direction: String,
    /// 开票业务日期（`YYYY-MM-DD`）。
    pub issued_at: String,
    /// 含税金额。
    pub gross_amount: Amount,
    /// 不含税金额。
    pub net_amount: Amount,
    /// 税额。
    pub tax_amount: Amount,
    /// 当前应收账户的净分配含税金额。
    pub allocated_to_account: Amount,
    /// 当前发票是否已被红冲。
    pub reversed: bool,
}

/// W13 详情查询参数。
#[derive(Debug, Clone, Default, Deserialize)]
pub struct CardFundsReviewDetailParams {
    /// 从正式待办进入时必须携带的任务 ID。
    pub work_item_id: Option<String>,
}

/// W13 卡券票款强类型领域动作。
#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum CardFundsReviewAllowedAction {
    /// 期初且确无历史票款事实时从零起算。
    ConfirmZero,
    /// 核对已登记票款事实后通过。
    Approve,
    /// 驳回当前复核。
    Reject,
    /// 进入正式回款登记。
    RegisterReceipt,
    /// 进入正式销项发票登记。
    RegisterInvoice,
}

impl CardFundsReviewAllowedAction {
    /// 返回稳定动作代码。
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::ConfirmZero => "CONFIRM_ZERO",
            Self::Approve => "APPROVE",
            Self::Reject => "REJECT",
            Self::RegisterReceipt => "REGISTER_RECEIPT",
            Self::RegisterInvoice => "REGISTER_INVOICE",
        }
    }
}

/// W13 单个领域动作阻断事实。
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct CardFundsReviewActionBlockerView {
    /// 被阻断的 W13 动作。
    pub action: String,
    /// 稳定阻断码。
    pub code: String,
    /// 面向当前处理人的安全说明。
    pub message: String,
}

/// W13 历史票款登记的账户分配意图。
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
#[serde(deny_unknown_fields)]
pub struct CardFundsRegistrationAllocation {
    /// 目标应收子账；当前命令只允许正式任务关联的账户。
    pub target_account_id: ReceivableAccountId,
    /// 分配含税金额。
    pub amount: Amount,
}

/// W13 历史回款原子登记命令。
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
#[serde(deny_unknown_fields)]
pub struct RegisterCardFundsReceiptRequest {
    /// 当前开放复核任务。
    pub work_item_id: WorkItemId,
    /// 当前任务乐观锁版本。
    #[validate(custom(function = "non_blank", message = "任务版本不能为空"))]
    pub expected_task_version: String,
    /// 任务冻结的销售版本。
    #[validate(custom(function = "non_blank", message = "对象版本不能为空"))]
    pub expected_subject_version: String,
    /// 提交前的票款事实版本。
    #[validate(custom(function = "non_blank", message = "票款事实版本不能为空"))]
    pub expected_funds_fact_version: String,
    /// 回款单号；为空时由幂等键生成稳定号码。
    pub receipt_no: Option<String>,
    /// 实际到账时间。
    pub received_at: Instant,
    /// 回款含税金额。
    pub gross_amount: Amount,
    /// 目标账户分配；合计必须等于回款金额。
    #[validate(length(min = 1, message = "至少提供一条回款分配"), nested)]
    pub allocations: Vec<CardFundsRegistrationAllocation>,
    /// 银行流水或证据引用。
    #[validate(custom(function = "non_blank", message = "回款证据不能为空"))]
    pub evidence_reference: String,
    /// 业务请求幂等键。
    #[validate(length(min = 1, max = 128, message = "幂等键不能为空"))]
    pub idempotency_key: String,
}

/// W13 历史销项发票原子登记命令。
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
#[serde(deny_unknown_fields)]
pub struct RegisterCardFundsInvoiceRequest {
    /// 当前开放复核任务。
    pub work_item_id: WorkItemId,
    /// 当前任务乐观锁版本。
    #[validate(custom(function = "non_blank", message = "任务版本不能为空"))]
    pub expected_task_version: String,
    /// 任务冻结的销售版本。
    #[validate(custom(function = "non_blank", message = "对象版本不能为空"))]
    pub expected_subject_version: String,
    /// 提交前的票款事实版本。
    #[validate(custom(function = "non_blank", message = "票款事实版本不能为空"))]
    pub expected_funds_fact_version: String,
    /// 发票号码；为空时由幂等键生成稳定号码。
    pub invoice_no: Option<String>,
    /// 开票业务日期。
    pub invoice_date: BusinessDate,
    /// 含税金额。
    pub gross_amount: Amount,
    /// 不含税金额。
    pub net_amount: Amount,
    /// 税额。
    pub tax_amount: Amount,
    /// 目标账户分配；合计必须等于发票含税金额。
    #[validate(length(min = 1, message = "至少提供一条发票分配"), nested)]
    pub allocations: Vec<CardFundsRegistrationAllocation>,
    /// 发票证据引用。
    #[validate(custom(function = "non_blank", message = "发票证据不能为空"))]
    pub evidence_reference: String,
    /// 业务请求幂等键。
    #[validate(length(min = 1, max = 128, message = "幂等键不能为空"))]
    pub idempotency_key: String,
}

/// W13 历史票款原子登记结果。
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct CardFundsRegistrationResult {
    /// 登记后的票款事实版本。
    pub funds_fact_version: String,
    /// 登记后的账户领域版本。
    pub subject_hash: String,
    /// 净已收金额。
    pub settled_total: Amount,
    /// 净已开票金额。
    pub invoiced_total: Amount,
    /// 剩余开放金额。
    pub open_total: Amount,
    /// 剩余可开票金额。
    pub open_invoiceable_total: Amount,
    /// 本次登记的回款事实。
    pub receipt_facts: Vec<ReceivableReceiptFactView>,
    /// 本次登记的发票事实。
    pub invoice_facts: Vec<ReceivableInvoiceFactView>,
}

// ---------------------------------------------------------------------------
// 卡券票款复核（receivable_funds_review，W13）
// ---------------------------------------------------------------------------

/// W13 正式复核类型（HTTP 稳定代码）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum CardFundsReviewType {
    /// 卡券期初票款复核。
    Opening,
    /// 商城同步差额复核。
    SyncDelta,
}

/// W13 正式复核结果（HTTP 稳定代码）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum CardFundsReviewResult {
    /// 通过。
    Approved,
    /// 驳回。
    Rejected,
}

#[cfg(test)]
impl CardFundsReviewResult {
    /// 返回 HTTP 稳定代码。
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::Approved => "APPROVED",
            Self::Rejected => "REJECTED",
        }
    }
}

/// W13 正式复核结论（与结果组合受 Service 严格校验）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum CardFundsReviewConclusion {
    /// 已核实不存在上线前历史票款，从零起算。
    NoHistoryFromZero,
    /// 已登记正式票款事实且核对一致。
    RecordedFactsReconciled,
    /// 驳回。
    Rejected,
}

#[cfg(test)]
impl CardFundsReviewConclusion {
    /// 返回 HTTP 稳定代码。
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::NoHistoryFromZero => "NO_HISTORY_FROM_ZERO",
            Self::RecordedFactsReconciled => "RECORDED_FACTS_RECONCILED",
            Self::Rejected => "REJECTED",
        }
    }
}

/// W13 强类型正式决定；所有领域版本均位于 `decision` 内。
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
#[serde(deny_unknown_fields)]
pub struct CardFundsReviewDecision {
    /// 应收往来子账。
    pub receivable_account_id: ReceivableAccountId,
    /// 期望子账序号。
    #[validate(range(min = 1, message = "往来子账序号必须从 1 开始"))]
    pub expected_account_seq: u32,
    /// 期望账户领域版本（服务端不透明字符串）。
    #[validate(custom(function = "non_blank", message = "账户领域版本不能为空"))]
    #[validate(length(max = 128, message = "账户领域版本不能超过 128 个字符"))]
    pub expected_account_domain_version: String,
    /// 期望复核链尾；空链必须省略。
    #[validate(length(max = 128, message = "复核链尾不能超过 128 个字符"))]
    pub expected_review_chain_tail_id: Option<String>,
    /// 期望复核链版本（服务端不透明字符串）。
    #[validate(custom(function = "non_blank", message = "复核链版本不能为空"))]
    #[validate(length(max = 128, message = "复核链版本不能超过 128 个字符"))]
    pub expected_review_chain_version: String,
    /// 期望下一复核号。
    #[validate(range(min = 1, message = "下一复核号必须从 1 开始"))]
    pub expected_next_review_no: u32,
    /// 期望当前销售版本。
    #[validate(custom(function = "non_blank", message = "销售版本不能为空"))]
    #[validate(length(max = 128, message = "销售版本不能超过 128 个字符"))]
    pub expected_sales_order_revision_id: String,
    /// 期望票款事实版本（服务端不透明字符串）。
    #[validate(custom(function = "non_blank", message = "票款事实版本不能为空"))]
    #[validate(length(max = 128, message = "票款事实版本不能超过 128 个字符"))]
    pub expected_funds_fact_version: String,
    /// 复核类型。
    pub review_type: CardFundsReviewType,
    /// 复核结果。
    pub review_result: CardFundsReviewResult,
    /// 复核结论。
    pub conclusion: CardFundsReviewConclusion,
    /// 受控证据文件。
    #[validate(length(max = 20, message = "证据文件最多 20 个"))]
    pub evidence_document_ids: Vec<FileAssetId>,
    /// 受控证据引用。
    #[validate(length(max = 20, message = "证据引用最多 20 条"))]
    pub evidence_references: Vec<String>,
    /// 补充说明。
    #[validate(length(max = 512, message = "复核说明不能超过 512 个字符"))]
    pub comment: Option<String>,
    /// 驳回原因代码；仅 `REJECTED` 必填。
    #[validate(length(max = 64, message = "驳回原因代码不能超过 64 个字符"))]
    pub reason_code: Option<String>,
}

/// W13 唯一正式复核命令。
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
#[serde(deny_unknown_fields)]
pub struct CompleteCardFundsReviewCommand {
    /// 当前正式任务。
    pub work_item_id: WorkItemId,
    /// 查询所得任务版本；HTTP 接受不透明字符串，由 Service 严格解析。
    #[validate(custom(function = "non_blank", message = "任务版本不能为空"))]
    #[validate(length(max = 20, message = "任务版本不能超过 20 个字符"))]
    pub expected_task_version: String,
    /// 查询所得任务对象版本。
    #[validate(custom(function = "non_blank", message = "任务对象版本不能为空"))]
    #[validate(length(max = 128, message = "任务对象版本不能超过 128 个字符"))]
    pub expected_subject_version: String,
    /// 完整领域决定。
    #[validate(nested)]
    pub decision: CardFundsReviewDecision,
    /// 客户端稳定幂等键。
    #[validate(custom(function = "non_blank", message = "幂等键不能为空"))]
    #[validate(length(max = 128, message = "幂等键不能超过 128 个字符"))]
    pub idempotency_key: String,
}

/// 正式复核完成后的固定任务状态。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum CompletedWorkItemStatus {
    /// 已完成。
    Completed,
}

/// W13 驳回后在同一事务形成的后继工作项。
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct CardFundsReviewFollowUpWorkItem {
    /// 新工作项 ID。
    pub work_item_id: String,
    /// 与当前复核类型一致的正式工作项类型。
    pub work_item_type: String,
    /// 固定为 `OPEN`。
    pub status: String,
}

/// W13 正式复核业务结果。
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct CardFundsReviewBusinessResult {
    /// 新增复核事实。
    pub receivable_funds_review_id: String,
    /// 应收往来子账。
    pub receivable_account_id: String,
    /// 正式复核号。
    pub review_no: u32,
    /// 事务完成后的账户复核状态。
    pub account_review_status: String,
    /// 同事务写入的工作流动作。
    pub workflow_action_id: String,
    /// 可用于结果追踪与严格重放的稳定操作号。
    pub operation_id: String,
    /// 服务端完成时间（RFC 3339）。
    pub completed_at: String,
    /// 正式复核结果。
    pub review_result: CardFundsReviewResult,
    /// 正式复核结论。
    pub conclusion: CardFundsReviewConclusion,
    /// 仅驳回时返回同事务形成的后继待办。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub follow_up_work_item: Option<CardFundsReviewFollowUpWorkItem>,
}

/// W13 强类型正式复核命令结果。
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct CompleteCardFundsReviewResult {
    /// 已完成的原任务。
    pub work_item_id: String,
    /// 固定为 `COMPLETED`。
    pub work_item_status: CompletedWorkItemStatus,
    /// 领域正式结果。
    pub business_result: CardFundsReviewBusinessResult,
}
