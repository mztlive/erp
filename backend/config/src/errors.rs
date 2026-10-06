use nacos_sdk::api::error::Error as SdkError;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum Error {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("TOML parse error at byte range {span:?}; configuration content omitted")]
    Toml { span: Option<std::ops::Range<usize>> },

    #[error("Invalid configuration: {0}")]
    Invalid(String),

    #[error("Nacos request failed; check authentication, permissions and SDK connectivity")]
    Nacos,

    #[error("Nacos response rejected: code={code}, error_code={error_code}")]
    NacosResponse { code: i32, error_code: i32 },

    #[error("Nacos configuration not found; check Namespace UUID, Group and Data ID")]
    NacosNotFound,

    #[error("Nacos request timed out after 30 seconds")]
    NacosTimeout,
}

pub type Result<T> = std::result::Result<T, Error>;

impl From<toml::de::Error> for Error {
    fn from(error: toml::de::Error) -> Self {
        // TOML 错误包含配置原文；不得把密钥带入启动错误或刷新日志。
        Self::Toml { span: error.span() }
    }
}

impl From<SdkError> for Error {
    fn from(error: SdkError) -> Self {
        // 保留协议错误码，不保留服务端自由文本或 SDK 的请求载荷。
        match error {
            SdkError::ErrResponse(_, code, error_code, _) => Self::NacosResponse { code, error_code },
            SdkError::ConfigNotFound(_) => Self::NacosNotFound,
            _ => Self::Nacos,
        }
    }
}
