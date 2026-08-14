//! 域 D03 审批运行实体。
//!
//! 本模块实现 `approval_definition`、`approval_step_definition`、
//! `approval_instance` 与 `approval_step_instance` 四个审批状态源。
//! `work_item` 仅表达当前人工责任，不在本模块推进流程或形成业务事实。

mod definition;
mod instance;
mod step_definition;
mod step_instance;
mod types;

pub use crate::ids::{
    ApprovalDefinitionId, ApprovalInstanceId, ApprovalStepDefinitionId, ApprovalStepInstanceId,
};
pub use definition::{ApprovalDefinition, ApprovalDefinitionData};
pub use instance::{ApprovalInstance, ApprovalInstanceData};
pub use step_definition::{ApprovalStepDefinition, ApprovalStepDefinitionData};
pub use step_instance::{ApprovalStepInstance, ApprovalStepInstanceData};
pub use types::{
    ApprovalAssignmentMode, ApprovalDecision, ApprovalDefinitionStatus, ApprovalInstanceStatus,
    ApprovalRuntimeKind, ApprovalStepStatus,
};
