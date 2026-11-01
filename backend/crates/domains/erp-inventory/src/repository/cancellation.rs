//! 库存调整撤回事实的拥有域集合访问器。

use mongodb::Database;
use persistence_core::Repository;

use crate::entity::cancellation::StockAdjustmentCancellation;

/// 不可变库存撤回事实集合；每审批实例恰有一条。
pub trait StockAdjustmentCancellationExt {
    const STOCK_ADJUSTMENT_CANCELLATIONS: &'static str = "stock_adjustment_cancellations";
    /// 获取库存撤回事实仓储。
    /// # 参数
    /// 无。
    /// # 返回
    /// 返回本域不可变事实仓储。
    /// # 错误
    /// 不返回错误。
    fn stock_adjustment_cancellations(&self) -> Repository<'_, StockAdjustmentCancellation>;
}

impl StockAdjustmentCancellationExt for Database {
    fn stock_adjustment_cancellations(&self) -> Repository<'_, StockAdjustmentCancellation> {
        Repository::new(self, Self::STOCK_ADJUSTMENT_CANCELLATIONS)
    }
}
