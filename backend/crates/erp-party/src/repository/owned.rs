//! [`persistence_core::Repository`] 的集合别名。
//!
//! 领域方法是泛型仓储上的扩展 trait。

pub type PartyRepository<'a> = persistence_core::Repository<'a, crate::entity::party::Party>;
pub type PartyRevisionRepository<'a> = persistence_core::Repository<'a, crate::entity::party::PartyRevision>;
pub type PartyContactRepository<'a> = persistence_core::Repository<'a, crate::entity::party::PartyContact>;
pub type PartyAddressRepository<'a> = persistence_core::Repository<'a, crate::entity::party::PartyAddress>;
pub type PartyTaxProfileRepository<'a> =
    persistence_core::Repository<'a, crate::entity::party::PartyTaxProfile>;
pub type PartyBankAccountRepository<'a> =
    persistence_core::Repository<'a, crate::entity::party::PartyBankAccount>;
