//! 域 D19 `payable` 的 DTO（Handler 直接复用，禁止在 handler 内重复定义同构类型）。
//!
//! 字段名与 HTTP 契约一致（api-contract.md）：分页参数扁平传递；时间一律秒级
//! 时间戳；金额一律十进制字符串；业务日期为 `YYYY-MM-DD`。
//! 契约来源：`erp-client/features/supplier-payables/types.ts`（W12）。

/// 契约目标形状的分页响应（api-contract §3）：`items` + `total` + `page` + `page_size`。
pub use application_core::PageView;
/// 排序方向。
pub use application_core::SortDir;
/// 校验文本去除首尾空白后非空。
pub(crate) use application_core::non_blank;
/// 校验排序参数（白名单 + 方向），返回归一化排序字段与方向。
pub(crate) use application_core::normalize_sort;
use application_core::{QueryIds, normalized_text, page_or_default, page_size_or_default};
use erp_core::common::time::{BusinessDate, Instant};
use erp_core::ids::{SupplierAccountId, WorkItemId};
use erp_core::money::Amount;
use serde::{Deserialize, Serialize};
use validator::Validate;

use crate::Result;
use crate::entity::payable::{EntryDirection, PayableAccountStatus, PayableEntryType, PayableSourceType};
/// 财务付款详情消费的冲正状态；由组合读模型映射退货事实，保持原 HTTP 编码。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PaymentReversalStatus {
    /// 草稿。
    Draft,
    /// 审批中。
    #[serde(rename = "IN_APPROVAL")]
    InApproval,
    /// 已过账。
    Posted,
    /// 已冲正。
    Reversed,
}

/// 应付往来子账列表允许的排序字段白名单。
pub(crate) const PAYABLE_ACCOUNT_SORT_FIELDS: &[&str] =
    &["gross_total", "settled_total", "open_total", "open_invoiceable_total", "created_at"];
/// 供应商付款单列表允许的排序字段白名单。
pub(crate) const SUPPLIER_PAYMENT_SORT_FIELDS: &[&str] = &["paid_at", "amount", "created_at"];
/// 进项发票分配列表允许的排序字段白名单。
pub(crate) const PURCHASE_INVOICE_ALLOCATION_SORT_FIELDS: &[&str] = &["created_at"];

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
#[derive(Debug, Clone, Default, Serialize, Deserialize, Validate)]
#[serde(deny_unknown_fields)]
pub struct PayableAccountListParams {
    /// 跨页必须携带当前授权和业务版本。
    #[validate(length(min = 1, max = 256))]
    pub scope_version: Option<String>,
    /// 来源采购单当前采购负责人，逗号分隔，最多 100 项；只收窄授权结果。
    pub procurement_owner_user_ids: Option<QueryIds>,
    /// 来源采购单当前业务组织，逗号分隔，最多 100 项；只收窄授权结果。
    pub org_unit_ids: Option<QueryIds>,
    /// 组织筛选是否包含有效下级；缺省为 false。
    pub include_descendants: Option<bool>,
    /// 应付来源单据稳定身份。
    pub source_document_id: Option<String>,
    /// 主体名称与关联单据号字面量关键词。
    #[validate(length(max = 200))]
    pub q: Option<String>,
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
pub struct PayableAccountListQuery {
    /// 跨页授权和业务版本。
    pub scope_version: Option<String>,
    /// 来源采购单当前采购负责人精确身份条件。
    pub procurement_owner_user_ids: Option<QueryIds>,
    /// 来源采购单当前业务组织，只收窄授权结果。
    pub org_unit_ids: Option<QueryIds>,
    /// 组织筛选是否包含有效下级。
    pub include_descendants: Option<bool>,
    /// 应付来源单据稳定身份。
    pub source_document_id: Option<String>,
    /// 主体名称与关联单据号字面量关键词。
    pub q: Option<String>,
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
    pub fn normalized(&self) -> Result<PayableAccountListQuery> {
        let (sort_by, sort_dir) = normalize_sort(&self.sort_by, &self.sort_dir, PAYABLE_ACCOUNT_SORT_FIELDS)?;
        if self.include_descendants == Some(true) && self.org_unit_ids.is_none() {
            return Err(crate::Error::ValidationError("包含下级时必须提供组织筛选".into()));
        }
        Ok(PayableAccountListQuery {
            scope_version: self.scope_version.clone(),
            procurement_owner_user_ids: self.procurement_owner_user_ids.clone(),
            org_unit_ids: self.org_unit_ids.clone(),
            include_descendants: self.include_descendants,
            source_document_id: normalized_text(self.source_document_id.as_deref()),
            q: normalized_text(self.q.as_deref()),
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
