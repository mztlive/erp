use serde::{Deserialize, Serialize};
use validator::Validate;

use crate::query::non_blank;

use super::command::SavePurchaseOrderLine;
use super::query::DocumentApprovalView;

/// 撤回采购变更审批请求。原因必填。
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
#[serde(deny_unknown_fields)]
pub struct CancelPurchaseChangeApprovalRequest {
    /// 期望的单据乐观锁版本。
    #[validate(range(min = 1, message = "乐观锁版本必须大于 0"))]
    pub expected_lock_version: u64,
    /// 非空撤回原因。
    #[validate(length(min = 1, max = 512, message = "撤回原因不能为空"))]
    pub reason: String,
    /// 业务请求幂等键。
    #[validate(length(min = 1, max = 128, message = "幂等键不能为空"))]
    pub idempotency_key: String,
}

/// 发起采购变更请求。
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct StartPurchaseChangeRequest {
    /// 期望的采购单乐观锁版本。
    #[validate(range(min = 1, message = "乐观锁版本必须大于 0"))]
    pub expected_lock_version: u64,
    /// 采购变化原因。
    #[validate(custom(function = "non_blank", message = "变更原因不能为空"))]
    pub reason: String,
    /// 幂等键。
    #[validate(custom(function = "non_blank", message = "幂等键不能为空"))]
    pub idempotency_key: String,
}

/// 发起采购变更结果。
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct StartPurchaseChangeResult {
    /// 变更单主键。
    pub change_id: String,
    /// 基准版本。
    pub base_revision_id: String,
    /// 基准版本号。
    pub base_revision_no: u32,
    /// 采购单新乐观锁版本。
    pub lock_version: u64,
    /// 业务引用。
    pub reference: String,
}

/// 提交采购变更目标内容请求（完整头、行及销售分配）。
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct SubmitPurchaseChangeRequest {
    /// 期望的变更单乐观锁版本。
    #[validate(range(min = 1, message = "乐观锁版本必须大于 0"))]
    pub expected_lock_version: u64,
    /// 目标付款条件；缺省沿用基准版本快照。
    pub payment_term_code: Option<String>,
    /// 目标完整行集合。
    pub lines: Vec<SavePurchaseOrderLine>,
    /// 幂等键。
    #[validate(custom(function = "non_blank", message = "幂等键不能为空"))]
    pub idempotency_key: String,
}

/// 采购变更提交结果。
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct PurchaseChangeSubmitResult {
    /// 变更单主键。
    pub change_id: String,
    /// 形成的不可变目标提交。
    pub submission_id: String,
    /// 提交序号。
    pub submission_no: String,
    /// 变更单状态。
    pub status: String,
    /// 变更单新乐观锁版本。
    pub lock_version: u64,
    /// 业务引用。
    pub reference: String,
}

/// 采购变更生效请求（§8.1.3：基准版本校验 + 新版本 + 差额 + 指针推进）。
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct EffectPurchaseChangeRequest {
    /// 期望的变更单乐观锁版本。
    #[validate(range(min = 1, message = "乐观锁版本必须大于 0"))]
    pub expected_lock_version: u64,
    /// 目标提交。
    #[validate(custom(function = "non_blank", message = "提交ID不能为空"))]
    pub submission_id: String,
    /// 幂等键。
    #[validate(custom(function = "non_blank", message = "幂等键不能为空"))]
    pub idempotency_key: String,
}

/// 采购变更生效结果。
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct PurchaseChangeEffectResult {
    /// 变更单主键。
    pub change_id: String,
    /// 形成的新采购版本。
    pub revision_id: String,
    /// 新版本号。
    pub revision_no: u32,
    /// 追加的应付差额分录。
    pub payable_delta_entry_id: Option<String>,
    /// 采购单新乐观锁版本。
    pub purchase_order_lock_version: u64,
    /// 业务引用。
    pub reference: String,
}

/// 采购变更单列表查询参数。
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct PurchaseChangeOrderListParams {
    /// 原采购单筛选。
    pub purchase_order_id: Option<String>,
    /// 状态筛选。
    pub status: Option<String>,
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

/// 采购变更单视图。
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct PurchaseChangeOrderView {
    /// 变更单主键。
    pub id: String,
    /// 原采购单。
    pub purchase_order_id: String,
    /// 基准版本。
    pub base_revision_id: String,
    /// 变更原因。
    pub reason: String,
    /// 状态。
    pub status: String,
    /// 当前不可变目标提交。
    pub current_submission_id: Option<String>,
    /// 生效后形成的新采购版本。
    pub effective_revision_id: Option<String>,
    /// 乐观锁版本。
    pub version: u64,
    /// 创建时间（秒级时间戳）。
    pub created_at: u64,
    /// 统一只读审批结构。客户端不得据此选择定义或审批人。
    pub approval: DocumentApprovalView,
}
