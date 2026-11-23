//! 选品册访问密码与公开解锁合同。

use serde::{Deserialize, Serialize};
use validator::Validate;

use super::PublicSelectionPageView;

/// 访问密码解锁请求；禁止将请求调试打印到日志。
#[derive(Clone, Serialize, Deserialize, Validate)]
pub struct UnlockSelectionRequest {
    /// 选品册访问密码。
    pub password: String,
}

/// 解锁结果；授权仅对应本册、本会话与当前密码版本。
#[derive(Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PublicSelectionAccessView {
    /// 后续公开请求携带的短期访问令牌。
    pub access_token: String,
    /// 当前公开选品页。
    pub page: PublicSelectionPageView,
}

/// 设置或更换选品册访问密码。
#[derive(Clone, Serialize, Deserialize, Validate)]
pub struct SalesSelectionPasswordRequest {
    /// 册版本。
    pub expected_version: u64,
    /// 幂等键。
    pub idempotency_key: String,
    /// 新访问密码。
    #[validate(length(min = 8, max = 64))]
    pub access_password: String,
}
