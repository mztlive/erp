use entity_core::NOT_DELETED_TIMESTAMP_BSON;
use erp_core::money::Amount;
use mongodb::bson::doc;
use persistence_core::{Executor, Result};

use super::write::{ReceivableAccountWriteExt, amount_bson, progress_pipeline};
use crate::entity::receivable::ReceivableAccount;

#[allow(async_fn_in_trait)]
pub trait ReceivableAccountSettlementExt {
    /// 条件核销：增加已核销进度（不超额核销）。
    ///
    /// 原子写入口（P2 计划 §5）：以写条件而非读后判断保证
    /// `settled_total + 本次核销 <= gross_total`，不满足时**整个更新不生效**
    /// （matched 为 0），返回 `false` 且金额与状态均不变。核销进度同时重算
    /// `open_total` 与派生状态，全部在同一条件更新内完成，不会产生负开放余额。
    /// 单文档更新本身原子，可在 Service 的过账事务内参与回滚。
    ///
    /// # 参数
    /// * `id` - 应收往来子账 ID
    /// * `amount` - 本次核销含税金额（正数）
    /// * `updated_by` - 本次更新执行人
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 核销在额度内并已生效时返回 `true`；超过剩余开放余额被拒绝时返回 `false`。
    ///
    /// # 错误
    /// 当 MongoDB 更新失败时返回错误。
    async fn apply_settlement(
        &self,
        id: &str,
        amount: &Amount,
        updated_by: &str,
        executor: &mut dyn Executor,
    ) -> Result<bool>;

    /// 条件核销冲减：减少已核销进度（不产生负已核销）。
    ///
    /// 反向核销（`REVERSE` 分配）的原子写入口：以写条件保证
    /// `本次冲减 <= settled_total`，不满足时整个更新不生效，返回 `false`。
    /// 用于冲正/退款时追加反向核销，防止冲减超过已核销金额。
    ///
    /// # 参数
    /// * `id` - 应收往来子账 ID
    /// * `amount` - 本次冲减含税金额（正数）
    /// * `updated_by` - 本次更新执行人
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 冲减在已核销额度内并已生效时返回 `true`；超过已核销金额被拒绝时返回 `false`。
    ///
    /// # 错误
    /// 当 MongoDB 更新失败时返回错误。
    async fn revert_settlement(
        &self,
        id: &str,
        amount: &Amount,
        updated_by: &str,
        executor: &mut dyn Executor,
    ) -> Result<bool>;
}

impl ReceivableAccountSettlementExt for persistence_core::Repository<'_, ReceivableAccount> {
    async fn apply_settlement(
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
                    { "$add": ["$settled_total", &amount] },
                    "$gross_total",
                ],
            },
        };
        self.conditional_update(
            filter,
            progress_pipeline("settled_total", "open_total", &amount, true, updated_by),
            executor,
        )
        .await
    }

    async fn revert_settlement(
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
                "$gte": ["$settled_total", &amount],
            },
        };
        self.conditional_update(
            filter,
            progress_pipeline("settled_total", "open_total", &amount, false, updated_by),
            executor,
        )
        .await
    }
}
