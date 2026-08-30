use super::response::ApiResponse;
use axum::{
    http::{header::RETRY_AFTER, HeaderValue, StatusCode},
    response::IntoResponse,
    Json,
};
use std::collections::BTreeMap;

const TECHNICAL_MESSAGE_TERMS: &[&str] = &[
    "work_item",
    "idempotency",
    "lockVersion",
    "幂等",
    "投影",
    "水位",
    "快照",
    "内容指纹",
    "锁版本",
    "服务端",
    "客户端",
    "前端",
    "数据库",
    "Mongo",
    "SQL",
    "状态机接口",
    "接口未交付",
    "接口",
    "事务",
    "指纹",
    "后端",
    "validation error",
    "payload",
    "canonical",
    "handler",
    "blocker",
    "view",
    "status",
    "dto",
    "enum",
    "rbac",
];

const USER_ACTION_MARKERS: &[&str] = &["请", "可以", "可在", "重新", "刷新", "稍后", "联系", "返回"];

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

    #[error("{0}")]
    Coded(services::ErrorCode),
}

impl From<database::Error> for Error {
    /// 将仓储错误转换为 HTTP 边界错误。
    ///
    /// 唯一键与乐观锁冲突使用 409，其余仓储错误保持内部错误语义。
    /// 唯一键冲突优先按已知索引名给出字段级提示。
    fn from(error: database::Error) -> Self {
        services::Error::from(error).into()
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
            services::Error::Coded(code) => Error::Coded(code),
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
        let http_status = self.http_status();
        let message = self.user_message();
        let code = self.error_code().to_string();
        let field_errors = self.field_errors();
        let retryable = self.retryable();

        let body = ApiResponse::<()> {
            status: http_status.as_u16(),
            message,
            code: Some(code),
            field_errors,
            retryable: Some(retryable),
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
    /// 返回稳定的 HTTP 状态码。
    ///
    /// # 返回
    /// 返回与错误分类对应的协议状态码。
    pub(crate) fn http_status(&self) -> StatusCode {
        match self {
            Error::Internal(_) | Error::Repository(_) | Error::OutcomeUnknown(_) => {
                StatusCode::INTERNAL_SERVER_ERROR
            }
            Error::BadRequest(_) | Error::Validation(_) => StatusCode::BAD_REQUEST,
            Error::NotFound(_) => StatusCode::NOT_FOUND,
            Error::Conflict(_) => StatusCode::CONFLICT,
            Error::Unprocessable(_) | Error::Logic(_) => StatusCode::UNPROCESSABLE_ENTITY,
            Error::Forbidden(_) => StatusCode::FORBIDDEN,
            Error::Unauthorized(_) => StatusCode::UNAUTHORIZED,
            Error::RateLimited(error) if error.retry_after_secs().is_some() => StatusCode::TOO_MANY_REQUESTS,
            Error::RateLimited(_) => StatusCode::INTERNAL_SERVER_ERROR,
            Error::Coded(code) => match code.class() {
                services::ErrorClass::Internal => StatusCode::INTERNAL_SERVER_ERROR,
                services::ErrorClass::Conflict => StatusCode::CONFLICT,
                services::ErrorClass::BusinessRule => StatusCode::UNPROCESSABLE_ENTITY,
                services::ErrorClass::Forbidden => StatusCode::FORBIDDEN,
            },
        }
    }

    /// 返回稳定的外部错误码；不得依赖可变的用户提示文案判断错误类型。
    pub(crate) fn error_code(&self) -> &'static str {
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
            Error::Coded(code) => code.as_str(),
        }
    }

    /// 返回可直接展示给业务用户的错误说明。
    ///
    /// 内部错误和包含实现术语的消息必须在 HTTP 边界替换为安全说明；
    /// 已经是业务语言的消息保留原原因，并在缺少恢复指引时补充下一步。
    ///
    /// # 返回
    /// 返回不包含底层实现细节的中文错误说明。
    pub(crate) fn user_message(&self) -> String {
        match self {
            Error::Internal(_) | Error::Repository(_) => {
                "系统暂时无法完成操作，请稍后重试；如仍失败，请联系支持人员".to_string()
            }
            Error::OutcomeUnknown(_) => {
                "操作结果暂无法确认，请先查询当前状态，确认未处理后再决定是否重试".to_string()
            }
            Error::BadRequest(message) => {
                user_message_or(message, "提交内容不符合要求，请检查后重试", "请修改后重试")
            }
            Error::Validation(_) => "提交内容不符合要求，请根据字段提示修改后重试".to_string(),
            Error::NotFound(message) => {
                user_message_or(message, "没有找到所需资料，请刷新后重新选择", "请刷新后重新选择")
            }
            Error::Conflict(message) => user_message_or(
                message,
                "当前资料状态不允许继续操作，请刷新后核对",
                "请核对当前资料后再操作",
            ),
            Error::Unprocessable(message) => user_message_or(
                message,
                "当前业务条件不允许继续操作，请核对相关资料后重试",
                "请核对相关业务条件后再操作",
            ),
            Error::Logic(error) => user_message_or(
                &error.to_string(),
                "当前业务条件不允许继续操作，请核对相关资料后重试",
                "请核对相关业务条件后再操作",
            ),
            Error::Forbidden(message) => user_message_or(
                message,
                "当前账号没有执行此操作的权限，请联系管理员或有权限的同事",
                "请联系管理员或有权限的同事",
            ),
            Error::Unauthorized(_) => "登录状态已失效，请重新登录".to_string(),
            Error::RateLimited(error) if error.retry_after_secs().is_some() => {
                "请求过于频繁，请稍后重试".to_string()
            }
            Error::RateLimited(_) => "系统暂时无法完成操作，请稍后重试；如仍失败，请联系支持人员".to_string(),
            Error::Coded(code) => match code.class() {
                services::ErrorClass::Internal => {
                    "系统暂时无法完成操作，请稍后重试；如仍失败，请联系支持人员".to_string()
                }
                services::ErrorClass::Conflict => "当前资料状态不允许继续操作，请刷新后核对".to_string(),
                services::ErrorClass::BusinessRule => {
                    "当前业务条件不允许继续操作，请核对相关资料后重试".to_string()
                }
                services::ErrorClass::Forbidden => {
                    "当前账号没有执行此操作的权限，请联系管理员或有权限的同事".to_string()
                }
            },
        }
    }

    /// 返回字段级校验说明。
    ///
    /// # 返回
    /// 仅直接持有 `validator::ValidationErrors` 时返回字段与用户提示；
    /// 其它错误返回 `None`。
    pub(crate) fn field_errors(&self) -> Option<BTreeMap<String, String>> {
        let Error::Validation(errors) = self else {
            return None;
        };
        let fields = errors
            .field_errors()
            .iter()
            .map(|(field, errors)| {
                let message = errors
                    .iter()
                    .filter_map(|error| error.message.as_deref())
                    .find(|message| user_message_is_safe(message))
                    .unwrap_or("该字段填写不符合要求")
                    .to_string();
                ((*field).to_string(), message)
            })
            .collect::<BTreeMap<_, _>>();
        (!fields.is_empty()).then_some(fields)
    }

    /// 返回前端能否安全提供原操作重试入口。
    ///
    /// # 返回
    /// 仅网络外的临时系统失败和限流允许直接重试；业务冲突与结果未知
    /// 必须先核对当前状态。
    pub(crate) fn retryable(&self) -> bool {
        matches!(
            self,
            Error::Internal(_) | Error::Repository(_) | Error::RateLimited(_)
        ) || matches!(self, Error::Coded(code) if code.retryable())
    }
}

/// 将业务原因规范为安全且包含下一步的用户说明。
fn user_message_or(message: &str, fallback: &str, next_step: &str) -> String {
    if !user_message_is_safe(message) {
        return fallback.to_string();
    }
    let message = message.trim().trim_end_matches(['。', '；', '，']);
    if USER_ACTION_MARKERS.iter().any(|marker| message.contains(marker)) {
        return message.to_string();
    }
    format!("{message}，{next_step}")
}

/// 判断内部错误原因是否符合用户展示合同。
fn user_message_is_safe(message: &str) -> bool {
    let message = message.trim();
    if message.is_empty()
        || !message
            .chars()
            .any(|character| ('\u{3400}'..='\u{9fff}').contains(&character))
    {
        return false;
    }
    let normalized = message.to_ascii_lowercase();
    if TECHNICAL_MESSAGE_TERMS
        .iter()
        .any(|term| normalized.contains(&term.to_ascii_lowercase()))
    {
        return false;
    }
    !message
        .split(|character: char| !(character.is_ascii_alphanumeric() || character == '_'))
        .any(is_internal_code_token)
}

/// 判断一个 ASCII 词是否为不应展示的内部稳定码或枚举值。
fn is_internal_code_token(token: &str) -> bool {
    if matches!(token, "SKU" | "ERP" | "PDF" | "CSV") || token.len() < 2 {
        return false;
    }
    token.chars().any(|character| character.is_ascii_uppercase())
        && token
            .chars()
            .all(|character| character.is_ascii_uppercase() || character.is_ascii_digit() || character == '_')
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
        assert_eq!(
            body["errorMessage"],
            "系统暂时无法完成操作，请稍后重试；如仍失败，请联系支持人员"
        );
        assert_eq!(body["retryable"], true);
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
            "操作结果暂无法确认，请先查询当前状态，确认未处理后再决定是否重试"
        );
        assert_eq!(body["retryable"], false);
        assert!(!body.to_string().contains("driver details"));
    }

    #[tokio::test]
    async fn business_error_keeps_reason_and_adds_next_step() {
        let response = Error::Conflict("主体编号已存在".into()).into_response();
        let body = to_bytes(response.into_body(), usize::MAX)
            .await
            .expect("response body should be readable");
        let body: Value = serde_json::from_slice(&body).expect("response body should be valid JSON");

        assert_eq!(body["errorMessage"], "主体编号已存在，请核对当前资料后再操作");
        assert_eq!(body["retryable"], false);
    }

    #[tokio::test]
    async fn technical_business_error_uses_safe_fallback() {
        let response = Error::Logic(entities::Error::from("同步水位不得回退")).into_response();
        let body = to_bytes(response.into_body(), usize::MAX)
            .await
            .expect("response body should be readable");
        let body: Value = serde_json::from_slice(&body).expect("response body should be valid JSON");

        assert_eq!(
            body["errorMessage"],
            "当前业务条件不允许继续操作，请核对相关资料后重试"
        );
        assert!(!body.to_string().contains("水位"));
    }

    #[tokio::test]
    async fn serialized_validation_details_use_safe_fallback() {
        let response = Error::BadRequest("customer_id: Validation error: required [客户不能为空]".into())
            .into_response();
        let body = to_bytes(response.into_body(), usize::MAX)
            .await
            .expect("response body should be readable");
        let body: Value = serde_json::from_slice(&body).expect("response body should be valid JSON");

        assert_eq!(body["errorMessage"], "提交内容不符合要求，请检查后重试");
        assert!(!body.to_string().contains("customer_id"));
    }

    #[tokio::test]
    async fn business_abbreviations_remain_readable() {
        let response = Error::Unprocessable("SKU 已停用".into()).into_response();
        let body = to_bytes(response.into_body(), usize::MAX)
            .await
            .expect("response body should be readable");
        let body: Value = serde_json::from_slice(&body).expect("response body should be valid JSON");

        assert_eq!(body["errorMessage"], "SKU 已停用，请核对相关业务条件后再操作");
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
