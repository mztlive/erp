#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("IO错误: {0}")]
    IoError(#[from] std::io::Error),

    #[error("路径错误: {0}")]
    PathError(String),

    #[error("文件不存在")]
    NotFound,
}

pub type Result<T> = std::result::Result<T, Error>;
