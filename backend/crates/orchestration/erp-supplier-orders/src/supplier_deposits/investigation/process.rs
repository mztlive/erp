//! 有界只读资金扫描；先可靠保存全页，再处理本地证据，最后记录独立余额观察。

use async_trait::async_trait;
use erp_core::common::time::Instant;
use erp_supply::ports::connector::common::{Page, Snapshot};
use erp_supply::ports::connector::funds::{
    SupplierFundsBalance, SupplierFundsObservation, SupplierFundsQuery, SupplierFundsSource,
};

use crate::{Error, Result};

/// 查询证据与账务处理各自计数，空查询不形成资金结果结论。
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct FundsInvestigationRun {
    pub received: usize,
    pub applied: usize,
    pub needs_review: usize,
    pub settlement_pending: usize,
}

impl FundsInvestigationRun {
    pub(super) fn merge(&mut self, other: Self) {
        self.received += other.received;
        self.applied += other.applied;
        self.needs_review += other.needs_review;
        self.settlement_pending += other.settlement_pending;
    }
}

/// 可靠接收和恢复合同，Mongo实现仍通过财务拥有领域事务步骤。
#[async_trait]
pub trait FundsInvestigationStore: Send + Sync {
    /// 返回已核验外部资金账号。当前账户绑定的稳定引用。
    fn account_reference(&self) -> &str;
    /// 恢复已可靠接收的证据；单项未知结果必须保留，不以时间释放。
    ///
    /// 本轮实际应用、待核对和待核销数量。
    ///
    /// 存储或资格失败时停止。
    async fn resume(&self) -> Result<FundsInvestigationRun>;
    /// 取得已经固定窗口和游标的下一页读取请求。
    ///
    /// `now`：当前服务器时间，只用于新窗口。
    ///
    /// 原窗口原游标，不根据余额或消息改变。
    ///
    /// 检查点损坏或读取失败时停止。
    async fn query(&self, now: Instant) -> Result<SupplierFundsQuery>;
    /// 原子保存整页证据和处理意图后推进读取检查点。
    ///
    /// `query` / `page`：原固定窗口和实际响应。
    ///
    /// 本页可靠接收条数，重复证据不重置应用状态。
    ///
    /// 保存失败、同号异载荷或旧检查点不得推进。
    async fn receive_page(
        &self,
        query: &SupplierFundsQuery,
        page: Page<Snapshot<SupplierFundsObservation>>,
    ) -> Result<usize>;
    /// 独立保存余额采样，不据其差额造充值或扣款。
    ///
    /// `balance`：来源余额、实际观察时间和加密原文。
    ///
    /// 采样已经可靠保存。
    ///
    /// 跨账号、缺证据或保存失败时拒绝。
    async fn observe_balance(&self, balance: Snapshot<SupplierFundsBalance>) -> Result<()>;
}

/// 实际生产与替身测试共用同一编排，外部读取不在资金事务中执行。
pub struct FundsInvestigationProcess<'a> {
    pub source: &'a dyn SupplierFundsSource,
    pub store: &'a dyn FundsInvestigationStore,
}

impl FundsInvestigationProcess<'_> {
    /// 为资金账号调查时，先恢复已保存的本地证据，再用原窗口和游标读取至多一页，整页接收后才应用并独立采样余额。
    /// 本流程不重放任何远端资金写请求。
    ///
    /// `now`：当前服务器时间。
    ///
    /// 返回各阶段计数；零交易不表示未扣款或已查明消息。
    ///
    /// 任一接收失败停止后续应用和检查点推进；原证据保持可恢复。
    pub async fn run(&self, now: Instant) -> Result<FundsInvestigationRun> {
        if self.source.account_reference() != self.store.account_reference() {
            return Err(Error::Forbidden("资金查询能力与本地账户归属不一致".into()));
        }

        let mut result = self.store.resume().await?;
        let query = self.store.query(now).await?;
        if query.limit.get() > 50 || query.since >= query.through {
            return Err(Error::ValidationError("资金查询窗口或分页上限无效".into()));
        }

        let page = self
            .source
            .transactions(&query)
            .await
            .map_err(|_| Error::BusinessLogicError("供应商资金明细读取失败，原调查保持待核实".into()))?;
        if page.items.len() > usize::try_from(query.limit.get()).expect("固定批次不超过50") {
            return Err(Error::BusinessLogicError("供应商资金明细超过本次分页上限".into()));
        }

        result.received += self.store.receive_page(&query, page).await?;
        result.merge(self.store.resume().await?);

        let balance = self
            .source
            .balance()
            .await
            .map_err(|_| Error::BusinessLogicError("供应商资金余额读取失败，未修改本地账本".into()))?;
        self.store.observe_balance(balance).await?;
        Ok(result)
    }
}

#[cfg(test)]
mod tests;
