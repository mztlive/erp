#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("IO错误: {0}")]
    IoError(#[from] std::io::Error),

    #[error("路径错误: {0}")]
    PathError(String),

    #[error("存储配置错误: {0}")]
    InvalidConfig(String),

    #[error("文件不存在")]
    NotFound,

    #[error("S3 存储操作失败: {0}")]
    S3(String),
}

pub type Result<T> = std::result::Result<T, Error>;
