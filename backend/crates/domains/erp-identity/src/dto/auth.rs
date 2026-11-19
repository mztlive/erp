use erp_core::AccountKind;
use serde::{Deserialize, Serialize};
use validator::Validate;

/// 接收后台登录凭证；身份类型参数不能切换到供应商门户认证。
#[derive(Debug, Deserialize, Validate)]
pub struct AuthRequest {
    /// 待验证的登录账号，长度为 3 至 32 个字符。
    #[validate(length(min = 3, max = 32, message = "账号长度须为 3 至 32 个字符"))]
    pub account: String,
    /// 待验证的密码，长度为 6 至 32 个字符。
    #[validate(length(min = 6, max = 32, message = "密码长度须为 6 至 32 个字符"))]
    pub password: String,
    /// 请求声明的账号类型；后台认证仅接受 `Admin`。
    pub account_kind: AccountKind,
}

/// 后台登录成功后返回给客户端的会话信息。
#[derive(Debug, Serialize, Deserialize)]
pub struct AuthResponse {
    /// 后续受保护请求使用的 JWT。
    pub token: String,
}

#[cfg(test)]
mod tests {
    use super::*;
    // 验证后台登录参数拒绝超出长度上限的账号和密码。
    #[test]
    fn backoffice_auth_request_rejects_unbounded_credentials() {
        let request = AuthRequest {
            account: "x".repeat(33),
            password: "x".repeat(33),
            account_kind: AccountKind::Admin,
        };
        assert!(request.validate().is_err());
    }
}
