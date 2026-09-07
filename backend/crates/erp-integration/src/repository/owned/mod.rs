//! 组合基础持久化的集成域自有仓储。

mod inbox_message;
mod integration_error_task;
mod reconciliation_difference;
mod reconciliation_difference_resolution;

pub use inbox_message::InboxMessageRepository;
pub use integration_error_task::IntegrationErrorTaskRepository;
pub use reconciliation_difference::ReconciliationDifferenceRepository;
pub use reconciliation_difference_resolution::ReconciliationDifferenceResolutionRepository;
