//! Owned party repositories composed from persistence-core.

mod party;
mod party_address;
mod party_bank_account;
mod party_contact;
mod party_revision;
mod party_tax_profile;

pub use party::PartyRepository;
pub use party_address::PartyAddressRepository;
pub use party_bank_account::PartyBankAccountRepository;
pub use party_contact::PartyContactRepository;
pub use party_revision::PartyRevisionRepository;
pub use party_tax_profile::PartyTaxProfileRepository;
