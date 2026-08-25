//! 域 D21 `returns` 的 DTO（Handler 直接复用，禁止在 handler 内重复定义同构类型）。
//!
//! 字段名与 HTTP 契约一致（api-contract.md）：分页参数扁平传递；时间一律秒级
//! 时间戳；金额一律十进制字符串；数量一律十进制字符串。
//! 契约来源：W05 销售单、W09 收货发货、W11 客户往来、W12 供应商往来。

use entities::common::time::Instant;
use entities::ids::{
    CustomerAcceptanceId, CustomerAccountId, CustomerReceiptId, PayableEntryId, PurchaseOrderId,
    PurchaseOrderRevisionLineId, PurchaseReturnOrderId, ReceivableEntryId, SalesOrderId, SalesOrderLineId,
    SalesReturnCaseId, SupplierAccountId, SupplierPaymentId, WarehouseId,
};
use entities::money::{Amount, Quantity};
use entities::returns::{
    CaseType, CustomerRefundStatus, PaymentReversalStatus, PurchaseReturnStatus, ReceiptReversalStatus,
    ReturnMode, ReturnRoute, SalesReturnCaseStatus, SupplierRefundStatus,
};
use serde::{Deserialize, Serialize};
use validator::Validate;

use crate::errors::{Error, Result};
use crate::query::{normalized_text, page_or_default, page_size_or_default};

/// 销售退货处理单列表允许的排序字段白名单。
pub(crate) const SALES_RETURN_CASE_SORT_FIELDS: &[&str] = &["discovered_at", "created_at"];
/// 采购退货单列表允许的排序字段白名单。
pub(crate) const PURCHASE_RETURN_ORDER_SORT_FIELDS: &[&str] = &["created_at"];
/// 客户退款列表允许的排序字段白名单。
pub(crate) const CUSTOMER_REFUND_SORT_FIELDS: &[&str] = &["occurred_at", "amount", "created_at"];
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

/// 契约目标形状的分页响应（api-contract §3）。
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
// 销售退货/拒收处理单（sales_return_case）
// ---------------------------------------------------------------------------

/// 销售退货/拒收处理单创建请求（W05 退货入口：处理单 + 明细行原子可见）。
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct CreateSalesReturnCaseRequest {
    /// 退货/拒收处理号（唯一，幂等键）。
    #[validate(custom(function = "non_blank", message = "退货处理号不能为空"))]
    pub return_no: String,
    /// 原销售单。
    pub sales_order_id: SalesOrderId,
    /// 验收依据（拒收等场景存在）。
    pub acceptance_id: Option<CustomerAcceptanceId>,
    /// 处理类型。
    pub case_type: CaseType,
    /// 原因。
    #[validate(custom(function = "non_blank", message = "原因不能为空"))]
    pub reason: String,
    /// 发现时间（秒级时间戳）。
    pub discovered_at: Instant,
    /// 退货路线。
    pub return_route: ReturnRoute,
    /// 退货明细行。
    #[validate(length(min = 1, message = "至少提供一条退货明细"))]
    pub lines: Vec<CreateSalesReturnLineRequest>,
}

/// 销售退货明细创建请求行。
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct CreateSalesReturnLineRequest {
    /// 原销售明细。
    pub sales_order_line_id: SalesOrderLineId,
    /// 申请退回数量。
    pub requested_quantity: Quantity,
}

/// 销售退货明细响应视图。
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct SalesReturnLineView {
    /// 实体主键。
    pub id: String,
    /// 原销售明细。
    pub sales_order_line_id: String,
    /// 申请退回数量。
    pub requested_quantity: Quantity,
    /// 实际退回数量。
    pub received_quantity: Option<Quantity>,
    /// 退回验收结果。
    pub quality_result: Option<String>,
    /// 可重新入库数量。
    pub restockable_quantity: Option<Quantity>,
}

/// 销售退货/拒收处理单响应视图。
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct SalesReturnCaseView {
    /// 实体主键。
    pub id: String,
    /// 退货/拒收处理号。
    pub return_no: String,
    /// 原销售单。
    pub sales_order_id: String,
    /// 验收依据。
    pub acceptance_id: Option<String>,
    /// 处理类型。
    pub case_type: CaseType,
    /// 原因。
    pub reason: String,
    /// 发现时间（秒级时间戳）。
    pub discovered_at: Instant,
    /// 退货路线。
    pub return_route: ReturnRoute,
    /// 处理单状态。
    pub status: SalesReturnCaseStatus,
    /// 乐观锁版本。
    pub version: u64,
    /// 创建时间（秒级时间戳）。
    pub created_at: u64,
    /// 退货明细行。
    pub lines: Vec<SalesReturnLineView>,
}

/// 销售退货/拒收处理单列表查询参数。
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct SalesReturnCaseListParams {
    /// 退货处理号模糊筛选。
    pub return_no: Option<String>,
    /// 原销售单筛选。
    pub sales_order_id: Option<SalesOrderId>,
    /// 处理单状态筛选。
    pub status: Option<SalesReturnCaseStatus>,
    /// 页码（1 起）。
    #[validate(range(min = 1, message = "页码必须大于0"))]
    pub page: Option<u64>,
    /// 单页条数（1–100）。
    #[validate(range(min = 1, max = 100, message = "分页大小必须在1-100之间"))]
    pub page_size: Option<u32>,
    /// 排序字段（白名单：`discovered_at`/`created_at`）。
    pub sort_by: Option<String>,
    /// 排序方向（`asc`/`desc`）。
    pub sort_dir: Option<String>,
}

/// 归一化后的销售退货处理单列表查询参数。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct SalesReturnCaseListQuery {
    /// 退货处理号模糊筛选。
    pub return_no: Option<String>,
    /// 原销售单筛选。
    pub sales_order_id: Option<SalesOrderId>,
    /// 处理单状态筛选。
    pub status: Option<SalesReturnCaseStatus>,
    /// 分页与排序参数。
    pub paging: PageParams,
}

impl SalesReturnCaseListParams {
    /// 归一化销售退货处理单列表查询参数。
    ///
    /// # 返回
    /// 返回不依赖仓储类型的规范化查询参数。
    ///
    /// # 错误
    /// 排序字段不在白名单或排序方向非法时返回 `ValidationError`。
    pub(crate) fn normalized(&self) -> Result<SalesReturnCaseListQuery> {
        let (sort_by, sort_dir) =
            normalize_sort(&self.sort_by, &self.sort_dir, SALES_RETURN_CASE_SORT_FIELDS)?;
        Ok(SalesReturnCaseListQuery {
            return_no: normalized_text(self.return_no.as_deref()),
            sales_order_id: self.sales_order_id.clone(),
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
// 采购退货单（purchase_return_order）
// ---------------------------------------------------------------------------

/// 采购退货单创建请求（W09 退货入口：退货单 + 明细行原子可见）。
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct CreatePurchaseReturnOrderRequest {
    /// 采购退货单号（唯一，幂等键）。
    #[validate(custom(function = "non_blank", message = "采购退货单号不能为空"))]
    pub purchase_return_no: String,
    /// 原采购单。
    pub purchase_order_id: PurchaseOrderId,
    /// 客户侧依据（销售退货/拒收处理单，可空）。
    pub sales_return_case_id: Option<SalesReturnCaseId>,
    /// 退货模式。
    pub return_mode: ReturnMode,
    /// 退货明细行。
    #[validate(length(min = 1, message = "至少提供一条退货明细"))]
    pub lines: Vec<CreatePurchaseReturnLineRequest>,
}

/// 采购退货明细创建请求行。
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct CreatePurchaseReturnLineRequest {
    /// 原采购明细。
    pub purchase_order_revision_line_id: PurchaseOrderRevisionLineId,
    /// 退货数量。
    pub return_quantity: Quantity,
    /// 公司仓退货时必填的仓库。
    pub warehouse_id: Option<WarehouseId>,
}

/// 采购退货明细响应视图。
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct PurchaseReturnLineView {
    /// 实体主键。
    pub id: String,
    /// 原采购明细。
    pub purchase_order_revision_line_id: String,
    /// 退货数量。
    pub return_quantity: Quantity,
    /// 仓库。
    pub warehouse_id: Option<String>,
}

/// 采购退货单响应视图。
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct PurchaseReturnOrderView {
    /// 实体主键。
    pub id: String,
    /// 采购退货单号。
    pub purchase_return_no: String,
    /// 原采购单。
    pub purchase_order_id: String,
    /// 客户侧依据。
    pub sales_return_case_id: Option<String>,
    /// 退货模式。
    pub return_mode: ReturnMode,
    /// 退货单状态。
    pub status: PurchaseReturnStatus,
    /// 乐观锁版本。
    pub version: u64,
    /// 创建时间（秒级时间戳）。
    pub created_at: u64,
    /// 退货明细行。
    pub lines: Vec<PurchaseReturnLineView>,
}

/// 采购退货单列表查询参数。
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct PurchaseReturnOrderListParams {
    /// 采购退货单号模糊筛选。
    pub purchase_return_no: Option<String>,
    /// 原采购单筛选。
    pub purchase_order_id: Option<PurchaseOrderId>,
    /// 退货单状态筛选。
    pub status: Option<PurchaseReturnStatus>,
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

/// 归一化后的采购退货单列表查询参数。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct PurchaseReturnOrderListQuery {
    /// 采购退货单号模糊筛选。
    pub purchase_return_no: Option<String>,
    /// 原采购单筛选。
    pub purchase_order_id: Option<PurchaseOrderId>,
    /// 退货单状态筛选。
    pub status: Option<PurchaseReturnStatus>,
    /// 分页与排序参数。
    pub paging: PageParams,
}

impl PurchaseReturnOrderListParams {
    /// 归一化采购退货单列表查询参数。
    ///
    /// # 返回
    /// 返回不依赖仓储类型的规范化查询参数。
    ///
    /// # 错误
    /// 排序字段不在白名单或排序方向非法时返回 `ValidationError`。
    pub(crate) fn normalized(&self) -> Result<PurchaseReturnOrderListQuery> {
        let (sort_by, sort_dir) =
            normalize_sort(&self.sort_by, &self.sort_dir, PURCHASE_RETURN_ORDER_SORT_FIELDS)?;
        Ok(PurchaseReturnOrderListQuery {
            purchase_return_no: normalized_text(self.purchase_return_no.as_deref()),
            purchase_order_id: self.purchase_order_id.clone(),
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
// 客户退款（customer_refund）
// ---------------------------------------------------------------------------

/// 按原资金事实一次创建并提交退款/冲正审批的命令。
///
/// 服务端从原事实解析客户、供应商与默认金额，并在一个事务内完成单据创建、
/// 审批绑定、状态迁移、不可变快照、运行事实、入口任务和审计。
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
#[serde(deny_unknown_fields)]
pub struct CommitReturnFactRequest {
    /// 原回款或原付款主键。
    #[validate(custom(function = "non_blank", message = "原资金事实不能为空"))]
    pub source_fact_id: String,
    /// 本次金额；为空时使用原资金事实全额。
    pub amount: Option<Amount>,
    /// 原因说明。
    #[validate(custom(function = "non_blank", message = "原因说明不能为空"))]
    pub reason: String,
    /// 业务请求幂等键。
    #[validate(length(min = 1, max = 128, message = "幂等键不能为空"))]
    pub idempotency_key: String,
}

/// 客户退款一次提交命令。
pub type CommitCustomerRefundRequest = CommitReturnFactRequest;
/// 供应商退款一次提交命令。
pub type CommitSupplierRefundRequest = CommitReturnFactRequest;
/// 回款冲正一次提交命令。
pub type CommitReceiptReversalRequest = CommitReturnFactRequest;
/// 付款冲正一次提交命令。
pub type CommitPaymentReversalRequest = CommitReturnFactRequest;

/// 客户退款创建请求（W11 纠错入口：草稿，财务经办/复核分离）。
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct CreateCustomerRefundRequest {
    /// 退款单号（唯一，幂等键）。
    #[validate(custom(function = "non_blank", message = "退款单号不能为空"))]
    pub refund_no: String,
    /// 销售退货/拒收处理单（可空）。
    pub sales_return_case_id: Option<SalesReturnCaseId>,
    /// 客户。
    pub customer_id: CustomerAccountId,
    /// 原回款（与 `original_receivable_entry_id` 必须且只能选一）。
    pub original_receipt_id: Option<CustomerReceiptId>,
    /// 原应收分录（与 `original_receipt_id` 必须且只能选一）。
    pub original_receivable_entry_id: Option<ReceivableEntryId>,
    /// 原因代码（可空）。
    pub reason_code: Option<String>,
    /// 原因说明（必填）。
    #[validate(custom(function = "non_blank", message = "退款原因不能为空"))]
    pub reason_text: String,
    /// 退款金额（正数）。
    pub amount: Amount,
    /// 财务经办人。
    #[validate(custom(function = "non_blank", message = "经办人不能为空"))]
    pub handled_by: String,
    /// 财务复核人（不得与经办人相同）。
    #[validate(custom(function = "non_blank", message = "复核人不能为空"))]
    pub reviewed_by: String,
    /// 实际退款时间（秒级时间戳）。
    pub occurred_at: Instant,
}

/// 客户退款过账请求（仅由最终通过动作内部消费；HTTP 旁路已关闭）。
#[derive(Debug, Clone, Serialize, Deserialize, Validate, Default)]
pub struct PostCustomerRefundRequest {}

/// 客户退款提交审批请求。客户端不得选择定义或审批人。
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
#[serde(deny_unknown_fields)]
pub struct SubmitCustomerRefundRequest {
    /// 期望的单据乐观锁版本。
    #[validate(range(min = 1, message = "乐观锁版本必须大于 0"))]
    pub expected_version: u64,
    /// 业务请求幂等键。
    #[validate(length(min = 1, max = 128, message = "幂等键不能为空"))]
    pub idempotency_key: String,
}

/// 撤回客户退款审批请求。原因必填。
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
#[serde(deny_unknown_fields)]
pub struct CancelCustomerRefundApprovalRequest {
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

/// 客户退款响应视图。
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct CustomerRefundView {
    /// 实体主键。
    pub id: String,
    /// 退款单号。
    pub refund_no: String,
    /// 退款状态。
    pub status: CustomerRefundStatus,
    /// 销售退货/拒收处理单。
    pub sales_return_case_id: Option<String>,
    /// 客户。
    pub customer_id: String,
    /// 原回款。
    pub original_receipt_id: Option<String>,
    /// 原应收分录。
    pub original_receivable_entry_id: Option<String>,
    /// 原因代码。
    pub reason_code: Option<String>,
    /// 原因说明。
    pub reason_text: String,
    /// 退款金额。
    pub amount: Amount,
    /// 财务经办人。
    pub handled_by: String,
    /// 财务复核人。
    pub reviewed_by: String,
    /// 实际退款时间（秒级时间戳）。
    pub occurred_at: Instant,
    /// 乐观锁版本。
    pub version: u64,
    /// 创建时间（秒级时间戳）。
    pub created_at: u64,
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

/// 客户退款列表查询参数。
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct CustomerRefundListParams {
    /// 退款单号模糊筛选。
    pub refund_no: Option<String>,
    /// 客户筛选。
    pub customer_id: Option<CustomerAccountId>,
    /// 退款状态筛选。
    pub status: Option<CustomerRefundStatus>,
    /// 页码（1 起）。
    #[validate(range(min = 1, message = "页码必须大于0"))]
    pub page: Option<u64>,
    /// 单页条数（1–100）。
    #[validate(range(min = 1, max = 100, message = "分页大小必须在1-100之间"))]
    pub page_size: Option<u32>,
    /// 排序字段（白名单：`occurred_at`/`amount`/`created_at`）。
    pub sort_by: Option<String>,
    /// 排序方向（`asc`/`desc`）。
    pub sort_dir: Option<String>,
}

/// 归一化后的客户退款列表查询参数。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct CustomerRefundListQuery {
    /// 退款单号模糊筛选。
    pub refund_no: Option<String>,
    /// 客户筛选。
    pub customer_id: Option<CustomerAccountId>,
    /// 退款状态筛选。
    pub status: Option<CustomerRefundStatus>,
    /// 分页与排序参数。
    pub paging: PageParams,
}

impl CustomerRefundListParams {
    /// 归一化客户退款列表查询参数。
    ///
    /// # 返回
    /// 返回不依赖仓储类型的规范化查询参数。
    ///
    /// # 错误
    /// 排序字段不在白名单或排序方向非法时返回 `ValidationError`。
    pub(crate) fn normalized(&self) -> Result<CustomerRefundListQuery> {
        let (sort_by, sort_dir) = normalize_sort(&self.sort_by, &self.sort_dir, CUSTOMER_REFUND_SORT_FIELDS)?;
        Ok(CustomerRefundListQuery {
            refund_no: normalized_text(self.refund_no.as_deref()),
            customer_id: self.customer_id.clone(),
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
// 供应商退款（supplier_refund）
// ---------------------------------------------------------------------------

/// 供应商退款创建请求（W12 纠错入口：草稿，财务经办/复核分离）。
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct CreateSupplierRefundRequest {
    /// 退款单号（唯一，幂等键）。
    #[validate(custom(function = "non_blank", message = "退款单号不能为空"))]
    pub refund_no: String,
    /// 采购退货/错付款依据（可空）。
    pub purchase_return_order_id: Option<PurchaseReturnOrderId>,
    /// 供应商。
    pub supplier_id: SupplierAccountId,
    /// 原付款（与 `original_payable_entry_id` 必须且只能选一）。
    pub original_payment_id: Option<SupplierPaymentId>,
    /// 原应付分录（与 `original_payment_id` 必须且只能选一）。
    pub original_payable_entry_id: Option<PayableEntryId>,
    /// 原因代码（可空）。
    pub reason_code: Option<String>,
    /// 原因说明（必填）。
    #[validate(custom(function = "non_blank", message = "退款原因不能为空"))]
    pub reason_text: String,
    /// 退款金额（正数）。
    pub amount: Amount,
    /// 财务经办人。
    #[validate(custom(function = "non_blank", message = "经办人不能为空"))]
    pub handled_by: String,
    /// 财务复核人（不得与经办人相同）。
    #[validate(custom(function = "non_blank", message = "复核人不能为空"))]
    pub reviewed_by: String,
    /// 实际退款时间（秒级时间戳）。
    pub occurred_at: Instant,
}

/// 供应商退款过账请求（仅由最终通过动作内部消费；HTTP 旁路已关闭）。
#[derive(Debug, Clone, Serialize, Deserialize, Validate, Default)]
pub struct PostSupplierRefundRequest {}

/// 供应商退款提交审批请求。客户端不得选择定义或审批人。
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
#[serde(deny_unknown_fields)]
pub struct SubmitSupplierRefundRequest {
    /// 期望的单据乐观锁版本。
    #[validate(range(min = 1, message = "乐观锁版本必须大于 0"))]
    pub expected_version: u64,
    /// 业务请求幂等键。
    #[validate(length(min = 1, max = 128, message = "幂等键不能为空"))]
    pub idempotency_key: String,
}

/// 撤回供应商退款审批请求。原因必填。
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
#[serde(deny_unknown_fields)]
pub struct CancelSupplierRefundApprovalRequest {
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

/// 供应商退款响应视图。
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct SupplierRefundView {
    /// 实体主键。
    pub id: String,
    /// 退款单号。
    pub refund_no: String,
    /// 退款状态。
    pub status: SupplierRefundStatus,
    /// 采购退货/错付款依据。
    pub purchase_return_order_id: Option<String>,
    /// 供应商。
    pub supplier_id: String,
    /// 原付款。
    pub original_payment_id: Option<String>,
    /// 原应付分录。
    pub original_payable_entry_id: Option<String>,
    /// 原因代码。
    pub reason_code: Option<String>,
    /// 原因说明。
    pub reason_text: String,
    /// 退款金额。
    pub amount: Amount,
    /// 财务经办人。
    pub handled_by: String,
    /// 财务复核人。
    pub reviewed_by: String,
    /// 实际退款时间（秒级时间戳）。
    pub occurred_at: Instant,
    /// 乐观锁版本。
    pub version: u64,
    /// 创建时间（秒级时间戳）。
    pub created_at: u64,
    /// 统一只读审批结构。客户端不得据此选择定义或审批人。
    pub approval: DocumentApprovalView,
}

// ---------------------------------------------------------------------------
// 回款冲正（receipt_reversal）与付款冲正（payment_reversal）
// ---------------------------------------------------------------------------

/// 回款冲正创建请求（W11 纠错入口：草稿，财务经办/复核分离）。
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct CreateReceiptReversalRequest {
    /// 冲正单号（唯一，幂等键）。
    #[validate(custom(function = "non_blank", message = "冲正单号不能为空"))]
    pub reversal_no: String,
    /// 被冲正的原客户回款。
    pub original_customer_receipt_id: CustomerReceiptId,
    /// 原因代码（可空）。
    pub reason_code: Option<String>,
    /// 原因说明（必填）。
    #[validate(custom(function = "non_blank", message = "冲正原因不能为空"))]
    pub reason_text: String,
    /// 冲正金额（正数）。
    pub amount: Amount,
    /// 财务经办人。
    #[validate(custom(function = "non_blank", message = "经办人不能为空"))]
    pub handled_by: String,
    /// 财务复核人（不得与经办人相同）。
    #[validate(custom(function = "non_blank", message = "复核人不能为空"))]
    pub reviewed_by: String,
    /// 冲正实际发生时间（秒级时间戳）。
    pub occurred_at: Instant,
}

/// 回款冲正过账请求（仅由最终通过动作内部消费；HTTP 旁路已关闭）。
#[derive(Debug, Clone, Serialize, Deserialize, Validate, Default)]
pub struct PostReceiptReversalRequest {}

/// 回款冲正提交审批请求。客户端不得选择定义或审批人。
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
#[serde(deny_unknown_fields)]
pub struct SubmitReceiptReversalRequest {
    /// 期望的单据乐观锁版本。
    #[validate(range(min = 1, message = "乐观锁版本必须大于 0"))]
    pub expected_version: u64,
    /// 业务请求幂等键。
    #[validate(length(min = 1, max = 128, message = "幂等键不能为空"))]
    pub idempotency_key: String,
}

/// 撤回回款冲正审批请求。原因必填。
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
#[serde(deny_unknown_fields)]
pub struct CancelReceiptReversalApprovalRequest {
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

/// 回款冲正响应视图。
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct ReceiptReversalView {
    /// 实体主键。
    pub id: String,
    /// 冲正单号。
    pub reversal_no: String,
    /// 冲正状态。
    pub status: ReceiptReversalStatus,
    /// 被冲正的原客户回款。
    pub original_customer_receipt_id: String,
    /// 原因代码。
    pub reason_code: Option<String>,
    /// 原因说明。
    pub reason_text: String,
    /// 冲正金额。
    pub amount: Amount,
    /// 财务经办人。
    pub handled_by: String,
    /// 财务复核人。
    pub reviewed_by: String,
    /// 冲正实际发生时间（秒级时间戳）。
    pub occurred_at: Instant,
    /// 乐观锁版本。
    pub version: u64,
    /// 创建时间（秒级时间戳）。
    pub created_at: u64,
    /// 统一只读审批结构。客户端不得据此选择定义或审批人。
    pub approval: DocumentApprovalView,
}

/// 付款冲正创建请求（W12 纠错入口：草稿，财务经办/复核分离）。
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct CreatePaymentReversalRequest {
    /// 冲正单号（唯一，幂等键）。
    #[validate(custom(function = "non_blank", message = "冲正单号不能为空"))]
    pub reversal_no: String,
    /// 被冲正的原供应商付款。
    pub original_supplier_payment_id: SupplierPaymentId,
    /// 原因代码（可空）。
    pub reason_code: Option<String>,
    /// 原因说明（必填）。
    #[validate(custom(function = "non_blank", message = "冲正原因不能为空"))]
    pub reason_text: String,
    /// 冲正金额（正数）。
    pub amount: Amount,
    /// 财务经办人。
    #[validate(custom(function = "non_blank", message = "经办人不能为空"))]
    pub handled_by: String,
    /// 财务复核人（不得与经办人相同）。
    #[validate(custom(function = "non_blank", message = "复核人不能为空"))]
    pub reviewed_by: String,
    /// 冲正实际发生时间（秒级时间戳）。
    pub occurred_at: Instant,
}

/// 付款冲正过账请求（仅由最终通过动作内部消费；HTTP 旁路已关闭）。
#[derive(Debug, Clone, Serialize, Deserialize, Validate, Default)]
pub struct PostPaymentReversalRequest {}

/// 付款冲正提交审批请求。客户端不得选择定义或审批人。
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
#[serde(deny_unknown_fields)]
pub struct SubmitPaymentReversalRequest {
    /// 期望的单据乐观锁版本。
    #[validate(range(min = 1, message = "乐观锁版本必须大于 0"))]
    pub expected_version: u64,
    /// 业务请求幂等键。
    #[validate(length(min = 1, max = 128, message = "幂等键不能为空"))]
    pub idempotency_key: String,
}

/// 撤回付款冲正审批请求。原因必填。
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
#[serde(deny_unknown_fields)]
pub struct CancelPaymentReversalApprovalRequest {
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

/// 付款冲正响应视图。
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct PaymentReversalView {
    /// 实体主键。
    pub id: String,
    /// 冲正单号。
    pub reversal_no: String,
    /// 冲正状态。
    pub status: PaymentReversalStatus,
    /// 被冲正的原供应商付款。
    pub original_supplier_payment_id: String,
    /// 原因代码。
    pub reason_code: Option<String>,
    /// 原因说明。
    pub reason_text: String,
    /// 冲正金额。
    pub amount: Amount,
    /// 财务经办人。
    pub handled_by: String,
    /// 财务复核人。
    pub reviewed_by: String,
    /// 冲正实际发生时间（秒级时间戳）。
    pub occurred_at: Instant,
    /// 乐观锁版本。
    pub version: u64,
    /// 创建时间（秒级时间戳）。
    pub created_at: u64,
    /// 统一只读审批结构。客户端不得据此选择定义或审批人。
    pub approval: DocumentApprovalView,
}

#[cfg(test)]
mod tests {
    use super::{
        normalize_sort, CustomerRefundListParams, PurchaseReturnOrderListParams, SalesReturnCaseListParams,
        SortDir,
    };
    use entities::returns::{CustomerRefundStatus, PurchaseReturnStatus, SalesReturnCaseStatus};

    #[test]
    fn sort_whitelist_rejects_unknown_fields_and_directions() {
        assert!(normalize_sort(&Some("name".to_string()), &None, &["created_at"]).is_err());
        assert!(normalize_sort(&None, &Some("up".to_string()), &["created_at"]).is_err());

        let (field, direction) = normalize_sort(&None, &None, &["created_at"]).unwrap();
        assert_eq!(field, "created_at");
        assert_eq!(direction, SortDir::Desc);
    }

    #[test]
    fn sales_return_case_list_params_normalize_filters() {
        let params = SalesReturnCaseListParams {
            return_no: Some(" RT-1 ".to_string()),
            sales_order_id: None,
            status: Some(SalesReturnCaseStatus::Processing),
            page: Some(2),
            page_size: Some(50),
            sort_by: Some("discovered_at".to_string()),
            sort_dir: Some("asc".to_string()),
        };
        let query = params.normalized().unwrap();
        assert_eq!(query.return_no.as_deref(), Some("RT-1"));
        assert_eq!(query.status, Some(SalesReturnCaseStatus::Processing));
        assert_eq!(query.paging.page, 2);
        assert_eq!(query.paging.sort_by, "discovered_at");
        assert_eq!(query.paging.sort_dir, SortDir::Asc);
    }

    #[test]
    fn return_and_refund_list_params_normalize() {
        let purchase = PurchaseReturnOrderListParams {
            purchase_return_no: Some("PR-1".to_string()),
            purchase_order_id: None,
            status: Some(PurchaseReturnStatus::Draft),
            page: None,
            page_size: None,
            sort_by: None,
            sort_dir: None,
        };
        assert_eq!(
            purchase.normalized().unwrap().status,
            Some(PurchaseReturnStatus::Draft)
        );

        let refund = CustomerRefundListParams {
            refund_no: None,
            customer_id: None,
            status: Some(CustomerRefundStatus::Posted),
            page: None,
            page_size: Some(25),
            sort_by: None,
            sort_dir: None,
        };
        assert_eq!(refund.normalized().unwrap().paging.page_size, 25);
    }
}
