//! 供应商履约订单范围列表、行展示姓名、交接与候选 DTO。

use application_core::OwnershipPage;
use serde::{Deserialize, Serialize};
use validator::Validate;

use super::supplier_fulfillment::SupplierFulfillmentOrderView;
use crate::error::Result;

/// 已授权列表行。跟进人与处理人姓名来自本行账号，不来自筛选候选。
#[derive(Debug, Clone, Serialize)]
pub struct SupplierFulfillmentOrderListItem {
    /// 订单行。
    #[serde(flatten)]
    pub order: SupplierFulfillmentOrderView,
    /// 订单上的内部跟进人姓名。账号不存在或姓名为空白时省略。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub follow_up_user_name: Option<String>,
    /// 当前开放异常任务处理人姓名。无开放任务、账号不存在或姓名为空白时省略。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub handler_user_name: Option<String>,
}

/// 列表响应保持现有字段并声明独立的授权时点及版本。
#[derive(Debug, Clone, Serialize)]
pub struct SupplierFulfillmentOrderListView {
    /// 分页结果与归属口径。行上带跟进人、处理人姓名。
    #[serde(flatten)]
    pub data: OwnershipPage<SupplierFulfillmentOrderListItem>,
    /// 跨页与导出必须原样回传的范围版本。
    pub scope_version: String,
    /// RBAC 策略版本。
    pub policy_version: u64,
    /// 组织配置版本。
    pub organization_version: u64,
    /// 授权解析时点。
    pub as_of: String,
    /// 角色无有效范围时为 `no_scope`；有规则但对象为空时不设置。
    pub empty_reason: Option<&'static str>,
    /// 当前履约订单范围口径摘要，不含内部授权证明。
    pub scope_summary: &'static str,
}

/// 供应商履约订单跟进人显式交接请求。
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
#[serde(deny_unknown_fields)]
pub struct HandoverFulfillmentOrderRequest {
    /// 目标跟进人。
    #[validate(length(min = 1, max = 128, message = "目标跟进人不能为空"))]
    pub target_user_id: String,
    /// 显式目标业务组织；省略表示保留原组织。
    pub target_org_unit_id: Option<String>,
    /// 非空交接原因。
    #[validate(length(min = 1, max = 512, message = "交接原因不能为空"))]
    pub reason: String,
    /// 期望的订单乐观锁版本。
    #[validate(range(min = 1, message = "乐观锁版本必须大于 0"))]
    pub expected_version: u64,
    /// 业务请求幂等键。
    #[validate(length(min = 1, max = 128, message = "幂等键不能为空"))]
    pub idempotency_key: String,
    /// 是否同时转交当前开放 W26 异常任务；缺省 false，不得改派审批任务。
    #[serde(default)]
    pub transfer_open_exception_tasks: bool,
}

/// 供应商履约订单交接结果。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct HandoverFulfillmentOrderView {
    /// 订单稳定 ID。
    pub order_id: String,
    /// 交接后跟进人。
    pub follow_up_user_id: String,
    /// 交接后业务组织。
    pub business_org_unit_id: String,
    /// 交接后订单版本。
    pub version: u64,
    /// 实际转交的开放 W26 任务 ID。
    pub transferred_work_item_ids: Vec<String>,
}

/// 履约订单交接待选目标。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct FulfillmentHandoverCandidateView {
    /// 目标账号 ID。
    pub user_id: String,
    /// 显示名。
    pub display_name: String,
    /// 登录账号。
    pub account: String,
}

/// 校验交接请求必填字段。
///
/// # 参数
/// * `req` - 交接请求
///
/// # 返回
/// 校验通过时成功。
///
/// # 错误
/// 目标、原因、版本或幂等键非法时拒绝。
pub fn validate_handover_request(req: &HandoverFulfillmentOrderRequest) -> Result<()> {
    req.validate()?;
    if req.target_user_id.trim().is_empty() {
        return Err(crate::Error::ValidationError("目标跟进人不能为空".into()));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn handover_defaults_do_not_transfer_exception_tasks() {
        let req: HandoverFulfillmentOrderRequest = serde_json::from_value(serde_json::json!({
            "target_user_id": "buyer-2",
            "reason": "岗位调整",
            "expected_version": 1,
            "idempotency_key": "key-1"
        }))
        .unwrap();
        assert!(!req.transfer_open_exception_tasks);
        assert!(validate_handover_request(&req).is_ok());
    }

    #[test]
    fn handover_rejects_unknown_fields() {
        let parsed = serde_json::from_value::<HandoverFulfillmentOrderRequest>(serde_json::json!({
            "target_user_id": "buyer-2",
            "reason": "岗位调整",
            "expected_version": 1,
            "idempotency_key": "key-1",
            "owner": "张三"
        }));
        assert!(parsed.is_err());
    }
}
