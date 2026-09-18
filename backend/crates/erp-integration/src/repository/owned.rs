//! Collection-scoped aliases for [`persistence_core::Repository`].
//!
//! Domain-specific methods are extension traits on the generic repository.

pub type InboxMessageRepository<'a> =
    persistence_core::Repository<'a, crate::entity::integration_ops::InboxMessage>;

pub type IntegrationErrorTaskRepository<'a> =
    persistence_core::Repository<'a, crate::entity::integration_ops::IntegrationErrorTask>;

pub type ReconciliationDifferenceRepository<'a> =
    persistence_core::Repository<'a, crate::entity::integration_ops::ReconciliationDifference>;

pub type ReconciliationDifferenceResolutionRepository<'a> =
    persistence_core::Repository<'a, crate::entity::integration_ops::ReconciliationDifferenceResolution>;
