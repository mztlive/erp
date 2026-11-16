//! 页面授权命名用例：先持久互斥，再在事务外单次调用官方端点，最后CAS保存密文及审计。
mod access;
mod dto;
mod engine;
mod policy;
pub use policy::connection_facts_port;
mod read;
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
use erp_identity::AuthorizationService;
use erp_supply::ports::jd_vop::authorization::AuthorizationHttp;
pub use erp_supply::ports::jd_vop::authorization::{AuthorizationError, Result};
use mongodb::Database;
use persistence_core::Transactional;
pub use tasks::JdVopAuthorizationTasks;
pub use tokens::JdVopTokenConfiguration;

#[cfg(test)]
mod tests;

/// 供应商指定连接的页面授权入口；任务生命周期由应用持有的共享边界负责。
pub struct JdVopAuthorizationProcess {
    access: ConnectionAccess,
    engine: Result<AuthorizationEngine>,
}

impl JdVopAuthorizationProcess {
    /// 装配指定部署账号的页面授权用例，不读取数据库或发送HTTP。
    /// # 参数
    /// 应用数据库、可选京东配置、共享授权服务、应用共享任务边界与可用授权通信实现。
    /// # 返回
    /// 状态查询和单次授权入口；未配置时仍可返回脱敏未就绪状态。
    /// # 错误
    /// 不返回错误；配置错误保留为脱敏未就绪状态，不能发送授权。
    pub fn new(
        db: Database,
        settings: Option<&JdVopConfig>,
        authorization: Arc<AuthorizationService>,
        tasks: Arc<JdVopAuthorizationTasks>,
        http: Result<Arc<dyn AuthorizationHttp>>,
    ) -> Self {
        let access =
            ConnectionAccess { db, authorization, settings: settings.cloned(), protocol: http.clone() };
        let engine = settings
            .ok_or(AuthorizationError::Unavailable)
            .and_then(|settings| AuthorizationEngine::new(access.clone(), settings, tasks, http?));
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
        let access = self.access.clone();
        let engine = self.engine.clone();
        let (id, actor, operation_id) = (id.to_owned(), actor.clone(), operation_id.map(str::to_owned));
        self.access
            .db
            .client()
            .with_transaction(move |executor| {
                Box::pin(async move {
                    Ok::<_, read::TransactionError>(
                        read::status(
                            &access,
                            engine.as_ref().map_err(|error| *error),
                            &id,
                            &actor,
                            operation_id.as_deref(),
                            executor,
                        )
                        .await?,
                    )
                })
            })
            .await
            .map_err(|error| error.0)
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
        let access = self.access.clone();
        let engine = self.engine.clone();
        let target_id = id.to_owned();
        let current_actor = actor.clone();
        let (request, command, replayed) = self
            .access
            .db
            .client()
            .with_transaction(move |executor| {
                Box::pin(async move {
                    let (command, replayed) = read::authorization_command(
                        &access,
                        engine.as_ref().map_err(|error| *error),
                        &target_id,
                        &current_actor,
                        &request,
                        executor,
                    )
                    .await?;
                    Ok::<_, read::TransactionError>((request, command, replayed))
                })
            })
            .await
            .map_err(|error| error.0)?;
        if let Some(status) = replayed {
            return Ok(status);
        }
        self.engine.as_ref().map_err(|error| *error)?.authorize_prepared(id, request, actor, command).await
    }
}
