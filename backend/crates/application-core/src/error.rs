//! 稳定应用错误分类与不含领域实现依赖的应用错误。

/// 服务层错误分类。协议层仅按该分类决定传输语义，不解析错误文案。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ErrorClass {
    /// 部署或服务内部不变量损坏。
    Internal,
    /// 当前状态或乐观锁冲突。
    Conflict,
    /// 业务前置条件不满足。
    BusinessRule,
    /// 当前主体没有执行权限。
    Forbidden,
}

/// 应用合同层可映射错误。
///
/// 不含旧 `entities`/`database` 错误或审批业务枚举；领域与持久化错误在上层显式映射。
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// 部署或服务内部不变量损坏。
    #[error("系统内部错误: {0}")]
    Internal(String),

    /// 请求参数或查询合同不合法。
    #[error("参数验证失败: {0}")]
    ValidationError(String),
}

/// 应用合同结果别名。
pub type Result<T> = std::result::Result<T, Error>;
