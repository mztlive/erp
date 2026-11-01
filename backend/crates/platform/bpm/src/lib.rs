//! 纯流程领域与状态引擎。
//!
//! 本 crate 禁止依赖 ERP 业务实体、MongoDB、HTTP、权限或待办。
//! ID 与时间一律由调用方传入；禁止读取系统时钟或自行生成主键。

pub mod engine;
pub mod error;
pub mod graph;
pub mod ids;
pub mod model;

pub use engine::{BpmEvent, TransitionPlan};
pub use error::{Error, Result};
pub use ids::{
    ApprovalCommandReceiptId, ApprovalInstanceAssigneeId, ApprovalNodeDefinitionId, ApprovalNodeExecutionId,
    ApprovalProcessDefinitionId, ApprovalProcessInstanceId, ApprovalTransitionDefinitionId,
};
pub use model::{ParticipantId, ProcessKind, SubjectRef, Timestamp};
