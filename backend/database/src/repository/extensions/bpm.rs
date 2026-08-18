//! BPM 模型仓储访问器。

use bpm::model::{
    ApprovalCommandReceipt, ApprovalInstanceAssignee, ApprovalNodeDefinition, ApprovalNodeExecution,
    ApprovalProcessDefinition, ApprovalProcessInstance, ApprovalTransitionDefinition,
};
use mongodb::Database;

use super::super::bpm::BpmWorkflowRepository;
use crate::Repository;

/// BPM 目标集合仓储访问器。
///
/// 只暴露 BPM 模型集合与聚合仓储 [`BpmWorkflowRepository`]；不得混入 ERP 实体。
pub trait BpmExt {
    /// `approval_process_definitions` 集合名。
    const APPROVAL_PROCESS_DEFINITIONS: &'static str = "approval_process_definitions";
    /// `approval_node_definitions` 集合名。
    const APPROVAL_NODE_DEFINITIONS: &'static str = "approval_node_definitions";
    /// `approval_transition_definitions` 集合名。
    const APPROVAL_TRANSITION_DEFINITIONS: &'static str = "approval_transition_definitions";
    /// `approval_process_instances` 集合名。
    const APPROVAL_PROCESS_INSTANCES: &'static str = "approval_process_instances";
    /// `approval_node_executions` 集合名。
    const APPROVAL_NODE_EXECUTIONS: &'static str = "approval_node_executions";
    /// `approval_instance_assignees` 集合名。
    const APPROVAL_INSTANCE_ASSIGNEES: &'static str = "approval_instance_assignees";
    /// `approval_command_receipts` 集合名。
    const APPROVAL_COMMAND_RECEIPTS: &'static str = "approval_command_receipts";

    /// 返回流程定义集合仓储。
    ///
    /// # 返回
    /// 返回 `Repository<'_, ApprovalProcessDefinition>`。
    fn approval_process_definitions(&self) -> Repository<'_, ApprovalProcessDefinition>;

    /// 返回节点定义集合仓储。
    ///
    /// # 返回
    /// 返回 `Repository<'_, ApprovalNodeDefinition>`。
    fn approval_node_definitions(&self) -> Repository<'_, ApprovalNodeDefinition>;

    /// 返回连线定义集合仓储。
    ///
    /// # 返回
    /// 返回 `Repository<'_, ApprovalTransitionDefinition>`。
    fn approval_transition_definitions(&self) -> Repository<'_, ApprovalTransitionDefinition>;

    /// 返回运行实例集合仓储。
    ///
    /// # 返回
    /// 返回 `Repository<'_, ApprovalProcessInstance>`。
    fn approval_process_instances(&self) -> Repository<'_, ApprovalProcessInstance>;

    /// 返回节点执行集合仓储。
    ///
    /// # 返回
    /// 返回 `Repository<'_, ApprovalNodeExecution>`。
    fn approval_node_executions(&self) -> Repository<'_, ApprovalNodeExecution>;

    /// 返回实例审批人集合仓储。
    ///
    /// # 返回
    /// 返回 `Repository<'_, ApprovalInstanceAssignee>`。
    fn approval_instance_assignees(&self) -> Repository<'_, ApprovalInstanceAssignee>;

    /// 返回命令收据集合仓储。
    ///
    /// # 返回
    /// 返回 `Repository<'_, ApprovalCommandReceipt>`。
    fn approval_command_receipts(&self) -> Repository<'_, ApprovalCommandReceipt>;

    /// 返回跨 BPM 目标集合的聚合仓储。
    ///
    /// # 返回
    /// 返回只映射 BPM 模型的 [`BpmWorkflowRepository`]。
    fn bpm_workflow(&self) -> BpmWorkflowRepository<'_>;
}

impl BpmExt for Database {
    fn approval_process_definitions(&self) -> Repository<'_, ApprovalProcessDefinition> {
        Repository::new(self, Self::APPROVAL_PROCESS_DEFINITIONS)
    }

    fn approval_node_definitions(&self) -> Repository<'_, ApprovalNodeDefinition> {
        Repository::new(self, Self::APPROVAL_NODE_DEFINITIONS)
    }

    fn approval_transition_definitions(&self) -> Repository<'_, ApprovalTransitionDefinition> {
        Repository::new(self, Self::APPROVAL_TRANSITION_DEFINITIONS)
    }

    fn approval_process_instances(&self) -> Repository<'_, ApprovalProcessInstance> {
        Repository::new(self, Self::APPROVAL_PROCESS_INSTANCES)
    }

    fn approval_node_executions(&self) -> Repository<'_, ApprovalNodeExecution> {
        Repository::new(self, Self::APPROVAL_NODE_EXECUTIONS)
    }

    fn approval_instance_assignees(&self) -> Repository<'_, ApprovalInstanceAssignee> {
        Repository::new(self, Self::APPROVAL_INSTANCE_ASSIGNEES)
    }

    fn approval_command_receipts(&self) -> Repository<'_, ApprovalCommandReceipt> {
        Repository::new(self, Self::APPROVAL_COMMAND_RECEIPTS)
    }

    fn bpm_workflow(&self) -> BpmWorkflowRepository<'_> {
        BpmWorkflowRepository::new(self)
    }
}
