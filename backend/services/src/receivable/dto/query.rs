//! 应收查询参数、列表/详情视图与分页归一化。

use entities::common::time::{BusinessDate, Instant};
use entities::ids::{CustomerAccountId, PartyId, ReceivableAccountId};
use entities::money::Amount;
use entities::receivable::{
    AccountReviewStatus, AllocationAction, CustomerReceiptStatus, EntryDirection, FundsReviewType,
    InvoiceDirection, InvoiceKind, InvoiceStatus, ReceivableAccountStatus, ReceivableEntryType, ReviewResult,
};
use serde::{Deserialize, Serialize};
use validator::Validate;

use super::card_funds::{
    CardFundsReviewActionBlockerView, CardFundsReviewAllowedAction, CardFundsReviewType,
    ReceivableInvoiceFactView, ReceivableReceiptFactView,
};
use super::{normalize_sort, SortDir};
use crate::errors::Result;
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

// ---------------------------------------------------------------------------
// 应收往来子账（receivable_account）
// ---------------------------------------------------------------------------

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

/// 应收往来子账列表摘要。
///
/// 列表只返回本页展示与建立核销目标所需字段；复核链、票款事实版本和当前任务
/// 等操作上下文由详情接口按单据读取。
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct ReceivableAccountSummaryView {
    /// 实体主键。
    pub id: String,
    /// 来源销售单。
    pub sales_order_id: String,
    /// 销售单业务单号。
    pub sales_order_no: String,
    /// 往来子账序号。
    pub account_seq: u32,
    /// 企业客户经营归属。
    pub customer_id: String,
    /// 当前销售版本冻结的客户名称。
    pub customer_name: String,
    /// 收款和开票往来主体。
    pub counterparty_party_id: String,
    /// 当前销售版本冻结的往来主体名称。
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
    /// 创建时间（秒级时间戳）。
    pub created_at: u64,
    /// 建立回款/发票核销目标所需的应收分录。
    pub entries: Vec<ReceivableEntryView>,
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

/// 应收往来子账列表查询参数（分页参数与筛选字段扁平传递）。
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct ReceivableAccountListParams {
    /// 子账、销售单、客户或往来主体关键字。
    pub q: Option<String>,
    /// 子账主键筛选。
    pub account_id: Option<ReceivableAccountId>,
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
    /// 子账、销售单、客户或往来主体关键字。
    pub q: Option<String>,
    /// 子账主键筛选。
    pub account_id: Option<ReceivableAccountId>,
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
            q: normalized_text(self.q.as_deref()),
            account_id: self.account_id.clone(),
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
    /// 来源销售单筛选；由 Service 解析核销分配关系。
    pub sales_order_id: Option<String>,
    /// 应收子账筛选；由 Service 解析核销分配关系。
    pub receivable_account_id: Option<ReceivableAccountId>,
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
    /// 来源销售单筛选。
    pub sales_order_id: Option<String>,
    /// 应收子账筛选。
    pub receivable_account_id: Option<ReceivableAccountId>,
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
            sales_order_id: normalized_text(self.sales_order_id.as_deref()),
            receivable_account_id: self.receivable_account_id.clone(),
            paging: PageParams {
                page: page_or_default(self.page),
                page_size: page_size_or_default(self.page_size),
                sort_by,
                sort_dir,
            },
        })
    }
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
    /// 来源销售单筛选；由 Service 解析发票分配关系。
    pub sales_order_id: Option<String>,
    /// 应收子账筛选；由 Service 解析发票分配关系。
    pub receivable_account_id: Option<ReceivableAccountId>,
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
    /// 来源销售单筛选。
    pub sales_order_id: Option<String>,
    /// 应收子账筛选。
    pub receivable_account_id: Option<ReceivableAccountId>,
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
            sales_order_id: normalized_text(self.sales_order_id.as_deref()),
            receivable_account_id: self.receivable_account_id.clone(),
            paging: PageParams {
                page: page_or_default(self.page),
                page_size: page_size_or_default(self.page_size),
                sort_by,
                sort_dir,
            },
        })
    }
}
