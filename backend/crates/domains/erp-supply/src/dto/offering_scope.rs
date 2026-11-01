//! 供给维护人交接 DTO。

use application_core::non_blank;
use serde::{Deserialize, Serialize};
use validator::Validate;

/// 供给维护人显式交接请求。
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
#[serde(deny_unknown_fields)]
pub struct HandoverSupplierOfferingRequest {
    /// 目标维护人。
    #[validate(custom(function = "non_blank", message = "目标维护人不能为空"))]
    pub target_user_id: String,
    /// 显式目标业务组织；省略表示保留原组织。
    pub target_org_unit_id: Option<String>,
    /// 非空交接原因。
    #[validate(length(min = 1, max = 512, message = "交接原因不能为空"))]
    pub reason: String,
    /// 期望的供给乐观锁版本。
    #[validate(range(min = 1, message = "乐观锁版本必须大于 0"))]
    pub expected_version: u64,
    /// 业务请求幂等键。
    #[validate(length(min = 1, max = 128, message = "幂等键不能为空"))]
    pub idempotency_key: String,
}

/// 供给维护人交接结果。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct HandoverSupplierOfferingView {
    /// 供给稳定 ID。
    pub offering_id: String,
    /// 交接后维护人。
    pub maintainer_user_id: String,
    /// 交接后业务组织。
    pub business_org_unit_id: String,
    /// 交接后供给版本。
    pub version: u64,
}

/// 供给交接待选目标。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct HandoverCandidateView {
    /// 目标账号 ID。
    pub user_id: String,
    /// 显示名。
    pub display_name: String,
    /// 登录账号。
    pub account: String,
}
