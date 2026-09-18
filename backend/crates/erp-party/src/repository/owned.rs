//! Collection-scoped aliases for [`persistence_core::Repository`].
//!
//! Domain-specific methods are extension traits on the generic repository.

pub type PartyRepository<'a> = persistence_core::Repository<'a, crate::entity::party::Party>;
pub type PartyRevisionRepository<'a> = persistence_core::Repository<'a, crate::entity::party::PartyRevision>;
pub type PartyContactRepository<'a> = persistence_core::Repository<'a, crate::entity::party::PartyContact>;
pub type PartyAddressRepository<'a> = persistence_core::Repository<'a, crate::entity::party::PartyAddress>;
pub type PartyTaxProfileRepository<'a> =
    persistence_core::Repository<'a, crate::entity::party::PartyTaxProfile>;
pub type PartyBankAccountRepository<'a> =
    persistence_core::Repository<'a, crate::entity::party::PartyBankAccount>;
