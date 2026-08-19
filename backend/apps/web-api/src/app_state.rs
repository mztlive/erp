use config::{Config, SafeConfig};
use mongodb::Database;
use services::approval::definition::ApprovalDefinitionService;
use services::approval::execution::ApprovalRuntimeService;
use services::iam::SharedRbacService;
use services::party::SensitiveDataCodec;
use services::ApprovalNotificationOutboxPort;
use std::sync::Arc;
use std::time::Duration;
use storage::S3Storage;
use tokio::sync::{watch, RwLock};
use tokio::task::JoinHandle;
use tracing::{error, info};

/// worker 轮询间隔。
const OUTBOX_POLL_INTERVAL: Duration = Duration::from_secs(1);

/// 审批通知 outbox worker 句柄。停止时不再领取新租约。
pub struct ApprovalOutboxWorker {
    /// 置位后 worker 停止领取新租约。
    stop_tx: watch::Sender<bool>,
    /// 后台轮询任务。
    join: JoinHandle<()>,
}

impl ApprovalOutboxWorker {
    /// 停止领取新租约并等待当前批次结束。
    ///
    /// 进程被强制终止时同样不再领取；未完成租约会到期后由其他实例接管。
    pub async fn stop(self) {
        if self.stop_tx.send(true).is_err() {
            info!("审批通知 outbox worker 已退出");
        }
        if let Err(error) = self.join.await {
            error!(error = %error, "等待审批通知 outbox worker 结束失败");
        }
    }
}

#[derive(Clone)]
pub struct AppState {
    db: Database,
    config: SafeConfig,
    jwt_engine: Arc<RwLock<Option<crate::core::auth::JwtEngine>>>,
    rbac: SharedRbacService,
    storage: Arc<S3Storage>,
    sensitive_data: Arc<SensitiveDataCodec>,
    approval_definition: Arc<ApprovalDefinitionService>,
    approval_runtime_service: Arc<ApprovalRuntimeService>,
    approval_outbox: Arc<ApprovalNotificationOutboxPort>,
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
        let sensitive_data = Arc::new(SensitiveDataCodec::from_secret(
            config.snapshot().app.secret.as_bytes(),
        ));
        let rbac = services::iam::shared_rbac_service(db.clone());
        let approval_definition = Arc::new(ApprovalDefinitionService::new(db.clone(), Arc::clone(&rbac)));
        let approval_runtime_service = Arc::new(ApprovalRuntimeService::new(db.clone(), Arc::clone(&rbac)));
        let approval_outbox = Arc::new(ApprovalNotificationOutboxPort::new(db.clone()));
        Self {
            db,
            config,
            jwt_engine: Arc::new(RwLock::new(None)),
            rbac,
            storage: Arc::new(storage),
            sensitive_data,
            approval_definition,
            approval_runtime_service,
            approval_outbox,
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

    /// 返回进程内注入的目标审批运行服务。
    ///
    /// 本波次只交付注入点；Handler 改走本访问器归 P3-HTTP owns，不得在此越权改 Handler。
    ///
    /// # 返回
    /// 返回启动时构造的真实 [`ApprovalRuntimeService`]；未 cut-over 类型必须失败关闭。
    pub fn approval_runtime_service(&self) -> Arc<ApprovalRuntimeService> {
        Arc::clone(&self.approval_runtime_service)
    }

    /// 返回进程内注入的通知 outbox 应用端口。
    ///
    /// # 返回
    /// 返回 services 层端口；HTTP 不得直连审批仓储。
    pub fn approval_outbox_port(&self) -> Arc<ApprovalNotificationOutboxPort> {
        Arc::clone(&self.approval_outbox)
    }

    /// 启动审批通知 outbox worker。
    ///
    /// 领取租约后在事务外调用失败关闭发送口；进程停止时不再领取新租约。
    ///
    /// # 返回
    /// 返回可用于显式停止的 worker 句柄。
    pub fn start_approval_outbox_worker(&self) -> ApprovalOutboxWorker {
        let (stop_tx, stop_rx) = watch::channel(false);
        let port = self.approval_outbox_port();
        let worker_id = format!("web-api-{}", id_generator::next_id());
        let join = tokio::spawn(run_approval_outbox_worker(port, worker_id, stop_rx));
        ApprovalOutboxWorker { stop_tx, join }
    }

    /// 返回启动时固定的 S3 存储客户端。
    ///
    /// # 返回
    /// 返回所有上传 handler 共享的单例客户端；S3 配置变更需重启后生效。
    pub fn storage(&self) -> &S3Storage {
        self.storage.as_ref()
    }

    /// 返回启动时固定的敏感数据编解码器。
    ///
    /// # 返回
    /// 返回敏感资料 Service 共享的进程内单例；启动密钥变化后必须先迁移既有密文。
    pub fn sensitive_data(&self) -> Arc<SensitiveDataCodec> {
        Arc::clone(&self.sensitive_data)
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

/// 运行 outbox worker，直到收到停止信号。
///
/// # 参数
/// * `port` - services 层 outbox 端口
/// * `worker_id` - 本进程租约持有者
/// * `stop_rx` - 停止信号
async fn run_approval_outbox_worker(
    port: Arc<ApprovalNotificationOutboxPort>,
    worker_id: String,
    mut stop_rx: watch::Receiver<bool>,
) {
    info!(worker_id = %worker_id, "审批通知 outbox worker 已启动");
    loop {
        if *stop_rx.borrow() {
            info!(worker_id = %worker_id, "审批通知 outbox worker 停止领取新租约");
            return;
        }
        if let Err(error) = port.process_tick(&worker_id).await {
            error!(worker_id = %worker_id, error = %error, "审批通知 outbox 批次处理失败");
        }
        tokio::select! {
            changed = stop_rx.changed() => {
                if changed.is_err() || *stop_rx.borrow() {
                    info!(worker_id = %worker_id, "审批通知 outbox worker 已停止");
                    return;
                }
            }
            () = tokio::time::sleep(OUTBOX_POLL_INTERVAL) => {}
        }
    }
}
