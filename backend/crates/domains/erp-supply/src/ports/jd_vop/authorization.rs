//! 京东凭据提供与授权通信的窄合同；持久化实现由编排层提供。
use async_trait::async_trait;
use serde::Deserialize;
use url::Url;
use uuid::Uuid;
use zeroize::Zeroizing;

use super::oauth::JdVopRefreshResult;
use crate::entity::supplier_api::SupplierApiConnection;
/// 页面授权固定脱敏错误的结果别名。
pub type Result<T> = std::result::Result<T, AuthorizationError>;

/// 只含固定分类，不持有授权输入、供应商正文或底层HTTP错误。
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum AuthorizationError {
    #[error("授权资料格式不正确，请核对账号、密码、回调地址和私钥文件")]
    InvalidInput,
    #[error("令牌资料格式不正确，请填写同次授权的两项令牌和实际到期时间")]
    InvalidTokens,
    #[error("当前账号不能授权此供应商连接，请联系管理员核对权限")]
    Forbidden,
    #[error("授权状态已变化，请先查询原结果后再操作")]
    Conflict,
    #[error("京东连接与当前凭据配置不一致，请联系管理员核验")]
    BindingMismatch,
    #[error("京东账号与原授权账号不一致，请使用原采购账号或联系管理员核验")]
    IdentityMismatch,
    #[error("京东授权配置或状态暂不可用，请联系管理员核验")]
    Unavailable,
    #[error("原授权结果待核实，请查询状态，不要重复提交")]
    Unknown,
    #[error("京东未接受登录资料，请核对后重新授权")]
    Rejected,
    #[error("已有授权正在处理，请稍后查询结果")]
    Busy,
}

/// 页面单次授权命令；密码及私钥退出作用域时清除，不实现 Debug 或 Serialize。
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct JdVopAuthorizationRequest {
    pub operation_id: String,
    pub expected_version: Option<u64>,
    pub username: Zeroizing<String>,
    pub password: Zeroizing<String>,
    pub redirect_uri: String,
    pub private_key: Zeroizing<String>,
    pub evidence_reference: Option<String>,
}

/// 查询原授权操作时只接受不含凭据的操作ID。
#[derive(Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct JdVopAuthorizationStatusQuery {
    pub operation_id: Option<String>,
}

impl JdVopAuthorizationRequest {
    /// 校验授权资料的有界字段和回调地址。
    /// 字段均满足受控授权合同后成功。
    /// 字段非法时返回固定脱敏错误。
    pub fn validate(&self) -> Result<()> {
        if Uuid::parse_str(&self.operation_id).is_err()
            || self.expected_version.is_some_and(|version| version == 0 || version > i64::MAX.unsigned_abs())
            || !bounded(&self.username, 256)
            || self.username.trim() != self.username.as_str()
            || !bounded(&self.password, 4096)
            || self.private_key.is_empty()
            || self.private_key.len() > 32768
            || self
                .evidence_reference
                .as_ref()
                .is_some_and(|value| !bounded(value, 1024) || value.trim().is_empty())
        {
            return Err(AuthorizationError::InvalidInput);
        }

        callback_uri(&self.redirect_uri)?;
        Ok(())
    }
}

impl JdVopAuthorizationStatusQuery {
    /// 校验可选操作ID，不解析或回显任何授权正文。
    /// 未指定操作或符合UUID格式时成功。
    /// 操作ID非法时返回固定错误。
    pub fn validate(&self) -> Result<()> {
        if self.operation_id.as_ref().is_some_and(|id| Uuid::parse_str(id).is_err()) {
            return Err(AuthorizationError::InvalidInput);
        }
        Ok(())
    }
}

fn bounded(value: &str, maximum: usize) -> bool {
    !value.is_empty() && value.len() <= maximum && !value.chars().any(char::is_control)
}

/// 校验授权回调地址，不访问其网络。
/// 保留原查询参数的合法地址。
/// 非 HTTP(S)、账号密码、片段或保留查询字段时拒绝。
pub fn callback_uri(value: &str) -> Result<Url> {
    let uri = Url::parse(value).map_err(|_| AuthorizationError::InvalidInput)?;
    if value.len() > 2048
        || value.trim() != value
        || uri.host_str().is_none()
        || !matches!(uri.scheme(), "http" | "https")
        || !uri.username().is_empty()
        || uri.password().is_some()
        || uri.fragment().is_some()
        || uri.query_pairs().any(|(key, _)| matches!(key.as_ref(), "code" | "state" | "error"))
    {
        return Err(AuthorizationError::InvalidInput);
    }
    Ok(uri)
}

/// 已通过本地协议预检的短生命周期授权参数，不允许日志或序列化。
pub struct AuthorizationParameters {
    pub query: Zeroizing<String>,
    pub callback: Url,
}

/// 官方授权响应的凭据与账号观察；编排必须按原版本持久化。
pub struct AuthorizationGrant {
    pub tokens: JdVopRefreshResult,
    pub identity: Option<Zeroizing<String>>,
}

/// 应用注入的京东授权技术协议；每一步通信均由编排单独持久化发送身份。
#[async_trait]
pub trait AuthorizationHttp: Send + Sync {
    /// 检查授权通信是否完成初始化，不影响本地绑定和资料校验。
    /// HTTP 客户端已就绪时成功，不发送请求。
    /// HTTP 客户端初始化失败时返回未就绪错误。
    fn ensure_available(&self) -> Result<()>;

    /// 重新核验持久化连接与当前技术账号绑定，不发送请求。
    /// 连接绑定仍有效时成功。
    /// 技术配置或引用不匹配时拒绝。
    /// `connection` 为同事务加载的持久化连接。
    fn verify_binding(&self, connection: &SupplierApiConnection) -> Result<()>;

    /// 预检及编码资料，不发送网络；失败时编排不得认领发送。
    /// 不含明文密码的已编码授权参数。
    /// 资料格式或私钥非法时拒绝。
    /// 单次授权资料与当前应用标识。
    fn prepare(&self, request: &JdVopAuthorizationRequest, app_key: &str) -> Result<AuthorizationParameters>;
    /// 编码已取得的授权码，不发送网络。
    /// 短生命周期交换参数。
    /// 不启动网络。
    /// 应用标识、应用密钥与刚取得的原授权码。
    fn exchange_parameters(&self, app_key: &str, app_secret: &str, code: &str) -> Zeroizing<String>;
    /// 只发送原授权请求一次，返回官方重定向中的授权码。
    /// 原回调地址携带的授权码。
    /// 明确拒绝或结果未知时停止执行。
    async fn code(&self, parameters: AuthorizationParameters) -> Result<Zeroizing<String>>;
    /// 只交换原授权码一次；失败不得重放。
    /// 新凭据与可选的账号观察。
    /// 交换结果未知或响应无效时拒绝。
    async fn exchange(&self, parameters: Zeroizing<String>) -> Result<AuthorizationGrant>;
}
