use erp_core::AccountKind;
use serde::{Deserialize, Serialize};
use validator::Validate;

/// 认证请求结构体。
///
/// 用于接收客户端发送的登录认证请求。
#[derive(Debug, Deserialize, Validate)]
pub struct AuthRequest {
    #[validate(length(min = 3, max = 32))]
    pub account: String,
    #[validate(length(min = 6, max = 32))]
    pub password: String,
    pub account_kind: AccountKind,
}

/// 认证响应结构体。
///
/// 用于向客户端返回认证结果。
#[derive(Debug, Serialize, Deserialize)]
pub struct AuthResponse {
    pub token: String,
}

#[cfg(test)]
mod tests {
    use super::*;
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
