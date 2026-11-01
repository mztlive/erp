//! 供应商维护人与能力负责人交接 DTO。

use application_core::non_blank;
use serde::{Deserialize, Serialize};
use validator::Validate;

/// 显式供应商维护人交接请求。
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
#[serde(deny_unknown_fields)]
pub struct HandoverSupplierRequest {
    /// 期望的供应商乐观锁版本。
    #[validate(range(min = 1, message = "乐观锁版本必须大于 0"))]
    pub expected_version: u64,
    /// 目标整体维护人。
    #[validate(custom(function = "non_blank", message = "目标维护人不能为空"))]
    pub target_user_id: String,
    /// 显式目标业务组织；省略表示保留原组织。
    pub target_org_unit_id: Option<String>,
    /// 非空交接原因。
    #[validate(length(min = 1, max = 512, message = "交接原因不能为空"))]
    pub reason: String,
    /// 业务请求幂等键。
    #[validate(length(min = 1, max = 128, message = "幂等键不能为空"))]
    pub idempotency_key: String,
}

/// 显式供给能力负责人交接请求。
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
#[serde(deny_unknown_fields)]
pub struct HandoverSupplierCapabilityRequest {
    /// 期望的能力乐观锁版本。
    #[validate(range(min = 1, message = "乐观锁版本必须大于 0"))]
    pub expected_version: u64,
    /// 目标能力负责人。
    #[validate(custom(function = "non_blank", message = "目标能力负责人不能为空"))]
    pub target_user_id: String,
    /// 非空交接原因。
    #[validate(length(min = 1, max = 512, message = "交接原因不能为空"))]
    pub reason: String,
    /// 业务请求幂等键。
    #[validate(length(min = 1, max = 128, message = "幂等键不能为空"))]
    pub idempotency_key: String,
}

/// 供应商维护人交接结果。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct HandoverSupplierView {
    /// 供应商 ID。
    pub supplier_id: String,
    /// 交接后整体维护人。
    pub maintainer_user_id: String,
    /// 交接后业务组织。
    pub business_org_unit_id: String,
    /// 交接后供应商版本。
    pub version: u64,
}

/// 供给能力负责人交接结果。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct HandoverSupplierCapabilityView {
    /// 供应商 ID。
    pub supplier_id: String,
    /// 能力 ID。
    pub capability_id: String,
    /// 交接后能力负责人。
    pub owner_user_id: String,
    /// 交接后能力版本。
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
