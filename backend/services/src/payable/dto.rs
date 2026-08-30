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
use entities::returns::PaymentReversalStatus;
use serde::{Deserialize, Serialize};
use validator::Validate;

use crate::errors::Result;
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
pub use crate::query::SortDir;

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
pub(crate) use crate::query::normalize_sort;

/// 契约目标形状的分页响应（api-contract §3）：`items` + `total` + `page` + `page_size`。
pub use crate::query::PageView;

/// 校验文本去除首尾空白后非空。
use crate::query::non_blank;

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
    /// 来源业务单号（采购单号或结算单号；缺失时为空，不得回退内部 ID）。
    pub source_document_no: Option<String>,
    /// 来源内序号。
    pub source_sequence: u32,
    /// 入账时间（秒级时间戳）。
    pub posted_at: Instant,
}

/// 应付往来子账列表摘要。
///
/// 列表契约只包含本页展示和建立分配目标所需字段；收款账户等敏感详情必须通过
/// 详情或受控揭示接口读取。
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct PayableAccountSummaryView {
    /// 实体主键。
    pub id: String,
    /// 来源单据 ID。
    pub source_document_id: String,
    /// 来源单据业务单号。
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
    /// 建立付款/发票分配目标所需的应付分录。
    pub entries: Vec<PayableEntryView>,
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
    /// 当前默认收款账户；未配置时为空并禁止付款。
    pub payment_recipient: Option<PaymentRecipientView>,
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

/// 付款工作台使用的收款账户安全摘要。
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct PaymentRecipientView {
    /// 收款银行账户事实行主键，用于提交时检测主数据漂移。
    pub bank_account_id: String,
    /// 收款账户乐观锁版本，用于提交时阻止并发主数据变更。
    pub version: u64,
    /// 收款户名。
    pub account_name: String,
    /// 开户银行。
    pub bank_name: String,
    /// 开户支行。
    pub bank_branch_name: Option<String>,
    /// 收款账号掩码。
    pub account_number_masked: String,
}

/// 付款工作台揭示完整收款账号请求。
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
#[serde(deny_unknown_fields)]
pub struct RevealPaymentRecipientRequest {
    /// 当前开放付款执行任务。
    pub work_item_id: WorkItemId,
    /// 查询所得任务乐观锁版本。
    #[validate(custom(function = "non_blank", message = "任务版本不能为空"))]
    #[validate(length(max = 20, message = "任务版本不能超过 20 个字符"))]
    pub expected_task_version: String,
    /// 页面展示的收款账户事实行主键。
    #[validate(custom(function = "non_blank", message = "收款账户不能为空"))]
    #[validate(length(max = 64, message = "收款账户标识不能超过 64 个字符"))]
    pub expected_bank_account_id: String,
    /// 页面展示的收款账户乐观锁版本。
    #[validate(range(min = 1, message = "收款账户版本必须大于0"))]
    pub expected_bank_account_version: u64,
}

/// 付款工作台短时揭示的完整收款账号。
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct PaymentRecipientRevealView {
    /// 收款银行账户事实行主键。
    pub bank_account_id: String,
    /// 完整收款账号。只允许响应当前任务责任人，不得写入日志或持久化副本。
    pub account_number: String,
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

/// 供应商付款登记字段（仅作为付款任务原子提交的一部分使用）。
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

/// 付款核销分配请求行（§8.3-1：同一供应商、分配合计等于付款金额）。
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct PaymentAllocationLineRequest {
    /// 被核销应付分录。
    pub payable_entry_id: PayableEntryId,
    /// 本次核销金额（正数）。
    pub allocated_amount: Amount,
}

/// 供应商付款原子登记并过账请求。
///
/// 服务端在一个事务内完成任务责任校验、收款账户冻结、付款、核销与审计。
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
#[serde(deny_unknown_fields)]
pub struct CommitSupplierPaymentRequest {
    /// 当前开放付款执行任务。
    pub work_item_id: WorkItemId,
    /// 查询所得任务乐观锁版本。
    #[validate(custom(function = "non_blank", message = "任务版本不能为空"))]
    #[validate(length(max = 20, message = "任务版本不能超过 20 个字符"))]
    pub expected_task_version: String,
    /// 页面展示的当前默认收款账户；提交时不一致必须刷新重试。
    #[validate(custom(function = "non_blank", message = "收款账户不能为空"))]
    #[validate(length(max = 64, message = "收款账户标识不能超过 64 个字符"))]
    pub expected_payee_bank_account_id: String,
    /// 页面展示的当前默认收款账户乐观锁版本。
    #[validate(range(min = 1, message = "收款账户版本必须大于0"))]
    pub expected_payee_bank_account_version: u64,
    /// 本次付款完整字段。
    pub payment: CreateSupplierPaymentRequest,
    /// 提交时冻结的待过账核销分配。
    #[validate(length(min = 1, message = "至少提供一条核销分配"))]
    pub allocations: Vec<PaymentAllocationLineRequest>,
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
    /// 被核销应付子账；分录缺失时为空。
    pub payable_account_id: Option<String>,
    /// 应付来源类型；分录或子账缺失时为空。
    pub source_type: Option<PayableSourceType>,
    /// 来源单据内部身份；缺失时为空，界面不得当单号展示。
    pub source_document_id: Option<String>,
    /// 来源业务单号（采购单号或结算单号；缺失时为空）。
    pub source_document_no: Option<String>,
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
    /// 供应商编号（主数据缺失时为空）。
    pub supplier_no: Option<String>,
    /// 供应商名称（主数据缺失时为空，不得回退供应商 ID）。
    pub supplier_name: Option<String>,
    /// 付款时冻结的收款账户摘要；历史付款可能为空。
    pub payment_recipient: Option<PaymentRecipientView>,
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
    /// 关联付款冲正记录，按创建时间倒序；仅作追踪，不计入付款金额。
    pub related_reversals: Vec<SupplierPaymentReversalView>,
}

/// 供应商付款关联的冲正记录摘要。
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct SupplierPaymentReversalView {
    /// 付款冲正主键，仅供受控详情路由使用。
    pub id: String,
    /// 冲正单号。
    pub reversal_no: String,
    /// 冲正状态。
    pub status: PaymentReversalStatus,
    /// 冲正原因。
    pub reason_text: String,
    /// 冲正金额。
    pub amount: Amount,
    /// 冲正发生时间。
    pub occurred_at: Instant,
    /// 创建时间（秒级时间戳）。
    pub created_at: u64,
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
    /// 业务命令幂等键；同键同载荷回放首次登记结果。
    #[validate(length(min = 1, max = 128, message = "幂等键不能为空"))]
    pub idempotency_key: String,
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
            payable_account_id: None,
            source_type: None,
            source_document_id: None,
            source_document_no: None,
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

    /// 分配实体转视图时来源展示字段为空，由展示装配补全，避免把分录 ID 当单号。
    #[test]
    fn payment_allocation_view_leaves_source_blank_before_enrichment() {
        use std::str::FromStr;

        use entities::common::time::Instant;
        use entities::ids::{PayableEntryId, PaymentAllocationId, SupplierPaymentId};
        use entities::money::Amount;
        use entities::payable::{AllocationAction, PaymentAllocation, PaymentAllocationData};

        use super::PaymentAllocationView;

        let allocation = PaymentAllocation::new(
            PaymentAllocationId::new("alloc-1"),
            PaymentAllocationData {
                supplier_payment_id: SupplierPaymentId::new("pay-1"),
                payable_entry_id: PayableEntryId::new("pe-1"),
                allocation_seq: 1,
                allocation_action: AllocationAction::Apply,
                allocated_amount: Amount::from_str("10.00").unwrap(),
                allocated_at: Instant::from_unix_secs(1_700_000_000),
                reverses_allocation_id: None,
            },
        )
        .unwrap();
        let view = PaymentAllocationView::from(&allocation);
        assert_eq!(view.payable_entry_id, "pe-1");
        assert!(view.payable_account_id.is_none());
        assert!(view.source_document_no.is_none());
        assert!(view.source_document_id.is_none());
    }

    /// 付款提交必须携带页面已核对的收款账户身份与版本。
    #[test]
    fn payment_commit_requires_expected_recipient_account() {
        use super::CommitSupplierPaymentRequest;

        let without_recipient = serde_json::json!({
            "work_item_id": "wi-1",
            "expected_task_version": "3",
            "payment": {
                "payment_no": "FK-1",
                "supplier_id": "supplier-1",
                "paid_at": 1,
                "amount": "10.00",
                "bank_reference": null,
                "bank_receipt_asset_id": "asset-1"
            },
            "allocations": [{"payable_entry_id": "pe-1", "allocated_amount": "10"}],
            "idempotency_key": "k1"
        });
        assert!(serde_json::from_value::<CommitSupplierPaymentRequest>(without_recipient).is_err());

        let blank_recipient = serde_json::from_value::<CommitSupplierPaymentRequest>(serde_json::json!({
            "work_item_id": "wi-1",
            "expected_task_version": "3",
            "expected_payee_bank_account_id": " ",
            "expected_payee_bank_account_version": 1,
            "payment": {
                "payment_no": "FK-1",
                "supplier_id": "supplier-1",
                "paid_at": 1,
                "amount": "10.00",
                "bank_reference": null,
                "bank_receipt_asset_id": "asset-1"
            },
            "allocations": [{"payable_entry_id": "pe-1", "allocated_amount": "10"}],
            "idempotency_key": "k1"
        }))
        .expect("空白收款账户可完成协议反序列化");
        assert!(blank_recipient.validate().is_err());

        let missing_version = serde_json::json!({
            "work_item_id": "wi-1",
            "expected_task_version": "3",
            "expected_payee_bank_account_id": "bank-1",
            "payment": {
                "payment_no": "FK-1",
                "supplier_id": "supplier-1",
                "paid_at": 1,
                "amount": "10.00",
                "bank_reference": null,
                "bank_receipt_asset_id": "asset-1"
            },
            "allocations": [{"payable_entry_id": "pe-1", "allocated_amount": "10"}],
            "idempotency_key": "k1"
        });
        assert!(serde_json::from_value::<CommitSupplierPaymentRequest>(missing_version).is_err());
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
