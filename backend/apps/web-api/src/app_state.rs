use config::{Config, SafeConfig};
use mongodb::Database;
use services::iam::SharedRbacService;
use std::sync::Arc;
use storage::S3Storage;
use tokio::sync::{watch, RwLock};

#[derive(Clone)]
pub struct AppState {
    db: Database,
    config: SafeConfig,
    jwt_engine: Arc<RwLock<Option<crate::core::auth::JwtEngine>>>,
    rbac: SharedRbacService,
    storage: Arc<S3Storage>,
}

impl AppState {
    /// 创建 AppState 实例。
    ///
    /// # 参数
    /// * `db` - 应用启动时建立的数据库连接
    /// * `config` - 配置数据
    /// * `storage` - 启动时已构建的 S3 存储客户端
    ///
    /// # 返回
    /// 返回创建的实例。
    pub fn new(db: Database, config: SafeConfig, storage: S3Storage) -> Self {
        let rbac = services::iam::shared_rbac_service(db.clone());
        Self {
            db,
            config,
            jwt_engine: Arc::new(RwLock::new(None)),
            rbac,
            storage: Arc::new(storage),
        }
    }

    /// 订阅配置变更通知。
    ///
    /// # 返回
    /// 返回配置变更的接收器。
    pub fn subscribe_config(&self) -> watch::Receiver<Config> {
        self.config.subscribe()
    }

    /// 获取当前配置快照。
    ///
    /// # 返回
    /// 返回当前不可变配置副本。
    pub fn config_snapshot(&self) -> Config {
        self.config.snapshot()
    }

    /// 返回数据库实例。
    ///
    /// # 返回
    /// 返回数据库实例的克隆。
    pub fn db(&self) -> Database {
        self.db.clone()
    }

    /// 返回共享 Casbin RBAC 服务。
    ///
    /// # 返回
    /// 返回共享 RBAC 服务。
    pub fn rbac(&self) -> SharedRbacService {
        Arc::clone(&self.rbac)
    }

    /// 返回启动时固定的 S3 存储客户端。
    ///
    /// # 返回
    /// 返回所有上传 handler 共享的单例客户端；S3 配置变更需重启后生效。
    pub fn storage(&self) -> &S3Storage {
        self.storage.as_ref()
    }

    /// 使 JWT 引擎缓存失效。
    ///
    /// # 返回
    /// 无返回值。
    pub async fn invalidate_jwt_engine(&self) {
        let mut engine_guard = self.jwt_engine.write().await;
        *engine_guard = None;
    }

    /// 获取 JWT 引擎实例。
    ///
    /// # 返回
    /// 返回执行结果，`Ok` 表示成功，`Err` 表示失败。
    ///
    /// # 错误
    /// 当验证失败或底层操作失败时返回错误。
    pub async fn jwt_engine(
        &self,
    ) -> std::result::Result<crate::core::auth::JwtEngine, crate::core::auth::JwtError> {
        let mut engine_guard = self.jwt_engine.write().await;

        if let Some(engine) = engine_guard.as_ref() {
            return Ok(engine.clone());
        }

        let config = self.config_snapshot();
        let engine = crate::core::auth::JwtEngine::new(config.app.secret)?;
        *engine_guard = Some(engine.clone());
        Ok(engine)
    }
}
