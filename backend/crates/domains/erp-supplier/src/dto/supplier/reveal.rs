use application_core::non_blank;
use serde::{Deserialize, Serialize};
use validator::Validate;

/// 供应商详情中的单个敏感字段揭示入口。
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct SupplierSensitiveFieldView {
    /// 页面稳定标签。
    pub label: String,
    /// 掩码展示值。
    pub masked_value: String,
    /// 受字段、事实行和供应商约束的短时令牌。
    pub reveal_token: String,
    /// 令牌过期时间（Unix 秒）。
    pub expires_at: u64,
}

/// 敏感字段揭示请求。
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct RevealSupplierSensitiveRequest {
    /// 详情接口签发的短时令牌。
    #[validate(custom(function = "non_blank", message = "揭示令牌不能为空"))]
    pub reveal_token: String,
}

/// 敏感字段揭示结果。
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct SupplierSensitiveRevealView {
    /// 解密后的明文；仅返回给已通过专用权限校验的当前请求。
    pub value: String,
}
