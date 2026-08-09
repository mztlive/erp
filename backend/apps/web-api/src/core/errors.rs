use super::response::ApiResponse;
use axum::{
    http::{header::RETRY_AFTER, HeaderValue, StatusCode},
    response::IntoResponse,
    Json,
};

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("系统内部错误: {0}")]
    Internal(String),

    #[error("请求参数错误: {0}")]
    BadRequest(String),

    #[error("资源不存在: {0}")]
    NotFound(String),

    #[error("数据冲突: {0}")]
    Conflict(String),

    #[error("业务规则不满足: {0}")]
    Unprocessable(String),

    #[error("权限不足: {0}")]
    Forbidden(String),

    #[error("认证失败: {0}")]
    Unauthorized(String),

    #[error("操作结果暂无法确认，请查询当前状态后再决定是否重试")]
    OutcomeUnknown(#[source] database::Error),

    #[error(transparent)]
    RateLimited(#[from] crate::core::rate_limit::Error),

    #[error(transparent)]
    Repository(database::Error),

    #[error(transparent)]
    Logic(#[from] entities::Error),

    #[error(transparent)]
    Validation(#[from] validator::ValidationErrors),
}

impl From<database::Error> for Error {
    /// 将仓储错误转换为 HTTP 边界错误。
    ///
    /// 唯一键与乐观锁冲突使用 409，其余仓储错误保持内部错误语义。
    /// 唯一键冲突优先按已知索引名给出字段级提示。
    fn from(error: database::Error) -> Self {
        match error {
            error @ database::Error::DuplicateKey(_) => {
                Self::Conflict(duplicate_key_conflict_message(&error))
            }
            database::Error::OptimisticLockingError => {
                Self::Conflict("数据已被其他请求修改，请刷新后重试".to_string())
            }
            error @ database::Error::CommitOutcomeUnknown(_) => Self::OutcomeUnknown(error),
            other => Self::Repository(other),
        }
    }
}

/// 将唯一键冲突映射为面向用户的冲突提示。
///
/// # 参数
/// * `error` - 已归类为 `DuplicateKey` 的仓储错误
///
/// # 返回
/// 已知索引返回字段级中文提示；无法识别时返回通用冲突提示。
fn duplicate_key_conflict_message(error: &database::Error) -> String {
    match error.duplicate_index_name() {
        Some("uk_parties_party_no") => "主体编号已存在".to_string(),
        Some("uk_parties_credit_code") => "统一社会信用代码已存在".to_string(),
        Some("uk_party_bank_accounts_bank_account_no") => "银行账户编号已存在".to_string(),
        Some("uk_party_bank_accounts_party_hmac") => "该主体下银行账号已存在".to_string(),
        Some("uk_supplier_accounts_party") => "该主体已绑定供应商角色".to_string(),
        Some("uk_supplier_accounts_supplier_no") => "供应商编号已存在".to_string(),
        Some("uk_customer_accounts_party") => "该主体已绑定客户角色".to_string(),
        Some("uk_customer_accounts_customer_no") => "客户编号已存在".to_string(),
        _ => "数据已存在，请勿重复提交".to_string(),
    }
}

impl From<String> for Error {
    /// 从给定值构建实例。
    ///
    /// # 参数
    /// * `msg` - 错误信息
    ///
    /// # 返回
    /// 返回创建的实例。
    fn from(msg: String) -> Self {
        Error::Internal(msg)
    }
}

impl From<&str> for Error {
    /// 从给定值构建实例。
    ///
    /// # 参数
    /// * `msg` - 错误信息
    ///
    /// # 返回
    /// 返回创建的实例。
    fn from(msg: &str) -> Self {
        Error::Internal(msg.to_string())
    }
}

impl From<std::io::Error> for Error {
    /// 从给定值构建实例。
    ///
    /// # 参数
    /// * `err` - 错误对象
    ///
    /// # 返回
    /// 返回创建的实例。
    fn from(err: std::io::Error) -> Self {
        Error::Internal(err.to_string())
    }
}

impl From<services::Error> for Error {
    /// 从给定值构建实例。
    ///
    /// # 参数
    /// * `err` - 错误对象
    ///
    /// # 返回
    /// 返回创建的实例。
    fn from(err: services::Error) -> Self {
        match err {
            services::Error::ValidationError(msg) => Error::BadRequest(msg),
            services::Error::NotFound(msg) => Error::NotFound(msg),
            services::Error::ConflictError(msg) => Error::Conflict(msg),
            services::Error::BusinessLogicError(msg) => Error::Unprocessable(msg),
            services::Error::Forbidden(msg) => Error::Forbidden(msg),
            services::Error::Unauthenticated(msg) => Error::Unauthorized(msg),
            services::Error::Logic(err) => Error::Logic(err),
            services::Error::Internal(msg) => Error::Internal(msg),
            services::Error::OutcomeUnknown(error) => Error::OutcomeUnknown(error),
            other => Error::Internal(other.to_string()),
        }
    }
}

impl IntoResponse for Error {
    /// 转换为 HTTP 响应。
    ///
    /// # 返回
    /// 返回 `axum::response::Response` 实例。
    fn into_response(self) -> axum::response::Response {
        let retry_after_secs = match &self {
            Error::RateLimited(error) => error.retry_after_secs(),
            _ => None,
        };
        let (http_status, message) = match &self {
            Error::Internal(_) | Error::Repository(_) => {
                (StatusCode::INTERNAL_SERVER_ERROR, "系统内部错误".to_string())
            }
            Error::OutcomeUnknown(_) => (
                StatusCode::INTERNAL_SERVER_ERROR,
                "操作结果暂无法确认，请查询当前状态后再决定是否重试".to_string(),
            ),
            Error::BadRequest(_) | Error::Validation(_) => (StatusCode::BAD_REQUEST, self.to_string()),
            Error::NotFound(_) => (StatusCode::NOT_FOUND, self.to_string()),
            Error::Conflict(_) => (StatusCode::CONFLICT, self.to_string()),
            Error::Unprocessable(_) | Error::Logic(_) => (StatusCode::UNPROCESSABLE_ENTITY, self.to_string()),
            Error::Forbidden(_) => (StatusCode::FORBIDDEN, self.to_string()),
            Error::Unauthorized(_) => (StatusCode::UNAUTHORIZED, "Unauthorized".to_string()),
            Error::RateLimited(error) => match error.retry_after_secs() {
                Some(_) => (
                    StatusCode::TOO_MANY_REQUESTS,
                    "请求过于频繁，请稍后重试".to_string(),
                ),
                None => (StatusCode::INTERNAL_SERVER_ERROR, "系统内部错误".to_string()),
            },
        };

        let body = ApiResponse::<()> {
            status: http_status.as_u16(),
            message,
            code: Some(self.error_code().to_string()),
            data: None,
            success: false,
        };

        let mut response = (http_status, Json(body)).into_response();
        if http_status == StatusCode::TOO_MANY_REQUESTS {
            if let Some(retry_after_secs) = retry_after_secs {
                if let Ok(value) = HeaderValue::try_from(retry_after_secs.to_string()) {
                    response.headers_mut().insert(RETRY_AFTER, value);
                }
            }
        }
        response
    }
}

impl Error {
    /// 返回稳定的外部错误码；不得依赖可变的用户提示文案判断错误类型。
    fn error_code(&self) -> &'static str {
        match self {
            Error::Internal(_) | Error::Repository(_) => "INTERNAL_ERROR",
            Error::BadRequest(_) | Error::Validation(_) => "INVALID_REQUEST",
            Error::NotFound(_) => "NOT_FOUND",
            Error::Conflict(_) => "CONFLICT",
            Error::Unprocessable(_) | Error::Logic(_) => "BUSINESS_RULE_BLOCKED",
            Error::Forbidden(_) => "PERMISSION_DENIED",
            Error::Unauthorized(_) => "UNAUTHENTICATED",
            Error::OutcomeUnknown(_) => "OUTCOME_UNKNOWN",
            Error::RateLimited(_) => "RATE_LIMITED",
        }
    }
}

pub type Result<T> = std::result::Result<ApiResponse<T>, Error>;

#[cfg(test)]
mod tests {
    use super::Error;
    use axum::body::to_bytes;
    use axum::http::{header::RETRY_AFTER, StatusCode};
    use axum::response::IntoResponse;
    use serde_json::Value;

    #[test]
    fn maps_service_error_with_semantics() {
        let not_found: Error = services::Error::NotFound("x".into()).into();
        let conflict: Error = services::Error::ConflictError("x".into()).into();
        let validation: Error = services::Error::ValidationError("x".into()).into();
        let business: Error = services::Error::BusinessLogicError("x".into()).into();
        let forbidden: Error = services::Error::Forbidden("x".into()).into();
        let unauthorized: Error = services::Error::Unauthenticated("x".into()).into();
        let logic: Error = services::Error::Logic(entities::Error::from("x")).into();
        let outcome_unknown: Error = services::Error::from(database::Error::CommitOutcomeUnknown(
            mongodb::error::Error::custom("unknown"),
        ))
        .into();

        assert!(matches!(not_found, Error::NotFound(_)));
        assert!(matches!(conflict, Error::Conflict(_)));
        assert!(matches!(validation, Error::BadRequest(_)));
        assert!(matches!(business, Error::Unprocessable(_)));
        assert!(matches!(forbidden, Error::Forbidden(_)));
        assert!(matches!(unauthorized, Error::Unauthorized(_)));
        assert!(matches!(logic, Error::Logic(_)));
        assert!(matches!(outcome_unknown, Error::OutcomeUnknown(_)));
    }

    #[test]
    fn maps_optimistic_locking_error_to_conflict() {
        let error: Error = database::Error::OptimisticLockingError.into();

        assert!(matches!(error, Error::Conflict(_)));
    }

    #[test]
    fn duplicate_key_race_maps_to_http_conflict() {
        let repository_error = database::Error::DuplicateKey(mongodb::error::Error::custom("duplicate key"));
        let service_error = services::Error::from(repository_error);
        let response = Error::from(service_error).into_response();

        assert_eq!(response.status(), StatusCode::CONFLICT);
    }

    #[test]
    fn maps_http_status_code_correctly() {
        let cases = [
            (
                Error::BadRequest("x".into()),
                StatusCode::BAD_REQUEST,
                "INVALID_REQUEST",
            ),
            (Error::NotFound("x".into()), StatusCode::NOT_FOUND, "NOT_FOUND"),
            (Error::Conflict("x".into()), StatusCode::CONFLICT, "CONFLICT"),
            (
                Error::Unprocessable("x".into()),
                StatusCode::UNPROCESSABLE_ENTITY,
                "BUSINESS_RULE_BLOCKED",
            ),
            (
                Error::Forbidden("x".into()),
                StatusCode::FORBIDDEN,
                "PERMISSION_DENIED",
            ),
            (
                Error::Unauthorized("x".into()),
                StatusCode::UNAUTHORIZED,
                "UNAUTHENTICATED",
            ),
            (
                Error::RateLimited(crate::core::rate_limit::Error::ConcurrencyExceeded),
                StatusCode::TOO_MANY_REQUESTS,
                "RATE_LIMITED",
            ),
            (
                Error::Logic(entities::Error::from("x")),
                StatusCode::UNPROCESSABLE_ENTITY,
                "BUSINESS_RULE_BLOCKED",
            ),
            (
                Error::Internal("x".into()),
                StatusCode::INTERNAL_SERVER_ERROR,
                "INTERNAL_ERROR",
            ),
            (
                Error::OutcomeUnknown(database::Error::CommitOutcomeUnknown(
                    mongodb::error::Error::custom("unknown"),
                )),
                StatusCode::INTERNAL_SERVER_ERROR,
                "OUTCOME_UNKNOWN",
            ),
        ];

        for (err, expected_status, expected_code) in cases {
            assert_eq!(err.error_code(), expected_code);
            let response = err.into_response();
            assert_eq!(response.status(), expected_status);
        }
    }

    #[tokio::test]
    async fn internal_error_does_not_expose_underlying_message() {
        let response = Error::Internal("database password leaked".into()).into_response();
        let body = to_bytes(response.into_body(), usize::MAX)
            .await
            .expect("response body should be readable");
        let body: Value = serde_json::from_slice(&body).expect("response body should be valid JSON");

        assert_eq!(body["status"], 500);
        assert_eq!(body["errorMessage"], "系统内部错误");
        assert_eq!(body["success"], false);
        assert!(!body.to_string().contains("database password leaked"));
    }

    #[tokio::test]
    async fn unknown_commit_outcome_has_stable_non_sensitive_response() {
        let response = Error::OutcomeUnknown(database::Error::CommitOutcomeUnknown(
            mongodb::error::Error::custom("driver details"),
        ))
        .into_response();
        let body = to_bytes(response.into_body(), usize::MAX)
            .await
            .expect("response body should be readable");
        let body: Value = serde_json::from_slice(&body).expect("response body should be valid JSON");

        assert_eq!(body["status"], 500);
        assert_eq!(
            body["errorMessage"],
            "操作结果暂无法确认，请查询当前状态后再决定是否重试"
        );
        assert!(!body.to_string().contains("driver details"));
    }

    #[test]
    fn rate_limit_error_sets_429_and_retry_after() {
        let response =
            Error::RateLimited(crate::core::rate_limit::Error::KeyExceeded { retry_after_secs: 12 })
                .into_response();

        assert_eq!(response.status(), StatusCode::TOO_MANY_REQUESTS);
        assert_eq!(response.headers().get(RETRY_AFTER).unwrap(), "12");
    }

    #[test]
    fn unavailable_rate_limit_state_fails_as_internal_error() {
        let response = Error::RateLimited(crate::core::rate_limit::Error::Unavailable).into_response();

        assert_eq!(response.status(), StatusCode::INTERNAL_SERVER_ERROR);
        assert!(response.headers().get(RETRY_AFTER).is_none());
    }
}
