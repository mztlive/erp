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

impl Error {
    /// 构造字段校验错误。
    ///
    /// # 参数
    /// * `field` - 字段名
    /// * `message` - 校验说明
    ///
    /// # 返回
    /// 返回结构化校验错误。
    ///
    /// # 错误
    /// 本身不返回错误，仅构造错误值。
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
