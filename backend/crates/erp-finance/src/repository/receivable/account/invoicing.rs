use erp_core::ids::ReceivableAccountId;
use erp_core::money::Amount;
use persistence_core::{Executor, Repository, Result};

use super::InvoicingBatchResult;
use crate::entity::receivable::ReceivableAccount;
use crate::repository::invoicing::{InvoicingDirection, update, update_many};

#[allow(async_fn_in_trait)]
pub trait ReceivableAccountInvoicingExt {
    /// 条件开票：增加净已开票进度（不超过可开票额度）。
    ///
    /// 销项蓝票 `APPLY` 的原子写入口：以写条件保证
    /// `invoiced_total + 本次开票 <= invoiceable_total`，不满足时整个更新不生效，
    /// 返回 `false`。同时重算 `open_invoiceable_total`，不会产生负可开票余额。
    ///
    /// # 参数
    /// * `id` - 应收往来子账 ID
    /// * `amount` - 本次开票含税金额（正数）
    /// * `updated_by` - 本次更新执行人
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 开票在额度内并已生效时返回 `true`；超过剩余可开票额度被拒绝时返回 `false`。
    ///
    /// # 错误
    /// 当 MongoDB 更新失败时返回错误。
    async fn apply_invoicing(
        &self,
        id: &str,
        amount: &Amount,
        updated_by: &str,
        executor: &mut dyn Executor,
    ) -> Result<bool>;

    /// 批量条件开票：按子账聚合增量原子更新开票进度（FIN-R10）。
    ///
    /// 对已去重并按账户聚合的 `deltas` 逐账户执行条件更新（`invoicing_guard`
    /// 保证 `invoiced_total + delta <= invoiceable_total`），并按输入顺序
    /// 报告每个账户的命中情况；调用方（Service）负责将 `rejected` 转译为
    /// 业务错误，失败时整个事务回滚，不产生部分写入。
    ///
    /// # 参数
    /// * `deltas` - 按子账聚合的开票增量（已去重、首次出现顺序，同一账户只出现一次）
    /// * `updated_by` - 本次更新执行人
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 返回按账户报告命中情况的 [`InvoicingBatchResult`]。
    ///
    /// # 错误
    /// 当 MongoDB 更新失败时返回错误。
    ///
    /// # 约束
    /// 聚合口径（同一账户增量求和、同一账户只更新一次）由 Service 经领域
    /// 计划保证；本方法不自行开启事务、不决定跨账户业务结论。
    async fn apply_invoicings_many(
        &self,
        deltas: &[(ReceivableAccountId, Amount)],
        updated_by: &str,
        executor: &mut dyn Executor,
    ) -> Result<InvoicingBatchResult>;

    /// 条件开票冲减：减少净已开票进度（不产生负已开票）。
    ///
    /// 销项红票 `REVERSE` 的原子写入口：以写条件保证 `本次红冲 <= invoiced_total`，
    /// 不满足时整个更新不生效，返回 `false`。累计红冲由 P3 登记事务结合
    /// `reverses_allocation_id` 校验，本方法防止已开票进度被冲成负数。
    ///
    /// # 参数
    /// * `id` - 应收往来子账 ID
    /// * `amount` - 本次红冲含税金额（正数）
    /// * `updated_by` - 本次更新执行人
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 红冲在已开票额度内并已生效时返回 `true`；超过已开票金额被拒绝时返回 `false`。
    ///
    /// # 错误
    /// 当 MongoDB 更新失败时返回错误。
    async fn revert_invoicing(
        &self,
        id: &str,
        amount: &Amount,
        updated_by: &str,
        executor: &mut dyn Executor,
    ) -> Result<bool>;

    /// 批量条件开票冲减：按子账聚合增量原子回退开票进度（FIN-R11）。
    ///
    /// 对已去重并按账户聚合的 `deltas` 逐账户执行条件更新（写条件保证
    /// `本次红冲 <= invoiced_total`），并按输入顺序报告每个账户的命中情况；
    /// 调用方（Service）负责将 `rejected` 转译为业务错误，失败时整个事务回滚，
    /// 不产生部分写入。本方法只执行计划，不判断红票业务资格。
    ///
    /// # 参数
    /// * `deltas` - 按子账聚合的红冲增量（已去重、首次出现顺序，同一账户只出现一次）
    /// * `updated_by` - 本次更新执行人
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 返回按账户报告命中情况的 [`InvoicingBatchResult`]（`applied` 为命中，
    /// `rejected` 为超过已开票进度被拒绝）。
    ///
    /// # 错误
    /// 当 MongoDB 更新失败时返回错误。
    ///
    /// # 约束
    /// 聚合口径（同一账户增量求和、同一账户只更新一次）由 Service 经领域
    /// 计划保证；本方法不自行开启事务、不决定跨账户业务结论。
    async fn revert_invoicings_many(
        &self,
        deltas: &[(ReceivableAccountId, Amount)],
        updated_by: &str,
        executor: &mut dyn Executor,
    ) -> Result<InvoicingBatchResult>;
}

impl ReceivableAccountInvoicingExt for Repository<'_, ReceivableAccount> {
    async fn apply_invoicing(
        &self,
        id: &str,
        amount: &Amount,
        updated_by: &str,
        executor: &mut dyn Executor,
    ) -> Result<bool> {
        update(self, id, amount, updated_by, InvoicingDirection::Apply, executor).await
    }

    async fn apply_invoicings_many(
        &self,
        deltas: &[(ReceivableAccountId, Amount)],
        updated_by: &str,
        executor: &mut dyn Executor,
    ) -> Result<InvoicingBatchResult> {
        let result = update_many(self, deltas, updated_by, InvoicingDirection::Apply, executor).await?;
        Ok(InvoicingBatchResult { applied: result.applied, rejected: result.rejected })
    }

    async fn revert_invoicing(
        &self,
        id: &str,
        amount: &Amount,
        updated_by: &str,
        executor: &mut dyn Executor,
    ) -> Result<bool> {
        update(self, id, amount, updated_by, InvoicingDirection::Revert, executor).await
    }

    async fn revert_invoicings_many(
        &self,
        deltas: &[(ReceivableAccountId, Amount)],
        updated_by: &str,
        executor: &mut dyn Executor,
    ) -> Result<InvoicingBatchResult> {
        let result = update_many(self, deltas, updated_by, InvoicingDirection::Revert, executor).await?;
        Ok(InvoicingBatchResult { applied: result.applied, rejected: result.rejected })
    }
}
