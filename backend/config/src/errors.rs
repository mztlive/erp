use thiserror::Error;

#[derive(Debug, Error)]
pub enum Error {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("TOML parse error: {0}")]
    Toml(#[from] toml::de::Error),

    #[error("Invalid configuration: {0}")]
    Invalid(String),

    #[error("Nacos error: {0}")]
    Nacos(Box<nacos_sdk::api::error::Error>),
}

pub type Result<T> = std::result::Result<T, Error>;

impl From<nacos_sdk::api::error::Error> for Error {
    /// 从 Nacos SDK 错误转换为配置错误。
    ///
    /// # 参数
    /// * `err` - Nacos SDK 错误
    ///
    /// # 返回值
    /// 返回配置错误
    fn from(err: nacos_sdk::api::error::Error) -> Self {
        Error::Nacos(Box::new(err))
    }
}
