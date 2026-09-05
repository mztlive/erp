//! 库存调整提交启动：加载定义图、构造 `prepare_start` 输入并持久化运行事实。

mod mapping;
mod persist;
mod prepare;
#[cfg(test)]
mod tests;

pub(super) use mapping::build_adjustment_line_updates;
pub(super) use persist::{persist_stock_adjustment_start, StockAdjustmentStartPersistInput};
pub(super) use prepare::{
    actor_can_submit, build_stock_adjustment_start_input,
    ensure_stock_adjustment_submit_authorized_with_executor, find_stock_adjustment_start_result,
    load_bound_definition_graph, prepare_stock_adjustment_start, reconcile_stock_adjustment_start_receipt,
    StockAdjustmentStartInput,
};
