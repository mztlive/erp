//! 供 Handler 与 Process 复用的客户 HTTP/应用 DTO。

pub mod customer;
pub mod party_snapshot;

pub use customer::*;
pub use party_snapshot::{
    AddressType, EffectiveRecordStatus, PartyAddressView, PartyBankAccountView, PartyContactView,
    PartyRevisionView, PartyStatus, PartyTaxProfileView, SensitiveFieldKind,
};
