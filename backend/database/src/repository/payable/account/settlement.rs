use entities::payable::PayableAccount;
use entity_core::NOT_DELETED_TIMESTAMP_BSON;
use erp_core::ids::PayableAccountId;
use erp_core::money::Amount;
use mongodb::bson::{doc, Bson, Document};

use super::super::super::Repository;
use super::write::{amount_bson, progress_pipeline};
use super::SettlementBatchResult;
use persistence_core::Executor;
use persistence_core::Result;

impl<'a> Repository<'a, PayableAccount> {
    /// 条件核销：增加已核销进度（不超额核销）。
    ///
    /// 原子写入口（P2 计划 §5）：以写条件而非读后判断保证
    /// `settled_total + 本次核销 <= gross_total`，不满足时**整个更新不生效**
    /// （matched 为 0），返回 `false` 且金额与状态均不变。核销进度同时重算
    /// `open_total` 与派生状态，全部在同一条件更新内完成，不会产生负开放余额。
    /// 单文档更新本身原子，可在 Service 的过账事务内参与回滚。
    ///
    /// # 参数
    /// * `id` - 应付往来子账 ID
    /// * `amount` - 本次核销含税金额（正数）
    /// * `updated_by` - 本次更新执行人
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 核销在额度内并已生效时返回 `true`；超过剩余开放余额被拒绝时返回 `false`。
    ///
    /// # 错误
    /// 当 MongoDB 更新失败时返回错误。
    pub async fn apply_settlement(
        &self,
        id: &str,
        amount: &Amount,
        updated_by: &str,
        executor: &mut dyn Executor,
    ) -> Result<bool> {
        let amount = amount_bson(amount)?;
        let filter = settlement_guard(id, &amount);
        self.conditional_update(
            filter,
            progress_pipeline("settled_total", "open_total", &amount, true, updated_by),
            executor,
        )
        .await
    }

    /// 批量条件核销：按账户聚合增量逐个执行不超额核销。
    ///
    /// 对每个 `(账户, 增量)` 复用与 [`Self::apply_settlement`] 相同的写条件
    /// （`settled_total + 增量 <= gross_total`），每个账户一次原子条件更新，
    /// 返回逐账户命中结果：`applied` 为已生效账户，`rejected` 为超过剩余
    /// 开放余额被拒绝的账户，金额与状态均未变化。调用方（Service）负责把
    /// 全部更新放入同一事务，任一账户被拒绝即整体回滚，不产生半写入。
    ///
    /// # 参数
    /// * `deltas` - 按账户聚合的本次核销增量（同一账户只出现一次）
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
    pub async fn apply_settlements_many(
        &self,
        deltas: &[(PayableAccountId, Amount)],
        updated_by: &str,
        executor: &mut dyn Executor,
    ) -> Result<SettlementBatchResult> {
        let mut applied = Vec::new();
        let mut rejected = Vec::new();
        for (id, amount) in deltas {
            let amount = amount_bson(amount)?;
            let filter = settlement_guard(id.as_ref(), &amount);
            let hit = self
                .conditional_update(
                    filter,
                    progress_pipeline("settled_total", "open_total", &amount, true, updated_by),
                    executor,
                )
                .await?;
            if hit {
                applied.push(id.clone());
            } else {
                rejected.push(id.clone());
            }
        }
        Ok(SettlementBatchResult { applied, rejected })
    }

    /// 条件核销冲减：减少已核销进度（不产生负已核销）。
    ///
    /// 反向核销（`REVERSE` 分配）的原子写入口：以写条件保证
    /// `本次冲减 <= settled_total`，不满足时整个更新不生效，返回 `false`。
    /// 用于冲正/退款时追加反向核销，防止冲减超过已核销金额。
    ///
    /// # 参数
    /// * `id` - 应付往来子账 ID
    /// * `amount` - 本次冲减含税金额（正数）
    /// * `updated_by` - 本次更新执行人
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 冲减在已核销额度内并已生效时返回 `true`；超过已核销金额被拒绝时返回 `false`。
    ///
    /// # 错误
    /// 当 MongoDB 更新失败时返回错误。
    pub async fn revert_settlement(
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

/// 构造条件核销的写前置条件（不超额核销）。
///
/// 以写条件而非读后判断保证 `settled_total + 本次核销 <= gross_total`，
/// 不满足时整个更新不生效（matched 为 0）。
///
/// # 参数
/// * `id` - 应付往来子账 ID
/// * `amount` - 本次核销含税金额（已转为 Decimal128 形态）
///
/// # 返回
/// 返回未删除账户的核销额度守卫文档。
pub(super) fn settlement_guard(id: &str, amount: &Bson) -> Document {
    doc! {
        "id": id,
        "deleted_at": NOT_DELETED_TIMESTAMP_BSON,
        "$expr": {
            "$lte": [
                { "$add": ["$settled_total", amount] },
                "$gross_total",
            ],
        },
    }
}
