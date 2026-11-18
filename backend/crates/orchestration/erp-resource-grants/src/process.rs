use std::sync::Arc;

use erp_identity::entity::resource_access::event::{GrantEvent, GrantEventStatus};
use erp_identity::service::resource_access::{
    GrantEventService, GrantQueueHealth, GrantRecompute, ResourceGrantService,
};
use erp_identity::{AuthorizationService, Error, Result};
use mongodb::Database;
use persistence_core::{Executor, Transactional};

use super::facts::Facts;

#[derive(Clone)]
pub struct ResourceGrantProcess {
    pub(crate) db: Database,
    authorization: Arc<AuthorizationService>,
    pub(crate) queue: GrantEventService,
}

impl ResourceGrantProcess {
    /// 用组合根提供的数据库和共享授权服务组装自动授权处理器，供 worker 消费和对账；构造时不启动后台循环。
    pub fn new(db: Database, authorization: Arc<AuthorizationService>) -> Self {
        Self { queue: GrantEventService::new(db.clone()), db, authorization }
    }

    // 给身份域的来源管理服务注入业务事实端口；有自动事件时绑定原事件用于发布前重验，避免授权依据已过期。
    pub(crate) fn access(&self, event: Option<GrantEvent>) -> ResourceGrantService {
        ResourceGrantService::new(
            self.db.clone(),
            self.authorization.clone(),
            Arc::new(Facts { db: self.db.clone(), event }),
        )
    }

    /// 用本次唯一领取令牌处理至多一条授权事件，返回是否领到事件。授权读取或发布失败时返回错误，不回放产生事件的业务事务。
    /// 授权提交结果未知时保留租约，后续恢复先检查事件是否已处理，再决定是否发布。
    pub async fn tick(&self, token: &str, now: u64) -> Result<bool> {
        let Some(event) = self.queue.claim_event(token, now).await? else {
            return Ok(false);
        };

        let this = self.clone();
        let claim = event.clone();
        let result = self
            .db
            .client()
            .clone()
            .with_transaction(move |ex| Box::pin(async move { this.publish(claim, ex).await }))
            .await;

        if let Err(error) = result {
            if !matches!(error, Error::OutcomeUnknown(_)) {
                self.queue.fail_event(&event, now, &error.to_string()).await?;
            }
            return Err(error);
        }

        Ok(true)
    }

    async fn publish(&self, claim: GrantEvent, ex: &mut dyn Executor) -> Result<()> {
        let current = self.queue.claimed_event(&claim, ex).await?;
        if current.status == GrantEventStatus::Applied {
            return Ok(());
        }

        let old = self.queue.source(&current.source, ex).await?;
        if !old.as_ref().is_some_and(|s| s.terminated) {
            let update = Facts::compute(&self.db, &current, ex).await?;
            if !old.as_ref().is_some_and(|s| s.fact_version > update.fact_version) {
                self.access(Some(claim)).apply_source(update, ex).await?;
            }
        }

        self.queue.complete_event(current, ex).await
    }
}

impl ResourceGrantProcess {
    /// 按上页游标重新排队一页正式授权事件，供后台周期性重验，不替代管理员重算。返回下一页游标，`None` 表示本轮结束。
    pub async fn reconcile(&self, after: Option<&str>, now: u64) -> Result<Option<String>> {
        self.queue.requeue(&GrantRecompute::All, after, now).await
    }
    /// 按当前秒级时间查询授权队列积压和故障统计，供后台监控消费状态。
    pub async fn health(&self, now: u64) -> Result<GrantQueueHealth> {
        self.queue.grant_health(now).await
    }
}
