use erp_permissions::ParsePermissionError;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("{0}")]
    LogicError(String),

    /// 字段校验失败（结构化变体，逐步收敛字符串兜底；历史调用仍可用 `LogicError`）。
    #[error("字段校验失败：{field} {message}")]
    Validation { field: String, message: String },

    /// 状态迁移非法（数据模型第 7 章固定状态机，第 13 章禁止运行时扩展邻接矩阵）。
    #[error("非法状态迁移：{from} → {to}")]
    InvalidStateTransition { from: String, to: String },
}

impl From<&str> for Error {
    fn from(message: &str) -> Self {
        Self::LogicError(message.to_string())
    }
}

impl From<String> for Error {
    fn from(message: String) -> Self {
        Self::LogicError(message)
    }
}

impl From<ParsePermissionError> for Error {
    // 公共权限解析失败进入 ERP 共享错误时，保留原文案以兼容业务值校验的错误处理。
    fn from(error: ParsePermissionError) -> Self {
        Self::LogicError(error.to_string())
    }
}

impl Error {
    /// 字段规则失败时构造带字段名及说明的错误，便于上层保留结构而非拼成单一文案。
    pub fn validation(field: impl Into<String>, message: impl Into<String>) -> Self {
        Self::Validation { field: field.into(), message: message.into() }
    }
}

pub type Result<T> = std::result::Result<T, Error>;

#[cfg(test)]
mod tests {
    use super::Error;

    #[test]
    fn string_conversions_should_preserve_error_text() {
        let borrowed = Error::from("业务错误");
        let owned = Error::from("业务错误".to_string());
        assert_eq!(borrowed.to_string(), "业务错误");
        assert_eq!(owned.to_string(), "业务错误");
    }
}
