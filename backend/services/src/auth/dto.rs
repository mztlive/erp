use entities::AccountKind;
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

/// 账号密码登录参数。
///
/// 该 DTO 由 HTTP 与认证 Service 共同复用，Service 仍负责执行校验，避免
/// 非 HTTP 调用绕过输入合同。
#[derive(Debug, Deserialize, Validate)]
pub struct PasswordLoginPayload {
    #[validate(length(min = 4, max = 64))]
    pub account: String,
    #[validate(length(min = 6, max = 64))]
    pub password: String,
}

#[cfg(test)]
mod tests {
    use entities::AccountKind;

    use super::{AuthRequest, PasswordLoginPayload};
    use validator::Validate;

    #[test]
    fn password_login_payload_should_accept_valid_credentials() {
        let payload = PasswordLoginPayload {
            account: "consumer01".to_string(),
            password: "password123".to_string(),
        };

        assert!(payload.validate().is_ok());
    }

    #[test]
    fn password_login_payload_should_reject_short_password() {
        let payload = PasswordLoginPayload {
            account: "consumer01".to_string(),
            password: "short".to_string(),
        };

        assert!(payload.validate().is_err());
    }

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
