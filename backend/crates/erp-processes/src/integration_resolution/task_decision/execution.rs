//! W29 生产步骤端口：回执优先、单执行器与失败停止的唯一编排。

use std::future::Future;

use async_trait::async_trait;
use persistence_core::Executor;
use services::Result;

/// 首次回放命中时不准备事务；任意事务错误只做一次全新回放。
pub(super) async fn execute_with_receipt<T, R, RF, W, WF>(mut replay: R, write: W) -> Result<T>
where
    R: FnMut() -> RF,
    RF: Future<Output = Result<Option<T>>>,
    W: FnOnce() -> WF,
    WF: Future<Output = Result<T>>,
{
    if let Some(result) = replay().await? {
        return Ok(result);
    }
    match write().await {
        Ok(result) => Ok(result),
        Err(error) => match replay().await? {
            Some(result) => Ok(result),
            None => Err(error),
        },
    }
}

/// 正式任务命令的真实步骤；本域事实与 WorkItem 分别由其拥有者实施。
#[async_trait]
pub(super) trait TaskCommandPort: Send {
    type Item: Send + Sync;
    type Fact: Send + Sync;
    type Output: Send;

    async fn load_bound(&mut self, executor: &mut dyn Executor) -> Result<Self::Item>;
    async fn authorize(&mut self, item: &Self::Item, executor: &mut dyn Executor) -> Result<()>;
    async fn apply_domain(&mut self, executor: &mut dyn Executor) -> Result<Self::Fact>;
    fn transition(&mut self, item: &mut Self::Item, fact: &Self::Fact) -> Result<()>;
    async fn persist_task(&mut self, item: &mut Self::Item, executor: &mut dyn Executor) -> Result<()>;
    fn result(&mut self, fact: &Self::Fact) -> Result<Self::Output>;
    async fn receipt(&mut self, fact: &Self::Fact, executor: &mut dyn Executor) -> Result<()>;
}

/// 非终态动作保留“结果投影在回执之前”的原始首错顺序。
pub(super) async fn run_action<P: TaskCommandPort>(
    port: &mut P,
    executor: &mut dyn Executor,
) -> Result<P::Output> {
    let mut item = port.load_bound(executor).await?;
    port.authorize(&item, executor).await?;
    let fact = port.apply_domain(executor).await?;
    port.transition(&mut item, &fact)?;
    port.persist_task(&mut item, executor).await?;
    let result = port.result(&fact)?;
    port.receipt(&fact, executor).await?;
    Ok(result)
}

/// 完成命令保留“回执在最终结果投影之前”的原顺序。
pub(super) async fn run_completion<P: TaskCommandPort>(
    port: &mut P,
    executor: &mut dyn Executor,
) -> Result<P::Output> {
    let mut item = port.load_bound(executor).await?;
    port.authorize(&item, executor).await?;
    let fact = port.apply_domain(executor).await?;
    port.transition(&mut item, &fact)?;
    port.persist_task(&mut item, executor).await?;
    port.receipt(&fact, executor).await?;
    port.result(&fact)
}

/// 直接对账须先拒绝任意正式任务关联，再触碰本域差异。
#[async_trait]
pub(super) trait DirectCommandPort: Send {
    type Fact: Send + Sync;
    type Output: Send;

    async fn ensure_no_task(&mut self, executor: &mut dyn Executor) -> Result<()>;
    async fn apply_domain(&mut self, executor: &mut dyn Executor) -> Result<Self::Fact>;
    async fn receipt(&mut self, fact: &Self::Fact, executor: &mut dyn Executor) -> Result<()>;
    fn result(&mut self, fact: Self::Fact) -> Self::Output;
}

/// 沿同一执行器完成直接决定；首错立即停止，不写后续回执。
pub(super) async fn run_direct<P: DirectCommandPort>(
    port: &mut P,
    executor: &mut dyn Executor,
) -> Result<P::Output> {
    port.ensure_no_task(executor).await?;
    let fact = port.apply_domain(executor).await?;
    port.receipt(&fact, executor).await?;
    Ok(port.result(fact))
}

#[cfg(test)]
mod tests;
