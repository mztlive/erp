/// CLI 统一错误。
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// 凭据文件输入错误；不保留原始 JSON 或底层错误。
    #[error(transparent)]
    CredentialInput(#[from] cli::jd_vop_input::InputError),
    /// 输入任务异常，禁止输出任务中可能携带的原始内容。
    #[error("凭据输入任务失败；未执行导入")]
    CredentialInputTask,
    /// 京东凭据管理用例的稳定脱敏错误。
    #[error(transparent)]
    Connector(#[from] erp_supply::ports::connector::common::ConnectorError),
    /// 配置文件读取或校验失败。
    #[error(transparent)]
    Config(#[from] config::Error),
    /// MongoDB 连接、事务探测或索引初始化失败。
    #[error(transparent)]
    Database(#[from] persistence_core::Error),
    /// 身份用例失败。
    #[error(transparent)]
    Identity(#[from] erp_identity::Error),
    /// 命令行用法或交互输入不合法。
    #[error("{0}")]
    Usage(String),
}

/// CLI 结果别名。
pub type Result<T> = std::result::Result<T, Error>;
