//! W29 生产步骤端口：回执优先、单执行器与失败停止的唯一编排。

use std::future::Future;

use async_trait::async_trait;
use persistence_core::Executor;

use crate::{Error, Result};

/// 首次回放命中时不准备事务；任意事务错误只做一次全新回放。
///
/// # 参数
/// * `replay` - 读取已提交结果；命中时返回 `Some`。
/// * `write` - 首次未命中时执行的写入。
///
/// # 返回
/// 回放命中的结果，或写入成功后的结果。写入失败但随后回放命中时仍返回该结果。
///
/// # 错误
/// 首次回放失败时返回该错误。写入失败且二次回放无结果时返回写入错误；写入为 `OutcomeUnknown` 时保留该错误。二次回放自身失败且写入不是 `OutcomeUnknown` 时返回回放错误。
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
        Err(error) => match replay().await {
            Ok(Some(result)) => Ok(result),
            Ok(None) => Err(error),
            Err(_) if matches!(error, Error::OutcomeUnknown(_)) => Err(error),
            Err(recovery_error) => Err(recovery_error),
        },
    }
}

/// 正式任务命令的真实步骤；本域事实与 WorkItem 分别由其拥有者实施。
#[async_trait]
pub(super) trait TaskCommandPort: Send {
    type Item: Send + Sync;
    type Fact: Send + Sync;
    type Output: Send;

    /// 加载本命令绑定的任务。
    ///
    /// # 参数
    /// * `executor` - 本命令使用的执行器。
    ///
    /// # 返回
    /// 后续步骤使用的任务。
    ///
    /// # 错误
    /// 实现方拒绝绑定或读取失败时返回对应错误。
    async fn load_bound(&mut self, executor: &mut dyn Executor) -> Result<Self::Item>;
    /// 校验当前命令是否可以作用于已加载任务。
    ///
    /// # 参数
    /// * `item` - `load_bound` 返回的任务。
    /// * `executor` - 本命令使用的执行器。
    ///
    /// # 返回
    /// 无返回值。
    ///
    /// # 错误
    /// 授权不通过时返回实现方错误。
    async fn authorize(&mut self, item: &Self::Item, executor: &mut dyn Executor) -> Result<()>;
    /// 写入本域事实，不在此推进正式任务。
    ///
    /// # 参数
    /// * `executor` - 本命令使用的执行器。
    ///
    /// # 返回
    /// 本域写入产出的事实。
    ///
    /// # 错误
    /// 本域写入失败时返回实现方错误。
    async fn apply_domain(&mut self, executor: &mut dyn Executor) -> Result<Self::Fact>;
    /// 按事实推进内存中的任务状态，不落库。
    ///
    /// # 参数
    /// * `item` - 待推进的任务。
    /// * `fact` - 本域写入产出的事实。
    ///
    /// # 返回
    /// 无返回值。
    ///
    /// # 错误
    /// 状态不能按该事实推进时返回实现方错误。
    fn transition(&mut self, item: &mut Self::Item, fact: &Self::Fact) -> Result<()>;
    /// 持久化推进后的正式任务。
    ///
    /// # 参数
    /// * `item` - 已推进的任务。
    /// * `executor` - 本命令使用的执行器。
    ///
    /// # 返回
    /// 无返回值。
    ///
    /// # 错误
    /// 任务写入失败时返回实现方错误。
    async fn persist_task(&mut self, item: &mut Self::Item, executor: &mut dyn Executor) -> Result<()>;
    /// 由事实构造命令结果。
    ///
    /// # 参数
    /// * `fact` - 本域写入产出的事实。
    ///
    /// # 返回
    /// 命令结果。
    ///
    /// # 错误
    /// 结果无法由该事实构成时返回实现方错误。
    fn result(&mut self, fact: &Self::Fact) -> Result<Self::Output>;
    /// 写入命令回执。
    ///
    /// # 参数
    /// * `fact` - 本域写入产出的事实。
    /// * `executor` - 本命令使用的执行器。
    ///
    /// # 返回
    /// 无返回值。
    ///
    /// # 错误
    /// 回执写入失败时返回实现方错误。
    async fn receipt(&mut self, fact: &Self::Fact, executor: &mut dyn Executor) -> Result<()>;
}

/// 非终态动作保留“结果投影在回执之前”的原始首错顺序。
///
/// # 参数
/// * `port` - 正式任务命令步骤。
/// * `executor` - 同一次命令使用的执行器。
///
/// # 返回
/// 端口投影出的命令结果。回执在结果投影之后写入。
///
/// # 错误
/// 加载、授权、本域写入、状态迁移、任务持久化、结果投影或回执任一步失败时返回该错误并停止后续步骤。
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
///
/// # 参数
/// * `port` - 正式任务命令步骤。
/// * `executor` - 同一次命令使用的执行器。
///
/// # 返回
/// 回执写入之后由端口投影的完成结果。
///
/// # 错误
/// 加载、授权、本域写入、状态迁移、任务持久化、回执或结果投影任一步失败时返回该错误并停止后续步骤。
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

    /// 拒绝任何已关联的正式任务，通过后才允许改差异。
    ///
    /// # 参数
    /// * `executor` - 本命令使用的执行器。
    ///
    /// # 返回
    /// 无返回值。
    ///
    /// # 错误
    /// 已有正式任务关联或读取失败时返回实现方错误。
    async fn ensure_no_task(&mut self, executor: &mut dyn Executor) -> Result<()>;
    /// 写入直接决定的本域事实。
    ///
    /// # 参数
    /// * `executor` - 本命令使用的执行器。
    ///
    /// # 返回
    /// 本域写入产出的事实。
    ///
    /// # 错误
    /// 本域写入失败时返回实现方错误。
    async fn apply_domain(&mut self, executor: &mut dyn Executor) -> Result<Self::Fact>;
    /// 为已写入的事实保存命令回执。
    ///
    /// # 参数
    /// * `fact` - 本域写入产出的事实。
    /// * `executor` - 本命令使用的执行器。
    ///
    /// # 返回
    /// 无返回值。
    ///
    /// # 错误
    /// 回执写入失败时返回实现方错误。
    async fn receipt(&mut self, fact: &Self::Fact, executor: &mut dyn Executor) -> Result<()>;
    /// 由已提交事实构造命令结果，不再访问存储。
    ///
    /// # 参数
    /// * `fact` - 本域写入产出的事实，按值消耗。
    ///
    /// # 返回
    /// 直接决定的结果。
    ///
    /// # 错误
    /// 不返回错误。
    fn result(&mut self, fact: Self::Fact) -> Self::Output;
}

/// 沿同一执行器完成直接决定；首错立即停止，不写后续回执。
///
/// # 参数
/// * `port` - 直接对账步骤。
/// * `executor` - 同一次命令使用的执行器。
///
/// # 返回
/// 回执写入之后的决定结果。
///
/// # 错误
/// 任务关联检查、本域写入或回执失败时返回该错误并停止后续步骤。
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
