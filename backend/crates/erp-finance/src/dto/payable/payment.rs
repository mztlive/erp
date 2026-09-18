//! 域 D19 `payable` 的 DTO（Handler 直接复用，禁止在 handler 内重复定义同构类型）。
//!
//! 字段名与 HTTP 契约一致（api-contract.md）：分页参数扁平传递；时间一律秒级
//! 时间戳；金额一律十进制字符串；业务日期为 `YYYY-MM-DD`。
//! 契约来源：`erp-client/features/supplier-payables/types.ts`（W12）。

use application_core::{QueryIds, normalized_text, page_or_default, page_size_or_default};
use erp_core::common::time::Instant;
use erp_core::ids::{FileAssetId, PayableEntryId, SupplierAccountId, WorkItemId};
use erp_core::money::Amount;
use serde::{Deserialize, Serialize};
use validator::Validate;

use super::account::{
    PageParams, PaymentRecipientView, PaymentReversalStatus, SUPPLIER_PAYMENT_SORT_FIELDS, non_blank,
    normalize_sort,
};
use crate::Result;
use crate::entity::payable::{
    AllocationAction, PayableSourceType, PaymentAllocation, PendingPaymentAllocation, SupplierPaymentStatus,
};

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

impl PaymentAllocationLineRequest {
    /// 将请求行转换为领域待过账核销行。
    ///
    /// # 参数
    /// * `&self` - 请求分配行（分录 ID + 核销金额）
    ///
    /// # 返回
    /// 金额为正时返回 [`PendingPaymentAllocation`]，顺序与调用方输入一致。
    ///
    /// # 错误
    /// 金额非正时返回实体层 [`crate::Error::Logic`]（文案保持
    /// `付款金额必须为正数`）。
    ///
    /// # 约束
    /// 纯转换，不触及 I/O、时钟或 ID 生成；正数校验由
    /// [`PendingPaymentAllocation::new`] 唯一承担，本方法不复制规则。
    pub fn to_pending(&self) -> Result<PendingPaymentAllocation> {
        PendingPaymentAllocation::new(self.payable_entry_id.clone(), self.allocated_amount)
            .map_err(Into::into)
    }
}

/// 合并付款中除当前任务外的一条付款执行任务身份。
#[derive(Debug, Clone, Serialize, Deserialize, Validate, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct PaymentExecutionTaskRef {
    /// 开放付款执行任务。
    pub work_item_id: WorkItemId,
    /// 查询所得任务乐观锁版本。
    #[validate(custom(function = "non_blank", message = "任务版本不能为空"))]
    #[validate(length(max = 20, message = "任务版本不能超过 20 个字符"))]
    pub expected_task_version: String,
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
    /// 与当前任务一并核销的其它开放付款执行任务；缺省表示只付当前任务。
    #[serde(default)]
    #[validate(length(max = 49, message = "一次合并付款最多包含 50 条任务"))]
    #[validate(nested)]
    pub additional_work_items: Vec<PaymentExecutionTaskRef>,
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

impl CommitSupplierPaymentRequest {
    /// 将全部请求分配行转换为领域待过账集合。
    ///
    /// # 参数
    /// * `&self` - 原子付款提交请求
    ///
    /// # 返回
    /// 全部行合法时返回与输入顺序一致的 [`PendingPaymentAllocation`] 集合。
    ///
    /// # 错误
    /// 任一行金额非正时返回 [`crate::Error::Logic`]，首个失败即短路。
    ///
    /// # 约束
    /// 纯转换；净额、分录余额、序号与分配实体构造由
    /// [`crate::entity::payable::PaymentAllocationLedger`] 承担，本方法不下沉
    /// 那些规则，也不读取数据库。
    pub fn pending_allocations(&self) -> Result<Vec<PendingPaymentAllocation>> {
        self.allocations.iter().map(PaymentAllocationLineRequest::to_pending).collect()
    }
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
            reverses_allocation_id: allocation.reverses_allocation_id.as_ref().map(|id| id.to_string()),
        }
    }
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
#[derive(Debug, Clone, Default, Serialize, Deserialize, Validate)]
#[serde(deny_unknown_fields)]
pub struct SupplierPaymentListParams {
    /// 跨页必须携带当前授权和业务版本。
    #[validate(length(min = 1, max = 256))]
    pub scope_version: Option<String>,
    /// 核销来源采购单的当前采购负责人，逗号分隔，最多 100 项；只收窄授权结果。
    pub procurement_owner_user_ids: Option<QueryIds>,
    /// 付款经办人（付款提交执行人），逗号分隔，最多 100 项；只收窄授权结果。
    pub operator_user_ids: Option<QueryIds>,
    /// 核销来源采购单的当前业务组织，逗号分隔，最多 100 项；只收窄授权结果。
    pub org_unit_ids: Option<QueryIds>,
    /// 组织筛选是否包含有效下级；缺省为 false。
    pub include_descendants: Option<bool>,
    /// 付款单号或供应商名称关键词。
    #[validate(length(max = 200))]
    pub q: Option<String>,
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
pub struct SupplierPaymentListQuery {
    /// 跨页授权和业务版本。
    pub scope_version: Option<String>,
    /// 核销来源采购单的当前采购负责人精确身份条件。
    pub procurement_owner_user_ids: Option<QueryIds>,
    /// 付款经办人精确身份条件。
    pub operator_user_ids: Option<QueryIds>,
    /// 核销来源采购单的当前业务组织，只收窄授权结果。
    pub org_unit_ids: Option<QueryIds>,
    /// 组织筛选是否包含有效下级。
    pub include_descendants: Option<bool>,
    /// 付款单号或供应商名称关键词。
    pub q: Option<String>,
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
    pub fn normalized(&self) -> Result<SupplierPaymentListQuery> {
        let (sort_by, sort_dir) =
            normalize_sort(&self.sort_by, &self.sort_dir, SUPPLIER_PAYMENT_SORT_FIELDS)?;
        if self.include_descendants == Some(true) && self.org_unit_ids.is_none() {
            return Err(crate::Error::ValidationError("包含下级时必须提供组织筛选".into()));
        }
        Ok(SupplierPaymentListQuery {
            scope_version: self.scope_version.clone(),
            procurement_owner_user_ids: self.procurement_owner_user_ids.clone(),
            operator_user_ids: self.operator_user_ids.clone(),
            org_unit_ids: self.org_unit_ids.clone(),
            include_descendants: self.include_descendants,
            q: normalized_text(self.q.as_deref()),
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
