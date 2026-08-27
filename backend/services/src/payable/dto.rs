//! 域 D19 `payable` 的 DTO（Handler 直接复用，禁止在 handler 内重复定义同构类型）。
//!
//! 字段名与 HTTP 契约一致（api-contract.md）：分页参数扁平传递；时间一律秒级
//! 时间戳；金额一律十进制字符串；业务日期为 `YYYY-MM-DD`。
//! 契约来源：`erp-client/features/supplier-payables/types.ts`（W12）。

use entities::common::time::{BusinessDate, Instant};
use entities::ids::{FileAssetId, PayableAccountId, PayableEntryId, SupplierAccountId, WorkItemId};
use entities::money::Amount;
use entities::payable::{
    AllocationAction, EntryDirection, PayableAccountStatus, PayableEntryType, PayableSourceType,
    PaymentAllocation, SupplierPaymentStatus,
};
use serde::{Deserialize, Serialize};
use validator::Validate;

use crate::errors::{Error, Result};
use crate::query::{normalized_text, page_or_default, page_size_or_default};

/// 应付往来子账列表允许的排序字段白名单。
pub(crate) const PAYABLE_ACCOUNT_SORT_FIELDS: &[&str] = &[
    "gross_total",
    "settled_total",
    "open_total",
    "open_invoiceable_total",
    "created_at",
];
/// 供应商付款单列表允许的排序字段白名单。
pub(crate) const SUPPLIER_PAYMENT_SORT_FIELDS: &[&str] = &["paid_at", "amount", "created_at"];
/// 进项发票分配列表允许的排序字段白名单。
pub(crate) const PURCHASE_INVOICE_ALLOCATION_SORT_FIELDS: &[&str] = &["created_at"];

/// 排序方向。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SortDir {
    /// 升序。
    Asc,
    /// 降序。
    Desc,
}

/// 归一化后的分页查询 DTO。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PageParams {
    /// 页码（1 起）。
    pub page: u64,
    /// 单页条数（已 clamp 到 1–100）。
    pub page_size: u32,
    /// 排序字段（已过白名单校验）。
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

/// 校验文本去除首尾空白后非空。
fn non_blank(value: &str) -> std::result::Result<(), validator::ValidationError> {
    if value.trim().is_empty() {
        return Err(validator::ValidationError::new("不能为空白"));
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// 应付往来子账（payable_account）
// ---------------------------------------------------------------------------

/// 应付往来子账创建请求（W12「从采购单形成应付」：子账 + 原始应付分录）。
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct CreatePayableAccountRequest {
    /// 来源单据（采购单或第二期供应商结算单）。
    #[validate(custom(function = "non_blank", message = "来源单据不能为空"))]
    pub source_document_id: String,
    /// 往来供应商。
    pub supplier_id: SupplierAccountId,
    /// 来源类型。
    pub source_type: PayableSourceType,
    /// 含税应付总额。
    pub gross_total: Amount,
    /// 可收票含税总额（缺省等于含税应付总额）。
    #[serde(default)]
    pub invoiceable_total: Option<Amount>,
    /// 到期日（`YYYY-MM-DD`）。
    pub due_date: BusinessDate,
    /// 来源修订 ID（作为分录来源修订）。
    pub source_revision_id: String,
    /// 来源单据内序号（分录来源内序号，从 1 开始）。
    #[validate(range(min = 1, message = "来源内序号必须从 1 开始"))]
    pub source_sequence: u32,
}

/// 应付分录响应视图。
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct PayableEntryView {
    /// 实体主键。
    pub id: String,
    /// 分录类型。
    pub entry_type: PayableEntryType,
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
}

/// 应付往来子账响应视图（W12 应付台账行 + 详情）。
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct PayableAccountView {
    /// 实体主键。
    pub id: String,
    /// 来源单据 ID。
    pub source_document_id: String,
    /// 来源单据业务单号（采购单号等；未知来源为空）。
    pub source_document_no: Option<String>,
    /// 往来供应商。
    pub supplier_id: String,
    /// 供应商编号（主数据缺失时为空）。
    pub supplier_no: Option<String>,
    /// 供应商名称（主数据缺失时为空）。
    pub supplier_name: Option<String>,
    /// 来源类型。
    pub source_type: PayableSourceType,
    /// 含税应付总额。
    pub gross_total: Amount,
    /// 已核销含税总额。
    pub settled_total: Amount,
    /// 剩余开放含税余额。
    pub open_total: Amount,
    /// 可收票含税总额。
    pub invoiceable_total: Amount,
    /// 净已收票含税总额。
    pub invoiced_total: Amount,
    /// 剩余可收票含税额度。
    pub open_invoiceable_total: Amount,
    /// 子账状态。
    pub status: PayableAccountStatus,
    /// 乐观锁版本。
    pub version: u64,
    /// 创建时间（秒级时间戳）。
    pub created_at: u64,
    /// 应付分录。
    pub entries: Vec<PayableEntryView>,
}

/// 应付往来子账列表查询参数。
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct PayableAccountListParams {
    /// 往来供应商筛选。
    pub supplier_id: Option<SupplierAccountId>,
    /// 来源类型筛选。
    pub source_type: Option<PayableSourceType>,
    /// 子账状态筛选。
    pub status: Option<PayableAccountStatus>,
    /// 页码（1 起）。
    #[validate(range(min = 1, message = "页码必须大于0"))]
    pub page: Option<u64>,
    /// 单页条数（1–100）。
    #[validate(range(min = 1, max = 100, message = "分页大小必须在1-100之间"))]
    pub page_size: Option<u32>,
    /// 排序字段（白名单：`gross_total`/`open_total` 等）。
    pub sort_by: Option<String>,
    /// 排序方向（`asc`/`desc`）。
    pub sort_dir: Option<String>,
}

/// 归一化后的应付往来子账列表查询参数。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct PayableAccountListQuery {
    /// 往来供应商筛选。
    pub supplier_id: Option<SupplierAccountId>,
    /// 来源类型筛选。
    pub source_type: Option<PayableSourceType>,
    /// 子账状态筛选。
    pub status: Option<PayableAccountStatus>,
    /// 分页与排序参数。
    pub paging: PageParams,
}

impl PayableAccountListParams {
    /// 归一化应付往来子账列表查询参数。
    ///
    /// # 返回
    /// 返回不依赖仓储类型的规范化查询参数。
    ///
    /// # 错误
    /// 排序字段不在白名单或排序方向非法时返回 `ValidationError`。
    pub(crate) fn normalized(&self) -> Result<PayableAccountListQuery> {
        let (sort_by, sort_dir) = normalize_sort(&self.sort_by, &self.sort_dir, PAYABLE_ACCOUNT_SORT_FIELDS)?;
        Ok(PayableAccountListQuery {
            supplier_id: self.supplier_id.clone(),
            source_type: self.source_type,
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
// 供应商付款单（supplier_payment）
// ---------------------------------------------------------------------------

/// 供应商付款单创建请求（W12 登记草稿付款；过账与分配走 `post`）。
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct CreateSupplierPaymentRequest {
    /// 付款单号（唯一，幂等键）。
    #[validate(custom(function = "non_blank", message = "付款单号不能为空"))]
    pub payment_no: String,
    /// 收款供应商。
    pub supplier_id: SupplierAccountId,
    /// 实际付款时间（秒级时间戳）。
    pub paid_at: Instant,
    /// 含税付款金额。
    pub amount: Amount,
    /// 银行流水号（辅助检索，可空）。
    pub bank_reference: Option<String>,
    /// 银行回单图片资产。
    pub bank_receipt_asset_id: FileAssetId,
}

/// 付款核销分配请求行（§8.3-1：同一供应商、净分配不超过付款金额）。
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct PaymentAllocationLineRequest {
    /// 被核销应付分录。
    pub payable_entry_id: PayableEntryId,
    /// 本次核销金额（正数）。
    pub allocated_amount: Amount,
}

/// 供应商付款过账请求（仅由最终通过动作内部消费冻结分配；HTTP 旁路已关闭）。
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct PostSupplierPaymentRequest {
    /// 核销分配行（允许保留未分配余额）。
    #[validate(length(min = 1, message = "至少提供一条核销分配"))]
    pub allocations: Vec<PaymentAllocationLineRequest>,
}

/// 供应商付款提交审批请求。客户端不得选择定义或审批人。
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
#[serde(deny_unknown_fields)]
pub struct SubmitSupplierPaymentRequest {
    /// 当前开放付款执行任务。
    pub work_item_id: WorkItemId,
    /// 查询所得任务乐观锁版本。
    #[validate(custom(function = "non_blank", message = "任务版本不能为空"))]
    #[validate(length(max = 20, message = "任务版本不能超过 20 个字符"))]
    pub expected_task_version: String,
    /// 期望的单据乐观锁版本。
    #[validate(range(min = 1, message = "乐观锁版本必须大于 0"))]
    pub expected_version: u64,
    /// 业务请求幂等键。
    #[validate(length(min = 1, max = 128, message = "幂等键不能为空"))]
    pub idempotency_key: String,
    /// 提交时冻结的待过账核销分配。
    #[validate(length(min = 1, message = "至少提供一条核销分配"))]
    pub allocations: Vec<PaymentAllocationLineRequest>,
}

/// 供应商付款原子创建并提交审批请求。
///
/// 已有草稿提交 `payment_id + expected_version`；新登记提交完整 `payment`。
/// 服务端在一个事务内完成绑定、创建、冻结分配、审批启动与审计。
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
#[serde(deny_unknown_fields)]
pub struct CommitSupplierPaymentRequest {
    /// 当前开放付款执行任务。
    pub work_item_id: WorkItemId,
    /// 查询所得任务乐观锁版本。
    #[validate(custom(function = "non_blank", message = "任务版本不能为空"))]
    #[validate(length(max = 20, message = "任务版本不能超过 20 个字符"))]
    pub expected_task_version: String,
    /// 已有付款草稿主键。
    pub payment_id: Option<String>,
    /// 已有草稿期望乐观锁版本。
    pub expected_version: Option<u64>,
    /// 新付款完整字段；提交已有草稿时为空。
    pub payment: Option<CreateSupplierPaymentRequest>,
    /// 已有草稿补充或替换的银行回单；新付款使用 `payment.bank_receipt_asset_id`。
    pub bank_receipt_asset_id: Option<FileAssetId>,
    /// 提交时冻结的待过账核销分配。
    #[validate(length(min = 1, message = "至少提供一条核销分配"))]
    pub allocations: Vec<PaymentAllocationLineRequest>,
    /// 业务请求幂等键。
    #[validate(length(min = 1, max = 128, message = "幂等键不能为空"))]
    pub idempotency_key: String,
}

/// 撤回供应商付款审批请求。原因必填。
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
#[serde(deny_unknown_fields)]
pub struct CancelSupplierPaymentApprovalRequest {
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

/// 付款核销分配视图。
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct PaymentAllocationView {
    /// 实体主键。
    pub id: String,
    /// 付款单内追加序号。
    pub allocation_seq: u32,
    /// 分配动作。
    pub allocation_action: AllocationAction,
    /// 被核销应付分录。
    pub payable_entry_id: String,
    /// 核销金额。
    pub allocated_amount: Amount,
    /// 核销发生时间（秒级时间戳）。
    pub allocated_at: Instant,
    /// 反向分配引用的原 `APPLY`。
    pub reverses_allocation_id: Option<String>,
}

/// 供应商付款单响应视图。
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct SupplierPaymentView {
    /// 实体主键。
    pub id: String,
    /// 付款单号。
    pub payment_no: String,
    /// 付款单状态。
    pub status: SupplierPaymentStatus,
    /// 收款供应商。
    pub supplier_id: String,
    /// 实际付款时间（秒级时间戳）。
    pub paid_at: Instant,
    /// 含税付款金额。
    pub amount: Amount,
    /// 银行流水号。
    pub bank_reference: Option<String>,
    /// 银行回单图片元数据；历史付款可能为空。
    pub bank_receipt: Option<SupplierPaymentBankReceiptView>,
    /// 乐观锁版本。
    pub version: u64,
    /// 创建时间（秒级时间戳）。
    pub created_at: u64,
    /// 已核销合计（净）。
    pub allocated_total: Amount,
    /// 未分配余额。
    pub unallocated_amount: Amount,
    /// 付款核销分配行。
    pub allocations: Vec<PaymentAllocationView>,
    /// 统一只读审批结构。客户端不得据此选择定义或审批人。
    pub approval: DocumentApprovalView,
}

/// 供应商付款银行回单的安全展示元数据。
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct SupplierPaymentBankReceiptView {
    /// 文件资产主键；仅供付款提交续办与受控预览使用。
    pub asset_id: String,
    /// 原始展示文件名。
    pub file_name: String,
    /// 图片内容类型。
    pub content_type: String,
    /// 文件字节大小。
    pub byte_size: u64,
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

/// 供应商付款单列表查询参数。
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct SupplierPaymentListParams {
    /// 付款单号模糊筛选。
    pub payment_no: Option<String>,
    /// 收款供应商筛选。
    pub supplier_id: Option<SupplierAccountId>,
    /// 付款单状态筛选。
    pub status: Option<SupplierPaymentStatus>,
    /// 页码（1 起）。
    #[validate(range(min = 1, message = "页码必须大于0"))]
    pub page: Option<u64>,
    /// 单页条数（1–100）。
    #[validate(range(min = 1, max = 100, message = "分页大小必须在1-100之间"))]
    pub page_size: Option<u32>,
    /// 排序字段（白名单：`paid_at`/`amount`/`created_at`）。
    pub sort_by: Option<String>,
    /// 排序方向（`asc`/`desc`）。
    pub sort_dir: Option<String>,
}

/// 归一化后的供应商付款单列表查询参数。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct SupplierPaymentListQuery {
    /// 付款单号模糊筛选。
    pub payment_no: Option<String>,
    /// 收款供应商筛选。
    pub supplier_id: Option<SupplierAccountId>,
    /// 付款单状态筛选。
    pub status: Option<SupplierPaymentStatus>,
    /// 分页与排序参数。
    pub paging: PageParams,
}

impl SupplierPaymentListParams {
    /// 归一化供应商付款单列表查询参数。
    ///
    /// # 返回
    /// 返回不依赖仓储类型的规范化查询参数。
    ///
    /// # 错误
    /// 排序字段不在白名单或排序方向非法时返回 `ValidationError`。
    pub(crate) fn normalized(&self) -> Result<SupplierPaymentListQuery> {
        let (sort_by, sort_dir) =
            normalize_sort(&self.sort_by, &self.sort_dir, SUPPLIER_PAYMENT_SORT_FIELDS)?;
        Ok(SupplierPaymentListQuery {
            payment_no: normalized_text(self.payment_no.as_deref()),
            supplier_id: self.supplier_id.clone(),
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
// 进项发票登记与分配（purchase_invoice_allocation，D19 拥有；发票经 D18 仓储）
// ---------------------------------------------------------------------------

/// 进项发票登记过账请求（§8.3-2；发票实体经 D18 `invoices()` 仓储复用）。
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct RegisterPurchaseInvoiceRequest {
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
    /// 供应商（发票往来主体对应的供应商账号）。
    pub supplier_id: SupplierAccountId,
    /// 进项发票分配行（合计必须等于发票含税金额）。
    #[validate(length(min = 1, message = "至少提供一条发票分配"))]
    pub allocations: Vec<PurchaseInvoiceAllocationLineRequest>,
}

/// 进项发票分配请求行。
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct PurchaseInvoiceAllocationLineRequest {
    /// 采购单或供应商结算单应付子账。
    pub payable_account_id: PayableAccountId,
    /// 分配含税金额。
    pub allocated_gross_amount: Amount,
    /// 分配不含税金额。
    pub allocated_net_amount: Amount,
    /// 分配税额。
    pub allocated_tax_amount: Amount,
}

/// 进项发票分配视图。
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct PurchaseInvoiceAllocationView {
    /// 实体主键。
    pub id: String,
    /// 进项发票 ID。
    pub invoice_id: String,
    /// 发票内追加序号。
    pub allocation_seq: u32,
    /// 分配动作。
    pub allocation_action: AllocationAction,
    /// 采购单或供应商结算单应付子账。
    pub payable_account_id: String,
    /// 分配含税金额。
    pub allocated_gross_amount: Amount,
    /// 分配不含税金额。
    pub allocated_net_amount: Amount,
    /// 分配税额。
    pub allocated_tax_amount: Amount,
    /// 红票反向原蓝票分配。
    pub reverses_allocation_id: Option<String>,
}

/// 进项发票登记过账响应视图（发票 + 分配行）。
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct PurchaseInvoiceRegisteredView {
    /// 发票 ID。
    pub invoice_id: String,
    /// 发票号码。
    pub invoice_no: String,
    /// 含税金额。
    pub gross_amount: Amount,
    /// 分配行。
    pub allocations: Vec<PurchaseInvoiceAllocationView>,
}

/// 进项发票分配列表查询参数（按应付子账筛选）。
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct PurchaseInvoiceAllocationListParams {
    /// 应付往来子账筛选。
    pub payable_account_id: Option<PayableAccountId>,
    /// 页码（1 起）。
    #[validate(range(min = 1, message = "页码必须大于0"))]
    pub page: Option<u64>,
    /// 单页条数（1–100）。
    #[validate(range(min = 1, max = 100, message = "分页大小必须在1-100之间"))]
    pub page_size: Option<u32>,
    /// 排序字段（白名单：`created_at`）。
    pub sort_by: Option<String>,
    /// 排序方向（`asc`/`desc`）。
    pub sort_dir: Option<String>,
}

/// 归一化后的进项发票分配列表查询参数。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct PurchaseInvoiceAllocationListQuery {
    /// 应付往来子账筛选。
    pub payable_account_id: Option<PayableAccountId>,
    /// 分页与排序参数。
    pub paging: PageParams,
}

impl PurchaseInvoiceAllocationListParams {
    /// 归一化进项发票分配列表查询参数。
    ///
    /// # 返回
    /// 返回不依赖仓储类型的规范化查询参数。
    ///
    /// # 错误
    /// 排序字段不在白名单或排序方向非法时返回 `ValidationError`。
    pub(crate) fn normalized(&self) -> Result<PurchaseInvoiceAllocationListQuery> {
        let (sort_by, sort_dir) = normalize_sort(
            &self.sort_by,
            &self.sort_dir,
            PURCHASE_INVOICE_ALLOCATION_SORT_FIELDS,
        )?;
        Ok(PurchaseInvoiceAllocationListQuery {
            payable_account_id: self.payable_account_id.clone(),
            paging: PageParams {
                page: page_or_default(self.page),
                page_size: page_size_or_default(self.page_size),
                sort_by,
                sort_dir,
            },
        })
    }
}

/// 分配视图装配辅助（金额正数校验由实体层完成）。
impl From<&PaymentAllocation> for PaymentAllocationView {
    /// 从付款核销分配实体构造视图。
    ///
    /// # 参数
    /// * `allocation` - 付款核销分配实体
    ///
    /// # 返回
    /// 返回响应视图。
    fn from(allocation: &PaymentAllocation) -> Self {
        Self {
            id: allocation.base.id.clone(),
            allocation_seq: allocation.allocation_seq,
            allocation_action: allocation.allocation_action,
            payable_entry_id: allocation.payable_entry_id.to_string(),
            allocated_amount: allocation.allocated_amount,
            allocated_at: allocation.allocated_at,
            reverses_allocation_id: allocation
                .reverses_allocation_id
                .as_ref()
                .map(|id| id.to_string()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{
        normalize_sort, PayableAccountListParams, PurchaseInvoiceAllocationListParams, SortDir,
        SupplierPaymentListParams,
    };
    use entities::payable::{PayableAccountStatus, PayableSourceType, SupplierPaymentStatus};
    use validator::Validate;

    #[test]
    fn sort_whitelist_rejects_unknown_fields_and_directions() {
        assert!(normalize_sort(&Some("name".to_string()), &None, &["created_at"]).is_err());
        assert!(normalize_sort(&None, &Some("up".to_string()), &["created_at"]).is_err());

        let (field, direction) = normalize_sort(&None, &None, &["created_at"]).unwrap();
        assert_eq!(field, "created_at");
        assert_eq!(direction, SortDir::Desc);
    }

    #[test]
    fn payable_account_list_params_normalize_filters_and_paging() {
        let params = PayableAccountListParams {
            supplier_id: None,
            source_type: Some(PayableSourceType::PurchaseOrder),
            status: Some(PayableAccountStatus::Open),
            page: Some(2),
            page_size: Some(50),
            sort_by: Some("open_total".to_string()),
            sort_dir: Some("asc".to_string()),
        };
        let query = params.normalized().unwrap();
        assert_eq!(query.source_type, Some(PayableSourceType::PurchaseOrder));
        assert_eq!(query.status, Some(PayableAccountStatus::Open));
        assert_eq!(query.paging.page, 2);
        assert_eq!(query.paging.page_size, 50);
        assert_eq!(query.paging.sort_by, "open_total");
        assert_eq!(query.paging.sort_dir, SortDir::Asc);
    }

    #[test]
    fn payment_and_allocation_list_params_normalize() {
        let payment = SupplierPaymentListParams {
            payment_no: Some(" PAY-1 ".to_string()),
            supplier_id: None,
            status: Some(SupplierPaymentStatus::Posted),
            page: None,
            page_size: None,
            sort_by: None,
            sort_dir: None,
        };
        let query = payment.normalized().unwrap();
        assert_eq!(query.payment_no.as_deref(), Some("PAY-1"));
        assert_eq!(query.status, Some(SupplierPaymentStatus::Posted));

        let allocations = PurchaseInvoiceAllocationListParams {
            payable_account_id: None,
            page: Some(1),
            page_size: Some(25),
            sort_by: Some("created_at".to_string()),
            sort_dir: None,
        };
        let query = allocations.normalized().unwrap();
        assert_eq!(query.paging.page_size, 25);
    }

    #[test]
    fn submit_and_cancel_requests_reject_client_assignee_choice() {
        use super::{CancelSupplierPaymentApprovalRequest, SubmitSupplierPaymentRequest};

        assert!(
            serde_json::from_value::<SubmitSupplierPaymentRequest>(serde_json::json!({
                "work_item_id": "wi-1",
                "expected_task_version": "3",
                "expected_version": 1,
                "idempotency_key": "k1",
                "allocations": [{"payable_entry_id": "pe-1", "allocated_amount": "10"}],
                "definition_id": "forged"
            }))
            .is_err()
        );
        let submit: SubmitSupplierPaymentRequest = serde_json::from_value(serde_json::json!({
            "work_item_id": "wi-1",
            "expected_task_version": "3",
            "expected_version": 1,
            "idempotency_key": "k1",
            "allocations": [{"payable_entry_id": "pe-1", "allocated_amount": "10"}]
        }))
        .unwrap();
        assert_eq!(submit.work_item_id.to_string(), "wi-1");
        assert_eq!(submit.expected_task_version, "3");
        assert_eq!(submit.expected_version, 1);
        assert!(
            serde_json::from_value::<CancelSupplierPaymentApprovalRequest>(serde_json::json!({
                "expected_version": 1,
                "reason": "改金额",
                "idempotency_key": "k2",
                "assignee": "forged"
            }))
            .is_err()
        );
    }

    #[test]
    fn list_params_reject_unbounded_page_size() {
        let params = PayableAccountListParams {
            supplier_id: None,
            source_type: None,
            status: None,
            page: Some(0),
            page_size: Some(u32::MAX),
            sort_by: None,
            sort_dir: None,
        };
        assert!(params.validate().is_err());
    }
}
