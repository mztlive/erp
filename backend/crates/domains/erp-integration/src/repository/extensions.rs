//! 域 D34 `integration_ops` 仓储访问器。
//!
//! 集合名常量定义为 trait 关联常量（唯一权威来源，conventions §4.3「Repository
//! 与索引共用同一常量」）：`indexes/` 与 `repository/` 均为冻结声明下的私有
//! 子树，模块路径无法互相引用；关联常量随 trait 公开可达，两侧统一取
//! `<mongodb::Database as IntegrationOpsExt>::INBOX_MESSAGES` 等值。

use mongodb::Database;

use super::integration_ops::IntegrationOpsRepository;
use crate::repository::owned::{
    InboxMessageRepository, IntegrationErrorTaskRepository, ReconciliationDifferenceRepository,
    ReconciliationDifferenceResolutionRepository,
};

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

    /// 绑定 `inbox_message` 集合仓储，供领域查询或写入步骤复用。返回 `InboxMessageRepository<'_>`。
    fn inbox_messages(&self) -> InboxMessageRepository<'_>;

    /// 绑定 `integration_error_task` 集合仓储，供领域查询或写入步骤复用。返回
    /// `IntegrationErrorTaskRepository<'_>`。
    fn integration_error_tasks(&self) -> IntegrationErrorTaskRepository<'_>;

    /// 绑定 `reconciliation_difference` 集合仓储，供领域查询或写入步骤复用。
    ///
    /// 返回 `ReconciliationDifferenceRepository<'_>`。
    fn reconciliation_differences(&self) -> ReconciliationDifferenceRepository<'_>;

    /// 绑定 `reconciliation_difference_resolution` 集合仓储，供领域查询或写入步骤复用。
    ///
    /// 返回 `ReconciliationDifferenceResolutionRepository<'_>`。
    fn reconciliation_difference_resolutions(&self) -> ReconciliationDifferenceResolutionRepository<'_>;

    /// 绑定本域跨集合用例的仓储；读写和事务边界仍由调用方传入的执行器决定。返回 `IntegrationOpsRepository` 实例。
    fn integration_ops(&self) -> IntegrationOpsRepository<'_>;
}

impl IntegrationOpsExt for Database {
    fn inbox_messages(&self) -> InboxMessageRepository<'_> {
        InboxMessageRepository::new(self, Self::INBOX_MESSAGES)
    }

    fn integration_error_tasks(&self) -> IntegrationErrorTaskRepository<'_> {
        IntegrationErrorTaskRepository::new(self, Self::INTEGRATION_ERROR_TASKS)
    }

    fn reconciliation_differences(&self) -> ReconciliationDifferenceRepository<'_> {
        ReconciliationDifferenceRepository::new(self, Self::RECONCILIATION_DIFFERENCES)
    }

    fn reconciliation_difference_resolutions(&self) -> ReconciliationDifferenceResolutionRepository<'_> {
        ReconciliationDifferenceResolutionRepository::new(self, Self::RECONCILIATION_DIFFERENCE_RESOLUTIONS)
    }

    fn integration_ops(&self) -> IntegrationOpsRepository<'_> {
        IntegrationOpsRepository::new(self)
    }
}
