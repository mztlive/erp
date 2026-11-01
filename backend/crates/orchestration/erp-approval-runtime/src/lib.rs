//! 审批启动、冻结材料及精确命令结果恢复的跨域执行合同。
pub mod approval_materials;
pub mod approval_recovery;
pub mod approval_start;
pub mod approver_directory;
mod errors;
pub mod material_writes;
pub use errors::{Error, Result};
pub mod runtime_port;
pub use runtime_port::{ApprovalRuntime, ApprovalRuntimePort};
