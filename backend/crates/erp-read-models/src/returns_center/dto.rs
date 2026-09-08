//! 域 D21 `returns` 的 DTO（Handler 直接复用，禁止在 handler 内重复定义同构类型）。
//!
//! 字段名与 HTTP 契约一致（api-contract.md）：分页参数扁平传递；时间一律秒级
//! 时间戳；金额一律十进制字符串；数量一律十进制字符串。
//! 契约来源：W05 销售单、W09 收货发货、W11 客户往来、W12 供应商往来。

use erp_core::common::time::Instant;
use erp_core::ids::{CustomerAccountId, PurchaseOrderId, SalesOrderId};
use erp_core::money::{Amount, Quantity};
use erp_returns::entity::returns::{
    CaseType, CustomerRefundStatus, PaymentReversalStatus, PurchaseReturnStatus, ReceiptReversalStatus,
    ReturnMode, ReturnRoute, SalesReturnCaseStatus, SupplierRefundStatus,
};
use serde::{Deserialize, Serialize};
use validator::Validate;

use crate::Result;
use application_core::{normalized_text, page_or_default, page_size_or_default};

/// 销售退货处理单列表允许的排序字段白名单。
pub(crate) const SALES_RETURN_CASE_SORT_FIELDS: &[&str] = &["discovered_at", "created_at"];
/// 采购退货单列表允许的排序字段白名单。
pub(crate) const PURCHASE_RETURN_ORDER_SORT_FIELDS: &[&str] = &["created_at"];
/// 客户退款列表允许的排序字段白名单。
pub(crate) const CUSTOMER_REFUND_SORT_FIELDS: &[&str] = &["occurred_at", "amount", "created_at"];
/// 排序方向。
pub use application_core::SortDir;

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
pub(crate) use application_core::normalize_sort;

/// 契约目标形状的分页响应（api-contract §3）。
pub use application_core::PageView;

// ---------------------------------------------------------------------------
// 销售退货/拒收处理单（sales_return_case）

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
    /// 当前节点显示名，来自执行快照。
    pub current_node_name: Option<String>,
    /// 当前审批人显示名，来自执行快照。
    pub current_assignee_name: Option<String>,
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
    use erp_returns::entity::returns::{CustomerRefundStatus, PurchaseReturnStatus, SalesReturnCaseStatus};

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
