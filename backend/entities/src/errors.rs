#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("{0}")]
    LogicError(String),
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
