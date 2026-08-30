//! 域 D18 `receivable` 的 DTO（Handler 直接复用，禁止在 handler 内重复定义同构类型）。
//!
//! 字段名与 HTTP 契约一致（api-contract.md）：分页参数 `page`/`page_size`/
//! `sort_by`/`sort_dir` 扁平传递；时间一律秒级时间戳（`Instant` 序列化为整数）；
//! 金额一律十进制字符串（`entities::money::Amount`）；业务日期为 `YYYY-MM-DD`。
//!
//! 契约来源：`erp-client/features/customer-receivables/types.ts`（W11）、
//! `features/card-funds-review`（W13）；与前端 mock 的 camelCase/ISO 形态差异
//! 见 P3 PR「契约变更」一节（后端统一 snake_case + 秒级时间戳）。

use entities::common::time::{BusinessDate, Instant};
use entities::ids::{
    CustomerAccountId, FileAssetId, PartyId, ReceivableAccountId, ReceivableEntryId, WorkItemId,
};
use entities::money::Amount;
use entities::receivable::{
    AccountReviewStatus, AllocationAction, CustomerReceiptStatus, EntryDirection, FundsReviewType,
    InvoiceDirection, InvoiceKind, InvoiceStatus, ReceivableAccountStatus, ReceivableEntryType, ReviewResult,
};
use serde::{Deserialize, Serialize};
use validator::Validate;

use crate::errors::{Error, Result};
use crate::query::{normalized_text, page_or_default, page_size_or_default};
use crate::work_item::WorkItemView;

/// 应收往来子账列表允许的排序字段白名单（api-contract §4：Service 层校验）。
pub(crate) const RECEIVABLE_ACCOUNT_SORT_FIELDS: &[&str] = &[
    "account_seq",
    "gross_total",
    "settled_total",
    "open_total",
    "open_invoiceable_total",
    "created_at",
];
/// 客户回款单列表允许的排序字段白名单。
pub(crate) const CUSTOMER_RECEIPT_SORT_FIELDS: &[&str] = &["received_at", "amount", "created_at"];
/// 发票列表允许的排序字段白名单。
pub(crate) const INVOICE_SORT_FIELDS: &[&str] = &["invoice_date", "gross_amount", "net_amount", "created_at"];

/// 排序方向。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SortDir {
    /// 升序。
    Asc,
    /// 降序。
    Desc,
}

/// 归一化后的分页查询 DTO（Service → Repository 共用）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PageParams {
    /// 页码（1 起）。
    pub page: u64,
    /// 单页条数（已 clamp 到 1–100）。
    pub page_size: u32,
    /// 排序字段（已过白名单校验，`&'static str` 保证来源只可能是白名单）。
    pub sort_by: &'static str,
    /// 排序方向。
    pub sort_dir: SortDir,
}

/// 校验排序参数（白名单 + 方向），返回归一化排序字段与方向。
///
/// # 参数
/// * `sort_by` - 可选排序字段；空白视为未提供
/// * `sort_dir` - 可选排序方向；空白视为未提供
/// * `allowed_fields` - 白名单
///
/// # 返回
/// 返回 `(排序字段, 方向)`；未提供时默认 `("created_at", Desc)`。
///
/// # 错误
/// 字段不在白名单或方向不是 `asc`/`desc` 时返回 `ValidationError`。
pub(crate) fn normalize_sort(
    sort_by: &Option<String>,
    sort_dir: &Option<String>,
    allowed_fields: &'static [&'static str],
) -> Result<(&'static str, SortDir)> {
    let sort_by = match sort_by
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        Some(field) => allowed_fields
            .iter()
            .find(|allowed| **allowed == field)
            .copied()
            .ok_or_else(|| Error::ValidationError(format!("不支持的排序字段: {field}")))?,
        None => "created_at",
    };
    let sort_dir = match sort_dir
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        Some("asc") => SortDir::Asc,
        Some("desc") => SortDir::Desc,
        Some(other) => return Err(Error::ValidationError(format!("非法排序方向: {other}"))),
        None => SortDir::Desc,
    };
    Ok((sort_by, sort_dir))
}

/// 契约目标形状的分页响应（api-contract §3）：`items` + `total` + `page` + `page_size`。
#[derive(Debug, Clone, Serialize)]
pub struct PageView<T> {
    /// 当前页数据。
    pub items: Vec<T>,
    /// 满足筛选条件的总数（非当前页条数）。
    pub total: i64,
    /// 当前页码（1 起）。
    pub page: u64,
    /// 请求的分页大小。
    pub page_size: u32,
}

/// 校验文本去除首尾空白后非空（validator 的 `length(min=1)` 对纯空白字符串不生效）。
fn non_blank(value: &str) -> std::result::Result<(), validator::ValidationError> {
    if value.trim().is_empty() {
        return Err(validator::ValidationError::new("不能为空白"));
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// 应收往来子账（receivable_account）
// ---------------------------------------------------------------------------

/// 应收往来子账创建请求（W11「从销售单登记应收」：子账 + 原始应收分录）。
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct CreateReceivableAccountRequest {
    /// 来源销售单。
    pub sales_order_id: String,
    /// 往来子账序号（同一销售单内从 1 递增）。
    #[validate(range(min = 1, message = "往来子账序号必须从 1 开始"))]
    pub account_seq: u32,
    /// 企业客户经营归属。
    pub customer_id: CustomerAccountId,
    /// 收款和开票往来主体。
    pub counterparty_party_id: PartyId,
    /// 卡券票款复核状态缓存；缺省时由来源销售单业务性质派生，传入值必须与派生值一致。
    #[serde(default)]
    pub review_status: Option<AccountReviewStatus>,
    /// 含税应收总额。
    pub gross_total: Amount,
    /// 可开票含税总额（缺省等于含税应收总额）。
    #[serde(default)]
    pub invoiceable_total: Option<Amount>,
    /// 到期日（`YYYY-MM-DD`）。
    pub due_date: BusinessDate,
    /// 来源销售修订 ID（作为分录来源修订）。
    pub source_sales_order_revision_id: String,
    /// 来源单据序号（分录来源内序号，从 1 开始）。
    #[validate(range(min = 1, message = "来源内序号必须从 1 开始"))]
    pub source_sequence: u32,
}

/// 应收分录响应视图。
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct ReceivableEntryView {
    /// 实体主键。
    pub id: String,
    /// 分录类型。
    pub entry_type: ReceivableEntryType,
    /// 分录方向。
    pub direction: EntryDirection,
    /// 正数含税金额。
    pub amount: Amount,
    /// 到期日（`YYYY-MM-DD`）。
    pub due_date: BusinessDate,
    /// 来源单据 ID。
    pub source_document_id: String,
    /// 来源内序号。
    pub source_sequence: u32,
    /// 入账时间（秒级时间戳）。
    pub posted_at: Instant,
    /// 累计被冲减金额（抵销合计）。
    pub offset_total: Amount,
}

/// 卡券票款复核记录视图。
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct FundsReviewView {
    /// 实体主键。
    pub id: String,
    /// 子账内递增复核号。
    pub review_no: u32,
    /// 复核类型。
    pub review_type: FundsReviewType,
    /// 复核结果。
    pub review_result: ReviewResult,
    /// 财务复核人。
    pub reviewed_by: String,
    /// 复核时间（秒级时间戳）。
    pub reviewed_at: Instant,
    /// 复核证据引用。
    pub evidence_reference: Option<String>,
}

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

/// 应收往来子账响应视图（W11 应收台账行 + 详情）。
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct ReceivableAccountView {
    /// 实体主键。
    pub id: String,
    /// 来源销售单。
    pub sales_order_id: String,
    /// 销售单业务单号。
    pub sales_order_no: String,
    /// 当前不可变销售版本号。
    pub sales_order_revision_no: u32,
    /// 当前销售版本生效时间（秒级时间戳）。
    pub sales_order_snapshot_at: u64,
    /// 往来子账序号。
    pub account_seq: u32,
    /// 本子账开始适用的销售版本。
    pub source_sales_order_revision_id: String,
    /// 当前销售版本（W13 正式动作的领域版本锁）。
    pub current_sales_order_revision_id: String,
    /// 企业客户经营归属。
    pub customer_id: String,
    /// 当前销售版本冻结的客户名称。
    pub customer_name: String,
    /// 收款和开票往来主体。
    pub counterparty_party_id: String,
    /// 当前销售版本冻结的收款/开票往来主体名称；缺失时为空并阻断登记动作。
    pub counterparty_party_name: Option<String>,
    /// 卡券票款复核状态缓存。
    pub review_status: AccountReviewStatus,
    /// 含税应收总额。
    pub gross_total: Amount,
    /// 已核销含税总额。
    pub settled_total: Amount,
    /// 剩余开放含税余额。
    pub open_total: Amount,
    /// 可开票含税总额。
    pub invoiceable_total: Amount,
    /// 净已开含税总额。
    pub invoiced_total: Amount,
    /// 剩余可开票含税额度。
    pub open_invoiceable_total: Amount,
    /// 子账状态。
    pub status: ReceivableAccountStatus,
    /// 乐观锁版本。
    pub version: u64,
    /// W13 不透明账户领域版本；客户端不得自行递增或与其它版本互换。
    pub account_domain_version: String,
    /// 当前复核链尾；空链为 `None`。
    pub review_chain_tail_id: Option<String>,
    /// W13 不透明复核链版本。
    pub review_chain_version: String,
    /// 服务端计算的下一复核号。
    pub next_review_no: u32,
    /// W13 不透明票款事实版本。
    pub funds_fact_version: String,
    /// 当前账户关联的正式回款事实。
    pub receipt_facts: Vec<ReceivableReceiptFactView>,
    /// 当前账户关联的正式销项发票事实。
    pub invoice_facts: Vec<ReceivableInvoiceFactView>,
    /// 创建时间（秒级时间戳）。
    pub created_at: u64,
    /// 应收分录（含抵销合计）。
    pub entries: Vec<ReceivableEntryView>,
    /// 卡券票款复核链。
    pub reviews: Vec<FundsReviewView>,
    /// 当前操作人可见的 W13 正式任务。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub work_item: Option<WorkItemView>,
    /// 由正式任务类型确定的 W13 复核类型。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub active_review_type: Option<CardFundsReviewType>,
    /// W13 领域动作，不得从通用任务动作推导。
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub allowed_actions: Vec<CardFundsReviewAllowedAction>,
    /// W13 领域动作阻断事实。
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub action_blockers: Vec<CardFundsReviewActionBlockerView>,
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

/// 应收往来子账列表查询参数（分页参数与筛选字段扁平传递）。
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct ReceivableAccountListParams {
    /// 企业客户经营归属筛选。
    pub customer_id: Option<CustomerAccountId>,
    /// 收款和开票往来主体筛选。
    pub counterparty_party_id: Option<PartyId>,
    /// 子账状态筛选。
    pub status: Option<ReceivableAccountStatus>,
    /// 来源销售单筛选。
    pub sales_order_id: Option<String>,
    /// 卡券票款复核状态筛选。
    pub review_status: Option<AccountReviewStatus>,
    /// 页码（1 起）。
    #[validate(range(min = 1, message = "页码必须大于0"))]
    pub page: Option<u64>,
    /// 单页条数（1–100）。
    #[validate(range(min = 1, max = 100, message = "分页大小必须在1-100之间"))]
    pub page_size: Option<u32>,
    /// 排序字段（白名单：`account_seq`/`gross_total`/`open_total` 等）。
    pub sort_by: Option<String>,
    /// 排序方向（`asc`/`desc`）。
    pub sort_dir: Option<String>,
}

/// 归一化后的应收往来子账列表查询参数。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ReceivableAccountListQuery {
    /// 企业客户经营归属筛选。
    pub customer_id: Option<CustomerAccountId>,
    /// 收款和开票往来主体筛选。
    pub counterparty_party_id: Option<PartyId>,
    /// 子账状态筛选。
    pub status: Option<ReceivableAccountStatus>,
    /// 来源销售单筛选。
    pub sales_order_id: Option<String>,
    /// 卡券票款复核状态筛选。
    pub review_status: Option<AccountReviewStatus>,
    /// 分页与排序参数。
    pub paging: PageParams,
}

impl ReceivableAccountListParams {
    /// 归一化应收往来子账列表查询参数。
    ///
    /// 文本筛选去首尾空白、分页取默认值、排序字段过白名单校验。
    ///
    /// # 返回
    /// 返回不依赖仓储类型的规范化查询参数。
    ///
    /// # 错误
    /// 排序字段不在白名单或排序方向非法时返回 `ValidationError`。
    pub(crate) fn normalized(&self) -> Result<ReceivableAccountListQuery> {
        let (sort_by, sort_dir) =
            normalize_sort(&self.sort_by, &self.sort_dir, RECEIVABLE_ACCOUNT_SORT_FIELDS)?;
        Ok(ReceivableAccountListQuery {
            customer_id: self.customer_id.clone(),
            counterparty_party_id: self.counterparty_party_id.clone(),
            status: self.status,
            sales_order_id: normalized_text(self.sales_order_id.as_deref()),
            review_status: self.review_status,
            paging: PageParams {
                page: page_or_default(self.page),
                page_size: page_size_or_default(self.page_size),
                sort_by,
                sort_dir,
            },
        })
    }
}

// ---------------------------------------------------------------------------
// 客户回款单（customer_receipt）
// ---------------------------------------------------------------------------

/// 客户回款单创建请求（W11 登记草稿回款；过账与分配走 `post`）。
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct CreateCustomerReceiptRequest {
    /// 回款单号（唯一，幂等键）。
    #[validate(custom(function = "non_blank", message = "回款单号不能为空"))]
    pub receipt_no: String,
    /// 实际付款往来主体。
    pub counterparty_party_id: PartyId,
    /// 可选经营归属提示。
    pub customer_id: Option<CustomerAccountId>,
    /// 实际到账时间（秒级时间戳）。
    pub received_at: Instant,
    /// 含税到账金额。
    pub amount: Amount,
    /// 银行流水或凭证引用。
    pub bank_reference: Option<String>,
}

/// 回款核销分配请求行（§8.3-1：同一往来主体、净分配不超过回款金额）。
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct ReceiptAllocationLineRequest {
    /// 被核销应收分录。
    pub receivable_entry_id: ReceivableEntryId,
    /// 本次核销金额（正数）。
    pub allocated_amount: Amount,
}

/// 客户回款过账请求（仅由最终通过动作内部消费冻结分配；HTTP 旁路已关闭）。
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct PostCustomerReceiptRequest {
    /// 核销分配行（允许保留未分配余额）。
    #[validate(length(min = 1, message = "至少提供一条核销分配"))]
    pub allocations: Vec<ReceiptAllocationLineRequest>,
}

/// 客户回款提交审批请求。客户端不得选择定义或审批人。
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
#[serde(deny_unknown_fields)]
pub struct SubmitCustomerReceiptRequest {
    /// 期望的单据乐观锁版本。
    #[validate(range(min = 1, message = "乐观锁版本必须大于 0"))]
    pub expected_version: u64,
    /// 业务请求幂等键。
    #[validate(length(min = 1, max = 128, message = "幂等键不能为空"))]
    pub idempotency_key: String,
    /// 提交时冻结的待过账核销分配。
    #[validate(length(min = 1, message = "至少提供一条核销分配"))]
    pub allocations: Vec<ReceiptAllocationLineRequest>,
}

/// 客户回款原子创建并提交审批请求。
///
/// 已有草稿提交 `receipt_id + expected_version`；新登记提交完整 `receipt`。
/// 服务端在一个事务内完成绑定、创建、冻结分配、审批启动与审计。
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
#[serde(deny_unknown_fields)]
pub struct CommitCustomerReceiptRequest {
    /// 已有回款草稿主键。
    pub receipt_id: Option<String>,
    /// 已有草稿期望乐观锁版本。
    pub expected_version: Option<u64>,
    /// 新回款完整字段；提交已有草稿时为空。
    pub receipt: Option<CreateCustomerReceiptRequest>,
    /// 提交时冻结的待过账核销分配。
    #[validate(length(min = 1, message = "至少提供一条核销分配"))]
    pub allocations: Vec<ReceiptAllocationLineRequest>,
    /// 业务请求幂等键。
    #[validate(length(min = 1, max = 128, message = "幂等键不能为空"))]
    pub idempotency_key: String,
}

/// 撤回客户回款审批请求。原因必填。
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
#[serde(deny_unknown_fields)]
pub struct CancelCustomerReceiptApprovalRequest {
    /// 期望的单据乐观锁版本。
    #[validate(range(min = 1, message = "乐观锁版本必须大于 0"))]
    pub expected_version: u64,
    /// 非空撤回原因。
    #[validate(length(min = 1, max = 512, message = "撤回原因不能为空"))]
    pub reason: String,
    /// 业务请求幂等键。
    #[validate(length(min = 1, max = 128, message = "幂等键不能为空"))]
    pub idempotency_key: String,
}

/// 回款核销分配视图。
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct ReceiptAllocationView {
    /// 实体主键。
    pub id: String,
    /// 回款单内追加序号。
    pub allocation_seq: u32,
    /// 分配动作。
    pub allocation_action: AllocationAction,
    /// 被核销应收分录。
    pub receivable_entry_id: String,
    /// 本次核销金额。
    pub allocated_amount: Amount,
    /// 核销时间（秒级时间戳）。
    pub allocated_at: Instant,
    /// `REVERSE` 引用的原 `APPLY` 分配。
    pub reverses_allocation_id: Option<String>,
}

/// 客户回款单响应视图。
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct CustomerReceiptView {
    /// 实体主键。
    pub id: String,
    /// 回款单号。
    pub receipt_no: String,
    /// 回款单状态。
    pub status: CustomerReceiptStatus,
    /// 实际付款往来主体。
    pub counterparty_party_id: String,
    /// 可选经营归属提示。
    pub customer_id: Option<String>,
    /// 实际到账时间（秒级时间戳）。
    pub received_at: Instant,
    /// 含税到账金额。
    pub amount: Amount,
    /// 银行流水或凭证引用。
    pub bank_reference: Option<String>,
    /// 乐观锁版本。
    pub version: u64,
    /// 创建时间（秒级时间戳）。
    pub created_at: u64,
    /// 已核销合计（净）。
    pub allocated_total: Amount,
    /// 未分配余额。
    pub unallocated_amount: Amount,
    /// 核销分配行。
    pub allocations: Vec<ReceiptAllocationView>,
    /// 统一只读审批结构。客户端不得据此选择定义或审批人。
    pub approval: DocumentApprovalView,
}

/// 单据详情返回的统一只读审批结构。
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct DocumentApprovalView {
    /// `PROCESS_REQUIRED` 或 `NO_APPROVAL`。
    pub requirement: String,
    /// 创建时冻结的定义摘要；未绑定为空。
    pub definition: Option<DocumentApprovalDefinitionView>,
    /// 已启动后的实例摘要；未提交为空。
    pub instance: Option<DocumentApprovalInstanceView>,
    /// 有界最近历史。
    pub recent_history: Vec<DocumentApprovalHistoryItemView>,
    /// 完整历史分页游标。
    pub history_page: DocumentApprovalHistoryPageView,
    /// 服务端允许的动作；不含选择定义或审批人。
    pub allowed_actions: Vec<String>,
}

/// 绑定定义只读摘要。
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct DocumentApprovalDefinitionView {
    /// 定义主键。
    pub id: String,
    /// 定义名称。
    pub name: String,
    /// 定义业务版本。
    pub version: u32,
    /// 节点摘要。单据详情不展开审批人。
    pub nodes: Vec<DocumentApprovalNodeView>,
}

/// 定义节点只读摘要。
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct DocumentApprovalNodeView {
    /// 节点键。
    pub key: String,
    /// 节点名称。
    pub name: String,
}

/// 运行实例只读摘要。
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct DocumentApprovalInstanceView {
    /// 实例主键。
    pub id: String,
    /// 实例状态。
    pub status: String,
    /// 当前轮次。
    pub current_round_no: u32,
    /// 当前节点键。
    pub current_node: Option<String>,
    /// 当前审批人。
    pub current_assignee: Option<String>,
    /// 最近驳回原因。
    pub latest_rejection: Option<String>,
}

/// 有界历史项。
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct DocumentApprovalHistoryItemView {
    /// 执行主键。
    pub execution_id: String,
    /// 轮次。
    pub round_no: u32,
    /// 节点键。
    pub node_key: String,
    /// 结束结果。
    pub result: String,
}

/// 完整历史分页。
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct DocumentApprovalHistoryPageView {
    /// 下一页游标。
    pub next_cursor: Option<String>,
    /// 是否还有更多。
    pub has_more: bool,
}

/// 客户回款单列表查询参数（分页参数与筛选字段扁平传递）。
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct CustomerReceiptListParams {
    /// 回款单号模糊筛选。
    pub receipt_no: Option<String>,
    /// 实际付款往来主体筛选。
    pub counterparty_party_id: Option<PartyId>,
    /// 回款单状态筛选。
    pub status: Option<CustomerReceiptStatus>,
    /// 页码（1 起）。
    #[validate(range(min = 1, message = "页码必须大于0"))]
    pub page: Option<u64>,
    /// 单页条数（1–100）。
    #[validate(range(min = 1, max = 100, message = "分页大小必须在1-100之间"))]
    pub page_size: Option<u32>,
    /// 排序字段（白名单：`received_at`/`amount`/`created_at`）。
    pub sort_by: Option<String>,
    /// 排序方向（`asc`/`desc`）。
    pub sort_dir: Option<String>,
}

/// 归一化后的客户回款单列表查询参数。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct CustomerReceiptListQuery {
    /// 回款单号模糊筛选。
    pub receipt_no: Option<String>,
    /// 实际付款往来主体筛选。
    pub counterparty_party_id: Option<PartyId>,
    /// 回款单状态筛选。
    pub status: Option<CustomerReceiptStatus>,
    /// 分页与排序参数。
    pub paging: PageParams,
}

impl CustomerReceiptListParams {
    /// 归一化客户回款单列表查询参数。
    ///
    /// # 返回
    /// 返回不依赖仓储类型的规范化查询参数。
    ///
    /// # 错误
    /// 排序字段不在白名单或排序方向非法时返回 `ValidationError`。
    pub(crate) fn normalized(&self) -> Result<CustomerReceiptListQuery> {
        let (sort_by, sort_dir) =
            normalize_sort(&self.sort_by, &self.sort_dir, CUSTOMER_RECEIPT_SORT_FIELDS)?;
        Ok(CustomerReceiptListQuery {
            receipt_no: normalized_text(self.receipt_no.as_deref()),
            counterparty_party_id: self.counterparty_party_id.clone(),
            status: self.status,
            paging: PageParams {
                page: page_or_default(self.page),
                page_size: page_size_or_default(self.page_size),
                sort_by,
                sort_dir,
            },
        })
    }
}

// ---------------------------------------------------------------------------
// 发票（invoice，D18 拥有实体；D19 经本域 Repository 复用）
// ---------------------------------------------------------------------------

/// 发票登记请求（W11 登记草稿销项发票；登记过账与分配走 `post`）。
///
/// 客户端不得提交定义 ID 或审批人；未知字段失败关闭。
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
#[serde(deny_unknown_fields)]
pub struct CreateInvoiceRequest {
    /// 发票方向（销项 `Sales` / 进项 `Purchase`；D19 进项登记复用本域 DTO）。
    pub invoice_direction: InvoiceDirection,
    /// 蓝红类型（红票走 `red_issue` 接口，此处仅蓝票草稿）。
    pub invoice_kind: InvoiceKind,
    /// 客户或供应商。
    pub party_id: PartyId,
    /// 发票代码（无代码数电票为空）。
    pub invoice_code: Option<String>,
    /// 发票号码。
    #[validate(custom(function = "non_blank", message = "发票号码不能为空"))]
    pub invoice_no: String,
    /// 开票日期（`YYYY-MM-DD`）。
    pub invoice_date: BusinessDate,
    /// 含税金额。
    pub gross_amount: Amount,
    /// 不含税金额。
    pub net_amount: Amount,
    /// 税额。
    pub tax_amount: Amount,
    /// 发票尾差（可正可负）。
    #[serde(default)]
    pub rounding_adjustment_amount: Option<Amount>,
    /// 尾差原因。
    pub rounding_reason: Option<String>,
}

/// 销项发票分配请求行（§8.3-2：同一往来主体与方向、蓝票受发票有效余额与
/// 目标子账可开票额度双侧上限）。
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct SalesInvoiceAllocationLineRequest {
    /// 销售单可开票对象（应收往来子账）。
    pub receivable_account_id: ReceivableAccountId,
    /// 分配含税金额。
    pub allocated_gross_amount: Amount,
    /// 分配不含税金额。
    pub allocated_net_amount: Amount,
    /// 分配税额。
    pub allocated_tax_amount: Amount,
}

/// 发票登记过账请求（资金入口，规范化号码唯一索引构成去重机制）。
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
#[serde(deny_unknown_fields)]
pub struct PostInvoiceRequest {
    /// 当前开放销项开票执行任务。
    pub work_item_id: WorkItemId,
    /// 查询所得任务乐观锁版本。
    #[validate(custom(function = "non_blank", message = "任务版本不能为空"))]
    #[validate(length(max = 20, message = "任务版本不能超过 20 个字符"))]
    pub expected_task_version: String,
    /// 发票分配行（合计必须等于发票含税金额）。
    #[validate(length(min = 1, message = "至少提供一条发票分配"))]
    pub allocations: Vec<SalesInvoiceAllocationLineRequest>,
}

/// 销项发票原子登记请求。
///
/// 已有草稿时提交 `invoice_id + expected_version`；新登记时提交完整 `invoice`。
/// 服务端在一个事务内完成单据注册、发票创建、分配、子账进度与审计。
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
#[serde(deny_unknown_fields)]
pub struct CommitInvoiceRequest {
    /// 当前开放销项开票执行任务。
    pub work_item_id: WorkItemId,
    /// 查询所得任务乐观锁版本。
    #[validate(custom(function = "non_blank", message = "任务版本不能为空"))]
    #[validate(length(max = 20, message = "任务版本不能超过 20 个字符"))]
    pub expected_task_version: String,
    /// 已有发票草稿主键。
    pub invoice_id: Option<String>,
    /// 已有草稿期望乐观锁版本。
    pub expected_version: Option<u64>,
    /// 新发票完整字段；提交已有草稿时为空。
    pub invoice: Option<CreateInvoiceRequest>,
    /// 销项发票分配行。
    #[validate(length(min = 1, message = "至少提供一条发票分配"))]
    pub allocations: Vec<SalesInvoiceAllocationLineRequest>,
    /// 业务请求幂等键。
    #[validate(length(min = 1, max = 128, message = "幂等键不能为空"))]
    pub idempotency_key: String,
}

/// 红票原子开具请求。
///
/// 客户端只提交原票上的业务意图；服务端在事务内读取有效分配并生成反向行，
/// 禁止客户端搬运原分配 ID、净额或税额。
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
#[serde(deny_unknown_fields)]
pub struct CommitRedInvoiceRequest {
    /// 红票号码；为空时按幂等键生成稳定号码。
    pub invoice_no: Option<String>,
    /// 本次红冲含税金额；为空时红冲全部剩余有效分配。
    pub amount: Option<Amount>,
    /// 红冲业务原因，写入审计日志。
    #[validate(custom(function = "non_blank", message = "红冲原因不能为空"))]
    pub reason: String,
    /// 业务请求幂等键。
    #[validate(length(min = 1, max = 128, message = "幂等键不能为空"))]
    pub idempotency_key: String,
}

/// 销项发票分配视图。
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct SalesInvoiceAllocationView {
    /// 实体主键。
    pub id: String,
    /// 发票内追加序号。
    pub allocation_seq: u32,
    /// 分配动作。
    pub allocation_action: AllocationAction,
    /// 销售单可开票对象（应收往来子账）。
    pub receivable_account_id: String,
    /// 分配含税金额。
    pub allocated_gross_amount: Amount,
    /// 分配不含税金额。
    pub allocated_net_amount: Amount,
    /// 分配税额。
    pub allocated_tax_amount: Amount,
    /// 红票反向分配引用的原蓝票分配。
    pub reverses_allocation_id: Option<String>,
}

/// 发票响应视图。
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct InvoiceView {
    /// 实体主键。
    pub id: String,
    /// 发票方向。
    pub invoice_direction: InvoiceDirection,
    /// 蓝红类型。
    pub invoice_kind: InvoiceKind,
    /// 客户或供应商。
    pub party_id: String,
    /// 发票代码。
    pub invoice_code: Option<String>,
    /// 发票号码。
    pub invoice_no: String,
    /// 开票日期（`YYYY-MM-DD`）。
    pub invoice_date: BusinessDate,
    /// 含税金额。
    pub gross_amount: Amount,
    /// 不含税金额。
    pub net_amount: Amount,
    /// 税额。
    pub tax_amount: Amount,
    /// 发票尾差。
    pub rounding_adjustment_amount: Amount,
    /// 尾差原因。
    pub rounding_reason: Option<String>,
    /// 红票原蓝票。
    pub original_invoice_id: Option<String>,
    /// 发票状态。
    pub status: InvoiceStatus,
    /// 乐观锁版本。
    pub version: u64,
    /// 创建时间（秒级时间戳）。
    pub created_at: u64,
    /// 已分配含税合计（净）。
    pub allocated_total: Amount,
    /// 未分配含税余额。
    pub unallocated_amount: Amount,
    /// 发票分配行。
    pub allocations: Vec<SalesInvoiceAllocationView>,
}

/// 发票列表查询参数（分页参数与筛选字段扁平传递）。
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct InvoiceListParams {
    /// 发票方向筛选（销项/进项；D19 进项列表复用）。
    pub invoice_direction: Option<InvoiceDirection>,
    /// 蓝红类型筛选。
    pub invoice_kind: Option<InvoiceKind>,
    /// 客户或供应商筛选。
    pub party_id: Option<PartyId>,
    /// 发票号码模糊筛选。
    pub invoice_no: Option<String>,
    /// 发票状态筛选。
    pub status: Option<InvoiceStatus>,
    /// 页码（1 起）。
    #[validate(range(min = 1, message = "页码必须大于0"))]
    pub page: Option<u64>,
    /// 单页条数（1–100）。
    #[validate(range(min = 1, max = 100, message = "分页大小必须在1-100之间"))]
    pub page_size: Option<u32>,
    /// 排序字段（白名单：`invoice_date`/`gross_amount`/`net_amount`/`created_at`）。
    pub sort_by: Option<String>,
    /// 排序方向（`asc`/`desc`）。
    pub sort_dir: Option<String>,
}

/// 归一化后的发票列表查询参数。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct InvoiceListQuery {
    /// 发票方向筛选。
    pub invoice_direction: Option<InvoiceDirection>,
    /// 蓝红类型筛选。
    pub invoice_kind: Option<InvoiceKind>,
    /// 客户或供应商筛选。
    pub party_id: Option<PartyId>,
    /// 发票号码模糊筛选。
    pub invoice_no: Option<String>,
    /// 发票状态筛选。
    pub status: Option<InvoiceStatus>,
    /// 分页与排序参数。
    pub paging: PageParams,
}

impl InvoiceListParams {
    /// 归一化发票列表查询参数。
    ///
    /// # 返回
    /// 返回不依赖仓储类型的规范化查询参数。
    ///
    /// # 错误
    /// 排序字段不在白名单或排序方向非法时返回 `ValidationError`。
    pub(crate) fn normalized(&self) -> Result<InvoiceListQuery> {
        let (sort_by, sort_dir) = normalize_sort(&self.sort_by, &self.sort_dir, INVOICE_SORT_FIELDS)?;
        Ok(InvoiceListQuery {
            invoice_direction: self.invoice_direction,
            invoice_kind: self.invoice_kind,
            party_id: self.party_id.clone(),
            invoice_no: normalized_text(self.invoice_no.as_deref()),
            status: self.status,
            paging: PageParams {
                page: page_or_default(self.page),
                page_size: page_size_or_default(self.page_size),
                sort_by,
                sort_dir,
            },
        })
    }
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

#[cfg(test)]
mod tests {
    use super::{
        normalize_sort, CardFundsReviewConclusion, CardFundsReviewResult, CardFundsReviewType,
        CompleteCardFundsReviewCommand, CreateInvoiceRequest, CustomerReceiptListParams, InvoiceListParams,
        InvoiceView, ReceivableAccountListParams, SortDir,
    };
    use entities::common::time::BusinessDate;
    use entities::money::Amount;
    use entities::receivable::{
        CustomerReceiptStatus, InvoiceDirection, InvoiceKind, InvoiceStatus, ReceivableAccountStatus,
    };
    use std::str::FromStr;
    use validator::Validate;

    #[test]
    fn sort_whitelist_rejects_unknown_fields_and_directions() {
        assert!(normalize_sort(&Some("name".to_string()), &None, &["created_at"]).is_err());
        assert!(normalize_sort(&None, &Some("up".to_string()), &["created_at"]).is_err());

        let (field, direction) =
            normalize_sort(&Some(" created_at ".to_string()), &None, &["created_at"]).unwrap();
        assert_eq!(field, "created_at");
        assert_eq!(direction, SortDir::Desc);

        let (field, direction) = normalize_sort(&None, &Some("asc".to_string()), &["created_at"]).unwrap();
        assert_eq!(field, "created_at");
        assert_eq!(direction, SortDir::Asc);
    }

    #[test]
    fn receivable_account_list_params_normalize_filters_and_paging() {
        let params = ReceivableAccountListParams {
            customer_id: None,
            counterparty_party_id: None,
            status: Some(ReceivableAccountStatus::Open),
            sales_order_id: Some(" SO-1 ".to_string()),
            review_status: None,
            page: Some(2),
            page_size: Some(50),
            sort_by: Some("open_total".to_string()),
            sort_dir: Some("asc".to_string()),
        };
        let query = params.normalized().unwrap();
        assert_eq!(query.status, Some(ReceivableAccountStatus::Open));
        assert_eq!(query.sales_order_id.as_deref(), Some("SO-1"));
        assert_eq!(query.paging.page, 2);
        assert_eq!(query.paging.page_size, 50);
        assert_eq!(query.paging.sort_by, "open_total");
        assert_eq!(query.paging.sort_dir, SortDir::Asc);
    }

    #[test]
    fn list_params_reject_unbounded_page_size() {
        let params = ReceivableAccountListParams {
            customer_id: None,
            counterparty_party_id: None,
            status: None,
            sales_order_id: None,
            review_status: None,
            page: Some(0),
            page_size: Some(u32::MAX),
            sort_by: None,
            sort_dir: None,
        };
        assert!(params.validate().is_err());
    }

    #[test]
    fn receipt_and_invoice_list_params_normalize() {
        let receipt = CustomerReceiptListParams {
            receipt_no: Some(" RC-1 ".to_string()),
            counterparty_party_id: None,
            status: Some(CustomerReceiptStatus::Posted),
            page: None,
            page_size: None,
            sort_by: None,
            sort_dir: None,
        };
        let query = receipt.normalized().unwrap();
        assert_eq!(query.receipt_no.as_deref(), Some("RC-1"));
        assert_eq!(query.status, Some(CustomerReceiptStatus::Posted));

        let invoice = InvoiceListParams {
            invoice_direction: Some(InvoiceDirection::Sales),
            invoice_kind: None,
            party_id: None,
            invoice_no: None,
            status: None,
            page: None,
            page_size: Some(99),
            sort_by: None,
            sort_dir: None,
        };
        let query = invoice.normalized().unwrap();
        assert_eq!(query.invoice_direction, Some(InvoiceDirection::Sales));
        assert_eq!(query.paging.page_size, 99);
    }

    /// 发票创建请求拒绝定义 ID / 审批人；视图不暴露审批区。
    #[test]
    fn invoice_create_and_view_have_no_approval_surface() {
        let valid = serde_json::json!({
            "invoice_direction": "sales",
            "invoice_kind": "blue",
            "party_id": "p-1",
            "invoice_no": "001",
            "invoice_date": "2026-08-06",
            "gross_amount": "100.00",
            "net_amount": "88.50",
            "tax_amount": "11.50"
        });
        assert!(serde_json::from_value::<CreateInvoiceRequest>(valid).is_ok());
        let forged = serde_json::json!({
            "invoice_direction": "sales",
            "invoice_kind": "blue",
            "party_id": "p-1",
            "invoice_no": "001",
            "invoice_date": "2026-08-06",
            "gross_amount": "100.00",
            "net_amount": "88.50",
            "tax_amount": "11.50",
            "definition_id": "forged",
            "assignee": "forged"
        });
        assert!(serde_json::from_value::<CreateInvoiceRequest>(forged).is_err());

        let view = InvoiceView {
            id: "inv-1".into(),
            invoice_direction: InvoiceDirection::Sales,
            invoice_kind: InvoiceKind::Blue,
            party_id: "p-1".into(),
            invoice_code: None,
            invoice_no: "001".into(),
            invoice_date: BusinessDate::from_ymd(2026, 8, 6).expect("日期合法"),
            gross_amount: Amount::from_str("100").expect("金额合法"),
            net_amount: Amount::from_str("88.50").expect("金额合法"),
            tax_amount: Amount::from_str("11.50").expect("金额合法"),
            rounding_adjustment_amount: Amount::from_str("0").expect("金额合法"),
            rounding_reason: None,
            original_invoice_id: None,
            status: InvoiceStatus::Draft,
            version: 1,
            created_at: 1,
            allocated_total: Amount::from_str("0").expect("金额合法"),
            unallocated_amount: Amount::from_str("100").expect("金额合法"),
            allocations: Vec::new(),
        };
        let value = serde_json::to_value(&view).expect("视图可序列化");
        let object = value.as_object().expect("视图为对象");
        assert!(!object.contains_key("approval"));
        assert!(!object.contains_key("definition_id"));
        assert!(!object.contains_key("assignee"));
    }

    #[test]
    fn complete_card_funds_review_command_accepts_documented_shape() {
        let command: CompleteCardFundsReviewCommand = serde_json::from_value(serde_json::json!({
            "work_item_id": "wi-1",
            "expected_task_version": "7",
            "expected_subject_version": "sor-9",
            "decision": {
                "receivable_account_id": "ra-1",
                "expected_account_seq": 1,
                "expected_account_domain_version": "4",
                "expected_review_chain_tail_id": "review-2",
                "expected_review_chain_version": "rcv:abc",
                "expected_next_review_no": 3,
                "expected_sales_order_revision_id": "sor-9",
                "expected_funds_fact_version": "ffv:def",
                "review_type": "SYNC_DELTA",
                "review_result": "REJECTED",
                "conclusion": "REJECTED",
                "evidence_document_ids": ["file-1"],
                "evidence_references": ["BANK-REF-1"],
                "comment": "金额不一致",
                "reason_code": "FACTS_MISMATCH"
            },
            "idempotency_key": "card-review-1"
        }))
        .unwrap();

        assert_eq!(command.expected_task_version, "7");
        assert_eq!(command.decision.review_type, CardFundsReviewType::SyncDelta);
        assert_eq!(command.decision.review_result, CardFundsReviewResult::Rejected);
        assert_eq!(command.decision.conclusion, CardFundsReviewConclusion::Rejected);
        assert!(command.validate().is_ok());
    }

    #[test]
    fn complete_card_funds_review_command_rejects_legacy_or_invented_fields() {
        let legacy = serde_json::json!({
            "receivable_account_id": "ra-1",
            "review_type": "opening",
            "review_result": "passed",
            "reviewed_by": "client-spoofed-user",
            "reviewed_at": 1_700_000_000,
            "evidence_reference": "legacy"
        });
        assert!(serde_json::from_value::<CompleteCardFundsReviewCommand>(legacy).is_err());

        let invented_subject_hash = serde_json::json!({
            "work_item_id": "wi-1",
            "expected_task_version": "1",
            "expected_subject_version": "sor-1",
            "decision": {
                "receivable_account_id": "ra-1",
                "expected_account_seq": 1,
                "expected_account_domain_version": "1",
                "expected_review_chain_version": "rcv:empty",
                "expected_next_review_no": 1,
                "expected_sales_order_revision_id": "sor-1",
                "expected_funds_fact_version": "ffv:empty",
                "expected_subject_hash": "client-invented",
                "review_type": "OPENING",
                "review_result": "APPROVED",
                "conclusion": "NO_HISTORY_FROM_ZERO",
                "evidence_document_ids": [],
                "evidence_references": ["核对记录"]
            },
            "idempotency_key": "card-review-2"
        });
        assert!(serde_json::from_value::<CompleteCardFundsReviewCommand>(invented_subject_hash).is_err());
    }

    #[test]
    fn submit_and_cancel_requests_reject_client_assignee_choice() {
        use super::{CancelCustomerReceiptApprovalRequest, SubmitCustomerReceiptRequest};

        assert!(
            serde_json::from_value::<SubmitCustomerReceiptRequest>(serde_json::json!({
                "expected_version": 1,
                "idempotency_key": "k1",
                "allocations": [{"receivable_entry_id": "re-1", "allocated_amount": "10"}],
                "definition_id": "forged"
            }))
            .is_err()
        );
        let submit: SubmitCustomerReceiptRequest = serde_json::from_value(serde_json::json!({
            "expected_version": 1,
            "idempotency_key": "k1",
            "allocations": [{"receivable_entry_id": "re-1", "allocated_amount": "10"}]
        }))
        .unwrap();
        assert_eq!(submit.expected_version, 1);
        assert!(
            serde_json::from_value::<CancelCustomerReceiptApprovalRequest>(serde_json::json!({
                "expected_version": 1,
                "reason": "改金额",
                "idempotency_key": "k2",
                "assignee": "forged"
            }))
            .is_err()
        );
    }
}
