//! 供应商履约调查、任务完成与售后动作合同。

use application_core::non_blank;
use erp_core::ids::{
    SupplierFulfillmentItemId, SupplierFulfillmentOrderId, SupplierOrderActionId, WorkItemId,
};
use erp_core::money::{Amount, Quantity};
use serde::{Deserialize, Serialize};
use validator::Validate;

use super::{SupplierFulfillmentOrderView, SupplierOrderActionLineView, SupplierOrderActionView};

/// W26 强类型领域动作。
#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum SupplierOrderAllowedAction {
    /// 查询权威原动作的供应商结果。
    QueryResult,
    /// 只在最新证据明确无结果时按原幂等语义重放。
    Replay,
    /// 以已验证终态证据完成正式任务。
    ConfirmVerifiedTerminalResult,
}

impl SupplierOrderAllowedAction {
    /// 返回稳定动作代码。
    pub fn as_str(self) -> &'static str {
        match self {
            Self::QueryResult => "QUERY_RESULT",
            Self::Replay => "REPLAY",
            Self::ConfirmVerifiedTerminalResult => "CONFIRM_VERIFIED_TERMINAL_RESULT",
        }
    }
}

/// W26 对象入口的供应商结果调查命令。
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
#[serde(deny_unknown_fields)]
pub struct SupplierOrderObjectInvestigationCommand {
    /// 供应商履约订单。
    pub order_id: SupplierFulfillmentOrderId,
    /// 查询所得订单乐观锁版本。
    #[validate(range(min = 1, message = "订单版本必须大于 0"))]
    pub expected_lock_version: u64,
    /// 固定调查动作。
    pub action: SupplierOrderInvestigationAction,
    /// 客户端生成的本次操作身份。
    #[validate(custom(function = "non_blank", message = "操作ID不能为空"))]
    #[validate(length(max = 64, message = "操作ID不能超过 64 个字符"))]
    pub operation_id: String,
    /// 被调查或按原幂等键重放的供应商原动作。
    pub target_supplier_action_id: SupplierOrderActionId,
    /// 客户端稳定请求幂等键；不得作为供应商重放幂等键。
    #[validate(custom(function = "non_blank", message = "请求标识不能为空"))]
    #[validate(length(max = 128, message = "请求标识不能超过 128 个字符"))]
    pub idempotency_key: String,
}

/// W26 正式任务入口的供应商结果调查命令。
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
#[serde(deny_unknown_fields)]
pub struct SupplierOrderTaskInvestigationCommand {
    /// 当前正式任务。
    pub work_item_id: WorkItemId,
    /// 查询所得任务版本；服务端按正整数字符串严格解析。
    #[validate(custom(function = "non_blank", message = "任务版本不能为空"))]
    pub expected_task_version: String,
    /// 查询所得任务主体版本。
    #[validate(custom(function = "non_blank", message = "任务主体版本不能为空"))]
    #[validate(length(max = 128, message = "任务主体版本不能超过 128 个字符"))]
    pub expected_subject_version: String,
    /// 带订单身份与版本的固定调查动作。
    #[validate(nested)]
    pub action: SupplierOrderTaskInvestigationAction,
    /// 客户端稳定请求幂等键；不得作为供应商重放幂等键。
    #[validate(custom(function = "non_blank", message = "请求标识不能为空"))]
    #[validate(length(max = 128, message = "请求标识不能超过 128 个字符"))]
    pub idempotency_key: String,
}

/// 任务调查命令内的固定动作载荷。
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
#[serde(deny_unknown_fields)]
pub struct SupplierOrderTaskInvestigationAction {
    /// 固定调查动作。
    #[serde(rename = "type")]
    pub action_type: SupplierOrderInvestigationAction,
    /// 供应商履约订单。
    pub order_id: SupplierFulfillmentOrderId,
    /// 查询所得订单乐观锁版本。
    #[validate(range(min = 1, message = "订单版本必须大于 0"))]
    pub expected_order_lock_version: u64,
    /// 被调查或按原幂等键重放的供应商原动作。
    pub target_supplier_action_id: SupplierOrderActionId,
    /// 客户端生成的本次操作身份。
    #[validate(custom(function = "non_blank", message = "操作ID不能为空"))]
    #[validate(length(max = 64, message = "操作ID不能超过 64 个字符"))]
    pub operation_id: String,
}

/// W26 允许的供应商结果调查动作。
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum SupplierOrderInvestigationAction {
    /// 查询原供应商动作结果。
    QueryResult,
    /// 已证明原请求无结果后，沿原供应商幂等键安全重放。
    Replay,
}

/// W26 唯一强类型任务完成命令。
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
#[serde(deny_unknown_fields)]
pub struct SupplierOrderTaskCompletionCommand {
    /// 当前正式任务。
    pub work_item_id: WorkItemId,
    /// 查询所得任务版本；服务端按正整数字符串严格解析。
    #[validate(custom(function = "non_blank", message = "任务版本不能为空"))]
    pub expected_task_version: String,
    /// 查询所得任务主体版本。
    #[validate(custom(function = "non_blank", message = "任务主体版本不能为空"))]
    #[validate(length(max = 128, message = "任务主体版本不能超过 128 个字符"))]
    pub expected_subject_version: String,
    /// 固定终态确认决定。
    #[validate(nested)]
    pub decision: SupplierOrderTaskCompletionDecision,
    /// 客户端稳定请求幂等键。
    #[validate(custom(function = "non_blank", message = "请求标识不能为空"))]
    #[validate(length(max = 128, message = "请求标识不能超过 128 个字符"))]
    pub idempotency_key: String,
}

/// W26 任务完成命令内的终态确认决定。
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
#[serde(deny_unknown_fields)]
pub struct SupplierOrderTaskCompletionDecision {
    /// 固定决定类型；其它值在反序列化边界即拒绝。
    #[serde(rename = "type")]
    pub decision_type: SupplierOrderTaskCompletionDecisionType,
    /// 供应商履约订单。
    pub order_id: SupplierFulfillmentOrderId,
    /// 查询所得订单乐观锁版本。
    #[validate(range(min = 1, message = "订单版本必须大于 0"))]
    pub expected_order_lock_version: u64,
    /// 查询或重放形成的服务端可验证终态证据。
    pub verified_supplier_action_result_id: SupplierOrderActionId,
    /// 待固定的业务终态。
    pub resolution: SupplierOrderResolution,
}

/// W26 唯一允许的任务完成决定类型。
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum SupplierOrderTaskCompletionDecisionType {
    /// 以服务端证据确认终态并完成原任务。
    ConfirmVerifiedTerminalResult,
}

/// W26 可由已验证供应商证据确认的终态。
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum SupplierOrderResolution {
    /// 供应商已接单。
    OrderAccepted,
    /// 供应商明确拒单。
    OrderRejected,
    /// 供应商履约完成。
    OrderCompleted,
    /// 供应商取消完成。
    Canceled,
    /// 供应商退款完成。
    Refunded,
}

impl SupplierOrderResolution {
    /// 返回稳定终态代码。
    pub fn as_str(self) -> &'static str {
        match self {
            Self::OrderAccepted => "ORDER_ACCEPTED",
            Self::OrderRejected => "ORDER_REJECTED",
            Self::OrderCompleted => "ORDER_COMPLETED",
            Self::Canceled => "CANCELED",
            Self::Refunded => "REFUNDED",
        }
    }

    /// 返回面向处理人的业务结果名称。
    pub fn label(self) -> &'static str {
        match self {
            Self::OrderAccepted => "供应商已接单",
            Self::OrderRejected => "供应商已拒单",
            Self::OrderCompleted => "供应商履约已完成",
            Self::Canceled => "供应商取消已完成",
            Self::Refunded => "供应商退款已完成",
        }
    }
}

/// 调查证据的服务端结论。
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum SupplierOrderInvestigationOutcome {
    /// 已由持久化业务事实证明终态。
    VerifiedTerminal,
    /// 供应商查询明确证明原请求没有形成结果。
    VerifiedNoResult,
    /// 查询或重放后仍无法证明原结果。
    ResultUnknown,
}

/// 调查命令结果状态。
#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum SupplierOrderInvestigationResultStatus {
    /// 已取得终态或明确无结果证据。
    Succeeded,
    /// 结果仍未知。
    Unknown,
    /// 当前服务端事实禁止继续。
    Blocked,
}

/// W26 调查证据视图。
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct SupplierOrderInvestigationEvidenceView {
    /// 服务端不可变证据动作 ID。
    pub evidence_id: String,
    /// 被调查的原供应商动作。
    pub target_supplier_action_id: String,
    /// 证据结论。
    pub outcome: SupplierOrderInvestigationOutcome,
    /// 证据记录时间（秒级时间戳）。
    pub recorded_at: i64,
    /// 服务端是否已证明可沿原供应商幂等键安全重放。
    pub can_safe_retry: bool,
    /// 已验证终态对应的供应商外部订单号。
    pub external_order_no: Option<String>,
    /// 权限安全的业务说明。
    pub summary: String,
    /// 已验证终态证据动作；仅 `VERIFIED_TERMINAL` 返回。
    pub verified_supplier_action_result_id: Option<String>,
    /// 已验证业务终态；仅 `VERIFIED_TERMINAL` 返回。
    pub verified_resolution: Option<SupplierOrderResolution>,
}

/// 当前动作阻断说明。
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct SupplierOrderActionBlockerView {
    /// 被阻断动作。
    pub action: String,
    /// 稳定阻断码。
    pub code: String,
    /// 权限安全的业务说明。
    pub message: String,
    /// 可选目标工作面。
    pub destination_workspace_id: Option<String>,
}

/// 供应商取消/退款动作提交请求
impl SupplierOrderActionBlockerView {
    /// 以必填阻断三元组构造视图；目标工作面默认为空。
    ///
    /// # 参数
    /// * `action` - 被阻断动作
    /// * `code` - 稳定阻断码
    /// * `message` - 权限安全的业务说明
    ///
    /// # 返回
    /// 返回无目标工作面的视图。
    ///
    /// # 错误
    /// 无。
    pub fn new(action: String, code: String, message: String) -> Self {
        Self { action, code, message, destination_workspace_id: None }
    }
}

/// 供应商取消/退款动作提交请求（动作行冻结实际提交给供应商的范围）。
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct SubmitAfterSalesActionRequest {
    /// 提交给供应商的动作行。
    #[validate(length(min = 1, message = "动作行至少一行"))]
    pub lines: Vec<AfterSalesActionLineRequest>,
    /// 原因代码（可选）。
    pub reason_code: Option<String>,
    /// 备注（可选）。
    pub comment: Option<String>,
}

/// 供应商取消/退款动作行请求。
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct AfterSalesActionLineRequest {
    /// 本供应商履约明细。
    pub supplier_fulfillment_item_id: SupplierFulfillmentItemId,
    /// 本动作提交数量。
    pub quantity: Quantity,
    /// 本动作提交金额。
    pub amount: Amount,
}

/// 供应商动作提交结果视图。
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct SubmitActionResultView {
    /// 动作响应视图。
    pub action: SupplierOrderActionView,
    /// 动作行。
    pub lines: Vec<SupplierOrderActionLineView>,
    /// 动作后订单视图。
    pub order: SupplierFulfillmentOrderView,
}

/// 供应商拒单结果登记请求（回调幂等键 `(connection_id, external_event_id)`，§6.19）。
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct RecordSupplierRejectRequest {
    /// 外部事件 ID（与连接组成回调幂等键）。
    #[validate(custom(function = "non_blank", message = "外部事件ID不能为空"))]
    pub external_event_id: String,
    /// 供应商状态版本。
    #[validate(custom(function = "non_blank", message = "供应商状态版本不能为空"))]
    pub supplier_status_version: String,
    /// 业务发生时间（秒级时间戳）。
    #[validate(range(min = 1, message = "发生时间必须大于 0"))]
    pub occurred_at: i64,
}

#[cfg(test)]
mod tests {
    use serde_json::json;
    use validator::Validate;

    #[test]
    fn w26_task_investigation_accepts_only_the_registered_shape() {
        let command: super::SupplierOrderTaskInvestigationCommand = serde_json::from_value(json!({
            "work_item_id": "work-item-1",
            "expected_task_version": "4",
            "expected_subject_version": "12",
            "action": {
                "type": "QUERY_RESULT",
                "order_id": "supplier-order-1",
                "expected_order_lock_version": 12,
                "target_supplier_action_id": "supplier-action-1",
                "operation_id": "operation-1"
            },
            "idempotency_key": "request-1"
        }))
        .unwrap();
        assert!(command.validate().is_ok());

        let unknown_field = serde_json::from_value::<super::SupplierOrderTaskInvestigationCommand>(json!({
            "work_item_id": "work-item-1",
            "expected_task_version": "4",
            "expected_subject_version": "12",
            "action": {
                "type": "QUERY_RESULT",
                "order_id": "supplier-order-1",
                "expected_order_lock_version": 12,
                "target_supplier_action_id": "supplier-action-1",
                "operation_id": "operation-1"
            },
            "idempotency_key": "request-1",
            "unknown_field": "COMPLETE"
        }));
        assert!(unknown_field.is_err());
    }

    #[test]
    fn w26_task_completion_rejects_unregistered_decisions() {
        let result = serde_json::from_value::<super::SupplierOrderTaskCompletionCommand>(json!({
            "work_item_id": "work-item-1",
            "expected_task_version": "4",
            "expected_subject_version": "12",
            "decision": {
                "type": "MARK_SUCCESS_LOCALLY",
                "order_id": "supplier-order-1",
                "expected_order_lock_version": 12,
                "verified_supplier_action_result_id": "evidence-1",
                "resolution": "ORDER_ACCEPTED"
            },
            "idempotency_key": "request-1"
        }));
        assert!(result.is_err());
    }
}
