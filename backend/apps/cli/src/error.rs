/// CLI 统一错误。
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// 配置文件读取或校验失败。
    #[error(transparent)]
    Config(#[from] config::Error),
    /// MongoDB 连接、事务探测或索引初始化失败。
    #[error(transparent)]
    Database(#[from] database::Error),
    /// 服务编排或领域校验失败。
    #[error(transparent)]
    Service(#[from] services::Error),
    /// 命令行用法或交互输入不合法。
    #[error("{0}")]
    Usage(String),
}

/// CLI 结果别名。
pub type Result<T> = std::result::Result<T, Error>;
