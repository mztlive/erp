//! 应收查询参数、列表/详情视图与分页归一化。

use application_core::{QueryIds, normalized_text, page_or_default, page_size_or_default};
use erp_core::common::time::{BusinessDate, Instant};
use erp_core::ids::{CustomerAccountId, PartyId, ReceivableAccountId};
use erp_core::money::Amount;
use serde::{Deserialize, Serialize};
use validator::Validate;

use super::{SortDir, normalize_sort};
use crate::Result;
use crate::entity::receivable::{
    AllocationAction, CustomerReceiptStatus, EntryDirection, InvoiceDirection, InvoiceKind, InvoiceStatus,
    ReceivableAccountStatus, ReceivableEntryType,
};

/// 应收往来子账列表允许的排序字段白名单（api-contract §4：Service 层校验）。
pub(crate) const RECEIVABLE_ACCOUNT_SORT_FIELDS: &[&str] =
    &["account_seq", "gross_total", "settled_total", "open_total", "open_invoiceable_total", "created_at"];
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

/// 应收往来子账列表查询参数（分页参数与筛选字段扁平传递）。
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
#[serde(deny_unknown_fields)]
pub struct ReceivableAccountListParams {
    /// 跨页必须携带当前授权和业务版本。
    #[validate(length(min = 1, max = 256))]
    pub scope_version: Option<String>,
    /// 关联销售单当前负责销售，逗号分隔，最多 100 项；只收窄授权结果。
    pub sales_owner_user_ids: Option<QueryIds>,
    /// 子账登记经办人，逗号分隔，最多 100 项；只收窄授权结果。
    pub operator_user_ids: Option<QueryIds>,
    /// 关联销售单当前业务组织，逗号分隔，最多 100 项；只收窄授权结果。
    pub org_unit_ids: Option<QueryIds>,
    /// 组织筛选是否包含有效下级；缺省为 false。
    pub include_descendants: Option<bool>,
    /// 子账、销售单、客户或往来主体关键字。
    #[validate(length(max = 200))]
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
pub struct ReceivableAccountListQuery {
    /// 跨页授权和业务版本。
    pub scope_version: Option<String>,
    /// 关联销售单当前负责销售精确身份条件。
    pub sales_owner_user_ids: Option<QueryIds>,
    /// 子账登记经办人精确身份条件。
    pub operator_user_ids: Option<QueryIds>,
    /// 关联销售单当前业务组织，只收窄授权结果。
    pub org_unit_ids: Option<QueryIds>,
    /// 组织筛选是否包含有效下级。
    pub include_descendants: Option<bool>,
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
    pub fn normalized(&self) -> Result<ReceivableAccountListQuery> {
        let (sort_by, sort_dir) =
            normalize_sort(&self.sort_by, &self.sort_dir, RECEIVABLE_ACCOUNT_SORT_FIELDS)?;
        if self.include_descendants == Some(true) && self.org_unit_ids.is_none() {
            return Err(crate::Error::ValidationError("包含下级时必须提供组织筛选".into()));
        }
        Ok(ReceivableAccountListQuery {
            scope_version: self.scope_version.clone(),
            sales_owner_user_ids: self.sales_owner_user_ids.clone(),
            operator_user_ids: self.operator_user_ids.clone(),
            org_unit_ids: self.org_unit_ids.clone(),
            include_descendants: self.include_descendants,
            q: normalized_text(self.q.as_deref()),
            account_id: self.account_id.clone(),
            customer_id: self.customer_id.clone(),
            counterparty_party_id: self.counterparty_party_id.clone(),
            status: self.status,
            sales_order_id: normalized_text(self.sales_order_id.as_deref()),
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

/// 回款经办人角色；登记与核销分别查询，不得混用。
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ReceiptOperatorKind {
    /// 登记经办人（回款单创建人）。
    Register,
    /// 核销经办人（提交核销分配的执行人）。
    Settle,
}

/// 客户回款单列表查询参数（分页参数与筛选字段扁平传递）。
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
#[serde(deny_unknown_fields)]
pub struct CustomerReceiptListParams {
    /// 跨页必须携带当前授权和业务版本。
    #[validate(length(min = 1, max = 256))]
    pub scope_version: Option<String>,
    /// 核销关联销售单的当前负责销售，逗号分隔，最多 100 项；只收窄授权结果。
    pub sales_owner_user_ids: Option<QueryIds>,
    /// 回款经办人，逗号分隔，最多 100 项；动作类型由 `operator_kind` 显式选择。
    pub operator_user_ids: Option<QueryIds>,
    /// 经办人动作类型；提供经办人条件时必填。
    pub operator_kind: Option<ReceiptOperatorKind>,
    /// 核销关联销售单的当前业务组织，逗号分隔，最多 100 项；只收窄授权结果。
    pub org_unit_ids: Option<QueryIds>,
    /// 组织筛选是否包含有效下级；缺省为 false。
    pub include_descendants: Option<bool>,
    /// 主体名称与关联单据号字面量关键词。
    #[validate(length(max = 200))]
    pub q: Option<String>,
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
pub struct CustomerReceiptListQuery {
    /// 跨页授权和业务版本。
    pub scope_version: Option<String>,
    /// 核销关联销售单的当前负责销售精确身份条件。
    pub sales_owner_user_ids: Option<QueryIds>,
    /// 回款经办人精确身份条件。
    pub operator_user_ids: Option<QueryIds>,
    /// 经办人动作类型。
    pub operator_kind: Option<ReceiptOperatorKind>,
    /// 核销关联销售单的当前业务组织，只收窄授权结果。
    pub org_unit_ids: Option<QueryIds>,
    /// 组织筛选是否包含有效下级。
    pub include_descendants: Option<bool>,
    /// 主体名称与关联单据号字面量关键词。
    pub q: Option<String>,
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
    pub fn normalized(&self) -> Result<CustomerReceiptListQuery> {
        let (sort_by, sort_dir) =
            normalize_sort(&self.sort_by, &self.sort_dir, CUSTOMER_RECEIPT_SORT_FIELDS)?;
        if self.operator_user_ids.is_some() && self.operator_kind.is_none() {
            return Err(crate::Error::ValidationError("查询经办人时必须显式选择登记或核销".into()));
        }
        if self.include_descendants == Some(true) && self.org_unit_ids.is_none() {
            return Err(crate::Error::ValidationError("包含下级时必须提供组织筛选".into()));
        }
        Ok(CustomerReceiptListQuery {
            scope_version: self.scope_version.clone(),
            sales_owner_user_ids: self.sales_owner_user_ids.clone(),
            operator_user_ids: self.operator_user_ids.clone(),
            operator_kind: self.operator_kind,
            org_unit_ids: self.org_unit_ids.clone(),
            include_descendants: self.include_descendants,
            q: normalized_text(self.q.as_deref()),
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
    /// 销项蓝票消耗的批准申请；历史及进项发票为空。
    pub sales_invoice_request_id: Option<String>,
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
#[serde(deny_unknown_fields)]
pub struct InvoiceListParams {
    /// 跨页必须携带当前授权和业务版本。
    #[validate(length(min = 1, max = 256))]
    pub scope_version: Option<String>,
    /// 分配关联销售单的当前负责销售，逗号分隔，最多 100 项；只收窄授权结果。
    pub sales_owner_user_ids: Option<QueryIds>,
    /// 分配关联采购单的当前采购负责人，逗号分隔，最多 100 项；只收窄授权结果。
    pub procurement_owner_user_ids: Option<QueryIds>,
    /// 发票登记经办人（发票创建人），逗号分隔，最多 100 项；只收窄授权结果。
    pub operator_user_ids: Option<QueryIds>,
    /// 分配关联销售单的当前业务组织，逗号分隔，最多 100 项；只收窄授权结果。
    pub org_unit_ids: Option<QueryIds>,
    /// 组织筛选是否包含有效下级；缺省为 false。
    pub include_descendants: Option<bool>,
    /// 主体名称与关联单据号字面量关键词。
    #[validate(length(max = 200))]
    pub q: Option<String>,
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
pub struct InvoiceListQuery {
    /// 跨页授权和业务版本。
    pub scope_version: Option<String>,
    /// 分配关联销售单的当前负责销售精确身份条件。
    pub sales_owner_user_ids: Option<QueryIds>,
    /// 分配关联采购单的当前采购负责人精确身份条件。
    pub procurement_owner_user_ids: Option<QueryIds>,
    /// 发票登记经办人精确身份条件。
    pub operator_user_ids: Option<QueryIds>,
    /// 分配关联销售单的当前业务组织，只收窄授权结果。
    pub org_unit_ids: Option<QueryIds>,
    /// 组织筛选是否包含有效下级。
    pub include_descendants: Option<bool>,
    /// 主体名称与关联单据号字面量关键词。
    pub q: Option<String>,
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
    pub fn normalized(&self) -> Result<InvoiceListQuery> {
        let (sort_by, sort_dir) = normalize_sort(&self.sort_by, &self.sort_dir, INVOICE_SORT_FIELDS)?;
        if self.include_descendants == Some(true) && self.org_unit_ids.is_none() {
            return Err(crate::Error::ValidationError("包含下级时必须提供组织筛选".into()));
        }
        Ok(InvoiceListQuery {
            scope_version: self.scope_version.clone(),
            sales_owner_user_ids: self.sales_owner_user_ids.clone(),
            procurement_owner_user_ids: self.procurement_owner_user_ids.clone(),
            operator_user_ids: self.operator_user_ids.clone(),
            org_unit_ids: self.org_unit_ids.clone(),
            include_descendants: self.include_descendants,
            q: normalized_text(self.q.as_deref()),
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
