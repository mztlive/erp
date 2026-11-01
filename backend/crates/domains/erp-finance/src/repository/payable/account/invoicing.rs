use erp_core::ids::PayableAccountId;
use erp_core::money::Amount;
use persistence_core::{Executor, Repository, Result};

use super::InvoicingBatchResult;
use crate::entity::payable::PayableAccount;
use crate::repository::invoicing::{InvoicingDirection, update, update_many};

#[allow(async_fn_in_trait)]
pub trait PayableAccountInvoicingExt {
    /// 条件收票：增加净已收票进度（不超过可收票额度）。
    ///
    /// 进项蓝票 `APPLY` 的原子写入口：以写条件保证
    /// `invoiced_total + 本次收票 <= invoiceable_total`，不满足时整个更新不生效，
    /// 返回 `false`。同时重算 `open_invoiceable_total`，不会产生负可收票余额。
    ///
    /// # 参数
    /// * `id` - 应付往来子账 ID
    /// * `amount` - 本次收票含税金额（正数）
    /// * `updated_by` - 本次更新执行人
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 收票在额度内并已生效时返回 `true`；超过剩余可收票额度被拒绝时返回 `false`。
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

    /// 批量条件收票：按账户聚合增量逐个执行不超额收票。
    ///
    /// 对每个 `(账户, 增量)` 复用与 [`Self::apply_invoicing`] 相同的写条件
    /// （`invoiced_total + 增量 <= invoiceable_total`），每个账户一次原子条件
    /// 更新，返回逐账户命中结果：`applied` 为已生效账户，`rejected` 为超过
    /// 剩余可收票额度被拒绝的账户，金额与状态均未变化。调用方（Service）
    /// 负责把全部更新放入同一事务，任一账户被拒绝即整体回滚，不产生半写入。
    ///
    /// # 参数
    /// * `deltas` - 按账户聚合的本次收票增量（同一账户只出现一次）
    /// * `updated_by` - 本次更新执行人
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 返回逐账户的命中结果；空输入不访问数据库并返回空结果。
    ///
    /// # 错误
    /// 当 MongoDB 更新失败时返回错误。
    ///
    /// # 约束
    /// 聚合口径（同一账户增量求和、同一账户只更新一次）由 Service 经领域
    /// 计划保证；本方法不自行开启事务、不决定跨账户业务结论。
    async fn apply_invoicings_many(
        &self,
        deltas: &[(PayableAccountId, Amount)],
        updated_by: &str,
        executor: &mut dyn Executor,
    ) -> Result<InvoicingBatchResult>;

    /// 条件收票冲减：减少净已收票进度（不产生负已收票）。
    ///
    /// 进项红票 `REVERSE` 的原子写入口：以写条件保证 `本次红冲 <= invoiced_total`，
    /// 不满足时整个更新不生效，返回 `false`。累计红冲由 P3 登记事务结合
    /// `reverses_allocation_id` 校验，本方法防止已收票进度被冲成负数。
    ///
    /// # 参数
    /// * `id` - 应付往来子账 ID
    /// * `amount` - 本次红冲含税金额（正数）
    /// * `updated_by` - 本次更新执行人
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 红冲在已收票额度内并已生效时返回 `true`；超过已收票金额被拒绝时返回 `false`。
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

    /// 批量条件收票冲减：按子账聚合增量原子回退收票进度（FIN-R11）。
    ///
    /// 对已去重并按账户聚合的 `deltas` 逐账户执行条件更新（写条件保证
    /// `本次红冲 <= invoiced_total`），并按输入顺序报告命中情况；调用方
    /// （Service）负责将 `rejected` 转译为业务错误，失败时整个事务回滚。
    /// 本方法只执行计划，不判断红票业务资格。
    ///
    /// # 参数
    /// * `deltas` - 按子账聚合的红冲增量（已去重、首次出现顺序，同一账户只出现一次）
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
    /// 聚合口径由 Service 经领域计划保证；本方法不自行开启事务、
    /// 不决定跨账户业务结论。
    async fn revert_invoicings_many(
        &self,
        deltas: &[(PayableAccountId, Amount)],
        updated_by: &str,
        executor: &mut dyn Executor,
    ) -> Result<InvoicingBatchResult>;
}

impl PayableAccountInvoicingExt for Repository<'_, PayableAccount> {
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
        deltas: &[(PayableAccountId, Amount)],
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
        deltas: &[(PayableAccountId, Amount)],
        updated_by: &str,
        executor: &mut dyn Executor,
    ) -> Result<InvoicingBatchResult> {
        let result = update_many(self, deltas, updated_by, InvoicingDirection::Revert, executor).await?;
        Ok(InvoicingBatchResult { applied: result.applied, rejected: result.rejected })
    }
}
