//! 受控令牌读取合同。刷新由拥有凭据的服务执行，协议客户端不自行登录或持久化令牌。
use async_trait::async_trait;
use erp_core::common::time::Instant;

use crate::entity::failure::SupplierFailureClass;
use crate::ports::connector::common::{ConnectorError, ConnectorResult};

/// 带账号、版本及有效期的访问凭据；禁止 Debug 和业务序列化。
pub struct JdVopAccessToken {
    value: String,
    version: String,
    account_scope: String,
    expires_at: Instant,
}

impl JdVopAccessToken {
    /// 保存受控凭据存储返回的令牌快照。
    /// # 参数
    /// `value` 是密文存储解出的令牌，其他参数为同一读取快照的版本、账号和有效期。
    /// # 返回
    /// 不会通过 Debug 或 Serialize 暴露内容的凭据对象。
    /// # 错误
    /// 令牌、版本或账号不完整时返回脱敏鉴权错误。
    pub fn new(
        value: String,
        version: String,
        account_scope: String,
        expires_at: Instant,
    ) -> ConnectorResult<Self> {
        if value.trim().is_empty()
            || value.trim() != value
            || value.len() > 8192
            || value.chars().any(char::is_control)
            || [&version, &account_scope].iter().any(|v| {
                v.trim().is_empty()
                    || v.len() > 256
                    || v.trim() != v.as_str()
                    || v.chars().any(char::is_control)
            })
        {
            return Err(token_error());
        }
        Ok(Self { value, version, account_scope, expires_at })
    }

    /// 返回凭据存储的版本，供受控刷新进行版本竞争。
    /// # 参数
    /// 无。
    /// # 返回
    /// 当前读取快照版本，不包含令牌内容。
    /// # 错误
    /// 不返回错误。
    pub fn version(&self) -> &str {
        &self.version
    }

    /// 为当前账号取得未过期的原始令牌，不改变存储版本。
    /// # 参数
    /// 服务器账号命名空间和发送前时刻。
    /// # 返回
    /// 仅供签名编码使用的凭据借用。
    /// # 错误
    /// 账号不符或已经到期时拒绝发送。
    pub fn value_for(&self, account: &str, now: Instant) -> ConnectorResult<&str> {
        if self.account_scope != account || self.expires_at <= now {
            return Err(token_error());
        }
        Ok(&self.value)
    }
}

/// 由凭据拥有服务提供稳定引用的读取；实现须负责单飞刷新及版本化更新。
#[async_trait]
pub trait JdVopTokenProvider: Send + Sync {
    /// 取得稳定技术引用的有效令牌快照，不接受业务请求传入令牌正文。
    /// # 参数
    /// `reference` 来自受控部署配置。
    /// # 返回
    /// 包含账号、版本及有效期的快照；同账号轮换不改变引用命名空间。
    /// # 错误
    /// 无授权、存储失败或刷新未完成时返回错误，调用方必须停止发送。
    async fn access_token(&self, reference: &str) -> ConnectorResult<JdVopAccessToken>;

    /// 等待已经开始的刷新持久化结束，供应用停止时收敛外部授权操作。
    /// # 参数
    /// 无；不启动新刷新或业务请求。
    /// # 返回
    /// 所有已经开始的受控刷新已经结束；普通只读提供器直接成功。
    /// # 错误
    /// 刷新任务边界无法收敛时返回脱敏错误。
    async fn settle_refresh(&self) -> ConnectorResult<()> {
        Ok(())
    }
}

/// 生成不携带凭据或存储错误正文的鉴权失败。
/// # 参数
/// 无。
/// # 返回
/// 稳定的令牌不可用错误。
/// # 错误
/// 不返回额外错误。
pub fn token_error() -> ConnectorError {
    ConnectorError {
        class: SupplierFailureClass::AuthSignature,
        code: "JD_VOP_TOKEN_UNAVAILABLE".into(),
        summary: "京东授权令牌无效、已过期或无法读取，请核对账号授权".into(),
        retry_after: None,
    }
}
