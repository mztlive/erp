//! 按正式业务事实装配记录读取授权，消费循环由 worker 管理。
mod backfill;
mod facts;
mod readiness;
mod rules;

use std::sync::Arc;

pub use backfill::{BackfillProblem, BackfillReport, BackfillRequest, GrantHistoryKind};
use erp_identity::service::resource_access::{
    GrantEventService, GrantQueueHealth, GrantRecompute, ResourceGrantService,
};
use erp_identity::{AuthorizationService, Error, Result};
use mongodb::Database;
use persistence_core::Transactional;
pub(in crate::wiring) use readiness::{GrantReadiness, invoice_pending, source_readiness};

use self::facts::Facts;

/// 记录授权的消费、对账和维护入口。
#[derive(Clone)]
pub struct ResourceGrantRuntime {
    db: Database,
    grants: ResourceGrantService,
    queue: GrantEventService,
}

impl ResourceGrantRuntime {
    /// 用组合根提供的数据库和共享授权服务组装自动授权处理器，供 worker 消费和对账；构造时不启动后台循环。
    pub(in crate::wiring) fn new(db: Database, authorization: Arc<AuthorizationService>) -> Self {
        let grants = ResourceGrantService::new(db.clone(), authorization, Arc::new(Facts { db: db.clone() }));
        Self { queue: GrantEventService::new(db.clone()), db, grants }
    }

    /// 用本次唯一领取令牌处理至多一条授权事件，返回是否领到事件。授权读取或发布失败时返回错误，不回放产生事件的业务事务。
    /// 授权提交结果未知时保留租约，后续恢复先检查事件是否已处理，再决定是否发布。
    pub async fn tick(&self, token: &str, now: u64) -> Result<bool> {
        let Some(event) = self.queue.claim_event(token, now).await? else {
            return Ok(false);
        };

        let grants = self.grants.clone();
        let claim = event.clone();
        let result = self
            .db
            .client()
            .clone()
            .with_transaction(move |ex| Box::pin(async move { grants.apply_source(&claim, ex).await }))
            .await;

        if let Err(error) = result {
            if !matches!(error, Error::OutcomeUnknown(_)) {
                self.queue.fail_event(&event, now, &error.to_string()).await?;
            }
            return Err(error);
        }

        Ok(true)
    }

    /// 按上页游标重新排队一页正式授权事件，供后台周期性重验，不替代管理员重算。返回下一页游标，`None` 表示本轮结束。
    pub async fn reconcile(&self, after: Option<&str>, now: u64) -> Result<Option<String>> {
        self.queue.requeue(&GrantRecompute::All, after, now).await
    }

    /// 按当前秒级时间查询授权队列积压和故障统计，供后台监控消费状态。
    pub async fn health(&self, now: u64) -> Result<GrantQueueHealth> {
        self.queue.grant_health(now).await
    }

    /// 返回已绑定业务事实的授权管理服务；各操作自行校验管理员权限。
    pub fn management(&self) -> ResourceGrantService {
        self.grants.clone()
    }
}
