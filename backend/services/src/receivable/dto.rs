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
use entities::ids::{CustomerAccountId, PartyId, ReceivableAccountId, ReceivableEntryId, WorkItemId};
use entities::money::Amount;
use entities::receivable::{
    AccountReviewStatus, AllocationAction, CustomerReceiptStatus, EntryDirection, FundsReviewType,
    InvoiceDirection, InvoiceKind, InvoiceStatus, ReceivableAccountStatus, ReceivableEntryType, ReviewResult,
};
use serde::{Deserialize, Serialize};
use validator::Validate;

use crate::errors::{Error, Result};
use crate::query::{normalized_text, page_or_default, page_size_or_default};

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
    /// 卡券票款复核状态缓存（缺省不适用）。
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

/// 应收往来子账复核缓存更新请求（W13 复核结论落账前的账户缓存刷新）。
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct UpdateReceivableAccountReviewRequest {
    /// 期望的乐观锁版本；与当前版本不一致时拒绝更新（409）。
    #[validate(range(min = 1, message = "乐观锁版本必须大于 0"))]
    pub version: u64,
    /// 复核状态。
    pub review_status: AccountReviewStatus,
    /// 复核人。
    pub reviewed_by: String,
    /// 复核时间（秒级时间戳）。
    pub reviewed_at: Instant,
    /// 复核证据引用。
    pub review_evidence_reference: String,
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

/// 应收往来子账响应视图（W11 应收台账行 + 详情）。
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct ReceivableAccountView {
    /// 实体主键。
    pub id: String,
    /// 来源销售单。
    pub sales_order_id: String,
    /// 往来子账序号。
    pub account_seq: u32,
    /// 企业客户经营归属。
    pub customer_id: String,
    /// 收款和开票往来主体。
    pub counterparty_party_id: String,
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
    /// 创建时间（秒级时间戳）。
    pub created_at: u64,
    /// 应收分录（含抵销合计）。
    pub entries: Vec<ReceivableEntryView>,
    /// 卡券票款复核链。
    pub reviews: Vec<FundsReviewView>,
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

/// 客户回款过账请求（资金入口，回款单号 + 状态迁移构成去重机制）。
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct PostCustomerReceiptRequest {
    /// 核销分配行（允许保留未分配余额）。
    #[validate(length(min = 1, message = "至少提供一条核销分配"))]
    pub allocations: Vec<ReceiptAllocationLineRequest>,
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
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
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
pub struct PostInvoiceRequest {
    /// 发票分配行（合计必须等于发票含税金额）。
    #[validate(length(min = 1, message = "至少提供一条发票分配"))]
    pub allocations: Vec<SalesInvoiceAllocationLineRequest>,
}

/// 红票开具请求（§8.3-3：红票反向原蓝票有效分配，累计不超过原分配）。
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct IssueRedInvoiceRequest {
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
    /// 红票反向分配行（引用原蓝票分配，合计等于红票金额）。
    #[validate(length(min = 1, message = "至少提供一条红冲分配"))]
    pub allocations: Vec<RedInvoiceAllocationLineRequest>,
}

/// 红票反向分配请求行。
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct RedInvoiceAllocationLineRequest {
    /// 被红冲的原蓝票分配。
    pub reverses_allocation_id: String,
    /// 分配含税金额。
    pub allocated_gross_amount: Amount,
    /// 分配不含税金额。
    pub allocated_net_amount: Amount,
    /// 分配税额。
    pub allocated_tax_amount: Amount,
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

/// 卡券票款复核追加请求（W13 正式复核：复核链尾锁定 + 账户复核缓存同步）。
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct AppendFundsReviewRequest {
    /// 往来子账。
    pub receivable_account_id: ReceivableAccountId,
    /// 对应 `CARD_FUNDS_REVIEW` 或 `CARD_FUNDS_DELTA_REVIEW` 任务。
    pub work_item_id: WorkItemId,
    /// 复核类型（期初复核 / 同步差额复核）。
    pub review_type: FundsReviewType,
    /// 复核结果。
    pub review_result: ReviewResult,
    /// 证据引用（与证据单据至少提供其一）。
    pub evidence_reference: Option<String>,
    /// 财务复核人。
    #[validate(custom(function = "non_blank", message = "复核人不能为空"))]
    pub reviewed_by: String,
    /// 复核时间（秒级时间戳）。
    pub reviewed_at: Instant,
}

#[cfg(test)]
mod tests {
    use super::{
        normalize_sort, CustomerReceiptListParams, InvoiceListParams, ReceivableAccountListParams, SortDir,
    };
    use entities::receivable::{CustomerReceiptStatus, InvoiceDirection, ReceivableAccountStatus};
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
}
