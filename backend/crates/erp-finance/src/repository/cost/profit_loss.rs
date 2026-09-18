//! 报表按授权销售单批量读取成本分配及对应事实，禁止按供应商成本猜测归属。
use mongodb::bson::doc;
use mongodb::options::FindOptions;
use persistence_core::{Executor, Result, mongo_ops};

use crate::entity::cost::{CostAllocation, CostEntry};

/// 单次报表的成本分配上限；额外一条用于报错而非静默截断。
pub const PROFIT_LOSS_ALLOCATION_LIMIT: usize = 100_000;

#[allow(async_fn_in_trait)]
pub trait CostAllocationProfitLossExt {
    /// 返回目标订单的分配，使用已有销售单索引；空范围不访问数据库。
    async fn profit_loss_allocations(
        &self,
        ids: &[String],
        executor: &mut dyn Executor,
    ) -> Result<Vec<CostAllocation>>;
}

impl CostAllocationProfitLossExt for persistence_core::Repository<'_, CostAllocation> {
    async fn profit_loss_allocations(
        &self,
        ids: &[String],
        executor: &mut dyn Executor,
    ) -> Result<Vec<CostAllocation>> {
        if ids.is_empty() {
            return Ok(vec![]);
        }
        let options = FindOptions::builder().limit((PROFIT_LOSS_ALLOCATION_LIMIT + 1) as i64).build();
        mongo_ops::find_many(
            &self.collection(),
            doc! { "deleted_at": 0_i64, "sales_order_id": { "$in": ids } },
            options,
            executor,
        )
        .await
    }
}

#[allow(async_fn_in_trait)]
pub trait CostEntryProfitLossExt {
    /// 按分配引用读取完整成本事实；调用方校验缺失事实，禁止视为零成本。
    async fn profit_loss_entries(
        &self,
        ids: &[String],
        executor: &mut dyn Executor,
    ) -> Result<Vec<CostEntry>>;
}

impl CostEntryProfitLossExt for persistence_core::Repository<'_, CostEntry> {
    async fn profit_loss_entries(
        &self,
        ids: &[String],
        executor: &mut dyn Executor,
    ) -> Result<Vec<CostEntry>> {
        if ids.is_empty() {
            return Ok(vec![]);
        }
        self.find_many(doc! { "id": { "$in": ids } }, executor).await
    }
}
