//! 域 D34 `integration_ops` 仓储访问器。
//!
//! 集合名常量定义为 trait 关联常量（唯一权威来源，conventions §4.3「Repository
//! 与索引共用同一常量」）：`indexes/` 与 `repository/` 均为冻结声明下的私有
//! 子树，模块路径无法互相引用；关联常量随 trait 公开可达，两侧统一取
//! `<mongodb::Database as IntegrationOpsExt>::INBOX_MESSAGES` 等值。

use entities::integration_ops::{
    InboxMessage, IntegrationErrorTask, ReconciliationDifference, ReconciliationDifferenceResolution,
};
use mongodb::Database;

use super::super::integration_ops::{
    InboxMessageFilter, IntegrationErrorTaskFilter, IntegrationOpsRepository, ReconciliationDifferenceFilter,
};
use crate::Repository;

/// 域 D34 仓储访问器。
pub trait IntegrationOpsExt {
    /// `inbox_message` 集合名。
    const INBOX_MESSAGES: &'static str = "inbox_messages";
    /// `integration_error_task` 集合名。
    const INTEGRATION_ERROR_TASKS: &'static str = "integration_error_tasks";
    /// `reconciliation_difference` 集合名。
    const RECONCILIATION_DIFFERENCES: &'static str = "reconciliation_differences";
    /// `reconciliation_difference_resolution` 集合名。
    const RECONCILIATION_DIFFERENCE_RESOLUTIONS: &'static str = "reconciliation_difference_resolutions";

    /// 入站消息列表筛选条件类型（定义见 `repository::integration_ops`）。
    type InboxMessageFilter;

    /// 错误任务列表筛选条件类型（定义见 `repository::integration_ops`）。
    type IntegrationErrorTaskFilter;

    /// 对账差异列表筛选条件类型（定义见 `repository::integration_ops`）。
    type ReconciliationDifferenceFilter;

    /// 获取 `inbox_message` 集合的 Repository。
    ///
    /// # 返回
    /// 返回 `Repository<'_, entities::integration_ops::InboxMessage>`。
    fn inbox_messages(&self) -> Repository<'_, InboxMessage>;

    /// 获取 `integration_error_task` 集合的 Repository。
    ///
    /// # 返回
    /// 返回 `Repository<'_, entities::integration_ops::IntegrationErrorTask>`。
    fn integration_error_tasks(&self) -> Repository<'_, IntegrationErrorTask>;

    /// 获取 `reconciliation_difference` 集合的 Repository。
    ///
    /// # 返回
    /// 返回 `Repository<'_, entities::integration_ops::ReconciliationDifference>`。
    fn reconciliation_differences(&self) -> Repository<'_, ReconciliationDifference>;

    /// 获取 `reconciliation_difference_resolution` 集合的 Repository。
    ///
    /// # 返回
    /// 返回 `Repository<'_, entities::integration_ops::ReconciliationDifferenceResolution>`。
    fn reconciliation_difference_resolutions(&self) -> Repository<'_, ReconciliationDifferenceResolution>;

    /// 获取承载跨集合事务写入的域专用仓储。
    ///
    /// # 返回
    /// 返回 `IntegrationOpsRepository` 实例。
    fn integration_ops(&self) -> IntegrationOpsRepository<'_>;
}

impl IntegrationOpsExt for Database {
    type InboxMessageFilter = InboxMessageFilter;
    type IntegrationErrorTaskFilter = IntegrationErrorTaskFilter;
    type ReconciliationDifferenceFilter = ReconciliationDifferenceFilter;

    fn inbox_messages(&self) -> Repository<'_, InboxMessage> {
        Repository::new(self, Self::INBOX_MESSAGES)
    }

    fn integration_error_tasks(&self) -> Repository<'_, IntegrationErrorTask> {
        Repository::new(self, Self::INTEGRATION_ERROR_TASKS)
    }

    fn reconciliation_differences(&self) -> Repository<'_, ReconciliationDifference> {
        Repository::new(self, Self::RECONCILIATION_DIFFERENCES)
    }

    fn reconciliation_difference_resolutions(&self) -> Repository<'_, ReconciliationDifferenceResolution> {
        Repository::new(self, Self::RECONCILIATION_DIFFERENCE_RESOLUTIONS)
    }

    fn integration_ops(&self) -> IntegrationOpsRepository<'_> {
        IntegrationOpsRepository::new(self)
    }
}
