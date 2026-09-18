//! 采购单提交启动：加载定义图、构造 `prepare_start` 输入并持久化运行事实。
//!
//! 本模块按 receipt/replay、input 构图、persist 三段拆分：
//! [`start_receipt`] 做定义图与收据读取，[`start_input`] 做启动输入构图，
//! [`start_persist`] 做同一事务内的顺序写入与审计。创建并提交路径与单纯
//! 提交路径共享同一启动装配。

mod start_input;
mod start_persist;
mod start_receipt;

#[cfg(test)]
pub(crate) use start_input::tests;
pub(super) use start_input::{PurchaseOrderStartInput, build_purchase_order_start_input};
pub(super) use start_persist::{
    PurchaseOrderStartPersistInput, PurchaseSubmitProcurementGuard, persist_purchase_order_start,
};
pub(super) use start_receipt::{
    load_bound_definition_graph, load_bound_definition_graph_with_executor, load_start_receipt,
    replay_purchase_order_start_with_executor,
};
