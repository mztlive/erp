//! 结算对账负责人交接与差异处理人改派 DTO。

use application_core::non_blank;
use serde::{Deserialize, Serialize};
use validator::Validate;

/// 显式交接对账负责人请求。
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
#[serde(deny_unknown_fields)]
pub struct HandoverSettlementRequest {
    /// 目标对账负责人。
    #[validate(custom(function = "non_blank", message = "目标对账负责人不能为空"))]
    pub target_user_id: String,
    /// 显式目标业务组织；省略表示保留原组织。
    pub target_org_unit_id: Option<String>,
    /// 非空交接原因。
    #[validate(length(min = 1, max = 512, message = "交接原因不能为空"))]
    pub reason: String,
    /// 期望的结算单乐观锁版本。
    #[validate(range(min = 1, message = "乐观锁版本必须大于 0"))]
    pub expected_version: u64,
    /// 业务请求幂等键。
    #[validate(length(min = 1, max = 128, message = "幂等键不能为空"))]
    pub idempotency_key: String,
}

/// 对账负责人交接结果。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct HandoverSettlementView {
    /// 结算单 ID。
    pub statement_id: String,
    /// 交接后对账负责人。
    pub prepared_by: String,
    /// 交接后业务组织。
    pub business_org_unit_id: String,
    /// 交接后版本。
    pub version: u64,
}

/// 独立改派差异处理人请求。
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
#[serde(deny_unknown_fields)]
pub struct ReassignSettlementDifferenceHandlerRequest {
    /// 目标差异处理人。
    #[validate(custom(function = "non_blank", message = "目标差异处理人不能为空"))]
    pub target_user_id: String,
    /// 非空改派原因。
    #[validate(length(min = 1, max = 512, message = "改派原因不能为空"))]
    pub reason: String,
    /// 期望的结算单乐观锁版本。
    #[validate(range(min = 1, message = "乐观锁版本必须大于 0"))]
    pub expected_version: u64,
    /// 业务请求幂等键。
    #[validate(length(min = 1, max = 128, message = "幂等键不能为空"))]
    pub idempotency_key: String,
}

/// 差异处理人改派结果。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ReassignSettlementDifferenceHandlerView {
    /// 结算单 ID。
    pub statement_id: String,
    /// 改派后差异处理人。
    pub difference_handler_user_id: String,
    /// 改派后版本。
    pub version: u64,
}

/// 交接待选目标。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct HandoverCandidateView {
    /// 目标账号 ID。
    pub user_id: String,
    /// 显示名。
    pub display_name: String,
    /// 登录账号。
    pub account: String,
}
