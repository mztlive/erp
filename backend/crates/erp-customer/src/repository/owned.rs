//! Collection-scoped aliases for [`persistence_core::Repository`].
//!
//! Domain-specific methods are extension traits on the generic repository.

pub type CustomerAccountRepository<'a> =
    persistence_core::Repository<'a, crate::entity::customer::CustomerAccount>;
pub type CustomerAssignmentRepository<'a> =
    persistence_core::Repository<'a, crate::entity::customer::CustomerAssignment>;
pub type CustomerProfileCommandRepository<'a> =
    persistence_core::Repository<'a, crate::entity::customer::CustomerProfileCommand>;
