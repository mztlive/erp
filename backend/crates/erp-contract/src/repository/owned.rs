//! Collection-scoped aliases for [`persistence_core::Repository`].
//!
//! Domain-specific methods are extension traits on the generic repository.

pub type ContractRepository<'a> = persistence_core::Repository<'a, crate::entity::contract::Contract>;

pub type ContractRevisionRepository<'a> =
    persistence_core::Repository<'a, crate::entity::contract::ContractRevision>;
