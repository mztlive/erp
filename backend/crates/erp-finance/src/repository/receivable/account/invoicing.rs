use entity_core::NOT_DELETED_TIMESTAMP_BSON;
use erp_core::ids::ReceivableAccountId;
use erp_core::money::Amount;
use mongodb::bson::{Bson, Document, doc};
use persistence_core::{Executor, Result};

use super::InvoicingBatchResult;
use super::write::{ReceivableAccountWriteExt, amount_bson, progress_pipeline};
use crate::entity::receivable::ReceivableAccount;

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

impl ReceivableAccountInvoicingExt for persistence_core::Repository<'_, ReceivableAccount> {
    async fn apply_invoicing(
        &self,
        id: &str,
        amount: &Amount,
        updated_by: &str,
        executor: &mut dyn Executor,
    ) -> Result<bool> {
        let amount = amount_bson(amount)?;
        let filter = doc! {
            "id": id,
            "deleted_at": NOT_DELETED_TIMESTAMP_BSON,
            "$expr": {
                "$lte": [
                    { "$add": ["$invoiced_total", &amount] },
                    "$invoiceable_total",
                ],
            },
        };
        self.conditional_update(
            filter,
            progress_pipeline("invoiced_total", "open_invoiceable_total", &amount, true, updated_by),
            executor,
        )
        .await
    }

    async fn apply_invoicings_many(
        &self,
        deltas: &[(ReceivableAccountId, Amount)],
        updated_by: &str,
        executor: &mut dyn Executor,
    ) -> Result<InvoicingBatchResult> {
        let mut applied = Vec::new();
        let mut rejected = Vec::new();
        for (id, amount) in deltas {
            let amount = amount_bson(amount)?;
            let filter = invoicing_guard(id.as_ref(), &amount);
            let hit = self
                .conditional_update(
                    filter,
                    progress_pipeline("invoiced_total", "open_invoiceable_total", &amount, true, updated_by),
                    executor,
                )
                .await?;
            if hit {
                applied.push(id.clone());
            } else {
                rejected.push(id.clone());
            }
        }
        Ok(InvoicingBatchResult { applied, rejected })
    }

    async fn revert_invoicing(
        &self,
        id: &str,
        amount: &Amount,
        updated_by: &str,
        executor: &mut dyn Executor,
    ) -> Result<bool> {
        let amount = amount_bson(amount)?;
        let filter = doc! {
            "id": id,
            "deleted_at": NOT_DELETED_TIMESTAMP_BSON,
            "$expr": {
                "$gte": ["$invoiced_total", &amount],
            },
        };
        self.conditional_update(
            filter,
            progress_pipeline("invoiced_total", "open_invoiceable_total", &amount, false, updated_by),
            executor,
        )
        .await
    }

    async fn revert_invoicings_many(
        &self,
        deltas: &[(ReceivableAccountId, Amount)],
        updated_by: &str,
        executor: &mut dyn Executor,
    ) -> Result<InvoicingBatchResult> {
        let mut applied = Vec::new();
        let mut rejected = Vec::new();
        for (id, amount) in deltas {
            let amount = amount_bson(amount)?;
            let filter = doc! {
                "id": id.to_string(),
                "deleted_at": NOT_DELETED_TIMESTAMP_BSON,
                "$expr": {
                    "$gte": ["$invoiced_total", &amount],
                },
            };
            let hit = self
                .conditional_update(
                    filter,
                    progress_pipeline("invoiced_total", "open_invoiceable_total", &amount, false, updated_by),
                    executor,
                )
                .await?;
            if hit {
                applied.push(id.clone());
            } else {
                rejected.push(id.clone());
            }
        }
        Ok(InvoicingBatchResult { applied, rejected })
    }
}

/// 构造条件开票的写前置条件（不超额开票）。
///
/// 以写条件而非读后判断保证 `invoiced_total + 本次开票 <= invoiceable_total`，
/// 不满足时整个更新不生效（matched 为 0）。金额形态与管道复用应付侧共用 helper，
/// 本文件只保留应收特有的守卫文档。
///
/// # 参数
/// * `id` - 应收往来子账 ID
/// * `amount` - 本次开票含税金额（已转为 Decimal128 形态）
///
/// # 返回
/// 返回未删除账户的开票额度守卫文档。
fn invoicing_guard(id: &str, amount: &Bson) -> Document {
    doc! {
        "id": id,
        "deleted_at": NOT_DELETED_TIMESTAMP_BSON,
        "$expr": {
            "$lte": [
                { "$add": ["$invoiced_total", amount] },
                "$invoiceable_total",
            ],
        },
    }
}
