//! 把既有应收子账选择发布为窄余额事实。

use erp_core::ids::SalesOrderId;
use persistence_core::{Executor, Result};

use super::account::ReceivableAccountRepositoryExt;
use crate::entity::receivable::ReceivableAccount;
use crate::entity::receivable::money_progress_facts::ReceivableMoneyProgressFact;

#[allow(async_fn_in_trait)]
pub trait ReceivableAccountMoneyProgressExt {
    /// 按销售单读取各应收子账余额事实，沿用既有子账筛选、排序与错误。
    ///
    /// 复用 `list_by_sales_order` 与调用方执行器，不新增聚合、复核状态过滤或事务。
    ///
    /// # 参数
    /// * `id` - 来源销售单。
    /// * `executor` - 调用方执行器。
    ///
    /// # 返回
    /// 返回按子账序号升序的余额事实；没有子账时返回空列表。
    ///
    /// # 错误
    /// 子账查询或反序列化失败时返回仓储错误。
    async fn money_progress_facts(
        &self,
        id: &SalesOrderId,
        executor: &mut dyn Executor,
    ) -> Result<Vec<ReceivableMoneyProgressFact>>;
}

impl ReceivableAccountMoneyProgressExt for persistence_core::Repository<'_, ReceivableAccount> {
    async fn money_progress_facts(
        &self,
        id: &SalesOrderId,
        executor: &mut dyn Executor,
    ) -> Result<Vec<ReceivableMoneyProgressFact>> {
        Ok(self.list_by_sales_order(id, executor).await?.iter().map(Into::into).collect())
    }
}
