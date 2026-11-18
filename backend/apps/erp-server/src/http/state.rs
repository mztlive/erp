//! HTTP 请求状态及请求衍生任务的生命周期。
use std::sync::Arc;

use config::SafeConfig;
use erp_connector_jd_vop::authorization_http::ReqwestAuthorizationHttp;
use erp_supplier_orders::supplier_credentials::authorization::{
    JdVopAuthorizationProcess, JdVopAuthorizationTasks,
};
use erp_supply::ports::jd_vop::authorization::{AuthorizationError, AuthorizationHttp};
use mongodb::Database;
use storage::S3Storage;
use tokio::sync::RwLock;

use super::auth::{JwtEngine, JwtError};
use super::notification_feed::NotificationFeed;
use super::tasks::RequestTasks;
use crate::wiring::Application;

/// HTTP 专属缓存与任务边界；业务能力由共享装配提供。
#[derive(Clone)]
pub struct HttpState {
    application: Application,
    jwt_engine: Arc<RwLock<Option<(String, JwtEngine)>>>,
    jd_vop_authorization: Arc<JdVopAuthorizationTasks>,
    tasks: RequestTasks,
    pub(crate) notification_feed: NotificationFeed,
}
impl HttpState {
    /// 为 HTTP 装配独立的缓存和任务管理。
    /// HTTP 状态。
    pub fn from_application(application: Application) -> Self {
        Self {
            application,
            jwt_engine: Arc::new(RwLock::new(None)),
            jd_vop_authorization: Arc::new(JdVopAuthorizationTasks::default()),
            tasks: RequestTasks::default(),
            notification_feed: NotificationFeed::default(),
        }
    }
    /// 构造使用失败关闭连接器的 HTTP 状态。
    pub fn new(db: Database, config: SafeConfig, storage: S3Storage) -> Self {
        Self::from_application(Application::new(db, config, storage))
    }
    /// 访问进程共享业务装配。
    /// 共享装配引用。
    pub fn application(&self) -> &Application {
        &self.application
    }
    // 访问请求衍生任务管理器。
    // 同一 HTTP 进程的任务管理器。
    pub(crate) fn tasks(&self) -> &RequestTasks {
        &self.tasks
    }
    /// 装配指定京东连接的页面授权入口，复用应用共享任务生命周期。
    /// 无；账号配置只从SafeConfig当前快照读取。
    /// 不发送HTTP的授权用例，未配置时状态保持未就绪。
    /// 不返回错误，敏感配置错误由用例转换为脱敏状态。
    pub fn jd_vop_authorization_process(&self) -> JdVopAuthorizationProcess {
        let config = self.application().config_snapshot();
        JdVopAuthorizationProcess::new(
            self.application().db(),
            config.jd_vop.as_ref(),
            self.application().authorization(),
            self.jd_vop_authorization.clone(),
            config
                .jd_vop
                .as_ref()
                .ok_or(AuthorizationError::Unavailable)
                .map(ReqwestAuthorizationHttp::new)
                .map(|http| Arc::new(http) as Arc<dyn AuthorizationHttp>),
        )
    }

    /// 在 Web API 停止受理并排空请求后，取出共享京东授权任务管理器，等待已启动的原授权保存结束。
    pub fn jd_vop_authorization_tasks(&self) -> Arc<JdVopAuthorizationTasks> {
        self.jd_vop_authorization.clone()
    }

    /// 用当前配置快照的密钥获取 JWT 引擎，供签发和验签复用；密钥变化时重建，初始化失败不沿用旧密钥。
    pub async fn jwt_engine(&self) -> std::result::Result<JwtEngine, JwtError> {
        let mut engine_guard = self.jwt_engine.write().await;

        engine_for_secret(&mut engine_guard, self.application().config_snapshot().app.secret)
    }
}

// 签发或验签时只复用与当前密钥相同的缓存引擎。持锁构造新引擎，成功后原子替换；构造错误返回给调用方。
fn engine_for_secret(cache: &mut Option<(String, JwtEngine)>, secret: String) -> Result<JwtEngine, JwtError> {
    if let Some((active, engine)) = cache.as_ref()
        && active == &secret
    {
        return Ok(engine.clone());
    }
    let engine = JwtEngine::new(secret.clone())?;
    *cache = Some((secret, engine.clone()));
    Ok(engine)
}

#[cfg(test)]
mod tests {
    use erp_core::AccountKind;

    use super::*;
    use crate::http::auth::jwt::TokenPayload;

    #[test]
    fn jwt_cache_reuses_current_secret_and_replaces_rotated_secret() {
        let mut cache = None;
        let old = "old-secret-at-least-thirty-two-bytes".to_string();
        let new = "new-secret-at-least-thirty-two-bytes".to_string();
        let first = engine_for_secret(&mut cache, old.clone()).unwrap();
        let payload = TokenPayload::backoffice("id".into(), "account".into(), AccountKind::Admin, 1);
        let token = first.create_token(payload).unwrap();
        assert!(engine_for_secret(&mut cache, old).unwrap().verify_token(&token).is_ok());
        assert!(engine_for_secret(&mut cache, new.clone()).unwrap().verify_token(&token).is_err());
        assert_eq!(cache.as_ref().unwrap().0, new);
        assert!(engine_for_secret(&mut cache, "short".into()).is_err());
        assert_eq!(cache.as_ref().unwrap().0, new);
    }
}
