//! 页面授权命名用例：先持久互斥，再在事务外单次调用官方端点，最后CAS保存密文及审计。
mod access;
mod dto;
mod engine;
mod http;
mod protocol;
mod status;
mod store;
mod tasks;
mod tokens;

use std::sync::Arc;

use access::ConnectionAccess;
use application_core::AuditActor;
use config::JdVopConfig;
pub use dto::{JdVopAuthorizationRequest, JdVopAuthorizationStatus, JdVopAuthorizationStatusQuery};
use engine::AuthorizationEngine;
use erp_core::common::time::Instant;
use erp_identity::SharedRbacService;
use mongodb::Database;
use persistence_core::NoTransaction;
pub use tasks::JdVopAuthorizationTasks;
pub use tokens::JdVopTokenConfiguration;

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

/// 供应商指定连接的页面授权入口；任务生命周期由应用持有的共享边界负责。
pub struct JdVopAuthorizationProcess {
    access: ConnectionAccess,
    engine: Result<AuthorizationEngine>,
}

impl JdVopAuthorizationProcess {
    /// 装配指定部署账号的页面授权用例，不读取数据库或发送HTTP。
    /// # 参数
    /// 应用数据库、可选京东配置、现有RBAC与应用共享任务边界。
    /// # 返回
    /// 状态查询和单次授权入口；未配置时仍可返回脱敏未就绪状态。
    /// # 错误
    /// 不返回错误；配置错误保留为脱敏未就绪状态，不能发送授权。
    pub fn new(
        db: Database,
        settings: Option<&JdVopConfig>,
        rbac: SharedRbacService,
        tasks: Arc<JdVopAuthorizationTasks>,
    ) -> Self {
        let access = ConnectionAccess { db, rbac, settings: settings.cloned() };
        let engine = settings
            .ok_or(AuthorizationError::Unavailable)
            .and_then(|settings| AuthorizationEngine::new(access.clone(), settings, tasks));
        Self { access, engine }
    }

    /// 按当前操作人的连接范围返回本地授权状态，不发起授权、续期或健康检查。
    /// # 参数
    /// 原供应商连接ID与已认证操作人。
    /// # 返回
    /// 不包含账号、密码、私钥、令牌或密文的状态。
    /// # 错误
    /// 无读取权限、连接范围不符或非京东连接时拒绝。
    pub async fn status(
        &self,
        id: &str,
        actor: &AuditActor,
        operation_id: Option<&str>,
    ) -> Result<JdVopAuthorizationStatus> {
        let connection = self.access.require(id, actor, "detail", &mut NoTransaction).await?;
        let Ok(engine) = &self.engine else {
            return Ok(JdVopAuthorizationStatus::empty(
                "unconfigured",
                false,
                "京东授权配置未就绪，请联系管理员配置应用资料",
            ));
        };
        if self.access.binding(&connection).is_err() {
            return Ok(JdVopAuthorizationStatus::empty(
                "unconfigured",
                false,
                "京东连接与已部署账号配置不一致，请联系管理员核验",
            ));
        }
        let mut status = engine.status(operation_id).await;
        let permitted = self.access.permission(actor, "authorize").await?;
        status.can_authorize &= permitted;
        status.can_configure_tokens = permitted
            && status.state != "unconfigured"
            && (status.state != "unreadable" || status.version.is_some())
            && status.recovery_after.is_none_or(|deadline| deadline <= Instant::now().unix_secs());
        Ok(status)
    }

    /// 领取单次授权并交给应用后台有界执行，页面立即获得pending或原操作状态。
    /// # 参数
    /// 原京东连接、短生命周期授权正文及已认证操作人。
    /// # 返回
    /// 只含脱敏状态；同operation重放不再次调用京东，令牌不返回页面。
    /// # 错误
    /// 资料、权限、连接绑定、账号身份、所见版本或并发边界不符时拒绝。
    pub async fn authorize(
        &self,
        id: &str,
        request: JdVopAuthorizationRequest,
        actor: &AuditActor,
    ) -> Result<JdVopAuthorizationStatus> {
        request.validate()?;
        let connection = self.access.require(id, actor, "authorize", &mut NoTransaction).await?;
        self.access.binding(&connection)?;
        let engine = self.engine.as_ref().map_err(|error| *error)?;
        engine.authorize(id, request, actor).await
    }
}
