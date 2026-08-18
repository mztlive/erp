use config::{Config, SafeConfig};
use database::{ApprovalIntegrationExt, NoTransaction};
use entities::approval_integration::ApprovalNotificationDeliveryStatus;
use entities::common::time::Instant;
use mongodb::Database;
use services::approval::definition::ApprovalDefinitionService;
use services::approval::execution::notification_worker::{
    apply_delivery_attempt, lease_until, DeliveryAttempt, NotificationSender,
};
use services::approval::execution::ApprovalRuntimeService;
use services::approval::{ApprovalDomainActionPort, ApprovalRuntimePort, InternalApprovalRuntime};
use services::iam::SharedRbacService;
use services::party::SensitiveDataCodec;
use std::sync::Arc;
use std::time::Duration;
use storage::S3Storage;
use tokio::sync::{watch, RwLock};
use tokio::task::JoinHandle;
use tracing::{error, info};

/// 未接入通知提供方时失败关闭，不得伪造成功投递。
#[derive(Clone, Copy, Debug, Default)]
struct FailClosedApprovalNotificationSender;

/// 单次 outbox 领取上限。
const OUTBOX_BATCH_LIMIT: u32 = 20;

/// worker 轮询间隔。
const OUTBOX_POLL_INTERVAL: Duration = Duration::from_secs(1);

impl NotificationSender for FailClosedApprovalNotificationSender {
    /// 以去重键调用发送口；提供方未接线时返回致命失败。
    ///
    /// # 参数
    /// * `dedup_key` - outbox 业务去重键
    ///
    /// # 返回
    /// 始终返回 `Fatal`，由 worker 按退避或死信落库。
    fn send_idempotent(&self, dedup_key: &str) -> DeliveryAttempt {
        error!(dedup_key = %dedup_key, "审批通知提供方未接入，投递失败关闭");
        DeliveryAttempt::Fatal
    }
}

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
    approval_notification_sender: FailClosedApprovalNotificationSender,
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
        Self {
            db,
            config,
            jwt_engine: Arc::new(RwLock::new(None)),
            rbac,
            storage: Arc::new(storage),
            sensitive_data,
            approval_definition,
            approval_runtime_service,
            approval_notification_sender: FailClosedApprovalNotificationSender,
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

    /// 返回进程内注入的审批定义管理服务。
    ///
    /// # 返回
    /// 返回启动时构造的真实 [`ApprovalDefinitionService`]，不得以 Noop 或 `Option` 绕过。
    pub fn approval_definition_service(&self) -> Arc<ApprovalDefinitionService> {
        Arc::clone(&self.approval_definition)
    }

    /// 返回进程内注入的目标审批运行服务。
    ///
    /// # 返回
    /// 返回启动时构造的真实 [`ApprovalRuntimeService`]；未 cut-over 类型必须失败关闭。
    pub fn approval_runtime_service(&self) -> Arc<ApprovalRuntimeService> {
        Arc::clone(&self.approval_runtime_service)
    }

    /// 返回旧卡券销售路径仍在使用的 INTERNAL 运行时端口。
    ///
    /// 目标单据类型必须走 [`Self::approval_runtime_service`]，不得回退本方法。
    /// 本入口由 P0-D 在全类型切换后删除。
    ///
    /// # 参数
    /// * `action_port` - 旧运行时领域动作端口
    ///
    /// # 返回
    /// 返回仅供未删除旧调用方使用的 INTERNAL 实现。
    pub fn approval_runtime(
        &self,
        action_port: Arc<dyn ApprovalDomainActionPort>,
    ) -> Arc<dyn ApprovalRuntimePort> {
        Arc::new(InternalApprovalRuntime::new(self.db(), action_port))
    }

    /// 启动审批通知 outbox worker。
    ///
    /// 领取租约后在事务外调用失败关闭发送口；进程停止时不再领取新租约。
    ///
    /// # 返回
    /// 返回可用于显式停止的 worker 句柄。
    pub fn start_approval_outbox_worker(&self) -> ApprovalOutboxWorker {
        let (stop_tx, stop_rx) = watch::channel(false);
        let db = self.db();
        let sender = self.approval_notification_sender;
        let worker_id = format!("web-api-{}", id_generator::next_id());
        let join = tokio::spawn(run_approval_outbox_worker(db, sender, worker_id, stop_rx));
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
/// * `db` - MongoDB
/// * `sender` - 失败关闭发送口
/// * `worker_id` - 本进程租约持有者
/// * `stop_rx` - 停止信号
async fn run_approval_outbox_worker(
    db: Database,
    sender: FailClosedApprovalNotificationSender,
    worker_id: String,
    mut stop_rx: watch::Receiver<bool>,
) {
    info!(worker_id = %worker_id, "审批通知 outbox worker 已启动");
    loop {
        if *stop_rx.borrow() {
            info!(worker_id = %worker_id, "审批通知 outbox worker 停止领取新租约");
            return;
        }
        if let Err(error) = process_outbox_tick(&db, sender, &worker_id).await {
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

/// 领取并处理一批 outbox。
///
/// # 参数
/// * `db` - MongoDB
/// * `sender` - 失败关闭发送口
/// * `worker_id` - 租约持有者
///
/// # 错误
/// 租约或 CAS 落库失败时返回服务错误。
async fn process_outbox_tick(
    db: &Database,
    sender: FailClosedApprovalNotificationSender,
    worker_id: &str,
) -> std::result::Result<(), services::Error> {
    let now = Instant::now();
    let until = lease_until(now);
    let leased = db
        .approval_notification_outbox()
        .lease_outbox_batch(worker_id, now, until, OUTBOX_BATCH_LIMIT, &mut NoTransaction)
        .await?;
    for item in leased {
        persist_delivery_attempt(db, sender, worker_id, item, now).await?;
    }
    Ok(())
}

/// 对已租约消息应用一次发送结果并 CAS 写回。
///
/// # 参数
/// * `db` - MongoDB
/// * `sender` - 失败关闭发送口
/// * `worker_id` - 租约持有者
/// * `item` - 已领取消息
/// * `now` - 当前时间
///
/// # 错误
/// 实体状态或仓储 CAS 失败时返回错误。
async fn persist_delivery_attempt(
    db: &Database,
    sender: FailClosedApprovalNotificationSender,
    worker_id: &str,
    mut item: entities::approval_integration::ApprovalNotificationOutbox,
    now: Instant,
) -> std::result::Result<(), services::Error> {
    let attempt = sender.send_idempotent(&item.dedup_key);
    if attempt == DeliveryAttempt::Delivered {
        db.approval_notification_outbox()
            .mark_outbox_delivered(&item.base.id, worker_id, now, &mut NoTransaction)
            .await?;
        return Ok(());
    }
    apply_delivery_attempt(&mut item, attempt, now)?;
    let error_class = outbox_error_class(attempt);
    if item.delivery_status == ApprovalNotificationDeliveryStatus::DeadLetter {
        db.approval_notification_outbox()
            .dead_letter_outbox(&item.base.id, worker_id, error_class, &mut NoTransaction)
            .await?;
        return Ok(());
    }
    db.approval_notification_outbox()
        .reschedule_outbox(
            &item.base.id,
            worker_id,
            item.next_attempt_at,
            error_class,
            &mut NoTransaction,
        )
        .await?;
    Ok(())
}

/// 返回可落库的失败分类。
///
/// # 参数
/// * `attempt` - 发送结果
///
/// # 返回
/// 返回不含敏感载荷的分类字面量。
fn outbox_error_class(attempt: DeliveryAttempt) -> &'static str {
    match attempt {
        DeliveryAttempt::Delivered => "delivered",
        DeliveryAttempt::Retryable => "retryable",
        DeliveryAttempt::Fatal => "fatal",
    }
}

#[cfg(test)]
mod tests {
    use super::{FailClosedApprovalNotificationSender, NotificationSender};
    use services::approval::execution::notification_worker::DeliveryAttempt;

    #[test]
    fn unconfigured_notification_sender_fails_closed() {
        let sender = FailClosedApprovalNotificationSender;
        assert_eq!(sender.send_idempotent("started:inst-1"), DeliveryAttempt::Fatal);
    }
}
