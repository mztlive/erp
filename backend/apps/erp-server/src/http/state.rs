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
use super::tasks::RequestTasks;
use crate::wiring::{Application, supplier_access};

/// HTTP 专属缓存与任务边界；业务能力由共享装配提供。
#[derive(Clone)]
pub struct HttpState {
    application: Application,
    jwt_engine: Arc<RwLock<Option<(String, JwtEngine)>>>,
    jd_vop_authorization: Arc<JdVopAuthorizationTasks>,
    tasks: RequestTasks,
}
impl HttpState {
    /// 为 HTTP 装配独立的缓存和任务管理。
    /// # 参数
    /// application 为已完成初始化的共享装配。
    /// # 返回
    /// HTTP 状态。
    /// # 错误
    /// 不执行 I/O，不返回错误。
    pub fn from_application(application: Application) -> Self {
        Self {
            application,
            jwt_engine: Arc::new(RwLock::new(None)),
            jd_vop_authorization: Arc::new(JdVopAuthorizationTasks::default()),
            tasks: RequestTasks::default(),
        }
    }
    /// 构造使用失败关闭连接器的 HTTP 状态。
    /// # 参数
    /// db、config、storage 为已经建立的基础资源。
    /// # 返回
    /// HTTP 状态。
    /// # 错误
    /// 不执行 I/O，不返回错误。
    pub fn new(db: Database, config: SafeConfig, storage: S3Storage) -> Self {
        Self::from_application(Application::new(db, config, storage))
    }
    /// 访问进程共享业务装配。
    /// # 参数
    /// 无。
    /// # 返回
    /// 共享装配引用。
    /// # 错误
    /// 无。
    pub fn application(&self) -> &Application {
        &self.application
    }
    /// 访问请求衍生任务管理器。
    /// # 参数
    /// 无。
    /// # 返回
    /// 同一 HTTP 进程的任务管理器。
    /// # 错误
    /// 无。
    pub(crate) fn tasks(&self) -> &RequestTasks {
        &self.tasks
    }
    /// 装配指定京东连接的页面授权入口，复用应用共享任务生命周期。
    /// # 参数
    /// 无；账号配置只从SafeConfig当前快照读取。
    /// # 返回
    /// 不发送HTTP的授权用例，未配置时状态保持未就绪。
    /// # 错误
    /// 不返回错误，敏感配置错误由用例转换为脱敏状态。
    pub fn jd_vop_authorization_process(&self) -> JdVopAuthorizationProcess {
        let config = self.application().config_snapshot();
        JdVopAuthorizationProcess::new(
            self.application().db(),
            config.jd_vop.as_ref(),
            self.application().rbac(),
            supplier_access(self.application().db(), self.application().rbac()),
            self.jd_vop_authorization.clone(),
            config
                .jd_vop
                .as_ref()
                .ok_or(AuthorizationError::Unavailable)
                .map(ReqwestAuthorizationHttp::new)
                .map(|http| Arc::new(http) as Arc<dyn AuthorizationHttp>),
        )
    }

    /// 返回应用共享授权任务边界，供停止HTTP受理后等待已开始授权保存完成。
    /// # 参数
    /// 无。
    /// # 返回
    /// 同一任务边界的共享引用。
    /// # 错误
    /// 不执行I/O，不返回错误。
    pub fn jd_vop_authorization_tasks(&self) -> Arc<JdVopAuthorizationTasks> {
        self.jd_vop_authorization.clone()
    }

    /// 获取 JWT 引擎实例。
    ///
    /// # 返回
    /// 返回执行结果，`Ok` 表示成功，`Err` 表示失败。
    ///
    /// # 错误
    /// 当验证失败或底层操作失败时返回错误。
    pub async fn jwt_engine(&self) -> std::result::Result<JwtEngine, JwtError> {
        let mut engine_guard = self.jwt_engine.write().await;

        engine_for_secret(&mut engine_guard, self.application().config_snapshot().app.secret)
    }
}

/// 缓存只复用相同密钥的引擎；配置变化后在持锁期间原子替换。
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
