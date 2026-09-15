//! 退货、退款与冲正的创建及审批命令请求；字段与原 HTTP 合同一致。
use application_core::non_blank;
use erp_core::common::time::Instant;
use erp_core::ids::{
    CustomerAcceptanceId, CustomerAccountId, CustomerReceiptId, PayableEntryId, PurchaseOrderId,
    PurchaseOrderRevisionLineId, PurchaseReturnOrderId, ReceivableEntryId, SalesOrderId, SalesOrderLineId,
    SalesReturnCaseId, SupplierAccountId, SupplierPaymentId, WarehouseId,
};
use erp_core::money::{Amount, Quantity};
use serde::{Deserialize, Serialize};
use validator::Validate;

use crate::entity::returns::{CaseType, ReturnMode, ReturnRoute};
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
