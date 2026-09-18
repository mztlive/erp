//! 域 D19 `payable` 的 DTO（Handler 直接复用，禁止在 handler 内重复定义同构类型）。
//!
//! 字段名与 HTTP 契约一致（api-contract.md）：分页参数扁平传递；时间一律秒级
//! 时间戳；金额一律十进制字符串；业务日期为 `YYYY-MM-DD`。
//! 契约来源：`erp-client/features/supplier-payables/types.ts`（W12）。

use application_core::{QueryIds, page_or_default, page_size_or_default};
use erp_core::common::time::BusinessDate;
use erp_core::ids::{PayableAccountId, SupplierAccountId};
use erp_core::money::Amount;
use serde::{Deserialize, Serialize};
use validator::Validate;

use super::account::{PURCHASE_INVOICE_ALLOCATION_SORT_FIELDS, PageParams, non_blank, normalize_sort};
use crate::Result;
use crate::entity::payable::AllocationAction;

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
#[derive(Debug, Clone, Default, Serialize, Deserialize, Validate)]
#[serde(deny_unknown_fields)]
pub struct PurchaseInvoiceAllocationListParams {
    /// 跨页必须携带当前授权和业务版本。
    #[validate(length(min = 1, max = 256))]
    pub scope_version: Option<String>,
    /// 应付子账来源采购单的当前采购负责人，逗号分隔，最多 100 项。
    pub procurement_owner_user_ids: Option<QueryIds>,
    /// 收票经办人（进项发票登记人），逗号分隔，最多 100 项；只收窄授权结果。
    pub operator_user_ids: Option<QueryIds>,
    /// 应付子账来源采购单的当前业务组织，逗号分隔，最多 100 项；只收窄授权结果。
    pub org_unit_ids: Option<QueryIds>,
    /// 组织筛选是否包含有效下级；缺省为 false。
    pub include_descendants: Option<bool>,
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
pub struct PurchaseInvoiceAllocationListQuery {
    /// 跨页授权和业务版本。
    pub scope_version: Option<String>,
    /// 应付子账来源采购单的当前采购负责人精确身份条件。
    pub procurement_owner_user_ids: Option<QueryIds>,
    /// 收票经办人精确身份条件。
    pub operator_user_ids: Option<QueryIds>,
    /// 应付子账来源采购单的当前业务组织，只收窄授权结果。
    pub org_unit_ids: Option<QueryIds>,
    /// 组织筛选是否包含有效下级。
    pub include_descendants: Option<bool>,
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
    pub fn normalized(&self) -> Result<PurchaseInvoiceAllocationListQuery> {
        let (sort_by, sort_dir) =
            normalize_sort(&self.sort_by, &self.sort_dir, PURCHASE_INVOICE_ALLOCATION_SORT_FIELDS)?;
        if self.include_descendants == Some(true) && self.org_unit_ids.is_none() {
            return Err(crate::Error::ValidationError("包含下级时必须提供组织筛选".into()));
        }
        Ok(PurchaseInvoiceAllocationListQuery {
            scope_version: self.scope_version.clone(),
            procurement_owner_user_ids: self.procurement_owner_user_ids.clone(),
            operator_user_ids: self.operator_user_ids.clone(),
            org_unit_ids: self.org_unit_ids.clone(),
            include_descendants: self.include_descendants,
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
