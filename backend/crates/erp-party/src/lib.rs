//! Party domain: stable party identity, subordinate facts and sensitive-data codec.

pub mod dto;
pub mod entity;
mod error;
pub mod indexes;
pub mod ports;
pub mod repository;
pub mod service;

pub use application_core::PageView;
pub use dto::party::{
    CreatePartyAddressRequest, CreatePartyBankAccountRequest, CreatePartyContactRequest, CreatePartyRequest,
    CreatePartyTaxProfileRequest, PartyAddressListParams, PartyAddressView, PartyBankAccountListParams,
    PartyBankAccountView, PartyContactListParams, PartyContactView, PartyListParams, PartyRevisionListParams,
    PartyRevisionView, PartyTaxProfileListParams, PartyTaxProfileView, PartyView, UpdatePartyAddressRequest,
    UpdatePartyBankAccountRequest, UpdatePartyContactRequest, UpdatePartyRequest,
    UpdatePartyTaxProfileRequest,
};
pub use entity::party::{
    select_current_default, AddressType, EffectiveRecordStatus, Party, PartyAddress,
    PartyAddressContentMatch, PartyAddressData, PartyAddressId, PartyAddressUpdate, PartyBankAccount,
    PartyBankAccountContentMatch, PartyBankAccountData, PartyBankAccountId, PartyBankAccountUpdate,
    PartyContact, PartyContactContentMatch, PartyContactData, PartyContactId, PartyContactUpdate, PartyData,
    PartyId, PartyKind, PartyOwned, PartyRevision, PartyRevisionData, PartyRevisionId, PartyStatus,
    PartyTaxProfile, PartyTaxProfileData, PartyTaxProfileId, PartyTaxProfileUpdate, PartyUpdate,
    QueryFingerprint, SensitiveFactReuse,
};
pub use error::{Error, Result};
pub use ports::{
    FailClosedAuditPort, FailClosedSupplierRolePort, PartyAuditPort, PreparedPartyAudit, SupplierRolePort,
};
pub use repository::{
    PartyAddressFilter, PartyAddressRepository, PartyBankAccountFilter, PartyBankAccountRepository,
    PartyContactFilter, PartyContactRepository, PartyDomainRepository, PartyExt, PartyFilter,
    PartyRepository, PartyRevisionFilter, PartyRevisionRepository, PartyTaxProfileFilter,
    PartyTaxProfileRepository,
};
pub use service::party::{
    ensure_outside_supplier_profile, PartyAddressService, PartyBankAccountService, PartyContactService,
    PartyDetailView, PartyService, PartyTaxProfileService, SensitiveDataCodec, SensitiveFieldKind,
    SensitiveRevealScope,
};
