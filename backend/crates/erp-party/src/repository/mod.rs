//! 主体 MongoDB 仓储与访问器。

pub mod extensions;
pub mod owned;
pub mod party;
pub mod prelude;

pub use extensions::PartyExt;
pub use owned::{
    PartyAddressRepository, PartyBankAccountRepository, PartyContactRepository, PartyRepository,
    PartyRevisionRepository, PartyTaxProfileRepository,
};
pub use party::{
    PartyAddressFilter, PartyAddressRepositoryExt, PartyBankAccountFilter, PartyBankAccountRepositoryExt,
    PartyContactFilter, PartyContactRepositoryExt, PartyDomainRepository, PartyFilter,
    PartyRepositoryCompanyExt, PartyRepositoryExt, PartyRevisionFilter, PartyRevisionRepositoryExt,
    PartyTaxProfileFilter, PartyTaxProfileRepositoryExt,
};

pub(crate) mod directory;

pub mod exact_identity;
