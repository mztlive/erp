//! Inventory line-update mapping used by local commands and processes.

use crate::dto::StockAdjustmentLineUpdateInput;
use crate::entity::inventory::StockAdjustmentLineUpdate;
use crate::error::{Error, Result};

/// 把服务输入转换为已解析的调整明细更新值对象。
///
/// # 参数
/// * `updates` - 客户端提交的明细更新
///
/// # 返回
/// 返回完成主键规范化与数量解析的值对象集合。
///
/// # 错误
/// 行主键或数量非法时返回 `ValidationError`。
pub fn build_adjustment_line_updates(
    updates: &[StockAdjustmentLineUpdateInput],
) -> Result<Vec<StockAdjustmentLineUpdate>> {
    updates
        .iter()
        .map(|update| {
            StockAdjustmentLineUpdate::new(update.line_id.clone(), &update.quantity, update.direction)
                .map_err(|error| Error::ValidationError(error.to_string()))
        })
        .collect()
}
