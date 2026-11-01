//! [`persistence_core::Repository`] 按集合区分的别名。
//!
//! 领域方法是泛型仓储上的扩展 trait。

pub type InboxMessageRepository<'a> =
    persistence_core::Repository<'a, crate::entity::integration_ops::InboxMessage>;

pub type IntegrationErrorTaskRepository<'a> =
    persistence_core::Repository<'a, crate::entity::integration_ops::IntegrationErrorTask>;

pub type ReconciliationDifferenceRepository<'a> =
    persistence_core::Repository<'a, crate::entity::integration_ops::ReconciliationDifference>;

pub type ReconciliationDifferenceResolutionRepository<'a> =
    persistence_core::Repository<'a, crate::entity::integration_ops::ReconciliationDifferenceResolution>;
