//! 授权队列公开用例；持久化逻辑由身份仓储拥有。
use persistence_core::Executor;

use super::ResourceAccessService;
use crate::Result;
use crate::entity::resource_access::event::GrantEvent;
use crate::entity::resource_access::{GrantSource, SourceKey};
use crate::repository::resource_grants::queue::GrantQueue;
pub use crate::repository::resource_grants::queue::{GrantQueueHealth, GrantRecompute, GrantRule};
impl ResourceAccessService {
    /// 原子领取到期事件或回收过期租约。
    /// # 参数
    /// owner 为每次领取唯一令牌，now 为秒级时间。
    /// # 返回
    /// 最多一个事件。
    /// # 错误
    /// 数据库失败或令牌为空时拒绝。
    pub async fn claim_event(&self, owner: &str, now: u64) -> Result<Option<GrantEvent>> {
        GrantQueue { db: self.db.clone() }.claim_event(owner, now).await
    }
    /// 在发布事务内读取并核验当前领取令牌。
    /// # 参数
    /// claim 为已领取事件，executor 为发布事务。
    /// # 返回
    /// 最新事件；已经提交的事件允许幂等返回。
    /// # 错误
    /// 租约被其他 worker 接管或事件缺失时拒绝。
    pub async fn claimed_event(&self, claim: &GrantEvent, executor: &mut dyn Executor) -> Result<GrantEvent> {
        GrantQueue { db: self.db.clone() }.claimed_event(claim, executor).await
    }
    /// 同授权结果保存处理完成状态；重放不写入。
    /// # 参数
    /// event 为事务内读取的事件，executor 为同一发布事务。
    /// # 返回
    /// 成功无返回值。
    /// # 错误
    /// 无事务、并发版本冲突或持久化失败时拒绝。
    pub async fn complete_event(&self, event: GrantEvent, executor: &mut dyn Executor) -> Result<()> {
        GrantQueue { db: self.db.clone() }.complete_event(event, executor).await
    }
    /// 仅更新仍由本次领取持有的事件，不覆盖已经提交或被接管的状态。
    /// # 参数
    /// event 为领取快照，now 为时间，reason 为稳定故障说明。
    /// # 返回
    /// 成功无返回值；丢失领取权不写入。
    /// # 错误
    /// 数据库失败时返回错误，过期租约仍可恢复。
    pub async fn fail_event(&self, event: &GrantEvent, now: u64, reason: &str) -> Result<()> {
        GrantQueue { db: self.db.clone() }.fail_event(event, now, reason).await
    }
    /// 读取单一来源状态，供发布时排除永久终止及旧事件。
    /// # 参数
    /// key 为可信事件来源，executor 为发布事务。
    /// # 返回
    /// 已存在的来源。
    /// # 错误
    /// 身份非法或读取失败时拒绝。
    pub async fn source(&self, key: &SourceKey, executor: &mut dyn Executor) -> Result<Option<GrantSource>> {
        GrantQueue { db: self.db.clone() }.source(key, executor).await
    }
    /// 读取待处理量、最老未完成时长及失败量。
    /// # 参数
    /// now 为当前秒级时间。
    /// # 返回
    /// 当前队列快照。
    /// # 错误
    /// 数据库失败时返回错误。
    pub async fn grant_health(&self, now: u64) -> Result<GrantQueueHealth> {
        GrantQueue { db: self.db.clone() }.grant_health(now).await
    }
    /// 对正式事件按主键有界重算，禁止从当前责任人猜测历史授权。
    /// # 参数
    /// filter 为类型化选择条件，after 为主键游标，now 为重新可领取时间。
    /// # 返回
    /// 本页末尾游标；空页表示一轮结束。每次最多 100 条。
    /// # 错误
    /// 查询或更新失败返回错误；已重新排队事件可安全重放。
    pub async fn requeue(
        &self,
        filter: &GrantRecompute,
        after: Option<&str>,
        now: u64,
    ) -> Result<Option<String>> {
        GrantQueue { db: self.db.clone() }.requeue(filter, after, now).await
    }
}

impl ResourceAccessService {
    /// 取得并校验正式事件的当前状态。
    /// # 参数
    /// expected 为已校验的业务事件，executor 为调用方执行器。
    /// # 返回
    /// 事件存在时返回状态。
    /// # 错误
    /// 同键异载荷或查询失败返回错误。
    pub async fn event(
        &self,
        expected: &GrantEvent,
        executor: &mut dyn Executor,
    ) -> Result<Option<GrantEvent>> {
        GrantQueue { db: self.db.clone() }.event(expected, executor).await
    }
}
