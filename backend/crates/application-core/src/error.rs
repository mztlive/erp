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
/// 变体与 [`ErrorClass`] 的映射见 [`Error::class`]，上层经它统一转换。
/// 本层变体与 [`ErrorClass`] 的对应关系：
///
/// 对应关系：`Internal` -> `Internal`；`ValidationError` -> `BusinessRule`。
/// 冲突与权限拒绝由拥有领域映射，本层不新增变体以保持公开签名稳定。
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// 部署或服务内部不变量损坏。
    #[error("系统内部错误: {0}")]
    Internal(String),

    /// 请求参数或查询合同不合法。
    #[error("参数验证失败: {0}")]
    ValidationError(String),
}

impl Error {
    /// 返回稳定的错误分类，供协议层决定传输语义。
    ///
    /// # 参数
    /// 无。
    ///
    /// # 返回
    /// 返回与变体对应的分类。
    ///
    /// # 错误
    /// 无。
    pub fn class(&self) -> ErrorClass {
        match self {
            Self::Internal(_) => ErrorClass::Internal,
            Self::ValidationError(_) => ErrorClass::BusinessRule,
        }
    }
}

impl From<validator::ValidationErrors> for Error {
    /// 将请求校验失败转为参数错误。
    ///
    /// # 参数
    /// * `errors` - 校验失败明细
    ///
    /// # 返回
    /// 返回参数验证错误。
    ///
    /// # 错误
    /// 无。
    fn from(errors: validator::ValidationErrors) -> Self {
        Self::ValidationError(errors.to_string())
    }
}

/// 应用合同结果别名。
pub type Result<T> = std::result::Result<T, Error>;
