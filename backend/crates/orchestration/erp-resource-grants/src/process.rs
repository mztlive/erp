use std::sync::Arc;

use erp_finance::repository::extensions::ReceivableExt;
use erp_identity::entity::resource_access::SourceKey;
use erp_identity::entity::resource_access::event::{GrantEvent, GrantEventStatus};
use erp_identity::service::resource_access::{
    GrantEventService, GrantQueueHealth, GrantRecompute, ResourceGrantService,
};
use erp_identity::{Error, Result, SharedRbacService};
use mongodb::Database;
use persistence_core::{Executor, Transactional};

use super::facts::Facts;

#[derive(Clone)]
pub struct ResourceGrantProcess {
    pub(crate) db: Database,
    pub(crate) rbac: SharedRbacService,
}

impl ResourceGrantProcess {
    /// 查询开票审批及后续转派的独立授权来源是否仍在生成。
    /// # 参数
    /// request_id 为申请 ID，recipient 为读取账号，executor 为当前读取事务。
    /// # 返回
    /// 任一已核实来源尚未生成时返回 true；永久终止或无证据不会提示待处理。
    /// # 错误
    /// 事实关联或读取失败时拒绝。
    pub async fn invoice_pending(
        &self,
        request_id: &str,
        recipient: &str,
        executor: &mut dyn Executor,
    ) -> Result<bool> {
        let initial = SourceKey {
            rule: "invoice_approved".into(),
            business_type: "sales_invoice_request".into(),
            business_id: request_id.into(),
            recipient: recipient.into(),
        };
        if self.source_readiness(&initial, executor).await? == GrantReadiness::NotReady {
            return Ok(true);
        }
        let Some(request) = self.db.sales_invoice_requests().find_by_id(request_id, executor).await? else {
            return Ok(false);
        };
        let Some(task_id) = request.work_item_id else {
            return Ok(false);
        };
        let reassigned = SourceKey {
            rule: "invoice_task_reassigned".into(),
            business_type: "work_item".into(),
            business_id: task_id,
            recipient: recipient.into(),
        };
        Ok(self.source_readiness(&reassigned, executor).await? == GrantReadiness::NotReady)
    }

    /// 装配独立授权处理器。
    /// # 参数
    /// db、rbac 为组合根共享设施。
    /// # 返回
    /// 不启动循环的处理器。
    /// # 错误
    /// 无。
    pub fn new(db: Database, rbac: SharedRbacService) -> Self {
        Self { db, rbac }
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
            self.rbac.clone(),
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
        let access = self.access(Some(claim.clone()));
        let current = self.queue().claimed_event(&claim, ex).await?;
        if current.status == GrantEventStatus::Applied {
            return Ok(());
        }

        let old = self.queue().source(&current.source, ex).await?;
        if old.as_ref().is_some_and(|s| s.terminated) {
            return self.queue().complete_event(current, ex).await;
        }

        let update = Facts { db: self.db.clone(), event: None }.compute(&current, ex).await?;
        if !old.as_ref().is_some_and(|s| s.fact_version > update.fact_version) {
            access.apply_source(update, ex).await?;
        }

        self.queue().complete_event(current, ex).await
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
        self.recompute(&GrantRecompute::All, after, now).await
    }
    /// 按来源、资源或规则分批重新排队。
    /// # 参数
    /// selection 为维护范围，after 为游标，now 为秒级时间。
    /// # 返回
    /// 下一页游标。
    /// # 错误
    /// 数据库失败返回错误。
    pub async fn recompute(
        &self,
        selection: &GrantRecompute,
        after: Option<&str>,
        now: u64,
    ) -> Result<Option<String>> {
        self.queue().requeue(selection, after, now).await
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

/// 仅表示读取授权生成状态，不授予功能权限或办理资格。
#[derive(Debug, PartialEq, Eq)]
pub enum GrantReadiness {
    Absent,
    NotReady,
    Ready,
    Terminated,
}
impl ResourceGrantProcess {
    /// 核对合法来源后返回读取授权是否就绪。
    /// # 参数
    /// expected 来自服务端正式事实，executor 为请求执行器。
    /// # 返回
    /// 待处理合法来源返回 NotReady，供业务接口映射为可刷新状态。
    /// # 错误
    /// 关联错误、事实读取失败或同键冲突时返回错误。
    pub async fn readiness(
        &self,
        expected: &GrantEvent,
        executor: &mut dyn Executor,
    ) -> Result<GrantReadiness> {
        let Some(event) = self.queue().event(expected, executor).await? else {
            return Ok(GrantReadiness::Absent);
        };
        let update = Facts { db: self.db.clone(), event: None }.compute(&event, executor).await?;
        let source = self.queue().source(&event.source, executor).await?;
        Ok(match source {
            Some(s) if s.terminated => GrantReadiness::Terminated,
            Some(s)
                if s.fact_version == update.fact_version
                    && s.rule_version == update.rule_version
                    && s.targets == update.targets =>
            {
                GrantReadiness::Ready
            },
            _ => GrantReadiness::NotReady,
        })
    }
}

impl ResourceGrantProcess {
    /// 对业务确定的来源读取正式事件，并以当前可信事实核验授权就绪。
    /// # 参数
    /// source 为服务端来源身份；executor 为当前请求事务。
    /// # 返回
    /// 没有正式事件返回 Absent；其他状态按当前来源、事实和授权结果判定。
    /// # 错误
    /// 关联损坏、账号失效或读取失败不作为待处理返回。
    pub async fn source_readiness(
        &self,
        source: &SourceKey,
        executor: &mut dyn Executor,
    ) -> Result<GrantReadiness> {
        let Some(event) = self.queue().source_event(source, executor).await? else {
            return Ok(GrantReadiness::Absent);
        };
        self.readiness(&event, executor).await
    }
}
