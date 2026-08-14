//! 域 D03 审批定义与运行实例仓储访问器。
//!
//! 集合名常量是仓储与索引共用的唯一权威来源，避免审批定义、实例和步骤在不同
//! 模块使用漂移的集合名。

use entities::approval::{
    ApprovalDefinition, ApprovalInstance, ApprovalStepDefinition, ApprovalStepInstance,
};
use mongodb::Database;

use super::super::approval::{ApprovalInstanceFilter, ApprovalRepository};
use crate::Repository;

/// 审批定义与运行实例仓储访问器。
pub trait ApprovalExt {
    /// `approval_definition` 集合名。
    const APPROVAL_DEFINITIONS: &'static str = "approval_definitions";
    /// `approval_step_definition` 集合名。
    const APPROVAL_STEP_DEFINITIONS: &'static str = "approval_step_definitions";
    /// `approval_instance` 集合名。
    const APPROVAL_INSTANCES: &'static str = "approval_instances";
    /// `approval_step_instance` 集合名。
    const APPROVAL_STEP_INSTANCES: &'static str = "approval_step_instances";

    /// 审批实例列表筛选条件类型。
    type ApprovalInstanceFilter;

    /// 获取审批定义集合仓储。
    ///
    /// # 返回
    /// 返回 `Repository<'_, ApprovalDefinition>`。
    fn approval_definitions(&self) -> Repository<'_, ApprovalDefinition>;

    /// 获取审批步骤定义集合仓储。
    ///
    /// # 返回
    /// 返回 `Repository<'_, ApprovalStepDefinition>`。
    fn approval_step_definitions(&self) -> Repository<'_, ApprovalStepDefinition>;

    /// 获取审批实例集合仓储。
    ///
    /// # 返回
    /// 返回 `Repository<'_, ApprovalInstance>`。
    fn approval_instances(&self) -> Repository<'_, ApprovalInstance>;

    /// 获取审批步骤实例集合仓储。
    ///
    /// # 返回
    /// 返回 `Repository<'_, ApprovalStepInstance>`。
    fn approval_step_instances(&self) -> Repository<'_, ApprovalStepInstance>;

    /// 获取审批定义跨集合写入仓储。
    ///
    /// # 返回
    /// 返回用于「草稿定义 + 全部步骤」事务写入的 [`ApprovalRepository`]。
    fn approval(&self) -> ApprovalRepository<'_>;
}

impl ApprovalExt for Database {
    type ApprovalInstanceFilter = ApprovalInstanceFilter;

    fn approval_definitions(&self) -> Repository<'_, ApprovalDefinition> {
        Repository::new(self, Self::APPROVAL_DEFINITIONS)
    }

    fn approval_step_definitions(&self) -> Repository<'_, ApprovalStepDefinition> {
        Repository::new(self, Self::APPROVAL_STEP_DEFINITIONS)
    }

    fn approval_instances(&self) -> Repository<'_, ApprovalInstance> {
        Repository::new(self, Self::APPROVAL_INSTANCES)
    }

    fn approval_step_instances(&self) -> Repository<'_, ApprovalStepInstance> {
        Repository::new(self, Self::APPROVAL_STEP_INSTANCES)
    }

    fn approval(&self) -> ApprovalRepository<'_> {
        ApprovalRepository::new(self)
    }
}
