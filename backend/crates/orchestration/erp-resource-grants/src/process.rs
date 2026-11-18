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
    pub(crate) authorization: Arc<AuthorizationService>,
}

impl ResourceGrantProcess {
    /// 装配独立授权处理器。
    /// # 参数
    /// db、authorization 为组合根的数据库及共享授权服务。
    /// # 返回
    /// 不启动循环的处理器。
    /// # 错误
    /// 无。
    pub fn new(db: Database, authorization: Arc<AuthorizationService>) -> Self {
        Self { db, authorization }
    }

    /// 装配独立队列能力。
    /// # 参数
    /// 无。
    /// # 返回
    /// 绑定当前数据库的事件服务。
    /// # 错误
    /// 不执行 I/O。
    pub(crate) fn queue(&self) -> GrantEventService {
        GrantEventService::new(self.db.clone())
    }

    /// 装配可信来源发布能力。
    /// # 参数
    /// event 为自动发布时需要重验的事件。
    /// # 返回
    /// 绑定可信事实端口的来源管理服务。
    /// # 错误
    /// 不执行 I/O。
    pub(crate) fn access(&self, event: Option<GrantEvent>) -> ResourceGrantService {
        ResourceGrantService::new(
            self.db.clone(),
            self.authorization.clone(),
            Arc::new(Facts { db: self.db.clone(), event }),
        )
    }

    /// 领取并处理一个事件；未知提交结果保持租约，恢复时先检查事件结果。
    /// # 参数
    /// token 为每次领取唯一令牌；now 为当前秒级时间。
    /// # 返回
    /// 是否领取到事件。
    /// # 错误
    /// 读取或发布失败返回错误，业务事务不会重放。
    pub async fn tick(&self, token: &str, now: u64) -> Result<bool> {
        let queue = self.queue();
        let Some(event) = queue.claim_event(token, now).await? else {
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
                queue.fail_event(&event, now, &error.to_string()).await?;
            }
            return Err(error);
        }

        Ok(true)
    }

    async fn publish(&self, claim: GrantEvent, ex: &mut dyn Executor) -> Result<()> {
        let queue = self.queue();
        let current = queue.claimed_event(&claim, ex).await?;
        if current.status == GrantEventStatus::Applied {
            return Ok(());
        }

        let old = queue.source(&current.source, ex).await?;
        if !old.as_ref().is_some_and(|s| s.terminated) {
            let update = Facts { db: self.db.clone(), event: None }.compute(&current, ex).await?;
            if !old.as_ref().is_some_and(|s| s.fact_version > update.fact_version) {
                self.access(Some(claim)).apply_source(update, ex).await?;
            }
        }

        queue.complete_event(current, ex).await
    }
}

impl ResourceGrantProcess {
    /// 周期有界重验所有正式事件；游标由 worker 保留。
    /// # 参数
    /// after 为上次页末主键，now 为秒级时间。
    /// # 返回
    /// 下一页游标；空值表示本轮结束。
    /// # 错误
    /// 持久化失败返回错误。
    pub async fn reconcile(&self, after: Option<&str>, now: u64) -> Result<Option<String>> {
        self.queue().requeue(&GrantRecompute::All, after, now).await
    }
    /// 查询授权积压与故障统计。
    /// # 参数
    /// now 为当前秒级时间。
    /// # 返回
    /// 队列状态。
    /// # 错误
    /// 查询失败时返回错误。
    pub async fn health(&self, now: u64) -> Result<GrantQueueHealth> {
        self.queue().grant_health(now).await
    }
}
